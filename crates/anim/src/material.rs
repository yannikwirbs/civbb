//! Animated material properties: Unity shader property names mapped to native material fields, read and written in
//! the units Unity's curves use (ADR 0025).

use civbb_format::material::{AlphaMode, Material, MaterialModel};

/// A native material field a curve can drive. Values are in Unity's units: colours sRGB-encoded with alpha linear,
/// smoothness rather than roughness, texture offsets with V up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// Base colour (PBR, Toon) or colour (Unlit), RGBA.
    BaseColor,
    /// Emission colour, RGB.
    Emission,
    /// Toon shade colour, RGB.
    ShadeColor,
    /// Toon rim colour, RGB.
    RimColor,
    /// Toon outline colour, RGB.
    OutlineColor,
    /// PBR metallic.
    Metallic,
    /// PBR smoothness, `1 - roughness`.
    Smoothness,
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
    /// Base colour texture tiling (x, y) and offset (z, w).
    MainTexSt,
}

/// Unity property names of the common avatar shader families and the field each maps to. One flat table for every
/// material until the exporter fingerprints shader families (design doc §10 "Shaders").
const NAMES: &[(&str, Field)] = &[
    ("_Color", Field::BaseColor),            // Standard, lilToon, Poiyomi, MToon, Unlit
    ("_BaseColor", Field::BaseColor),        // UTS2, URP
    ("_EmissionColor", Field::Emission),     // Standard, lilToon, Poiyomi, MToon
    ("_Emissive_Color", Field::Emission),    // UTS2
    ("_ShadeColor", Field::ShadeColor),      // MToon
    ("_ShadowColor", Field::ShadeColor),     // lilToon
    ("_1st_ShadeColor", Field::ShadeColor),  // UTS2
    ("_RimColor", Field::RimColor),          // lilToon, MToon
    ("_RimLightColor", Field::RimColor),     // Poiyomi, UTS2
    ("_OutlineColor", Field::OutlineColor),  // lilToon, MToon
    ("_LineColor", Field::OutlineColor),     // Poiyomi
    ("_Outline_Color", Field::OutlineColor), // UTS2
    ("_Metallic", Field::Metallic),          // Standard
    ("_Glossiness", Field::Smoothness),      // Standard
    ("_Smoothness", Field::Smoothness),      // URP
    ("_BumpScale", Field::NormalScale),      // Standard, lilToon, MToon
    ("_Cutoff", Field::Cutoff),              // Standard, lilToon, Poiyomi, MToon
    ("_ShadeShift", Field::ShadeShift),      // MToon
    ("_ShadeToony", Field::ShadeToony),      // MToon
    ("_RimFresnelPower", Field::RimPower),   // MToon
    ("_MatCapBlend", Field::MatcapStrength), // lilToon
    ("_MainTex_ST", Field::MainTexSt),       // every family
];

impl Field {
    /// Colours and texture transforms have four components; the rest are scalars.
    pub fn is_vector(self) -> bool {
        matches!(
            self,
            Field::BaseColor
                | Field::Emission
                | Field::ShadeColor
                | Field::RimColor
                | Field::OutlineColor
                | Field::MainTexSt
        )
    }
}

/// The field and component a curve's property name drives: `_Color.r`, `_MainTex_ST.z` or `_Metallic`. Vector
/// fields take a component suffix (`r g b a` or `x y z w`), scalars none.
pub fn parse(name: &str) -> Option<(Field, u8)> {
    let (base, component) = match name.rsplit_once('.') {
        Some((base, suffix)) => {
            let c = match suffix {
                "r" | "x" => 0,
                "g" | "y" => 1,
                "b" | "z" => 2,
                "a" | "w" => 3,
                _ => return None,
            };
            (base, Some(c))
        }
        None => (name, None),
    };
    let field = NAMES.iter().find(|(n, _)| *n == base)?.1;
    match (field.is_vector(), component) {
        (true, Some(c)) => Some((field, c)),
        (false, None) => Some((field, 0)),
        _ => None,
    }
}

/// The field's value in Unity's units, or `None` when the material has no such field.
pub fn get(m: &Material, field: Field) -> Option<[f32; 4]> {
    place(&mut m.clone(), field).map(|p| p.read())
}

/// Sets one component of the field from a value in Unity's units; other components are left as they are. Returns
/// whether the material has the field. Writing alpha to an RGB colour does nothing.
pub fn set(m: &mut Material, field: Field, component: u8, value: f32) -> bool {
    match place(m, field) {
        Some(p) => {
            p.write(component as usize, value);
            true
        }
        None => false,
    }
}

/// Where a field lives in a material and how its value converts.
enum Place<'a> {
    /// Linear RGB, optionally alpha.
    Color(&'a mut [f32]),
    Scalar(&'a mut f32),
    /// Stored as one minus the value (roughness for smoothness).
    OneMinus(&'a mut f32),
    /// Scale-offset with V down.
    St(&'a mut [f32; 4]),
}

fn place(m: &mut Material, field: Field) -> Option<Place<'_>> {
    use MaterialModel::*;
    if field == Field::Cutoff {
        return match &mut m.alpha {
            AlphaMode::Mask { cutoff } => Some(Place::Scalar(cutoff)),
            _ => None,
        };
    }
    Some(match (field, &mut m.model) {
        (Field::BaseColor, Pbr(p)) => Place::Color(&mut p.base_color),
        (Field::BaseColor, Toon(t)) => Place::Color(&mut t.base_color),
        (Field::BaseColor, Unlit(u)) => Place::Color(&mut u.color),
        (Field::Emission, Pbr(p)) => Place::Color(&mut p.emissive),
        (Field::Emission, Toon(t)) => Place::Color(&mut t.emissive),
        (Field::ShadeColor, Toon(t)) => Place::Color(&mut t.shade_color),
        (Field::RimColor, Toon(t)) => Place::Color(&mut t.rim_color),
        (Field::OutlineColor, Toon(t)) => Place::Color(&mut t.outline_color),
        (Field::Metallic, Pbr(p)) => Place::Scalar(&mut p.metallic),
        (Field::Smoothness, Pbr(p)) => Place::OneMinus(&mut p.roughness),
        (Field::NormalScale, Pbr(p)) => Place::Scalar(&mut p.normal_scale),
        (Field::NormalScale, Toon(t)) => Place::Scalar(&mut t.normal_scale),
        (Field::ShadeShift, Toon(t)) => Place::Scalar(&mut t.shade_shift),
        (Field::ShadeToony, Toon(t)) => Place::Scalar(&mut t.shade_toony),
        (Field::RimPower, Toon(t)) => Place::Scalar(&mut t.rim_power),
        (Field::MatcapStrength, Toon(t)) => Place::Scalar(&mut t.matcap_strength),
        (Field::MainTexSt, Pbr(p)) => Place::St(&mut p.base_color_tex.as_mut()?.scale_offset),
        (Field::MainTexSt, Toon(t)) => Place::St(&mut t.base_color_tex.as_mut()?.scale_offset),
        (Field::MainTexSt, Unlit(u)) => Place::St(&mut u.color_tex.as_mut()?.scale_offset),
        _ => return None,
    })
}

impl Place<'_> {
    fn read(&self) -> [f32; 4] {
        match self {
            Place::Color(c) => [
                to_srgb(c[0]),
                to_srgb(c[1]),
                to_srgb(c[2]),
                c.get(3).copied().unwrap_or(1.0),
            ],
            Place::Scalar(s) => [**s, 0.0, 0.0, 0.0],
            Place::OneMinus(s) => [1.0 - **s, 0.0, 0.0, 0.0],
            // Unity's offset is from the bottom: v' = 1 - ((1 - v) * s + o) = v * s + (1 - s - o).
            Place::St(st) => [st[0], st[1], st[2], 1.0 - st[1] - st[3]],
        }
    }

    fn write(self, component: usize, v: f32) {
        match self {
            Place::Color(c) if component < 3 => c[component] = to_linear(v),
            Place::Color(c) => {
                if let Some(a) = c.get_mut(3) {
                    *a = v;
                }
            }
            Place::Scalar(s) => *s = v,
            Place::OneMinus(s) => *s = 1.0 - v,
            Place::St(st) => match component {
                // Keep Unity's V offset while the V scale changes.
                1 => {
                    let offset = 1.0 - st[1] - st[3];
                    st[1] = v;
                    st[3] = 1.0 - v - offset;
                }
                3 => st[3] = 1.0 - st[1] - v,
                c => st[c] = v,
            },
        }
    }
}

/// sRGB-encoded to linear, the conversion Unity applies to colour properties in a linear-space project. Values above
/// 1 (HDR) follow the same curve.
pub fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear to sRGB-encoded, the inverse of [`to_linear`].
pub fn to_srgb(l: f32) -> f32 {
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}
