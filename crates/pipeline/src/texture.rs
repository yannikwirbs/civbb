//! Compiler stage 4 texture compression: images capped per platform and encoded to KTX2 with Basis Universal, UASTC
//! or XUASTC within the source's size (design doc §9, ADRs 0029, 0035).

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use basisu_c_sys::BasisTextureFormat;
use basisu_c_sys::common::{BU_COMP_FLAGS_KTX2_OUTPUT, BU_COMP_FLAGS_KTX2_UASTC_ZSTD, BU_COMP_FLAGS_SRGB};
use basisu_c_sys::extra::{
    BasisuEncoder, BasisuEncoderParams, SourceImage, SourceImageFormat, basisu_encoder_init, types,
};
use civbb_format::BlobId;
use civbb_format::material::{ColorSpace, ImageFormat};
use civbb_format::package::Platform;
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
    /// Of those, encoded as UASTC; the rest are XUASTC.
    pub uastc: usize,
    /// Of those, larger than their source image: no ladder codec fitted (JPEG sources).
    pub larger: usize,
    /// Textures halved to fit the platform's cap.
    pub capped: usize,
    /// Texture blob bytes before.
    pub bytes_before: usize,
    /// Texture blob bytes after.
    pub bytes_after: usize,
    /// Of the encoded textures, read from the cache instead of encoded.
    pub cached: usize,
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

/// How a texture is encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    /// UASTC LDR 4x4 without RDO, Zstandard-supercompressed: nearly lossless, transcodes to BC7 and ASTC directly.
    Uastc,
    /// XUASTC LDR 4x4 at a unified quality, 1 to 100 (below 100 the weight grid is DCT-coded).
    Xuastc(i32),
}

/// The codecs tried for each texture, best first: the first whose KTX2 is no larger than the texture's source blob
/// is kept, else the smallest of them (ADR 0035).
pub const LADDER: [Codec; 3] = [Codec::Uastc, Codec::Xuastc(90), Codec::Xuastc(75)];

/// Encoder threads at most: one 4096-texel encode needs about 3 GB.
const MAX_WORKERS: usize = 4;

/// Encodes RGBA8 texels as KTX2 with `codec`, with the mips the renderer makes for RGBA8 textures
/// ([`civbb_core::mips::mips`]). Single-threaded, so the same input gives the same bytes.
pub fn encode_ktx2(image: &RgbaImage, srgb: bool, codec: Codec) -> Result<Vec<u8>, String> {
    basisu_encoder_init();
    let mut encoder = BasisuEncoder::new();
    let (width, height) = image.dimensions();
    let mut levels = vec![image.as_raw().clone()];
    levels.extend(civbb_core::mips::mips(image.as_raw(), width, height, srgb));
    for (i, data) in levels.iter().enumerate() {
        let size = types::Extent3d {
            width: (width >> i).max(1),
            height: (height >> i).max(1),
            depth_or_array_layers: 1,
        };
        let format = SourceImageFormat::Rgba8;
        encoder
            .set_image_slice(i as u32, SourceImage { data, format, size })
            .map_err(|e| e.to_string())?;
    }
    let mut flags = BU_COMP_FLAGS_KTX2_OUTPUT;
    if srgb {
        flags |= BU_COMP_FLAGS_SRGB;
    }
    let (format, quality, effort) = match codec {
        Codec::Uastc => {
            flags |= BU_COMP_FLAGS_KTX2_UASTC_ZSTD;
            (BasisTextureFormat::UastcLdr4x4, 100, 2)
        }
        Codec::Xuastc(q) => (BasisTextureFormat::XuastcLdr4x4, q, 2),
    };
    encoder
        .compress(BasisuEncoderParams {
            basis_tex_format: format,
            quality_level: quality,
            effort_level: effort,
            flags_and_quality: flags,
            low_level_uastc_rdo_or_dct_quality: 0.0,
        })
        .map_err(|e| e.to_string())
}

/// Encodes `image` with each [`LADDER`] codec in turn until one is no larger than `budget` bytes; returns it, or the
/// smallest when none fits, and the codec.
fn encode_within(image: &RgbaImage, srgb: bool, budget: usize) -> Result<(Vec<u8>, Codec), String> {
    let mut best: Option<(Vec<u8>, Codec)> = None;
    for codec in LADDER {
        let ktx2 = encode_ktx2(image, srgb, codec)?;
        if ktx2.len() <= budget {
            return Ok((ktx2, codec));
        }
        if best.as_ref().is_none_or(|(b, _)| ktx2.len() < b.len()) {
            best = Some((ktx2, codec));
        }
    }
    Ok(best.expect("the ladder is not empty"))
}

/// The dependencies whose versions decide encoded bytes besides this file and the mip function, as in
/// `Cargo.lock` (a test checks): part of every cache key (ADR 0097).
pub const ENCODING_DEPENDENCIES: [(&str, &str); 4] = [
    ("basisu_c_sys", "0.9.1"),
    ("image", "0.25.10"),
    ("png", "0.18.1"),
    ("zune-jpeg", "0.5.15"),
];

/// The cache file name of a texture: a hash of everything its encoded bytes depend on (ADR 0097).
fn cache_key(source: &[u8], format: ImageFormat, srgb: bool, cap: u32) -> String {
    let code = [include_str!("texture.rs"), include_str!("../../core/src/mips.rs")];
    let key = format!(
        "{ENCODING_DEPENDENCIES:?} {:?} {:?} {format:?} {srgb} {cap} {}",
        BlobId::of(code[0].as_bytes()),
        BlobId::of(code[1].as_bytes()),
        BlobId::of(source)
    );
    format!("{}.ktx2c", BlobId::of(key.as_bytes()))
}

/// A cache entry: the codec (0 UASTC, else the XUASTC quality), whether it was capped, then the KTX2 bytes.
fn read_cached(path: &Path) -> Option<(Vec<u8>, Codec, bool)> {
    let mut bytes = fs::read(path).ok()?;
    let codec = match *bytes.first()? {
        0 => Codec::Uastc,
        q @ 1..=100 => Codec::Xuastc(i32::from(q)),
        _ => return None,
    };
    let capped = *bytes.get(1)? == 1;
    bytes.drain(..2);
    Some((bytes, codec, capped))
}

/// Writes a cache entry through a temporary file, so a reader never sees a partial one.
fn write_cached(path: &Path, ktx2: &[u8], codec: Codec, capped: bool) -> Result<(), String> {
    let q = match codec {
        Codec::Uastc => 0,
        Codec::Xuastc(q) => u8::try_from(q).map_err(|e| e.to_string())?,
    };
    let mut bytes = vec![q, u8::from(capped)];
    bytes.extend_from_slice(ktx2);
    let tmp = path.with_extension(format!("{}.{:?}.tmp", std::process::id(), std::thread::current().id()));
    fs::write(&tmp, bytes)
        .and_then(|()| fs::rename(&tmp, path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Encodes every PNG and JPEG texture of a validated import to KTX2, halving those larger than the platform's cap,
/// each with the best [`LADDER`] codec no larger than its source blob, and drops the source blobs. Textures are
/// encoded in parallel, one per thread at a time, on at most four threads.
pub fn compress(imported: &mut Imported, platform: Platform) -> Result<Compressed, String> {
    compress_cached(imported, platform, None)
}

/// [`compress`], reading and writing encoded textures in the directory `cache` when given (ADR 0097). The result
/// is the same with and without it.
pub fn compress_cached(
    imported: &mut Imported,
    platform: Platform,
    cache: Option<&Path>,
) -> Result<Compressed, String> {
    if let Some(dir) = cache {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let a = &mut imported.avatar;
    let cap = max_side(platform);
    let todo: Vec<usize> = (0..a.textures.len())
        .filter(|&i| a.textures[i].format != ImageFormat::Ktx2)
        .collect();
    let next = AtomicUsize::new(0);
    // Per texture: its KTX2 bytes, the codec, whether it was capped and whether it came from the cache.
    type Done = BTreeMap<usize, Result<(Vec<u8>, Codec, bool, bool), String>>;
    let done: Mutex<Done> = Mutex::new(BTreeMap::new());
    let workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(MAX_WORKERS)
        .min(todo.len());
    let (textures, blobs) = (&a.textures, &imported.blobs);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                while let Some(&i) = todo.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let t = &textures[i];
                    let result = (|| {
                        let bytes = blobs.get(&t.blob).ok_or("blob missing")?;
                        let srgb = t.color_space == ColorSpace::Srgb;
                        let entry = cache.map(|dir| dir.join(cache_key(bytes, t.format, srgb, cap)));
                        if let Some((ktx2, codec, capped)) = entry.as_deref().and_then(read_cached) {
                            return Ok((ktx2, codec, capped, true));
                        }
                        let mut image = decode(bytes, t.format)?;
                        let capped = image.width().max(image.height()) > cap;
                        while image.width().max(image.height()) > cap {
                            let (w, h) = ((image.width() / 2).max(1), (image.height() / 2).max(1));
                            image = image::imageops::resize(&image, w, h, FilterType::Triangle);
                        }
                        let (ktx2, codec) = encode_within(&image, srgb, bytes.len())?;
                        if let Some(entry) = &entry {
                            write_cached(entry, &ktx2, codec, capped)?;
                        }
                        Ok((ktx2, codec, capped, false))
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
        let (ktx2, codec, capped, cached) = result?;
        let t = &mut a.textures[i];
        let source = imported.blobs.get(&t.blob).map_or(0, Vec::len);
        c.larger += usize::from(ktx2.len() > source);
        t.format = ImageFormat::Ktx2;
        t.blob = BlobId::of(&ktx2);
        imported.blobs.insert(t.blob, ktx2);
        c.textures += 1;
        c.uastc += usize::from(codec == Codec::Uastc);
        c.capped += usize::from(capped);
        c.cached += usize::from(cached);
    }
    let used: Vec<BlobId> = a.textures.iter().map(|t| t.blob).collect();
    imported.blobs.retain(|id, _| used.contains(id));
    c.bytes_after = imported.blobs.values().map(Vec::len).sum();
    Ok(c)
}
