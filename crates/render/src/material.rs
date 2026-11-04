//! Materials as the uber-shaders read them: one fixed-size record per material in the material buffer, and the
//! pipeline each material draws with (design doc §6, ADR 0023).

use civbb_format::material::{AlphaMode, Material, MaterialModel, TextureSlot};

/// 32-bit words per material record (`Material` in `mesh.slang`).
pub const MATERIAL_WORDS: usize = 84;
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
    let mut shade = [0.0; 3];
    let mut rim = [0.0; 3];
    // Metallic, roughness, normal scale, alpha cutoff.
    let mut params = [0.0, 1.0, 1.0, 0.0];
    // Shade shift, shade toony, rim power, matcap strength.
    let mut toon = [0.0; 4];
    // Base, normal, metallic-roughness or shade, occlusion, emissive, matcap, outline width.
    let mut slots: [Option<TextureSlot>; 7] = [None; 7];
    // Outline colour and width.
    let mut outline = [0.0; 4];
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
            ];
        }
        MaterialModel::Toon(t) => {
            base_color = t.base_color;
            emissive = t.emissive;
            shade = t.shade_color;
            rim = t.rim_color;
            params[2] = t.normal_scale;
            toon = [t.shade_shift, t.shade_toony, t.rim_power, t.matcap_strength];
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
            ];
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
    vec4(xyz(shade));
    vec4(xyz(rim));
    vec4(params);
    vec4(toon);
    let mut slot_words = Vec::with_capacity(56);
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
    words.extend(&slot_words[48..]);
    words.try_into().expect("a material record is MATERIAL_WORDS long")
}
