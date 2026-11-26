//! Materials (PBR, Toon, Unlit models) and texture references.

use serde::{Deserialize, Serialize};

use crate::blob::BlobId;
use crate::math::Vec3;

/// A material: one of the native models plus shared render state (design doc §6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    /// Name from the source asset.
    pub name: String,
    /// Shading model and its parameters.
    pub model: MaterialModel,
    /// How alpha is used.
    pub alpha: AlphaMode,
    /// Render both faces.
    pub double_sided: bool,
    /// Draw order within the alpha pass, lowest first (ADR 0090); see [`Material::queue`].
    pub order: i32,
}

impl Material {
    /// The draw queue the renderer sorts by: the alpha pass's base (2000 opaque, 2450 mask, 3000 blend) plus `order`.
    pub fn queue(&self) -> i32 {
        self.alpha.queue_base().saturating_add(self.order)
    }
}

/// A material field curves can animate (ADR 0099), in the units the material stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialField {
    /// Base colour (PBR, Toon) or colour (Unlit), linear RGBA.
    BaseColor,
    /// Emission colour, linear RGB.
    Emission,
    /// Toon shade colour, linear RGB.
    ShadeColor,
    /// Toon rim colour, linear RGB.
    RimColor,
    /// Toon outline colour, linear RGB.
    OutlineColor,
    /// PBR metallic.
    Metallic,
    /// PBR roughness.
    Roughness,
    /// Normal map strength.
    NormalScale,
    /// Alpha cutoff of a masked material.
    Cutoff,
    /// Toon shade shift.
    ShadeShift,
    /// Toon shade toony.
    ShadeToony,
    /// Toon rim power.
    RimPower,
    /// The first toon matcap's strength.
    MatcapStrength,
    /// The base colour texture's [`TextureSlot::scale_offset`].
    BaseColorTransform,
    /// Toon light clamp's lower bound.
    LightMin,
    /// Toon light clamp's upper bound.
    LightMax,
}

impl MaterialField {
    /// Number of components: 4 for the base colour and the texture transform, 3 for the other colours, else 1.
    pub fn components(self) -> u8 {
        match self {
            MaterialField::BaseColor | MaterialField::BaseColorTransform => 4,
            f if f.is_color() => 3,
            _ => 1,
        }
    }

    /// Colours, whose curves may be sRGB-encoded.
    pub fn is_color(self) -> bool {
        matches!(
            self,
            MaterialField::BaseColor
                | MaterialField::Emission
                | MaterialField::ShadeColor
                | MaterialField::RimColor
                | MaterialField::OutlineColor
        )
    }
}

/// Alpha handling.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
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

impl AlphaMode {
    /// The draw queue of the mode's pass, which [`Material::order`] offsets.
    pub fn queue_base(self) -> i32 {
        match self {
            AlphaMode::Opaque => 2000,
            AlphaMode::Mask { .. } => 2450,
            AlphaMode::Blend => 3000,
        }
    }
}

/// The native shading models. Every source shader maps to one of these or is translated (design doc §10).
#[allow(clippy::large_enum_variant)] // An avatar has tens of materials; boxing the toon variant saves nothing that matters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaterialModel {
    /// Metallic-roughness PBR, as glTF 2.0.
    Pbr(Pbr),
    /// Toon shading with optional features; the avatar toon shader families lower into it (design doc §10).
    Toon(Toon),
    /// Unlit and mobile shaders.
    Unlit(Unlit),
}

/// A texture used by a material slot.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureSlot {
    /// Index into [`crate::Document::textures`].
    pub texture: u32,
    /// Which UV set to sample (0 or 1).
    pub uv_set: u8,
    /// `[scale_u, scale_v, offset_u, offset_v]`.
    pub scale_offset: [f32; 4],
}

/// Metallic-roughness PBR.
#[allow(missing_docs)] // Field names follow glTF 2.0 and are documented there.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pbr {
    pub base_color: [f32; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_color_tex: Option<TextureSlot>,
    pub metallic: f32,
    pub roughness: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metallic_roughness_tex: Option<TextureSlot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal_tex: Option<TextureSlot>,
    pub normal_scale: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub occlusion_tex: Option<TextureSlot>,
    pub emissive: Vec3,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emissive_tex: Option<TextureSlot>,
}

/// Toon shading (ADR 0090): a lit colour that turns into a shade colour across a ramp, plus optional features, each
/// computing one named thing. An absent feature computes nothing, and curves on its fields do nothing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Toon {
    /// Lit colour multiplier, linear RGBA.
    pub base_color: [f32; 4],
    /// Lit colour texture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_color_tex: Option<TextureSlot>,
    /// Colour in shadow, linear RGB.
    pub shade_color: Vec3,
    /// Shadow colour texture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shade_tex: Option<TextureSlot>,
    /// Shifts the light/shadow boundary, -1 to 1.
    pub shade_shift: f32,
    /// Hardness of the light/shadow boundary, 0 (soft) to 1 (hard).
    pub shade_toony: f32,
    /// Light plus ambient clamped per channel; `None` adds the ambient on top of the lit blend instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light_clamp: Option<LightClamp>,
    /// Tangent-space normal map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal: Option<Normal>,
    /// Light the surface emits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emission: Option<Emission>,
    /// Matcaps, blended in order; the renderer draws up to [`MATCAPS`].
    pub matcaps: Vec<Matcap>,
    /// Fresnel rim light.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rim: Option<Rim>,
    /// Darkens the lit colour towards grazing angles.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rim_shade: Option<RimShade>,
    /// The shadow beyond one colour: strength, a second shadow and a border colour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shade: Option<Shade>,
    /// Hue, saturation, value and gamma correction of the base colour texture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_adjust: Option<ColorAdjust>,
    /// An inverted-hull outline pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<Outline>,
}

/// Matcaps a toon material may have.
pub const MATCAPS: usize = 2;

/// The clamped light model: light plus ambient, clamped per channel to `min` and `max`, lights the shade-to-lit
/// blend, so with bounds 0 and 1 a lit surface shows its own colour at most. Without it, the light lights the blend
/// and the ambient adds on top, times the lit colour.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightClamp {
    pub min: f32,
    pub max: f32,
}

/// A tangent-space normal map, its perturbation scaled by `scale`.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Normal {
    pub tex: TextureSlot,
    pub scale: f32,
}

/// Emission: `color` (linear RGB) times the texture, multiplied towards itself times the albedo by `main_strength`,
/// added after lighting.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Emission {
    pub color: Vec3,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tex: Option<TextureSlot>,
    pub main_strength: f32,
}

/// The fresnel rim: `(1 - |n . v|)^power` ramps linearly from `border - blur / 2` to `border + blur / 2` (both
/// clamped to 0 to 1), fades towards the shadow by `shadow_mask`, and adds `color` (linear RGB), multiplied towards
/// itself times the albedo by `main_strength`. Border 0.5 and blur 1 leave the fresnel term as it is.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rim {
    pub color: Vec3,
    pub power: f32,
    pub border: f32,
    pub blur: f32,
    pub shadow_mask: f32,
    pub main_strength: f32,
}

/// Rim shade: after lighting, the colour is multiplied towards `color` by `strength * ramp((1 - |n . v|)^power)`, the
/// ramp linear from `border - blur / 2` to `border + blur / 2` (both clamped to 0 to 1).
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RimShade {
    pub color: Vec3,
    pub strength: f32,
    pub power: f32,
    pub border: f32,
    pub blur: f32,
}

/// The shadow beyond one colour: `Toon::shade_color` is the full shadow colour (times the shadow texture), and the
/// shadow mixes in by `strength`. Where the half-Lambert term is below the second band's ramp (`second_border`,
/// `second_blur`, as the rim's), the shadow turns towards the albedo times `second_color` by `second_strength`; then
/// it is multiplied towards itself times the albedo by `main_strength`; then it turns towards the lit colour by
/// `border_color` times a ramp that starts `border_range` before the first one.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shade {
    pub strength: f32,
    pub main_strength: f32,
    pub second_color: Vec3,
    pub second_strength: f32,
    pub second_border: f32,
    pub second_blur: f32,
    pub border_color: Vec3,
    pub border_range: f32,
}

/// Colour correction of the base colour texture's linear texels, before `base_color`: hue shift (turns), saturation
/// and value factors, gamma.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorAdjust {
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
    pub gamma: f32,
}

/// An outline: the mesh's back faces pushed out along the normal by `width` metres times the width texture's G
/// channel, unlit in `color` (linear RGB) times the colour texture. Width 0 draws none.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outline {
    pub width: f32,
    pub color: Vec3,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_tex: Option<TextureSlot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tex: Option<TextureSlot>,
}

/// A matcap: a texture sampled by the view-space normal.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Matcap {
    /// Matcap texture; `None` disables it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tex: Option<TextureSlot>,
    /// Blend strength.
    pub strength: f32,
    /// How it combines with the colour.
    pub blend: MatcapBlend,
    /// Tints the texture, linear RGB.
    pub color: Vec3,
    /// Scales the strength and the emission per texel by its red channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask_tex: Option<TextureSlot>,
    /// The matcap times this (and the mask) adds after lighting, whatever the blend.
    pub emission: f32,
}

/// How a matcap combines with the lit colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatcapBlend {
    /// The matcap times the strength adds.
    Add,
    /// The colour times the matcap, faded in by the strength.
    Multiply,
    /// The colour turns towards the matcap, lit as the colour is, by the strength.
    Replace,
}

/// Unlit colour or texture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unlit {
    /// Colour multiplier.
    pub color: [f32; 4],
    /// Colour texture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_tex: Option<TextureSlot>,
}

/// An image stored as a blob.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Ktx2,
}

/// Texel colour space.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorSpace {
    Srgb,
    Linear,
}
