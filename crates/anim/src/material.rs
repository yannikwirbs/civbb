//! Animated material fields (ADRs 0025, 0099): a [`MaterialField`] of a material read and written in the units the
//! material stores it, and the sRGB transfer for colour curves that declare `srgb`.

use civbb_format::material::{AlphaMode, Material, MaterialField, MaterialModel};

/// The field's components as stored (unused ones 0), or `None` when the material has no such field (a toon
/// material lacks the fields of its absent features, ADR 0090).
pub fn get(m: &Material, field: MaterialField) -> Option<[f32; 4]> {
    let mut out = [0.0; 4];
    let mut m = m.clone();
    let v = place(&mut m, field)?;
    out[..v.len()].copy_from_slice(v);
    Some(out)
}

/// Sets one component of the field; the others are left as they are. Returns whether the material has the field.
pub fn set(m: &mut Material, field: MaterialField, component: u8, value: f32) -> bool {
    match place(m, field) {
        Some(v) => {
            if let Some(c) = v.get_mut(component as usize) {
                *c = value;
            }
            true
        }
        None => false,
    }
}

/// Where a field lives in a material.
fn place(m: &mut Material, field: MaterialField) -> Option<&mut [f32]> {
    use MaterialField as F;
    use MaterialModel::*;
    if field == F::Cutoff {
        return match &mut m.alpha {
            AlphaMode::Mask { cutoff } => Some(std::slice::from_mut(cutoff)),
            _ => None,
        };
    }
    let one = std::slice::from_mut;
    Some(match (field, &mut m.model) {
        (F::BaseColor, Pbr(p)) => &mut p.base_color,
        (F::BaseColor, Toon(t)) => &mut t.base_color,
        (F::BaseColor, Unlit(u)) => &mut u.color,
        (F::Emission, Pbr(p)) => &mut p.emissive,
        (F::Emission, Toon(t)) => &mut t.emission.as_mut()?.color,
        (F::ShadeColor, Toon(t)) => &mut t.shade_color,
        (F::RimColor, Toon(t)) => &mut t.rim.as_mut()?.color,
        (F::OutlineColor, Toon(t)) => &mut t.outline.as_mut()?.color,
        (F::Metallic, Pbr(p)) => one(&mut p.metallic),
        (F::Roughness, Pbr(p)) => one(&mut p.roughness),
        (F::NormalScale, Pbr(p)) => one(&mut p.normal_scale),
        (F::NormalScale, Toon(t)) => one(&mut t.normal.as_mut()?.scale),
        (F::ShadeShift, Toon(t)) => one(&mut t.shade_shift),
        (F::ShadeToony, Toon(t)) => one(&mut t.shade_toony),
        (F::RimPower, Toon(t)) => one(&mut t.rim.as_mut()?.power),
        (F::MatcapStrength, Toon(t)) => one(&mut t.matcaps.first_mut()?.strength),
        (F::LightMin, Toon(t)) => one(&mut t.light_clamp.as_mut()?.min),
        (F::LightMax, Toon(t)) => one(&mut t.light_clamp.as_mut()?.max),
        (F::BaseColorTransform, Pbr(p)) => &mut p.base_color_tex.as_mut()?.scale_offset,
        (F::BaseColorTransform, Toon(t)) => &mut t.base_color_tex.as_mut()?.scale_offset,
        (F::BaseColorTransform, Unlit(u)) => &mut u.color_tex.as_mut()?.scale_offset,
        _ => return None,
    })
}

/// sRGB-encoded to linear, the conversion Unity applies to colour properties in a linear-space project
/// (`Mathf.GammaToLinearSpace`): values above 1 (HDR) are raised to 2.2, measured in Unity 2022.3.
pub fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else if c < 1.0 {
        ((c + 0.055) / 1.055).powf(2.4)
    } else {
        c.powf(2.2)
    }
}

/// Linear to sRGB-encoded, the inverse of [`to_linear`].
pub fn to_srgb(l: f32) -> f32 {
    if l <= 0.003_130_8 {
        l * 12.92
    } else if l < 1.0 {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    } else {
        l.powf(1.0 / 2.2)
    }
}
