//! VRM MToon materials (0.x `materialProperties`, 1.0 `VRMC_materials_mtoon`) to the Toon model.

use civbb_format::material::{
    AlphaMode, ColorSpace, MatcapBlend, Material, MaterialModel, TextureSlot, Toon, ToonLight,
};
use serde_json::Value;

use super::{arr, index, num};
use crate::gltf_import::Importer;

impl Importer<'_> {
    fn mtoon_slot(&mut self, v: &Value, space: ColorSpace, scale_offset: Option<[f32; 4]>) -> Option<TextureSlot> {
        let doc = self.doc;
        let i = index(if v.is_object() { &v["index"] } else { v })?;
        let Some(texture) = doc.textures().nth(i) else {
            self.warnings
                .push(format!("MToon texture {i} does not exist; slot dropped"));
            return None;
        };
        let tex_coord = index(&v["texCoord"]).unwrap_or(0) as u32;
        self.slot(&texture, tex_coord, scale_offset, space)
    }

    /// Replaces MToon materials with the Toon model; other materials keep their glTF import.
    pub(super) fn mtoon(&mut self, root: &Value, v1: bool) {
        let doc = self.doc;
        for (i, m) in doc.materials().enumerate() {
            let toon = if v1 {
                match m.extension_value("VRMC_materials_mtoon") {
                    Some(x) => self.mtoon_v1(i, x),
                    None => continue,
                }
            } else {
                let p = &root["materialProperties"][i];
                if p["shader"].as_str() != Some("VRM/MToon") {
                    continue;
                }
                self.mtoon_v0(i, p)
            };
            self.materials[i] = toon;
        }
    }

    fn mtoon_v1(&mut self, i: usize, x: &Value) -> Material {
        let base = self.materials[i].clone();
        let (base_color, base_color_tex, normal_tex, normal_scale, emissive, emissive_tex) = match &base.model {
            MaterialModel::Pbr(p) => (
                p.base_color,
                p.base_color_tex,
                p.normal_tex,
                p.normal_scale,
                p.emissive,
                p.emissive_tex,
            ),
            MaterialModel::Unlit(u) => (u.color, u.color_tex, None, 1.0, [0.0; 3], None),
            MaterialModel::Toon(t) => (t.base_color, t.base_color_tex, None, 1.0, [0.0; 3], None),
        };
        let matcap_tex = self.mtoon_slot(&x["matcapTexture"], ColorSpace::Srgb, None);
        let outline_width = match x["outlineWidthMode"].as_str() {
            Some("worldCoordinates") => num(&x["outlineWidthFactor"], 0.0),
            Some("screenCoordinates") => {
                self.warnings
                    .push(format!("material {i}: screen-space MToon outline dropped"));
                0.0
            }
            _ => 0.0,
        };
        let uv_anim = [
            "uvAnimationScrollXSpeedFactor",
            "uvAnimationScrollYSpeedFactor",
            "uvAnimationRotationSpeedFactor",
        ];
        if uv_anim.iter().any(|k| num(&x[*k], 0.0) != 0.0) {
            self.warnings.push(format!("material {i}: MToon UV animation dropped"));
        }
        let offset = x["renderQueueOffsetNumber"].as_i64().unwrap_or(0).clamp(-9, 9) as i32;
        Material {
            model: MaterialModel::Toon(Toon {
                base_color,
                base_color_tex,
                normal_tex,
                normal_scale,
                shade_color: arr(&x["shadeColorFactor"], [0.0; 3]),
                shade_tex: self.mtoon_slot(&x["shadeMultiplyTexture"], ColorSpace::Srgb, None),
                shade_shift: num(&x["shadingShiftFactor"], 0.0),
                shade_toony: num(&x["shadingToonyFactor"], 0.9),
                rim_color: arr(&x["parametricRimColorFactor"], [0.0; 3]),
                rim_power: num(&x["parametricRimFresnelPowerFactor"], 5.0),
                rim_shade: None,
                rim_ramp: None,
                shade_bands: None,
                tone: None,
                matcap_strength: if matcap_tex.is_some() {
                    arr(&x["matcapFactor"], [1.0; 3]).into_iter().fold(0.0, f32::max)
                } else {
                    0.0
                },
                matcap_tex,
                matcap_blend: MatcapBlend::Add,
                matcap_color: [1.0; 3],
                matcap_mask_tex: None,
                matcap_emission: 0.0,
                second_matcap: None,
                emissive,
                emissive_tex,
                emissive_main_strength: 0.0,
                outline_width,
                outline_color: arr(&x["outlineColorFactor"], [0.0; 3]),
                outline_width_tex: self.mtoon_slot(&x["outlineWidthMultiplyTexture"], ColorSpace::Linear, None),
                outline_tex: None,
                light: ToonLight::Additive,
                light_min: 0.0,
                light_max: 1.0,
            }),
            render_queue: base.render_queue + offset,
            ..base
        }
    }

    fn mtoon_v0(&mut self, i: usize, p: &Value) -> Material {
        let (f, v, t) = (&p["floatProperties"], &p["vectorProperties"], &p["textureProperties"]);
        // UniVRM 0.x stores texture tiling as [offset x, offset y, scale x, scale y].
        let main = arr(&v["_MainTex"], [0.0, 0.0, 1.0, 1.0]);
        let main_so = [main[2], main[3], main[0], main[1]];
        let base_color_tex = self.mtoon_slot(&t["_MainTex"], ColorSpace::Srgb, Some(main_so));
        let matcap_tex = self.mtoon_slot(&t["_SphereAdd"], ColorSpace::Srgb, None);
        let outline_width = match num(&f["_OutlineWidthMode"], 0.0) as i32 {
            // MToon 0.x world outline width is in centimetres.
            1 => num(&f["_OutlineWidth"], 0.0) * 0.01,
            2 => {
                self.warnings
                    .push(format!("material {i}: screen-space MToon outline dropped"));
                0.0
            }
            _ => 0.0,
        };
        let (alpha, default_queue) = match num(&f["_BlendMode"], 0.0) as i32 {
            1 => (
                AlphaMode::Mask {
                    cutoff: num(&f["_Cutoff"], 0.5),
                },
                2450,
            ),
            2 | 3 => (AlphaMode::Blend, 3000),
            _ => (AlphaMode::Opaque, 2000),
        };
        let color = |key: &str, default: [f32; 4]| srgb(arr(&v[key], default));
        let rgb = |c: [f32; 4]| [c[0], c[1], c[2]];
        Material {
            name: self.materials[i].name.clone(),
            model: MaterialModel::Toon(Toon {
                base_color: color("_Color", [1.0; 4]),
                base_color_tex,
                normal_tex: self.mtoon_slot(&t["_BumpMap"], ColorSpace::Linear, None),
                normal_scale: num(&f["_BumpScale"], 1.0),
                shade_color: rgb(color("_ShadeColor", [0.97, 0.81, 0.86, 1.0])),
                shade_tex: self.mtoon_slot(&t["_ShadeTexture"], ColorSpace::Srgb, None),
                shade_shift: num(&f["_ShadeShift"], 0.0),
                shade_toony: num(&f["_ShadeToony"], 0.9),
                rim_color: rgb(color("_RimColor", [0.0, 0.0, 0.0, 1.0])),
                rim_power: num(&f["_RimFresnelPower"], 1.0),
                rim_shade: None,
                rim_ramp: None,
                shade_bands: None,
                tone: None,
                matcap_strength: if matcap_tex.is_some() { 1.0 } else { 0.0 },
                matcap_blend: MatcapBlend::Add,
                matcap_color: [1.0; 3],
                matcap_mask_tex: None,
                matcap_emission: 0.0,
                second_matcap: None,
                matcap_tex,
                emissive: rgb(color("_EmissionColor", [0.0, 0.0, 0.0, 1.0])),
                emissive_tex: self.mtoon_slot(&t["_EmissionMap"], ColorSpace::Srgb, None),
                emissive_main_strength: 0.0,
                outline_width,
                outline_color: rgb(color("_OutlineColor", [0.0, 0.0, 0.0, 1.0])),
                outline_width_tex: self.mtoon_slot(&t["_OutlineWidthTexture"], ColorSpace::Linear, None),
                outline_tex: None,
                light: ToonLight::Additive,
                light_min: 0.0,
                light_max: 1.0,
            }),
            alpha,
            double_sided: num(&f["_CullMode"], 2.0) as i32 == 0,
            render_queue: p["renderQueue"]
                .as_i64()
                .map_or(default_queue, |q| q.clamp(0, 5000) as i32),
        }
    }
}

/// sRGB colour to linear; alpha stays as is.
fn srgb(c: [f32; 4]) -> [f32; 4] {
    let f = |x: f32| {
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    [f(c[0]), f(c[1]), f(c[2]), c[3]]
}
