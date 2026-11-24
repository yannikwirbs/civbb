//! Materials as the uber-shaders read them: one fixed-size record per material in the material buffer, and the
//! pipeline each material draws with (design doc §6, ADR 0023).

use civbb_format::material::{AlphaMode, MatcapBlend, Material, MaterialModel, TextureSlot, ToonLight};

/// 32-bit words per material record (`Material` in `mesh.slang`).
pub const MATERIAL_WORDS: usize = 140;
/// A texture slot without a texture.
pub const NONE: u32 = u32::MAX;

/// The fragment shader a material uses: one per native material model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Shader {
    /// Metallic-roughness PBR.
    Pbr,
    /// Toon shading.
    Toon,
    /// Unlit colour or texture.
    Unlit,
    /// A toon outline: the mesh's back faces pushed out along the normal, in the outline colour (ADR 0034).
    Outline,
}

/// What separates one pipeline from another: the shader, alpha blending (blended draws don't write depth) and
/// back-face culling. Alpha cutoff is a branch in the shader, not a pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PipelineKey {
    /// The fragment shader.
    pub shader: Shader,
    /// Alpha blended.
    pub blend: bool,
    /// Both faces drawn.
    pub double_sided: bool,
}

impl PipelineKey {
    /// Number of distinct keys, and so of pipelines.
    pub const COUNT: usize = 16;

    /// The key a material draws with.
    pub fn of(m: &Material) -> Self {
        PipelineKey {
            shader: match m.model {
                MaterialModel::Pbr(_) => Shader::Pbr,
                MaterialModel::Toon(_) => Shader::Toon,
                MaterialModel::Unlit(_) => Shader::Unlit,
            },
            blend: m.alpha == AlphaMode::Blend,
            double_sided: m.double_sided,
        }
    }

    /// The key of a material's outline draw: toon materials with an outline width; blended like the material.
    pub fn outline(m: &Material) -> Option<Self> {
        match &m.model {
            MaterialModel::Toon(t) if t.outline_width > 0.0 => Some(PipelineKey {
                shader: Shader::Outline,
                blend: m.alpha == AlphaMode::Blend,
                double_sided: false,
            }),
            _ => None,
        }
    }

    /// The key's index, `0 .. COUNT`.
    pub fn index(self) -> usize {
        self.shader as usize * 4 + usize::from(self.blend) * 2 + usize::from(self.double_sided)
    }

    /// Every key, in index order.
    pub fn all() -> impl Iterator<Item = PipelineKey> {
        [Shader::Pbr, Shader::Toon, Shader::Unlit, Shader::Outline]
            .into_iter()
            .flat_map(|shader| {
                [(false, false), (false, true), (true, false), (true, true)].map(|(blend, double_sided)| PipelineKey {
                    shader,
                    blend,
                    double_sided,
                })
            })
    }
}

/// Packs a material into its record. `textures` maps the avatar's texture indices to bindless indices; a slot
/// whose texture is out of range gets none.
pub fn pack(m: &Material, textures: &[u32]) -> [u32; MATERIAL_WORDS] {
    let base_color;
    let mut emissive = [0.0; 3];
    // Toon shadow colour, and 1 in w for the clamped light model.
    let mut shade = [0.0; 4];
    // Toon rim colour; the matcap blend in w (0 add, 1 multiply, 2 replace).
    let mut rim = [0.0; 4];
    // Metallic, roughness, normal scale, alpha cutoff.
    let mut params = [0.0, 1.0, 1.0, 0.0];
    // Shade shift, shade toony, rim power, matcap strength.
    let mut toon = [0.0; 4];
    // Base, normal, metallic-roughness or shade, occlusion, emissive, matcap, outline width, matcap mask, outline.
    let mut slots: [Option<TextureSlot>; 9] = [None; 9];
    // Outline colour and width.
    let mut outline = [0.0; 4];
    // Rim shade colour and strength (0: off); its ramp's start and end and its fresnel power.
    let mut rim_shade = [0.0; 4];
    let mut rim_shade_ramp = [0.0, 1.0, 1.0, 0.0];
    // Matcap tint; its emission weight in w.
    let mut matcap = [1.0, 1.0, 1.0, 0.0];
    // The clamped light's bounds.
    let mut light = [0.0, 1.0, 0.0, 0.0];
    // Shadow strength, main strength, and 1 to keep the shadow below the lit colour (lilToon's shade bands).
    let mut shade_mix = [1.0, 0.0, 0.0, 0.0];
    // Second shadow colour and strength.
    let mut shade_second = [0.0; 4];
    // The second shadow's ramp start and end, the border colour's ramp start and end (half-Lambert).
    let mut shade_ramps = [0.0, 1.0, 0.0, 1.0];
    // The shadow border colour; the emission's main strength in w.
    let mut shade_border = [0.0; 4];
    // Rim ramp start and end, shadow mask, main strength.
    let mut rim_ramp = [0.0, 1.0, 0.0, 0.0];
    // The base texture's tone correction: hue shift, saturation, value, gamma.
    let mut tone = [0.0, 1.0, 1.0, 1.0];
    match &m.model {
        MaterialModel::Pbr(p) => {
            base_color = p.base_color;
            emissive = p.emissive;
            params[..3].copy_from_slice(&[p.metallic, p.roughness, p.normal_scale]);
            slots = [
                p.base_color_tex,
                p.normal_tex,
                p.metallic_roughness_tex,
                p.occlusion_tex,
                p.emissive_tex,
                None,
                None,
                None,
                None,
            ];
        }
        MaterialModel::Toon(t) => {
            base_color = t.base_color;
            emissive = t.emissive;
            let clamped = if t.light == ToonLight::Clamped { 1.0 } else { 0.0 };
            shade = [t.shade_color[0], t.shade_color[1], t.shade_color[2], clamped];
            let blend = match t.matcap_blend {
                MatcapBlend::Add => 0.0,
                MatcapBlend::Multiply => 1.0,
                MatcapBlend::Replace => 2.0,
            };
            rim = [t.rim_color[0], t.rim_color[1], t.rim_color[2], blend];
            params[2] = t.normal_scale;
            if let Some(r) = t.rim_shade {
                rim_shade = [r.color[0], r.color[1], r.color[2], r.strength];
                let (lo, hi) = ramp(r.border, r.blur);
                rim_shade_ramp = [lo, hi, r.power, 0.0];
            }
            toon = [t.shade_shift, t.shade_toony, t.rim_power, t.matcap_strength];
            if let Some(b) = t.shade_bands {
                // The first ramp's start and end in half-Lambert terms, from MToon's shift and toony.
                let lo = (t.shade_toony - t.shade_shift) * 0.5;
                let hi = lo + 1.0 - t.shade_toony;
                let (lo2, hi2) = ramp(b.second_border, b.second_blur);
                shade_mix = [b.strength, b.main_strength, 1.0, 0.0];
                shade_second = [
                    b.second_color[0],
                    b.second_color[1],
                    b.second_color[2],
                    b.second_strength,
                ];
                shade_ramps = [lo2, hi2, (lo - b.border_range).max(0.0), hi];
                shade_border = [b.border_color[0], b.border_color[1], b.border_color[2], 0.0];
            }
            shade_border[3] = t.emissive_main_strength;
            tone = t.tone.unwrap_or(tone);
            if let Some(r) = t.rim_ramp {
                let (lo, hi) = ramp(r.border, r.blur);
                rim_ramp = [lo, hi, r.shadow_mask, r.main_strength];
            }
            outline = [
                t.outline_color[0],
                t.outline_color[1],
                t.outline_color[2],
                t.outline_width,
            ];
            slots = [
                t.base_color_tex,
                t.normal_tex,
                t.shade_tex,
                None,
                t.emissive_tex,
                t.matcap_tex,
                t.outline_width_tex,
                t.matcap_mask_tex,
                t.outline_tex,
            ];
            matcap = [
                t.matcap_color[0],
                t.matcap_color[1],
                t.matcap_color[2],
                t.matcap_emission,
            ];
            light = [t.light_min, t.light_max, 0.0, 0.0];
        }
        MaterialModel::Unlit(u) => {
            base_color = u.color;
            slots[0] = u.color_tex;
        }
    }
    if let AlphaMode::Mask { cutoff } = m.alpha {
        params[3] = cutoff;
    }
    let mut words = Vec::with_capacity(MATERIAL_WORDS);
    let mut vec4 = |v: [f32; 4]| words.extend(v.map(f32::to_bits));
    let xyz = |v: [f32; 3]| [v[0], v[1], v[2], 0.0];
    vec4(base_color);
    vec4(xyz(emissive));
    vec4(shade);
    vec4(rim);
    vec4(params);
    vec4(toon);
    let mut slot_words = Vec::with_capacity(72);
    for slot in slots {
        let (st, tex) = match slot.and_then(|s| Some((s, *textures.get(s.texture as usize)?))) {
            Some((s, index)) => (s.scale_offset, [index, u32::from(s.uv_set), 0, 0]),
            None => ([1.0, 1.0, 0.0, 0.0], [NONE, 0, 0, 0]),
        };
        slot_words.extend(st.map(f32::to_bits));
        slot_words.extend(tex);
    }
    // After the six slots so their offsets stay: the outline, then its width slot.
    words.extend(&slot_words[..48]);
    words.extend(outline.map(f32::to_bits));
    words.extend(&slot_words[48..56]);
    words.extend(rim_shade.map(f32::to_bits));
    words.extend(rim_shade_ramp.map(f32::to_bits));
    words.extend(matcap.map(f32::to_bits));
    words.extend(&slot_words[56..64]);
    words.extend(light.map(f32::to_bits));
    for v in [shade_mix, shade_second, shade_ramps, shade_border, rim_ramp, tone] {
        words.extend(v.map(f32::to_bits));
    }
    words.extend(&slot_words[64..]);
    words.try_into().expect("a material record is MATERIAL_WORDS long")
}

/// lilToon's ramp from `border - blur / 2` to `border + blur / 2`, both clamped to 0 to 1.
fn ramp(border: f32, blur: f32) -> (f32, f32) {
    (
        (border - blur * 0.5).clamp(0.0, 1.0),
        (border + blur * 0.5).clamp(0.0, 1.0),
    )
}
