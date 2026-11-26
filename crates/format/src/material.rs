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

/// A material field curves can animate (ADR 0099), in the units the material stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// Toon matcap strength.
    MatcapStrength,
    /// The base colour texture's [`TextureSlot::scale_offset`].
    BaseColorTransform,
    /// Toon clamped light's lower bound.
    LightMin,
    /// Toon clamped light's upper bound.
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
#[allow(clippy::large_enum_variant)] // An avatar has tens of materials; boxing the toon variant saves nothing that matters.
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
    /// Darkens the lit colour towards grazing angles (lilToon's rim shade).
    pub rim_shade: Option<RimShade>,
    /// lilToon's rim ramp and masks; `None` adds the plain fresnel rim.
    pub rim_ramp: Option<RimRamp>,
    /// lilToon's shadow strength, second shadow and border colour; `None` uses the shade colour as it is.
    pub shade_bands: Option<ShadeBands>,
    /// lilToon's tone correction of the base colour texture (`_MainTexHSVG`): hue shift (turns), saturation and value
    /// factors, gamma; applied to linear texels, before `base_color`. `None` leaves the texture as it is.
    pub tone: Option<[f32; 4]>,
    /// Matcap texture.
    pub matcap_tex: Option<TextureSlot>,
    /// Matcap blend strength.
    pub matcap_strength: f32,
    /// How the matcap combines with the lit colour.
    pub matcap_blend: MatcapBlend,
    /// Tints the matcap texture.
    pub matcap_color: Vec3,
    /// Scales the matcap strength per texel by its red channel.
    pub matcap_mask_tex: Option<TextureSlot>,
    /// The matcap times this (and the mask) adds after lighting, whatever the blend (Poiyomi's matcap emission).
    pub matcap_emission: f32,
    /// A second matcap, blended after the first (Poiyomi's second); `None` for one.
    pub second_matcap: Option<Matcap>,
    /// Emission colour.
    pub emissive: Vec3,
    /// Emission texture.
    pub emissive_tex: Option<TextureSlot>,
    /// Multiplies the emission towards itself times the albedo (lilToon's `_EmissionMainStrength`).
    pub emissive_main_strength: f32,
    /// Outline width in metres; 0 disables the outline pass.
    pub outline_width: f32,
    /// Outline colour.
    pub outline_color: Vec3,
    /// Per-vertex outline width mask.
    pub outline_width_tex: Option<TextureSlot>,
    /// Multiplies the outline colour (lilToon's `_OutlineTex`, usually the main texture).
    pub outline_tex: Option<TextureSlot>,
    /// How the main light and the ambient light the surface.
    pub light: ToonLight,
    /// The clamped model's lower bound on light plus ambient, per channel (lilToon's `_LightMinLimit`; 0 for the other
    /// families).
    pub light_min: f32,
    /// The clamped model's upper bound (lilToon's `_LightMaxLimit`; 1 for the other families).
    pub light_max: f32,
}

/// How a toon material combines the main light and the ambient.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToonLight {
    /// MToon: the light lights the shade-to-lit blend; the ambient adds on top, times the lit colour.
    Additive,
    /// lilToon, Poiyomi, UTS2 and VRChat's toon shaders: light plus ambient, clamped per channel to `Toon::light_min`
    /// and `Toon::light_max`, lights the blend, so with the default bounds a lit surface shows its own colour at most.
    Clamped,
}

/// lilToon's rim shade: after lighting, the colour is multiplied towards `color` by
/// `strength * ramp((1 - |n . v|)^power)`, the ramp linear from `border - blur / 2` to `border + blur / 2` (both clamped
/// to 0 to 1).
#[allow(missing_docs)] // Field names follow lilToon's `_RimShade*` properties.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RimShade {
    pub color: Vec3,
    pub strength: f32,
    pub power: f32,
    pub border: f32,
    pub blur: f32,
}

/// lilToon's rim light: the fresnel term `(1 - |n . v|)^rim_power` ramps linearly from `border - blur / 2` to
/// `border + blur / 2` (both clamped to 0 to 1), fades towards the shadow by `shadow_mask`, and the rim colour is
/// multiplied towards itself times the albedo by `main_strength`.
#[allow(missing_docs)] // Field names follow lilToon's `_Rim*` properties.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RimRamp {
    pub border: f32,
    pub blur: f32,
    pub shadow_mask: f32,
    pub main_strength: f32,
}

/// lilToon's shadow beyond one colour: `Toon::shade_color` is the full shadow colour (times the shadow texture), and
/// the shadow mixes in by `strength`. Where the half-Lambert term is below the second band's ramp (`second_border`,
/// `second_blur`, as the first's), the shadow turns towards the albedo times `second_color` by `second_strength`;
/// then it is multiplied towards itself times the albedo by `main_strength`; then it turns towards the lit colour by
/// `border_color` times a ramp that starts `border_range` before the first one.
#[allow(missing_docs)] // Field names follow lilToon's `_Shadow*` properties.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShadeBands {
    pub strength: f32,
    pub main_strength: f32,
    pub second_color: Vec3,
    pub second_strength: f32,
    pub second_border: f32,
    pub second_blur: f32,
    pub border_color: Vec3,
    pub border_range: f32,
}

/// A further toon matcap, with the first one's fields (`Toon::matcap_*`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Matcap {
    /// Matcap texture, sampled by the view-space normal; `None` disables it.
    pub tex: Option<TextureSlot>,
    /// Blend strength.
    pub strength: f32,
    /// How it combines with the colour.
    pub blend: MatcapBlend,
    /// Tints the texture.
    pub color: Vec3,
    /// Scales the strength and the emission per texel by its red channel.
    pub mask_tex: Option<TextureSlot>,
    /// The matcap times this adds after lighting.
    pub emission: f32,
}

/// How a toon material's matcap combines with the lit colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatcapBlend {
    /// MToon, lilToon's default: the matcap times the strength adds.
    Add,
    /// VRChat's MatCap Lit, lilToon's multiply mode: the colour times the matcap, faded in by the strength.
    Multiply,
    /// Poiyomi's replace: the colour turns towards the matcap, lit as the colour is, by the strength.
    Replace,
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
