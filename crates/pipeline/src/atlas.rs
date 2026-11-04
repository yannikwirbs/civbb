//! Compiler stage 4 texture atlasing: textures of one material model share an atlas, so materials that differ only
//! in their textures become one and `merge` joins their draws (design doc §9, ADR 0029).

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::io::Cursor;

use civbb_format::animator::Property;
use civbb_format::expressions::{ControlKind, Menu};
use civbb_format::material::{ImageFormat, Material, MaterialModel, Texture, TextureSlot};
use civbb_format::{Avatar, BlobId};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};

use crate::Imported;
use crate::prune::node_paths;
use crate::texture::decode;

/// Texels of gutter around each rectangle, wrapping its content as the repeating sampler does; also the alignment
/// of every rectangle and of the atlas sides, so mips 0 to 4 of a rectangle equal its texture's own (ADR 0035).
pub const GUTTER: u32 = 16;

const IDENTITY_ST: [f32; 4] = [1.0, 1.0, 0.0, 0.0];

/// Width and height in texels.
type Size = (u32, u32);

/// An atlas's size and the corner of each rectangle's footprint in it.
type Packing = (u32, u32, Vec<(u32, u32)>);

/// What atlasing did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Atlased {
    /// Atlas images made.
    pub atlases: usize,
    /// Textures fewer than before, atlases counted.
    pub textures: usize,
    /// Materials fewer than before: those that became one, and unused ones.
    pub materials: usize,
}

/// A material's texture slots in a fixed order, each with whether it is sampled by UV (matcaps are sampled by
/// normal).
fn slots(m: &mut MaterialModel) -> Vec<(&mut Option<TextureSlot>, bool)> {
    match m {
        MaterialModel::Pbr(p) => vec![
            (&mut p.base_color_tex, true),
            (&mut p.metallic_roughness_tex, true),
            (&mut p.normal_tex, true),
            (&mut p.occlusion_tex, true),
            (&mut p.emissive_tex, true),
        ],
        MaterialModel::Toon(t) => vec![
            (&mut t.base_color_tex, true),
            (&mut t.normal_tex, true),
            (&mut t.shade_tex, true),
            (&mut t.matcap_tex, false),
            (&mut t.emissive_tex, true),
            (&mut t.outline_width_tex, true),
        ],
        MaterialModel::Unlit(u) => vec![(&mut u.color_tex, true)],
    }
}

/// The UV-sampled slots of a material, in [`slots`] order.
fn uv_slots(m: &Material) -> Vec<Option<TextureSlot>> {
    let mut model = m.model.clone();
    slots(&mut model)
        .into_iter()
        .filter(|(_, uv)| *uv)
        .map(|(s, _)| *s)
        .collect()
}

/// Which atlas a material's textures could go to: its model and which UV slots it uses.
type SetKey = (u8, Vec<bool>);

fn set_key(m: &Material) -> SetKey {
    let model = match m.model {
        MaterialModel::Pbr(_) => 0,
        MaterialModel::Toon(_) => 1,
        MaterialModel::Unlit(_) => 2,
    };
    (model, uv_slots(m).iter().map(Option::is_some).collect())
}

/// The textures of a material's used UV slots: one atlas rectangle.
fn tuple(m: &Material) -> Vec<u32> {
    uv_slots(m).iter().flatten().map(|s| s.texture).collect()
}

/// A material with its UV textures and name blanked: materials equal this way are one after atlasing.
fn family(m: &Material) -> Material {
    let mut f = m.clone();
    f.name.clear();
    for (s, uv) in slots(&mut f.model) {
        if let (Some(s), true) = (s, uv) {
            s.texture = 0;
        }
    }
    f
}

/// Atlases the textures of a validated, pruned import in place; the avatar stays valid and every vertex samples the
/// same texels as before. `max_side` caps an atlas's sides.
pub fn atlas(imported: &mut Imported, max_side: u32) -> Result<Atlased, String> {
    compact(imported);
    let materials_before = imported.avatar.materials.len();
    let textures_before = imported.avatar.textures.len();
    let a = &imported.avatar;
    let (ok, tiles) = eligible(a, &imported.blobs);

    // Rectangles: distinct texture tuples of eligible materials, grouped by atlas set, in material order.
    struct Rect {
        textures: Vec<u32>,
        size: Size,
        materials: Vec<usize>,
    }
    let mut sets: Vec<(SetKey, Vec<Rect>)> = Vec::new();
    for (k, m) in a.materials.iter().enumerate() {
        let Some(size) = ok[k] else { continue };
        let (key, textures) = (set_key(m), tuple(m));
        let s = match sets.iter().position(|(s, _)| *s == key) {
            Some(s) => s,
            None => {
                sets.push((key, Vec::new()));
                sets.len() - 1
            }
        };
        let rects = &mut sets[s].1;
        match rects.iter_mut().find(|r| r.textures == textures) {
            Some(r) => r.materials.push(k),
            None => rects.push(Rect {
                textures,
                size,
                materials: vec![k],
            }),
        }
    }

    let mut decoded: HashMap<u32, RgbaImage> = HashMap::new();
    let mut new_textures: Vec<Texture> = Vec::new();
    let mut new_blobs: Vec<(BlobId, Vec<u8>)> = Vec::new();
    let mut new_slots: Vec<(usize, Vec<u32>)> = Vec::new();
    // Per material: where its rectangle's content sits, (x, y, w, h, atlas w, atlas h).
    let mut placed: Vec<Option<[u32; 6]>> = vec![None; a.materials.len()];
    let mut atlases = 0;
    for (_, mut rects) in sets {
        // Tallest first for shelf packing; then first fit into atlases within the cap.
        rects.sort_by_key(|r| (std::cmp::Reverse(r.size.1), std::cmp::Reverse(r.size.0)));
        let mut bins: Vec<Vec<usize>> = Vec::new();
        for r in 0..rects.len() {
            let fits = |bin: &[usize]| {
                let sizes: Vec<_> = bin.iter().chain([&r]).map(|&i| rects[i].size).collect();
                pack(&sizes, max_side).is_some()
            };
            match bins.iter().position(|b| fits(b)) {
                Some(b) => bins[b].push(r),
                None if fits(&[]) => bins.push(vec![r]),
                None => {}
            }
        }
        for bin in bins {
            let members: Vec<usize> = bin.iter().flat_map(|&r| rects[r].materials.iter().copied()).collect();
            let mut families: Vec<Material> = Vec::new();
            for &k in &members {
                let f = family(&a.materials[k]);
                if !families.contains(&f) {
                    families.push(f);
                }
            }
            if families.len() == members.len() {
                continue;
            }
            let sizes: Vec<_> = bin.iter().map(|&r| rects[r].size).collect();
            let (width, height, at) = pack(&sizes, max_side).expect("the bin was packed");
            // One image per distinct column of textures; slots with the same column share it.
            let slot_count = rects[bin[0]].textures.len();
            let mut columns: Vec<(Vec<u32>, u32)> = Vec::new();
            let mut slot_image = Vec::with_capacity(slot_count);
            for p in 0..slot_count {
                let column: Vec<u32> = bin.iter().map(|&r| rects[r].textures[p]).collect();
                let index = match columns.iter().find(|(c, _)| *c == column) {
                    Some(&(_, i)) => i,
                    None => {
                        let mut image = RgbaImage::new(width, height);
                        for (&t, &(x, y)) in column.iter().zip(&at) {
                            let src = match decoded.entry(t) {
                                Entry::Occupied(e) => e.into_mut(),
                                Entry::Vacant(e) => {
                                    let src = &a.textures[t as usize];
                                    let bytes = imported.blobs.get(&src.blob).ok_or("texture blob missing")?;
                                    e.insert(decode(bytes, src.format).map_err(|e| format!("{}: {e}", src.name))?)
                                }
                            };
                            blit(&mut image, src, x, y);
                        }
                        let png = encode_png(&image)?;
                        let first = &a.textures[column[0] as usize];
                        let index = (a.textures.len() + new_textures.len()) as u32;
                        let blob = BlobId::of(&png);
                        new_textures.push(Texture {
                            name: format!("atlas {atlases} {}", first.name),
                            format: ImageFormat::Png,
                            color_space: first.color_space,
                            blob,
                        });
                        new_blobs.push((blob, png));
                        columns.push((column, index));
                        index
                    }
                };
                slot_image.push(index);
            }
            atlases += 1;
            for (&r, &(x, y)) in bin.iter().zip(&at) {
                let (w, h) = rects[r].size;
                for &k in &rects[r].materials {
                    placed[k] = Some([x + GUTTER, y + GUTTER, w, h, width, height]);
                    new_slots.push((k, slot_image.clone()));
                }
            }
        }
    }

    let a = &mut imported.avatar;
    a.textures.extend(new_textures);
    imported.blobs.extend(new_blobs);
    for (k, images) in new_slots {
        let mut images = images.into_iter();
        for (s, uv) in slots(&mut a.materials[k].model) {
            if let (Some(s), true) = (s, uv) {
                s.texture = images.next().expect("one image per used UV slot");
            }
        }
    }
    // An atlased material equal to an earlier atlased one, name aside, is replaced by it.
    let unnamed = |m: &Material| Material {
        name: String::new(),
        ..m.clone()
    };
    let mut same: Vec<usize> = (0..a.materials.len()).collect();
    for k in (0..a.materials.len()).filter(|&k| placed[k].is_some()) {
        let m = unnamed(&a.materials[k]);
        if let Some(first) = (0..k).find(|&j| placed[j].is_some() && unnamed(&a.materials[j]) == m) {
            same[k] = first;
        }
    }
    for (m, tile) in a.meshes.iter_mut().zip(&tiles) {
        let mut done = vec![false; m.uv0.len()];
        for s in &mut m.submeshes {
            if let Some([x, y, w, h, aw, ah]) = placed[s.material as usize] {
                let start = s.first_index as usize;
                for &i in &m.indices[start..start + s.index_count as usize] {
                    if !std::mem::replace(&mut done[i as usize], true) {
                        let (uv, n) = (&mut m.uv0[i as usize], tile[i as usize]);
                        uv[0] = (x as f32 + (uv[0] - n[0]) * w as f32) / aw as f32;
                        uv[1] = (y as f32 + (uv[1] - n[1]) * h as f32) / ah as f32;
                    }
                }
            }
            s.material = same[s.material as usize] as u32;
        }
    }
    compact(imported);
    Ok(Atlased {
        atlases,
        textures: textures_before.saturating_sub(imported.avatar.textures.len()),
        materials: materials_before - imported.avatar.materials.len(),
    })
}

/// For each material, the size of its textures if it may be atlased (ADR 0029): repeated until every texture it
/// uses is used only by eligible materials of its set and no vertex it draws maps elsewhere. Also each mesh's
/// vertex UV tiles.
fn eligible(a: &Avatar, blobs: &BTreeMap<BlobId, Vec<u8>>) -> (Vec<Option<Size>>, Vec<Vec<[f32; 2]>>) {
    let dims: Vec<Option<Size>> = a
        .textures
        .iter()
        .map(|t| {
            let format = match t.format {
                ImageFormat::Png => image::ImageFormat::Png,
                ImageFormat::Jpeg => image::ImageFormat::Jpeg,
                ImageFormat::Ktx2 => return None,
            };
            let bytes = blobs.get(&t.blob)?;
            image::ImageReader::with_format(Cursor::new(bytes), format)
                .into_dimensions()
                .ok()
        })
        .collect();
    let mut ok: Vec<Option<Size>> = a
        .materials
        .iter()
        .map(|m| {
            let used: Vec<TextureSlot> = uv_slots(m).into_iter().flatten().collect();
            let plain = used.iter().all(|s| s.uv_set == 0 && s.scale_offset == IDENTITY_ST);
            let size = dims[used.first()?.texture as usize]?;
            (plain && used.iter().all(|s| dims[s.texture as usize] == Some(size))).then_some(size)
        })
        .collect();

    let paths = node_paths(&a.hierarchy);
    let by_path: HashMap<&str, usize> = paths.iter().enumerate().rev().map(|(i, p)| (p.as_str(), i)).collect();
    let mut animated = vec![false; a.hierarchy.nodes.len()];
    for c in a.clips.iter().flat_map(|c| &c.curves) {
        if let (Property::Material(_), Some(&i)) = (&c.binding.property, by_path.get(c.binding.path.as_str())) {
            animated[i] = true;
        }
    }
    // Each triangle must lie in one unit UV tile and each vertex in one tile; whole-tile offsets are shifted
    // back, which the repeating sampler makes equal.
    let mut tiles: Vec<Vec<[f32; 2]>> = Vec::with_capacity(a.meshes.len());
    for m in &a.meshes {
        let mut tile: Vec<Option<[f32; 2]>> = vec![None; m.uv0.len()];
        let mut bad = vec![false; m.uv0.len()];
        for t in m.indices.as_chunks::<3>().0.iter().filter(|_| !m.uv0.is_empty()) {
            let uv = t.map(|i| m.uv0[i as usize]);
            let n = [0, 1].map(|c| uv.iter().map(|p| p[c]).fold(f32::INFINITY, f32::min).floor());
            let inside = uv
                .iter()
                .all(|p| (0..2).all(|c| p[c].is_finite() && p[c] <= n[c] + 1.0));
            for &i in t {
                let (seen, bad) = (&mut tile[i as usize], &mut bad[i as usize]);
                *bad |= !inside || seen.is_some_and(|s| s != n);
                *seen = Some(n);
            }
        }
        for s in &m.submeshes {
            let start = s.first_index as usize;
            let fine = !m.uv0.is_empty()
                && m.indices[start..start + s.index_count as usize]
                    .iter()
                    .all(|&i| !bad[i as usize]);
            if animated[m.node as usize] || !fine {
                ok[s.material as usize] = None;
            }
        }
        tiles.push(tile.into_iter().map(Option::unwrap_or_default).collect());
    }

    #[derive(Clone, PartialEq)]
    enum Use {
        None,
        Set(SetKey),
        Pinned,
    }
    const UNSEEN: usize = usize::MAX;
    const OTHER: usize = usize::MAX - 1;
    const CONFLICT: usize = usize::MAX - 2;
    loop {
        let mut changed = false;
        let mut uses = vec![Use::None; a.textures.len()];
        let mut add = |t: u32, u: Use| {
            let slot = &mut uses[t as usize];
            *slot = match (&*slot, u) {
                (Use::None, u) => u,
                (Use::Set(s), Use::Set(u)) if *s == u => Use::Set(u),
                _ => Use::Pinned,
            };
        };
        for (k, m) in a.materials.iter().enumerate() {
            let mut model = m.model.clone();
            for (s, uv) in slots(&mut model) {
                if let Some(s) = s {
                    add(
                        s.texture,
                        if uv && ok[k].is_some() {
                            Use::Set(set_key(m))
                        } else {
                            Use::Pinned
                        },
                    );
                }
            }
        }
        if let Some(e) = &a.expressions {
            icons(&e.menu, &mut |t| add(t, Use::Pinned));
        }
        for (k, m) in a.materials.iter().enumerate() {
            if ok[k].is_some() && tuple(m).iter().any(|&t| uses[t as usize] == Use::Pinned) {
                ok[k] = None;
                changed = true;
            }
        }
        // Each vertex maps to one rectangle or stays where it is; materials meeting at a vertex otherwise drop out.
        let mut rect_ids: Vec<(SetKey, Vec<u32>)> = Vec::new();
        let rect_of: Vec<usize> = a
            .materials
            .iter()
            .enumerate()
            .map(|(k, m)| {
                if ok[k].is_none() {
                    return OTHER;
                }
                let id = (set_key(m), tuple(m));
                rect_ids.iter().position(|r| *r == id).unwrap_or_else(|| {
                    rect_ids.push(id);
                    rect_ids.len() - 1
                })
            })
            .collect();
        for m in &a.meshes {
            let mut owner = vec![UNSEEN; m.positions.len()];
            let ranges = || {
                m.submeshes.iter().map(|s| {
                    let start = s.first_index as usize;
                    (s.material as usize, &m.indices[start..start + s.index_count as usize])
                })
            };
            for (k, indices) in ranges() {
                for &i in indices {
                    let o = &mut owner[i as usize];
                    if *o == UNSEEN {
                        *o = rect_of[k];
                    } else if *o != rect_of[k] {
                        *o = CONFLICT;
                    }
                }
            }
            for (k, indices) in ranges() {
                if ok[k].is_some() && indices.iter().any(|&i| owner[i as usize] == CONFLICT) {
                    ok[k] = None;
                    changed = true;
                }
            }
        }
        if !changed {
            return (ok, tiles);
        }
    }
}

/// Calls `f` with every menu icon's texture.
fn icons(menu: &Menu, f: &mut impl FnMut(u32)) {
    for c in &menu.controls {
        if let Some(t) = c.icon {
            f(t);
        }
        if let ControlKind::SubMenu(sub) = &c.kind {
            icons(sub, f);
        }
    }
}

fn ceil_gutter(x: u32) -> u32 {
    x.div_ceil(GUTTER) * GUTTER
}

/// Shelf-packs rectangles of content `sizes` (tallest first), each with its gutter and on [`GUTTER`] boundaries, into
/// the smallest atlas whose sides are at most `max`: its size and each footprint's corner. `None` if they don't fit.
fn pack(sizes: &[Size], max: u32) -> Option<Packing> {
    let foot: Vec<Size> = sizes
        .iter()
        .map(|&(w, h)| (ceil_gutter(w + 2 * GUTTER), ceil_gutter(h + 2 * GUTTER)))
        .collect();
    let widest = foot.iter().map(|f| f.0).max()?;
    let area: u64 = foot.iter().map(|&(w, h)| u64::from(w) * u64::from(h)).sum();
    let mut width = widest.max(ceil_gutter((area as f64).sqrt().ceil() as u32));
    let step = ceil_gutter(width / 16);
    let mut best: Option<Packing> = None;
    while width <= max {
        let (mut x, mut y, mut shelf) = (0, 0, 0);
        let mut at = Vec::with_capacity(foot.len());
        for &(w, h) in &foot {
            if x + w > width {
                (x, y, shelf) = (0, y + shelf, 0);
            }
            at.push((x, y));
            x += w;
            shelf = shelf.max(h);
        }
        let height = y + shelf;
        let area = u64::from(width) * u64::from(height);
        if height <= max && best.as_ref().is_none_or(|b| area < u64::from(b.0) * u64::from(b.1)) {
            best = Some((width, height, at));
        }
        width += step;
    }
    best
}

/// Copies `src` into `atlas` with its content at `(x, y) + GUTTER`, wrapped around across the rest of its footprint.
fn blit(atlas: &mut RgbaImage, src: &RgbaImage, x: u32, y: u32) {
    let (w, h) = src.dimensions();
    let (fw, fh) = (ceil_gutter(w + 2 * GUTTER), ceil_gutter(h + 2 * GUTTER));
    for dy in 0..fh {
        let sy = (dy + h - GUTTER % h) % h;
        for dx in 0..fw {
            let sx = (dx + w - GUTTER % w) % w;
            atlas.put_pixel(x + dx, y + dy, *src.get_pixel(sx, sy));
        }
    }
}

fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Best, FilterType::Adaptive)
        .write_image(image.as_raw(), image.width(), image.height(), ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(png)
}

/// Removes materials no submesh draws, textures nothing uses and blobs no texture holds, renumbering references.
fn compact(imported: &mut Imported) {
    let a = &mut imported.avatar;
    let mut used = vec![false; a.materials.len()];
    for s in a.meshes.iter().flat_map(|m| &m.submeshes) {
        used[s.material as usize] = true;
    }
    let map = renumber(&used);
    let mut keep = used.iter();
    a.materials.retain(|_| *keep.next().expect("one flag per material"));
    for s in a.meshes.iter_mut().flat_map(|m| &mut m.submeshes) {
        s.material = map[s.material as usize];
    }

    let mut used = vec![false; a.textures.len()];
    for m in &mut a.materials {
        for (s, _) in slots(&mut m.model) {
            if let Some(s) = s {
                used[s.texture as usize] = true;
            }
        }
    }
    if let Some(e) = &a.expressions {
        icons(&e.menu, &mut |t| used[t as usize] = true);
    }
    let map = renumber(&used);
    let mut keep = used.iter();
    a.textures.retain(|_| *keep.next().expect("one flag per texture"));
    for m in &mut a.materials {
        for (s, _) in slots(&mut m.model) {
            if let Some(s) = s {
                s.texture = map[s.texture as usize];
            }
        }
    }
    if let Some(e) = &mut a.expressions {
        remap_icons(&mut e.menu, &map);
    }
    let held: Vec<BlobId> = a.textures.iter().map(|t| t.blob).collect();
    imported.blobs.retain(|id, _| held.contains(id));
}

fn remap_icons(menu: &mut Menu, map: &[u32]) {
    for c in &mut menu.controls {
        if let Some(t) = &mut c.icon {
            *t = map[*t as usize];
        }
        if let ControlKind::SubMenu(sub) = &mut c.kind {
            remap_icons(sub, map);
        }
    }
}

/// New index of each kept item.
fn renumber(keep: &[bool]) -> Vec<u32> {
    let mut next = 0;
    keep.iter()
        .map(|&k| {
            let i = next;
            next += u32::from(k);
            i
        })
        .collect()
}
