//! Texture atlasing and KTX2 compression (ADR 0029): every vertex samples the same texel through the atlas as
//! before, ineligible materials keep their textures, KTX2 decodes within each codec's error, encoding is
//! reproducible, caps apply, and on local avatars packages shrink.

use std::collections::BTreeMap;
use std::io::Cursor;

use civbb_format::animator::{Binding, Clip, Curve, Key, Property};
use civbb_format::hierarchy::{Hierarchy, Node};
use civbb_format::material::{
    AlphaMode, ColorSpace, ImageFormat, Material, MaterialModel, Pbr, Texture, TextureSlot, Unlit,
};
use civbb_format::math::Transform;
use civbb_format::mesh::{Mesh, Submesh};
use civbb_format::package::Platform;
use civbb_format::{Avatar, BlobId, Caps, validate};
use civbb_pipeline::atlas::GUTTER;
use civbb_pipeline::gltf_import::{Limits, dir_resolver, import};
use civbb_pipeline::texture::{LADDER, decode, encode_ktx2, max_side};
use civbb_pipeline::{Imported, atlas, compress, package, prune};
use image::RgbaImage;

fn valid(i: &Imported) {
    if let Err(e) = validate(&i.avatar, &Caps::default(), |id| i.blobs.contains_key(&id)) {
        panic!("{e:?}");
    }
}

/// A texture with distinct texels: a gradient with noise, so a wrong texel shows.
fn pattern(w: u32, h: u32, seed: u32) -> RgbaImage {
    let mut s = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    RgbaImage::from_fn(w, h, |x, y| {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let n = (s >> 26) as u8;
        image::Rgba([
            (x * 255 / w) as u8 ^ n,
            (y * 255 / h) as u8,
            (seed * 40) as u8 ^ (n << 2),
            255,
        ])
    })
}

/// Smooth colours with a little noise, closer to painted textures than [`pattern`].
fn smooth(w: u32, h: u32) -> RgbaImage {
    let mut s = 7u32;
    RgbaImage::from_fn(w, h, |x, y| {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let n = (s >> 29) as u8;
        image::Rgba([
            (x * 200 / w) as u8 + n,
            (y * 200 / h) as u8 + n,
            120 + ((x + y) * 60 / (w + h)) as u8,
            255,
        ])
    })
}

fn png(image: &RgbaImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn unlit(name: &str, texture: u32, color: [f32; 4], scale_offset: [f32; 4]) -> Material {
    Material {
        name: name.into(),
        model: MaterialModel::Unlit(Unlit {
            color,
            color_tex: Some(TextureSlot {
                texture,
                uv_set: 0,
                scale_offset,
            }),
        }),
        alpha: AlphaMode::Opaque,
        double_sided: false,
        render_queue: 2000,
    }
}

/// A grid of 6x6 vertices spanning `uv` (min u, min v, max u, max v), as one submesh of `material` appended to `m`.
fn grid(m: &mut Mesh, material: u32, uv: [f32; 4]) {
    let base = m.positions.len() as u32;
    let first_index = m.indices.len() as u32;
    for j in 0..6 {
        for i in 0..6 {
            let (fu, fv) = (i as f32 / 5.0, j as f32 / 5.0);
            m.positions.push([fu, fv, material as f32]);
            m.normals.push([0.0, 0.0, 1.0]);
            m.uv0.push([uv[0] + (uv[2] - uv[0]) * fu, uv[1] + (uv[3] - uv[1]) * fv]);
        }
    }
    for j in 0..5 {
        for i in 0..5 {
            let v = base + j * 6 + i;
            m.indices.extend([v, v + 1, v + 6, v + 6, v + 1, v + 7]);
        }
    }
    m.submeshes.push(Submesh {
        first_index,
        index_count: m.indices.len() as u32 - first_index,
        material,
    });
}

/// Root with Body (one mesh) and Glow (its material animated). Materials: `a` and `b` equal but for their
/// textures (`b`'s UVs one tile up), `c` another colour, `cross` with triangles spanning a tile edge, `scaled`
/// with a tiling scale, `glow` animated.
fn avatar() -> Imported {
    let node = |name: &str, parent| Node {
        name: name.into(),
        parent,
        rest: Transform {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        },
        active: true,
    };
    let sizes = [(32, 16), (16, 16), (20, 12), (16, 16), (8, 8), (16, 8)];
    let mut blobs = BTreeMap::new();
    let textures = sizes
        .iter()
        .enumerate()
        .map(|(i, &(w, h))| {
            let bytes = png(&pattern(w, h, i as u32));
            let blob = BlobId::of(&bytes);
            blobs.insert(blob, bytes);
            Texture {
                name: format!("t{i}"),
                format: ImageFormat::Png,
                color_space: ColorSpace::Srgb,
                blob,
            }
        })
        .collect();
    let white = [1.0; 4];
    let plain = [1.0, 1.0, 0.0, 0.0];
    let mut body = Mesh {
        name: "Body".into(),
        node: 1,
        ..Mesh::default()
    };
    grid(&mut body, 0, [0.0, 0.0, 1.0, 1.0]);
    grid(&mut body, 1, [0.1, 1.2, 0.9, 2.0]);
    grid(&mut body, 2, [0.25, 0.0, 0.75, 0.5]);
    grid(&mut body, 3, [0.5, 0.0, 1.5, 1.0]);
    grid(&mut body, 5, [0.0, 0.0, 1.0, 1.0]);
    let mut glow = Mesh {
        name: "Glow".into(),
        node: 2,
        ..Mesh::default()
    };
    grid(&mut glow, 4, [0.0, 0.0, 1.0, 1.0]);
    let key = |time| Key {
        time,
        value: 1.0,
        in_tangent: 0.0,
        out_tangent: 0.0,
    };
    Imported {
        avatar: Avatar {
            name: "atlas".into(),
            hierarchy: Hierarchy {
                nodes: vec![node("Root", None), node("Body", Some(0)), node("Glow", Some(0))],
            },
            meshes: vec![body, glow],
            materials: vec![
                unlit("a", 0, white, plain),
                unlit("b", 1, white, plain),
                unlit("c", 2, [0.5, 1.0, 1.0, 1.0], plain),
                unlit("cross", 3, white, plain),
                unlit("glow", 4, white, plain),
                unlit("scaled", 5, white, [2.0, 2.0, 0.0, 0.0]),
            ],
            textures,
            clips: vec![Clip {
                name: "glow".into(),
                length: 1.0,
                looping: false,
                curves: vec![Curve {
                    binding: Binding {
                        path: "Glow".into(),
                        property: Property::Material("_Color.r".into()),
                    },
                    keys: vec![key(0.0), key(1.0)],
                }],
            }],
            ..Avatar::default()
        },
        blobs,
        warnings: Vec::new(),
    }
}

/// The texel a UV lands on in `image` (repeating), with its neighbours when it is within float error of a texel
/// edge.
fn texels(image: &RgbaImage, uv: [f32; 2]) -> Vec<[u8; 4]> {
    let (w, h) = image.dimensions();
    let around = |c: f32, n: u32| {
        let x = f64::from(c) * f64::from(n);
        let mut v: Vec<u32> = [x - 1e-3, x, x + 1e-3]
            .iter()
            .map(|x| (x.floor() as i64).rem_euclid(i64::from(n)) as u32)
            .collect();
        v.dedup();
        v
    };
    let mut out = Vec::new();
    for y in around(uv[1], h) {
        for x in around(uv[0], w) {
            out.push(image.get_pixel(x, y).0);
        }
    }
    out
}

/// The image a material's colour slot samples, decoded.
fn colour_image(i: &Imported, m: &Material) -> RgbaImage {
    let slot = match &m.model {
        MaterialModel::Unlit(u) => u.color_tex,
        MaterialModel::Toon(t) => t.base_color_tex,
        MaterialModel::Pbr(p) => p.base_color_tex,
    }
    .unwrap();
    let t = &i.avatar.textures[slot.texture as usize];
    decode(&i.blobs[&t.blob], t.format).unwrap()
}

/// Every vertex drawn with a colour texture samples the same texel after as before (exactly: PNG atlases).
fn same_texels(before: &Imported, after: &Imported) -> usize {
    let mut checked = 0;
    for (mb, ma) in before.avatar.meshes.iter().zip(&after.avatar.meshes) {
        assert_eq!(mb.indices, ma.indices, "atlasing does not touch indices");
        let mut cache: BTreeMap<(bool, u32), RgbaImage> = BTreeMap::new();
        for (sb, sa) in mb.submeshes.iter().zip(&ma.submeshes) {
            let (matb, mata) = (
                &before.avatar.materials[sb.material as usize],
                &after.avatar.materials[sa.material as usize],
            );
            if uv_colour(matb).is_none() {
                continue;
            }
            let ib = cache
                .entry((false, sb.material))
                .or_insert_with(|| colour_image(before, matb))
                .clone();
            let ia = cache
                .entry((true, sa.material))
                .or_insert_with(|| colour_image(after, mata))
                .clone();
            let start = sb.first_index as usize;
            for &v in &mb.indices[start..start + sb.index_count as usize] {
                let (b, a) = (texels(&ib, mb.uv0[v as usize]), texels(&ia, ma.uv0[v as usize]));
                assert!(
                    b.iter().any(|t| a.contains(t)),
                    "{}: vertex {v} {:?} -> {:?}: {b:?} vs {a:?}",
                    matb.name,
                    mb.uv0[v as usize],
                    ma.uv0[v as usize]
                );
                checked += 1;
            }
        }
    }
    checked
}

fn uv_colour(m: &Material) -> Option<TextureSlot> {
    match &m.model {
        MaterialModel::Unlit(u) => u.color_tex,
        MaterialModel::Toon(t) => t.base_color_tex,
        MaterialModel::Pbr(p) => p.base_color_tex,
    }
}

#[test]
fn atlas_joins_eligible_materials_and_keeps_texels() {
    let before = avatar();
    valid(&before);
    let mut after = avatar();
    let atlased = atlas(&mut after, 4096).unwrap();
    valid(&after);
    assert_eq!(atlased.atlases, 1, "{atlased:?}");
    // a and b become one; c shares the atlas but not the material; cross, glow and scaled keep their own.
    let names: Vec<&str> = after.avatar.materials.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["a", "c", "cross", "glow", "scaled"]);
    let tex = |name: &str| {
        let m = after.avatar.materials.iter().find(|m| m.name == name).unwrap();
        after.avatar.textures[uv_colour(m).unwrap().texture as usize]
            .name
            .clone()
    };
    assert!(tex("a").starts_with("atlas 0"));
    assert_eq!(tex("a"), tex("c"));
    assert_eq!(
        (tex("cross"), tex("glow"), tex("scaled")),
        ("t3".into(), "t4".into(), "t5".into())
    );
    assert_eq!(after.avatar.textures.len(), 4);
    assert_eq!(after.blobs.len(), 4, "the atlased sources' blobs are gone");
    let body = &after.avatar.meshes[0];
    assert_eq!(body.submeshes[0].material, body.submeshes[1].material);
    assert_eq!(same_texels(&before, &after), 6 * 150, "every index of the six grids");
}

#[test]
fn atlas_rectangles_fit_the_cap() {
    // Three 32x16 sources cannot share a 48-texel atlas with their gutters; nothing is atlased.
    let mut small = avatar();
    assert_eq!(atlas(&mut small, 48).unwrap().atlases, 0);
    assert_eq!(small.avatar.materials.len(), 6);
    let mut fits = avatar();
    let atlased = atlas(&mut fits, 128).unwrap();
    assert_eq!(atlased.atlases, 1);
    let t = fits
        .avatar
        .textures
        .iter()
        .find(|t| t.name.starts_with("atlas"))
        .unwrap();
    let image = decode(&fits.blobs[&t.blob], t.format).unwrap();
    assert!(image.width() <= 128 && image.height() <= 128);
    assert_eq!(
        (image.width() % GUTTER, image.height() % GUTTER),
        (0, 0),
        "aligned for mips 0 to 4"
    );
}

/// RMS and largest per-channel error of a KTX2 blob's level 0 against `source`.
fn codec_error(ktx2: &[u8], source: &RgbaImage) -> (f64, u8) {
    let t = basisu::Transcoder::new(ktx2).unwrap();
    assert_eq!(t.base_dimensions(), source.dimensions());
    let rgba = t
        .transcode(0, basisu::TargetFormat::Rgba32, basisu::DecodeFlags::NONE)
        .unwrap();
    let (mut sum, mut max) = (0.0, 0);
    for (a, b) in rgba.iter().zip(source.as_raw()) {
        let d = a.abs_diff(*b);
        sum += f64::from(d) * f64::from(d);
        max = max.max(d);
    }
    ((sum / rgba.len() as f64).sqrt(), max)
}

/// A colour texture and a normal map on one PBR material.
fn pbr_avatar() -> (Imported, RgbaImage, RgbaImage) {
    let colour = smooth(64, 48);
    let normal = RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba([128 + (x as u8 / 2), 128 + (y as u8 / 2), 255, 255])
    });
    let mut i = avatar();
    let (pc, pn) = (png(&colour), png(&normal));
    let (bc, bn) = (BlobId::of(&pc), BlobId::of(&pn));
    i.blobs = BTreeMap::from([(bc, pc), (bn, pn)]);
    i.avatar.textures = vec![
        Texture {
            name: "colour".into(),
            format: ImageFormat::Png,
            color_space: ColorSpace::Srgb,
            blob: bc,
        },
        Texture {
            name: "normal".into(),
            format: ImageFormat::Png,
            color_space: ColorSpace::Linear,
            blob: bn,
        },
    ];
    let slot = |texture| TextureSlot {
        texture,
        uv_set: 0,
        scale_offset: [1.0, 1.0, 0.0, 0.0],
    };
    i.avatar.materials = vec![Material {
        name: "pbr".into(),
        model: MaterialModel::Pbr(Pbr {
            base_color: [1.0; 4],
            base_color_tex: Some(slot(0)),
            metallic: 0.0,
            roughness: 1.0,
            metallic_roughness_tex: None,
            normal_tex: Some(slot(1)),
            normal_scale: 1.0,
            occlusion_tex: None,
            emissive: [0.0; 3],
            emissive_tex: None,
        }),
        alpha: AlphaMode::Opaque,
        double_sided: false,
        render_queue: 2000,
    }];
    i.avatar.clips.clear();
    i.avatar
        .meshes
        .iter_mut()
        .flat_map(|m| &mut m.submeshes)
        .for_each(|s| s.material = 0);
    (i, colour, normal)
}

#[test]
fn compress_encodes_ktx2_within_codec_error() {
    let (mut i, colour, normal) = pbr_avatar();
    valid(&i);
    let c = compress(&mut i, Platform::Desktop).unwrap();
    valid(&i);
    assert_eq!((c.textures, c.uastc, c.capped), (2, 2, 0));
    assert_eq!(i.blobs.len(), 2, "sources dropped");
    for t in &i.avatar.textures {
        assert_eq!(t.format, ImageFormat::Ktx2);
    }
    let ktx2 = |n: usize| &i.blobs[&i.avatar.textures[n].blob];
    let source = |n: usize| basisu::Transcoder::new(ktx2(n)).unwrap().source_format();
    // UASTC, the ladder's first codec, is the smallest for both (the colour image does not fit its PNG).
    assert_eq!(source(0), basisu::SourceFormat::UastcLdr);
    assert_eq!(source(1), basisu::SourceFormat::UastcLdr);
    assert_eq!(
        basisu::Transcoder::new(ktx2(0)).unwrap().level_count(),
        7,
        "mips down to 1x1"
    );
    let (rms, max) = codec_error(ktx2(0), &colour);
    assert!(rms < 2.0 && max <= 8, "UASTC colour: rms {rms}, max {max}");
    let (rms, max) = codec_error(ktx2(1), &normal);
    assert!(rms < 1.0 && max <= 4, "UASTC: rms {rms}, max {max}");
}

#[test]
fn compress_takes_the_smallest_codec_when_none_fits_the_source() {
    // A flat image is a few hundred bytes as PNG: no ladder codec fits, so the smallest is kept and counted.
    let (mut i, _, _) = pbr_avatar();
    let flat = RgbaImage::from_pixel(256, 256, image::Rgba([200, 120, 40, 255]));
    let pf = png(&flat);
    let id = BlobId::of(&pf);
    i.blobs.insert(id, pf);
    i.avatar.textures[0].blob = id;
    let c = compress(&mut i, Platform::Desktop).unwrap();
    assert_eq!(c.larger, 1);
    let smallest = LADDER
        .iter()
        .map(|&codec| encode_ktx2(&flat, true, codec).unwrap().len())
        .min()
        .unwrap();
    assert_eq!(i.blobs[&i.avatar.textures[0].blob].len(), smallest);
}

#[test]
fn compress_is_reproducible() {
    let (mut a, _, _) = pbr_avatar();
    let (mut b, _, _) = pbr_avatar();
    compress(&mut a, Platform::Desktop).unwrap();
    compress(&mut b, Platform::Desktop).unwrap();
    assert_eq!(a.blobs, b.blobs);
}

#[test]
fn compress_caps_per_platform() {
    assert_eq!((max_side(Platform::Desktop), max_side(Platform::Quest)), (4096, 2048));
    let (mut i, _, _) = pbr_avatar();
    let wide = png(&pattern(4100, 8, 3));
    i.avatar.textures[0].blob = BlobId::of(&wide);
    i.blobs.insert(i.avatar.textures[0].blob, wide);
    let c = compress(&mut i, Platform::Quest).unwrap();
    assert_eq!(c.capped, 1);
    let t = basisu::Transcoder::new(&i.blobs[&i.avatar.textures[0].blob]).unwrap();
    assert_eq!(t.base_dimensions(), (1025, 2), "halved twice");
}

/// Builds each local VRM avatar (gitignored `corpus/local/vrm/`) as `civbb build` does: atlased texels match
/// exactly, colours at every vertex UV after KTX2 stay within codec error of the source, and the package shrinks.
/// Ignored by default: it encodes every local VRM through the codec ladder (about two minutes, and CI has no
/// corpus/local). Run `cargo test --release -p civbb-pipeline --test texture -- --ignored` after changing the atlas
/// or texture stages.
#[test]
#[ignore = "encodes every local VRM (about 2 min): run with --ignored after changing atlas or texture"]
fn local_avatars_atlas_and_shrink() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/local/vrm");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: corpus/local/vrm not present");
        return;
    };
    for f in entries.flatten().map(|e| e.path()) {
        if f.extension().is_none_or(|x| x != "vrm") {
            continue;
        }
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(&f).unwrap();
        let load = || {
            let resolve = dir_resolver(f.parent().unwrap(), Limits::default().bytes);
            let mut i = import(&bytes, &Limits::default(), resolve).unwrap();
            prune(&mut i.avatar);
            i
        };
        let before = load();
        let mut after = load();
        let atlased = atlas(&mut after, max_side(Platform::Desktop)).unwrap();
        valid(&after);
        let checked = same_texels(&before, &after);
        let png_bytes = package(load(), Platform::Desktop)
            .blobs
            .values()
            .map(Vec::len)
            .sum::<usize>();
        let c = compress(&mut after, Platform::Desktop).unwrap();
        valid(&after);
        // Colours at every textured vertex, KTX2 against the source PNG.
        let mut diffs: Vec<u8> = Vec::new();
        for (mb, ma) in before.avatar.meshes.iter().zip(&after.avatar.meshes) {
            for (sb, sa) in mb.submeshes.iter().zip(&ma.submeshes) {
                let (matb, mata) = (
                    &before.avatar.materials[sb.material as usize],
                    &after.avatar.materials[sa.material as usize],
                );
                let (Some(_), Some(slot)) = (uv_colour(matb), uv_colour(mata)) else {
                    continue;
                };
                let source = colour_image(&before, matb);
                let ktx2 = &after.blobs[&after.avatar.textures[slot.texture as usize].blob];
                let t = basisu::Transcoder::new(ktx2).unwrap();
                let (w, h) = t.base_dimensions();
                let rgba = t
                    .transcode(0, basisu::TargetFormat::Rgba32, basisu::DecodeFlags::NONE)
                    .unwrap();
                let decoded = RgbaImage::from_raw(w, h, rgba).unwrap();
                let start = sb.first_index as usize;
                for &v in &mb.indices[start..start + sb.index_count as usize] {
                    let (b, a) = (
                        texels(&source, mb.uv0[v as usize]),
                        texels(&decoded, ma.uv0[v as usize]),
                    );
                    diffs.extend((0..4).map(|ch| b[0][ch].abs_diff(a[0][ch])));
                }
            }
        }
        diffs.sort_unstable();
        let mean = diffs.iter().map(|&d| f64::from(d)).sum::<f64>() / diffs.len().max(1) as f64;
        let p99 = diffs.get(diffs.len() * 99 / 100).copied().unwrap_or(0);
        let ktx2_bytes = package(after, Platform::Desktop)
            .blobs
            .values()
            .map(Vec::len)
            .sum::<usize>();
        eprintln!(
            "{name}: {atlased:?}, {checked} texels same, {c:?}, KTX2 at vertex UVs mean {mean:.2} p99 {p99}, \
             package blobs {png_bytes} -> {ktx2_bytes} bytes"
        );
        assert!(mean < 6.0 && p99 < 40, "{name}: mean {mean}, p99 {p99}");
        assert!(ktx2_bytes < png_bytes, "{name}: package grew");
    }
}
