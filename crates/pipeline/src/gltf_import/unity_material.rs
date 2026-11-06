//! Unity materials from the source package's `X_toon_material` (ADR 0043): the shader family fingerprinted by shader
//! GUID, name or marker properties, then mapped by that family's table to the native Toon, PBR or Unlit model.
//!
//! The extension holds a material as Unity saved it (colours sRGB-encoded, tiling V-up), so shaders missing from the
//! exporter's project still map. Mappings are approximations of each family's look; parity scores measure them.

use std::collections::BTreeMap;

use civbb_anim::material::to_linear;
use civbb_format::material::{AlphaMode, ColorSpace, Material, MaterialModel, Pbr, TextureSlot, Toon, Unlit};
use civbb_format::math::Vec3;
use serde::Deserialize;

use super::Importer;

/// One material's `X_toon_material`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct XToonMaterial {
    /// Shader name; `Hidden/InternalErrorShader` when the shader was missing from the exporter's project.
    pub shader: String,
    /// The shader asset's GUID, also for missing shaders; empty for built-in ones.
    #[serde(default)]
    pub shader_guid: String,
    /// Custom render queue, or -1 for the shader's own.
    #[serde(default = "minus_one")]
    pub render_queue: i32,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub floats: BTreeMap<String, f32>,
    #[serde(default)]
    pub ints: BTreeMap<String, i32>,
    #[serde(default)]
    pub colors: BTreeMap<String, [f32; 4]>,
    #[serde(default)]
    pub textures: BTreeMap<String, XTexture>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct XTexture {
    /// glTF texture index.
    pub index: Option<u32>,
    /// Unity's `[scale x, scale y, offset x, offset y]`, V up.
    pub scale_offset: [f32; 4],
}

fn minus_one() -> i32 {
    -1
}

/// Shader families with a mapping table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// lilToon; the variant decides transparency and outline when the GUID or name gives it.
    LilToon(Option<LilVariant>),
    Poiyomi,
    Uts2,
    MToon,
    Standard,
    /// VRChat Mobile Toon Lit and Toon Standard.
    MobileToon,
    /// VRChat Mobile MatCap Lit.
    MobileMatcap,
    /// Unlit shaders and VRChat Mobile particles.
    Unlit,
    /// Anything else: a plain toon from the base colour.
    Unknown,
}

/// What a lilToon shader variant fixes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LilVariant {
    /// Opaque, cutout or transparent; `None` for the multi shader, which reads `_TransparentMode`.
    pub alpha: Option<Alpha>,
    /// Draws an outline pass.
    pub outline: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alpha {
    Opaque,
    Cutout,
    Transparent,
}

/// lilToon shader GUIDs (from its `.shader.meta` files, lilToon is MIT-licensed) with their file names, which encode
/// the variant: `cutout`, `trans`/`onetrans`/`twotrans`/`overlay`/`ref`/`gem` and an `o`/`oo` outline suffix.
const LILTOON: &[(&str, &str)] = &[
    ("df12117ecd77c31469c224178886498e", "lts"),
    ("85d6126cae43b6847aff4b13f4adb8ec", "lts_cutout"),
    ("3b4aa19949601f046a20ca8bdaee929f", "lts_cutout_o"),
    ("3b3957e6c393b114bab6f835b4ed8f5d", "lts_cutout_oo"),
    ("00795bf598b44dc4e9bd363348e77085", "lts_fakeshadow"),
    ("a8d94439709469942bc7dcc9156ba110", "lts_gem"),
    ("efa77a80ca0344749b4f19fdd5891cbe", "lts_o"),
    ("b269573b9937b8340b3e9e191a3ba5a8", "lts_onetrans"),
    ("7171688840c632447b22ec14e2bdef7e", "lts_onetrans_o"),
    ("fba17785d6b2c594ab6c0303c834da65", "lts_oo"),
    ("94274b8ef5d3af842b9427384cba3a8f", "lts_overlay"),
    ("33e950d038b8dfd4f824f3985c2abfb7", "lts_overlay_one"),
    ("dce3f3e248acc7b4daeda00daf616b4d", "lts_ref"),
    ("3fb94a39b2685ee4d9817dcaf6542d99", "lts_ref_blur"),
    ("3eef4aee6ba0de047b0d40409ea2891c", "lts_tess"),
    ("bbfffd5515b843c41a85067191cbf687", "lts_tess_cutout"),
    ("5ba517885727277409feada18effa4a6", "lts_tess_cutout_o"),
    ("c6d605ee23b18fc46903f38c67db701f", "lts_tess_o"),
    ("90f83c35b0769a748abba5d0880f36d5", "lts_tess_onetrans"),
    ("67ed0252d63362a4ab23707a720508b7", "lts_tess_onetrans_o"),
    ("afa1a194f5a2fd243bda3a17bca1b36e", "lts_tess_trans"),
    ("9b0c2630b12933248922527d4507cfa9", "lts_tess_trans_o"),
    ("7e398ea50f9b70045b1774e05b46a39f", "lts_tess_twotrans"),
    ("7e61dbad981ad4f43a03722155db1c6a", "lts_tess_twotrans_o"),
    ("165365ab7100a044ca85fc8c33548a62", "lts_trans"),
    ("3c79b10c7e0b2784aaa4c2f8dd17d55e", "lts_trans_o"),
    ("0c762f24b85918a49812fc5690619178", "lts_trans_oo"),
    ("6a77405f7dfdc1447af58854c7f43f39", "lts_twotrans"),
    ("9cf054060007d784394b8b0bb703e441", "lts_twotrans_o"),
    ("381af8ba8e1740a41b9768ccfb0416c2", "ltsl"),
    ("b957dce3d03ff5445ac989f8de643c7f", "ltsl_cutout"),
    ("8cf5267d397b04846856f6d3d9561da0", "ltsl_cutout_o"),
    ("583a88005abb81a4ebbce757b4851a0d", "ltsl_o"),
    ("34c2907eba944ed45a43970e0c11bcfd", "ltsl_onetrans"),
    ("701268c07d37f5441b25b2cb99fae4b3", "ltsl_onetrans_o"),
    ("d28e4b78ba8368e49a44f86c0291df58", "ltsl_overlay"),
    ("dc9ded9f9d6f16c4e92cbb8f4269ae31", "ltsl_overlay_one"),
    ("0e3ece1bd59542743bccadb21f68318e", "ltsl_trans"),
    ("1c12a37046f07ac4486881deaf0187ea", "ltsl_trans_o"),
    ("82226adb1a0b8c4418f574cfdcf523da", "ltsl_twotrans"),
    ("62df797f407281640a224388953448cc", "ltsl_twotrans_o"),
    ("9294844b15dca184d914a632279b24e1", "ltsmulti"),
    ("69f861c14129e724096c0955f8079012", "ltsmulti_gem"),
    ("51b2dee0ab07bd84d8147601ff89e511", "ltsmulti_o"),
    ("d7af54cdd86902d41b8c240e06b93009", "ltsmulti_ref"),
];

/// A lilToon variant from its shader file name (`lts_trans_o`) or shader name (`Hidden/lilToonTransparentOutline`).
fn lil_variant(name: &str) -> LilVariant {
    let n = name.to_ascii_lowercase();
    let parts: Vec<&str> = n.split('_').collect();
    let outline = parts.last().is_some_and(|p| *p == "o" || *p == "oo") || n.contains("outline");
    let alpha = if n.contains("multi") {
        None
    } else if n.contains("cutout") {
        Some(Alpha::Cutout)
    } else if ["trans", "overlay", "ref", "gem"].iter().any(|k| n.contains(k)) {
        Some(Alpha::Transparent)
    } else {
        Some(Alpha::Opaque)
    };
    LilVariant { alpha, outline }
}

/// The family of a saved material: its shader GUID, then its shader name, then marker properties (a material keeps
/// the properties of shaders it used before, so a GUID or name wins over markers).
pub fn family(x: &XToonMaterial) -> Family {
    if let Some((_, file)) = LILTOON.iter().find(|(g, _)| *g == x.shader_guid) {
        return Family::LilToon(Some(lil_variant(file)));
    }
    let name = x.shader.to_ascii_lowercase();
    let missing = name.is_empty() || name == "hidden/internalerrorshader";
    if !missing {
        if name.starts_with("vrchat/mobile/matcap") {
            return Family::MobileMatcap;
        }
        if name.starts_with("vrchat/mobile/toon") {
            return Family::MobileToon;
        }
        if name.starts_with("vrchat/mobile/particles") || name.starts_with("unlit/") || name.ends_with("/unlit") {
            return Family::Unlit;
        }
        if name.contains("poiyomi") {
            return Family::Poiyomi;
        }
        if name.contains("liltoon") {
            return Family::LilToon(Some(lil_variant(&name)));
        }
        if name.contains("unitychantoonshader") || name.contains("toon_doubleshadewithfeather") {
            return Family::Uts2;
        }
        if name.contains("mtoon") {
            return Family::MToon;
        }
        if name == "standard" {
            return Family::Standard;
        }
        return Family::Unknown;
    }
    let has = |k: &str| x.floats.contains_key(k);
    if has("_lilToonVersion") {
        Family::LilToon(None)
    } else if has("shader_master_label") || has("_ShaderOptimizerEnabled") {
        Family::Poiyomi
    } else if has("_utsVersion") {
        Family::Uts2
    } else if has("_MToonVersion") {
        Family::MToon
    } else {
        Family::Unknown
    }
}

/// Saved properties with their defaults.
struct Props<'a>(&'a XToonMaterial);

impl Props<'_> {
    /// A float property; Unity 2021+ saves some as ints.
    fn f(&self, k: &str, default: f32) -> f32 {
        let int = || self.0.ints.get(k).map(|&i| i as f32);
        self.0.floats.get(k).copied().or_else(int).unwrap_or(default)
    }

    fn on(&self, k: &str) -> bool {
        self.f(k, 0.0) != 0.0
    }

    /// A colour in linear space; alpha stays as saved.
    fn color(&self, k: &str, default: [f32; 4]) -> [f32; 4] {
        let c = self.0.colors.get(k).copied().unwrap_or(default);
        [to_linear(c[0]), to_linear(c[1]), to_linear(c[2]), c[3]]
    }

    fn rgb(&self, k: &str, default: [f32; 4]) -> Vec3 {
        let c = self.color(k, default);
        [c[0], c[1], c[2]]
    }
}

const WHITE: [f32; 4] = [1.0; 4];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

fn mul(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}

fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn scale(a: Vec3, s: f32) -> Vec3 {
    a.map(|x| x * s)
}

/// Unity's V-up `_ST` as the format's V-down scale and offset: `v' = 1 - ((1 - v) s + o) = v s + (1 - s - o)`.
fn st_to_gltf(st: [f32; 4]) -> [f32; 4] {
    [st[0], st[1], st[2], 1.0 - st[1] - st[3]]
}

/// A plain toon over a base colour: shadows a quarter darker, nothing else. The fallback where a family's own
/// shading has no counterpart.
fn plain_toon(base_color: [f32; 4], base_color_tex: Option<TextureSlot>) -> Toon {
    Toon {
        base_color,
        base_color_tex,
        normal_tex: None,
        normal_scale: 1.0,
        shade_color: scale([base_color[0], base_color[1], base_color[2]], 0.75),
        shade_tex: base_color_tex,
        shade_shift: 0.0,
        shade_toony: 0.9,
        rim_color: [0.0; 3],
        rim_power: 5.0,
        matcap_tex: None,
        matcap_strength: 0.0,
        emissive: [0.0; 3],
        emissive_tex: None,
        outline_width: 0.0,
        outline_color: [0.0; 3],
        outline_width_tex: None,
    }
}

impl Importer<'_> {
    /// Replaces every material that carries `X_toon_material` with its family's mapping.
    pub(super) fn unity_materials(&mut self) -> Result<(), String> {
        let doc = self.doc;
        for (i, m) in doc.materials().enumerate() {
            let Some(v) = m.extension_value("X_toon_material") else {
                continue;
            };
            let x = XToonMaterial::deserialize(v).map_err(|e| format!("X_toon_material: material {i}: {e}"))?;
            let name = self.materials[i].name.clone();
            self.materials[i] = Material {
                name,
                ..self.unity_material(&x)
            };
        }
        Ok(())
    }

    /// A texture property as a slot, or `None` when unset or unusable.
    fn unity_slot(&mut self, x: &XToonMaterial, key: &str, space: ColorSpace) -> Option<TextureSlot> {
        let t = x.textures.get(key)?;
        let index = t.index? as usize;
        let Some(texture) = self.doc.textures().nth(index) else {
            self.warnings.push(format!(
                "X_toon_material: {key} names texture {index}, which does not exist; slot dropped"
            ));
            return None;
        };
        self.slot(&texture, 0, Some(st_to_gltf(t.scale_offset)), space)
    }

    fn unity_material(&mut self, x: &XToonMaterial) -> Material {
        let p = Props(x);
        let family = family(x);
        let srgb = ColorSpace::Srgb;
        let main = self.unity_slot(x, "_MainTex", srgb);
        // Transparency from the render queue when the material sets one, else from the family.
        let queue_alpha = match x.render_queue {
            q if q >= 2501 => Some(Alpha::Transparent),
            q if q >= 2450 => Some(Alpha::Cutout),
            q if q >= 0 => Some(Alpha::Opaque),
            _ => None,
        };
        let mode_alpha = |mode: f32| match mode as i32 {
            0 => Alpha::Opaque,
            1 => Alpha::Cutout,
            _ => Alpha::Transparent,
        };
        let (model, alpha, cull) = match family {
            Family::LilToon(variant) => {
                let alpha = variant.and_then(|v| v.alpha).or(queue_alpha).unwrap_or_else(|| {
                    match p.f("_TransparentMode", 0.0) as i32 {
                        0 => Alpha::Opaque,
                        1 => Alpha::Cutout,
                        _ => Alpha::Transparent,
                    }
                });
                let outline = variant.is_some_and(|v| v.outline);
                if variant.is_none() {
                    self.warnings.push(format!(
                        "X_toon_material: lilToon shader {} unknown, outline left off",
                        x.shader_guid
                    ));
                }
                (
                    MaterialModel::Toon(self.liltoon(x, main, outline)),
                    alpha,
                    p.f("_Cull", 2.0),
                )
            }
            Family::Poiyomi => {
                let base = p.color("_Color", WHITE);
                let mut t = plain_toon(base, main);
                if let Some(normal) = self.unity_slot(x, "_BumpMap", ColorSpace::Linear) {
                    t.normal_tex = Some(normal);
                    t.normal_scale = p.f("_BumpScale", 1.0);
                }
                if p.on("_MatcapEnable") {
                    t.matcap_tex = self.unity_slot(x, "_Matcap", srgb);
                    t.matcap_strength = p.f("_MatcapIntensity", 1.0);
                }
                if p.on("_EnableRimLighting") {
                    t.rim_color = scale(p.rgb("_RimLightColor", WHITE), p.color("_RimLightColor", WHITE)[3]);
                }
                if p.on("_EnableEmission") {
                    t.emissive = scale(p.rgb("_EmissionColor", BLACK), p.f("_EmissionStrength", 1.0));
                    t.emissive_tex = self.unity_slot(x, "_EmissionMap", srgb);
                }
                let alpha = queue_alpha.unwrap_or_else(|| mode_alpha(p.f("_Mode", 0.0)));
                (MaterialModel::Toon(t), alpha, p.f("_Cull", 2.0))
            }
            Family::Uts2 => (
                MaterialModel::Toon(self.uts2(x, main)),
                queue_alpha.unwrap_or(Alpha::Opaque),
                p.f("_CullMode", 2.0),
            ),
            Family::MToon => {
                let base = p.color("_Color", WHITE);
                let mut t = plain_toon(base, main);
                t.shade_color = p.rgb("_ShadeColor", [0.97, 0.81, 0.86, 1.0]);
                t.shade_tex = self.unity_slot(x, "_ShadeTexture", srgb);
                t.shade_shift = p.f("_ShadeShift", 0.0);
                t.shade_toony = p.f("_ShadeToony", 0.9);
                t.normal_tex = self.unity_slot(x, "_BumpMap", ColorSpace::Linear);
                t.normal_scale = p.f("_BumpScale", 1.0);
                t.rim_color = p.rgb("_RimColor", BLACK);
                t.rim_power = p.f("_RimFresnelPower", 1.0);
                t.matcap_tex = self.unity_slot(x, "_SphereAdd", srgb);
                t.matcap_strength = if t.matcap_tex.is_some() { 1.0 } else { 0.0 };
                t.emissive = p.rgb("_EmissionColor", BLACK);
                t.emissive_tex = self.unity_slot(x, "_EmissionMap", srgb);
                if p.f("_OutlineWidthMode", 0.0) as i32 == 1 {
                    // World outline width in centimetres.
                    t.outline_width = p.f("_OutlineWidth", 0.0) * 0.01;
                    t.outline_color = p.rgb("_OutlineColor", BLACK);
                    t.outline_width_tex = self.unity_slot(x, "_OutlineWidthTexture", ColorSpace::Linear);
                }
                let alpha = queue_alpha.unwrap_or_else(|| mode_alpha(p.f("_BlendMode", 0.0)));
                (MaterialModel::Toon(t), alpha, p.f("_CullMode", 2.0))
            }
            Family::Standard => {
                let emission = x.keywords.iter().any(|k| k == "_EMISSION");
                if x.textures.get("_MetallicGlossMap").is_some_and(|t| t.index.is_some()) {
                    self.warnings
                        .push("X_toon_material: Standard metallic-gloss map (R metal, A smooth) dropped".into());
                }
                let pbr = Pbr {
                    base_color: p.color("_Color", WHITE),
                    base_color_tex: main,
                    metallic: p.f("_Metallic", 0.0),
                    roughness: 1.0 - p.f("_Glossiness", 0.5),
                    metallic_roughness_tex: None,
                    normal_tex: self.unity_slot(x, "_BumpMap", ColorSpace::Linear),
                    normal_scale: p.f("_BumpScale", 1.0),
                    occlusion_tex: self.unity_slot(x, "_OcclusionMap", ColorSpace::Linear),
                    emissive: if emission {
                        p.rgb("_EmissionColor", BLACK)
                    } else {
                        [0.0; 3]
                    },
                    emissive_tex: if emission {
                        self.unity_slot(x, "_EmissionMap", srgb)
                    } else {
                        None
                    },
                };
                let alpha = queue_alpha.unwrap_or_else(|| mode_alpha(p.f("_Mode", 0.0)));
                (MaterialModel::Pbr(pbr), alpha, 2.0)
            }
            Family::MobileToon | Family::MobileMatcap => {
                let mut t = plain_toon(p.color("_Color", WHITE), main);
                if family == Family::MobileMatcap {
                    t.matcap_tex = self.unity_slot(x, "_MatCap", srgb);
                    t.matcap_strength = if t.matcap_tex.is_some() { 1.0 } else { 0.0 };
                }
                if let Some(e) = self.unity_slot(x, "_EmissionMap", srgb) {
                    t.emissive_tex = Some(e);
                    t.emissive = p.rgb("_EmissionColor", WHITE);
                }
                (MaterialModel::Toon(t), queue_alpha.unwrap_or(Alpha::Opaque), 2.0)
            }
            Family::Unlit => {
                let u = Unlit {
                    color: p.color("_Color", WHITE),
                    color_tex: main,
                };
                let blend = x.shader.to_ascii_lowercase().contains("particles");
                let alpha = if blend {
                    Alpha::Transparent
                } else {
                    queue_alpha.unwrap_or(Alpha::Opaque)
                };
                (MaterialModel::Unlit(u), alpha, 2.0)
            }
            Family::Unknown => {
                self.warnings.push(format!(
                    "X_toon_material: shader {:?} has no mapping; plain toon",
                    x.shader
                ));
                let cull = x
                    .floats
                    .get("_Cull")
                    .or(x.floats.get("_CullMode"))
                    .copied()
                    .unwrap_or(2.0);
                (
                    MaterialModel::Toon(plain_toon(p.color("_Color", WHITE), main)),
                    queue_alpha.unwrap_or(Alpha::Opaque),
                    cull,
                )
            }
        };
        let (alpha, default_queue) = match alpha {
            Alpha::Opaque => (AlphaMode::Opaque, 2000),
            Alpha::Cutout => (
                AlphaMode::Mask {
                    cutoff: p.f("_Cutoff", 0.5).clamp(0.0, 1.0),
                },
                2450,
            ),
            Alpha::Transparent => (AlphaMode::Blend, 3000),
        };
        Material {
            name: String::new(),
            model,
            alpha,
            double_sided: cull as i32 == 0,
            render_queue: if x.render_queue >= 0 {
                x.render_queue.min(5000)
            } else {
                default_queue
            },
        }
    }

    /// lilToon: main colour, normal map, first shadow, rim, first matcap, emission and outline when enabled.
    fn liltoon(&mut self, x: &XToonMaterial, main: Option<TextureSlot>, outline: bool) -> Toon {
        let p = Props(x);
        let srgb = ColorSpace::Srgb;
        let base = p.color("_Color", WHITE);
        let mut t = plain_toon(base, main);
        let base_rgb = [base[0], base[1], base[2]];
        if p.on("_UseBumpMap") {
            t.normal_tex = self.unity_slot(x, "_BumpMap", ColorSpace::Linear);
            t.normal_scale = p.f("_BumpScale", 1.0);
        }
        if p.on("_UseShadow") {
            // Shadowed albedo is the albedo times the shadow colour, by strength; the shadow texture replaces the
            // main texture there when set. The boundary sits at `_ShadowBorder` in half-Lambert space, `_ShadowBlur`
            // wide (MToon's shift is where the ramp starts).
            let strength = p.f("_ShadowStrength", 1.0);
            t.shade_color = lerp(
                base_rgb,
                mul(base_rgb, p.rgb("_ShadowColor", [0.82, 0.76, 0.85, 1.0])),
                strength,
            );
            t.shade_tex = self.unity_slot(x, "_ShadowColorTex", srgb).or(main);
            let (border, blur) = (p.f("_ShadowBorder", 0.5), p.f("_ShadowBlur", 0.1));
            t.shade_shift = (border - blur * 0.5).clamp(-1.0, 1.0);
            t.shade_toony = (1.0 - blur).clamp(0.0, 1.0);
        } else {
            t.shade_color = base_rgb;
        }
        if p.on("_UseRim") {
            t.rim_color = scale(
                p.rgb("_RimColor", [0.66, 0.5, 0.48, 1.0]),
                p.color("_RimColor", WHITE)[3],
            );
            t.rim_power = p.f("_RimFresnelPower", 3.0);
        }
        if p.on("_UseMatCap") {
            t.matcap_tex = self.unity_slot(x, "_MatCapTex", srgb);
            t.matcap_strength = if t.matcap_tex.is_some() {
                p.f("_MatCapBlend", 1.0)
            } else {
                0.0
            };
        }
        if p.on("_UseEmission") {
            t.emissive = scale(p.rgb("_EmissionColor", WHITE), p.color("_EmissionColor", WHITE)[3]);
            t.emissive_tex = self.unity_slot(x, "_EmissionMap", srgb);
        }
        if outline {
            // lilToon outline width is in centimetres.
            t.outline_width = p.f("_OutlineWidth", 0.08) * 0.01;
            t.outline_color = p.rgb("_OutlineColor", [0.6, 0.56, 0.73, 1.0]);
            t.outline_width_tex = self.unity_slot(x, "_OutlineWidthMask", ColorSpace::Linear);
        }
        t
    }

    /// UTS2: base and first shade colour and maps, the base step and feather, normal map, rim, matcap, emission and
    /// outline (width in millimetres).
    fn uts2(&mut self, x: &XToonMaterial, main: Option<TextureSlot>) -> Toon {
        let p = Props(x);
        let srgb = ColorSpace::Srgb;
        let base = p.color("_BaseColor", WHITE);
        let mut t = plain_toon(base, main);
        t.shade_color = p.rgb("_1st_ShadeColor", WHITE);
        t.shade_tex = self.unity_slot(x, "_1st_ShadeMap", srgb).or(main);
        // The shade starts at `step - feather` in half-Lambert space and is full at `step`.
        let (step, feather) = (p.f("_BaseColor_Step", 0.5), p.f("_BaseShade_Feather", 0.0001));
        t.shade_shift = (step - feather).clamp(-1.0, 1.0);
        t.shade_toony = (1.0 - feather).clamp(0.0, 1.0);
        t.normal_tex = self.unity_slot(x, "_NormalMap", ColorSpace::Linear);
        t.normal_scale = p.f("_BumpScale", 1.0);
        if p.on("_RimLight") {
            t.rim_color = p.rgb("_RimLightColor", WHITE);
        }
        if p.on("_MatCap") {
            t.matcap_tex = self.unity_slot(x, "_MatCap_Sampler", srgb);
            t.matcap_strength = if t.matcap_tex.is_some() { 1.0 } else { 0.0 };
        }
        t.emissive = p.rgb("_Emissive_Color", BLACK);
        t.emissive_tex = self.unity_slot(x, "_Emissive_Tex", srgb);
        if x.keywords.iter().any(|k| k.starts_with("_OUTLINE_")) {
            t.outline_width = p.f("_Outline_Width", 0.0) * 0.001;
            t.outline_color = p.rgb("_Outline_Color", [0.5, 0.5, 0.5, 1.0]);
            t.outline_width_tex = self.unity_slot(x, "_Outline_Sampler", ColorSpace::Linear);
        }
        t
    }
}
