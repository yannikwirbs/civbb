//! Compiler stage 4 texture compression: images capped per platform and encoded to KTX2 with Basis Universal, UASTC
//! for normal maps and ETC1S for the rest (design doc §9, ADR 0029).

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use basisu_c_sys::BasisTextureFormat;
use basisu_c_sys::common::{
    BU_COMP_FLAGS_GEN_MIPS_WRAP, BU_COMP_FLAGS_KTX2_OUTPUT, BU_COMP_FLAGS_KTX2_UASTC_ZSTD, BU_COMP_FLAGS_SRGB,
};
use basisu_c_sys::extra::{
    BasisuEncoder, BasisuEncoderParams, SourceImage, SourceImageFormat, basisu_encoder_init, types,
};
use civbb_format::material::{ColorSpace, ImageFormat, MaterialModel};
use civbb_format::package::Platform;
use civbb_format::{Avatar, BlobId};
use image::RgbaImage;
use image::imageops::FilterType;

use crate::Imported;

/// Largest side a source image may have when decoded.
const MAX_DECODED_SIDE: u32 = 16384;

/// Largest texture side a runtime package for `platform` carries (ADR 0029).
pub fn max_side(platform: Platform) -> u32 {
    match platform {
        Platform::Desktop => 4096,
        Platform::Quest => 2048,
    }
}

/// What compression did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Compressed {
    /// Textures encoded to KTX2.
    pub textures: usize,
    /// Of those, encoded as UASTC (normal maps).
    pub uastc: usize,
    /// Textures halved to fit the platform's cap.
    pub capped: usize,
    /// Texture blob bytes before.
    pub bytes_before: usize,
    /// Texture blob bytes after.
    pub bytes_after: usize,
}

/// Decodes a PNG or JPEG blob to RGBA8 with bounded dimensions.
pub fn decode(bytes: &[u8], format: ImageFormat) -> Result<RgbaImage, String> {
    let format = match format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Jpeg => image::ImageFormat::Jpeg,
        ImageFormat::Ktx2 => return Err("already KTX2".into()),
    };
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DECODED_SIDE);
    limits.max_image_height = Some(MAX_DECODED_SIDE);
    reader.limits(limits);
    Ok(reader.decode().map_err(|e| e.to_string())?.into_rgba8())
}

/// Encodes RGBA8 texels as KTX2 with mips: UASTC at full quality when `uastc`, else ETC1S. Single-threaded, so the
/// same input gives the same bytes.
pub fn encode_ktx2(image: &RgbaImage, srgb: bool, uastc: bool) -> Result<Vec<u8>, String> {
    basisu_encoder_init();
    let mut encoder = BasisuEncoder::new();
    encoder
        .set_image(SourceImage {
            data: image.as_raw(),
            format: SourceImageFormat::Rgba8,
            size: types::Extent3d {
                width: image.width(),
                height: image.height(),
                depth_or_array_layers: 1,
            },
        })
        .map_err(|e| e.to_string())?;
    let mut flags = BU_COMP_FLAGS_KTX2_OUTPUT | BU_COMP_FLAGS_GEN_MIPS_WRAP;
    if srgb {
        flags |= BU_COMP_FLAGS_SRGB;
    }
    let (format, quality) = if uastc {
        flags |= BU_COMP_FLAGS_KTX2_UASTC_ZSTD;
        (BasisTextureFormat::UastcLdr4x4, 100)
    } else {
        (BasisTextureFormat::Etc1s, 75)
    };
    encoder
        .compress(BasisuEncoderParams {
            basis_tex_format: format,
            quality_level: quality,
            effort_level: 2,
            flags_and_quality: flags,
            low_level_uastc_rdo_or_dct_quality: 0.0,
        })
        .map_err(|e| e.to_string())
}

/// Indices of textures some material samples as a normal map.
pub fn normal_maps(a: &Avatar) -> Vec<bool> {
    let mut normal = vec![false; a.textures.len()];
    for m in &a.materials {
        let slot = match &m.model {
            MaterialModel::Pbr(p) => p.normal_tex,
            MaterialModel::Toon(t) => t.normal_tex,
            MaterialModel::Unlit(_) => None,
        };
        if let Some(s) = slot {
            normal[s.texture as usize] = true;
        }
    }
    normal
}

/// Encodes every PNG and JPEG texture of a validated import to KTX2, halving those larger than the platform's cap,
/// and drops the source blobs. Textures are encoded in parallel, one per thread at a time.
pub fn compress(imported: &mut Imported, platform: Platform) -> Result<Compressed, String> {
    let a = &mut imported.avatar;
    let normal = normal_maps(a);
    let cap = max_side(platform);
    let todo: Vec<usize> = (0..a.textures.len())
        .filter(|&i| a.textures[i].format != ImageFormat::Ktx2)
        .collect();
    let next = AtomicUsize::new(0);
    // Per texture: its KTX2 bytes and whether it was capped.
    type Done = BTreeMap<usize, Result<(Vec<u8>, bool), String>>;
    let done: Mutex<Done> = Mutex::new(BTreeMap::new());
    let workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(todo.len());
    let (textures, blobs) = (&a.textures, &imported.blobs);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                while let Some(&i) = todo.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let t = &textures[i];
                    let result = (|| {
                        let bytes = blobs.get(&t.blob).ok_or("blob missing")?;
                        let mut image = decode(bytes, t.format)?;
                        let capped = image.width().max(image.height()) > cap;
                        while image.width().max(image.height()) > cap {
                            let (w, h) = ((image.width() / 2).max(1), (image.height() / 2).max(1));
                            image = image::imageops::resize(&image, w, h, FilterType::Triangle);
                        }
                        let ktx2 = encode_ktx2(&image, t.color_space == ColorSpace::Srgb, normal[i])?;
                        Ok((ktx2, capped))
                    })()
                    .map_err(|e: String| format!("texture {:?}: {e}", t.name));
                    done.lock().expect("no worker panicked").insert(i, result);
                }
            });
        }
    });
    let mut c = Compressed {
        bytes_before: imported.blobs.values().map(Vec::len).sum(),
        ..Compressed::default()
    };
    for (i, result) in done.into_inner().expect("no worker panicked") {
        let (ktx2, capped) = result?;
        let t = &mut a.textures[i];
        t.format = ImageFormat::Ktx2;
        t.blob = BlobId::of(&ktx2);
        imported.blobs.insert(t.blob, ktx2);
        c.textures += 1;
        c.uastc += usize::from(normal[i]);
        c.capped += usize::from(capped);
    }
    let used: Vec<BlobId> = a.textures.iter().map(|t| t.blob).collect();
    imported.blobs.retain(|id, _| used.contains(id));
    c.bytes_after = imported.blobs.values().map(Vec::len).sum();
    Ok(c)
}
