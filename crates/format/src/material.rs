//! Materials (PBR, Toon, Unlit models) and texture references.

use serde::{Deserialize, Serialize};

use crate::blob::BlobId;
use crate::math::Vec3;

/// A material: one of the native models plus shared render state (design doc §6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// Name from the source asset.
    pub name: String,
    /// Shading model and its parameters.
    pub model: MaterialModel,
    /// How alpha is used.
    pub alpha: AlphaMode,
    /// Render both faces.
    pub double_sided: bool,
    /// Draw order hint, Unity render queue semantics (2000 geometry, 2450 cutout, 3000 transparent).
    pub render_queue: i32,
}

/// Alpha handling.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum AlphaMode {
    /// Alpha ignored.
    Opaque,
    /// Discard below `cutoff`.
    Mask {
        /// Alpha threshold.
        cutoff: f32,
    },
    /// Alpha blended, sorted back to front.
    Blend,
}

/// The native shading models. Every source shader maps to one of these or is translated (design doc §10).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MaterialModel {
    /// glTF metallic-roughness and Unity Standard.
    Pbr(Pbr),
    /// The common feature set of lilToon, Poiyomi, UTS2, Sunao and MToon.
    Toon(Toon),
    /// Unlit and mobile shaders.
    Unlit(Unlit),
}

/// A texture used by a material slot.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextureSlot {
    /// Index into [`crate::Avatar::textures`].
    pub texture: u32,
    /// Which UV set to sample (0 or 1).
    pub uv_set: u8,
    /// `[scale_u, scale_v, offset_u, offset_v]`.
    pub scale_offset: [f32; 4],
}

/// Metallic-roughness PBR.
#[allow(missing_docs)] // Field names follow glTF 2.0 and are documented there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pbr {
    pub base_color: [f32; 4],
    pub base_color_tex: Option<TextureSlot>,
    pub metallic: f32,
    pub roughness: f32,
    pub metallic_roughness_tex: Option<TextureSlot>,
    pub normal_tex: Option<TextureSlot>,
    pub normal_scale: f32,
    pub occlusion_tex: Option<TextureSlot>,
    pub emissive: Vec3,
    pub emissive_tex: Option<TextureSlot>,
}

/// Toon shading with the feature flags most avatar shaders share.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Toon {
    /// Lit colour multiplier.
    pub base_color: [f32; 4],
    /// Lit colour texture.
    pub base_color_tex: Option<TextureSlot>,
    /// Tangent-space normal map.
    pub normal_tex: Option<TextureSlot>,
    /// Normal map strength.
    pub normal_scale: f32,
    /// Colour in shadow.
    pub shade_color: Vec3,
    /// Shadow colour texture.
    pub shade_tex: Option<TextureSlot>,
    /// Shifts the light/shadow boundary, -1 to 1.
    pub shade_shift: f32,
    /// Hardness of the light/shadow boundary, 0 (soft) to 1 (hard).
    pub shade_toony: f32,
    /// Rim light colour; black disables it.
    pub rim_color: Vec3,
    /// Rim falloff exponent.
    pub rim_power: f32,
    /// Additive matcap texture.
    pub matcap_tex: Option<TextureSlot>,
    /// Matcap blend strength.
    pub matcap_strength: f32,
    /// Emission colour.
    pub emissive: Vec3,
    /// Emission texture.
    pub emissive_tex: Option<TextureSlot>,
    /// Outline width in metres; 0 disables the outline pass.
    pub outline_width: f32,
    /// Outline colour.
    pub outline_color: Vec3,
    /// Per-vertex outline width mask.
    pub outline_width_tex: Option<TextureSlot>,
}

/// Unlit colour or texture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Unlit {
    /// Colour multiplier.
    pub color: [f32; 4],
    /// Colour texture.
    pub color_tex: Option<TextureSlot>,
}

/// An image stored as a blob.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Texture {
    /// Name from the source asset.
    pub name: String,
    /// Encoding of the blob.
    pub format: ImageFormat,
    /// Whether texels are sRGB colour or linear data (normals, masks).
    pub color_space: ColorSpace,
    /// The encoded image.
    pub blob: BlobId,
}

/// Image encodings. Source packages carry PNG/JPEG; runtime packages carry KTX2.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Ktx2,
}

/// Texel colour space.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorSpace {
    Srgb,
    Linear,
}
