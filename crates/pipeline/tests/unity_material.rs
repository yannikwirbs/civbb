//! Unity materials from `X_toon_material` (ADR 0043): shader families fingerprinted by GUID, name and markers, and
//! each family's mapping to the native Toon, PBR and Unlit models, on hand-written saved properties.

use base64::Engine;
use civbb_format::material::{AlphaMode, MatcapBlend, Material, MaterialModel, Toon, ToonLight};
use civbb_pipeline::gltf_import::{Imported, Limits, import};
use serde_json::{Value, json};

/// lilToon's `lts_trans_o` (transparent, outline) shader GUID.
const LTS_TRANS_O: &str = "3c79b10c7e0b2784aaa4c2f8dd17d55e";

/// One triangle whose material carries `x` as its `X_toon_material`, with one embedded PNG as texture 0.
fn gltf(x: Value) -> Vec<u8> {
    gltf_png(x, b"\x89PNG\r\n\x1a\n\0\0\0\0")
}

/// [`gltf`] with `png` as texture 0's image.
fn gltf_png(x: Value, png: &[u8]) -> Vec<u8> {
    let mut bin: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    bin.extend_from_slice(png);
    let doc = json!({
        "asset": {"version": "2.0"},
        "extensionsUsed": ["X_toon_material"],
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "material": 0}]}],
        "materials": [{"name": "m", "extensions": {"X_toon_material": x}}],
        "textures": [{"source": 0}],
        "images": [{"bufferView": 1, "mimeType": "image/png"}],
        "bufferViews": [{"buffer": 0, "byteLength": 36}, {"buffer": 0, "byteOffset": 36, "byteLength": png.len()}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]}],
        "buffers": [{"byteLength": bin.len(), "uri": format!("data:application/octet-stream;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bin))}],
    });
    serde_json::to_vec(&doc).unwrap()
}

fn import_material(x: Value) -> Imported {
    import(&gltf(x), &Limits::default(), |_| None).unwrap()
}

fn material(x: Value) -> Material {
    import_material(x).avatar.materials[0].clone()
}

fn toon(m: &Material) -> &Toon {
    match &m.model {
        MaterialModel::Toon(t) => t,
        other => panic!("not toon: {other:?}"),
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

/// `texture` as a saved texture property on texture 0 with Unity tiling.
fn tex(st: [f32; 4]) -> Value {
    json!({"index": 0, "scale_offset": st})
}

#[test]
fn liltoon_by_guid_maps_its_variant_and_enabled_features() {
    let m = material(json!({
        "shader": "Hidden/InternalErrorShader", "shader_guid": LTS_TRANS_O, "render_queue": -1,
        "floats": {"_lilToonVersion": 45, "_Cull": 0, "_UseShadow": 1, "_ShadowStrength": 0.5, "_ShadowBorder": 0.6,
            "_ShadowBlur": 0.2, "_UseRim": 0, "_UseMatCap": 1, "_MatCapBlend": 0.7, "_UseEmission": 0,
            "_OutlineWidth": 0.1, "_BumpScale": 0.8, "_UseBumpMap": 0, "_UseRimShade": 1, "_RimShadeBorder": 0.6,
            "_LightMaxLimit": 0.8},
        "colors": {"_Color": [0.5, 0.5, 0.5, 1], "_ShadowColor": [0.5, 0.5, 0.5, 1], "_OutlineColor": [1, 0, 0, 1],
            "_RimColor": [1, 1, 1, 1], "_EmissionColor": [1, 1, 1, 1], "_MatCapColor": [0.5, 0.5, 0.5, 0.5]},
        "textures": {"_MainTex": tex([2.0, 3.0, 0.25, 0.5]), "_MatCapTex": tex([1.0, 1.0, 0.0, 0.0]),
            "_BumpMap": tex([1.0, 1.0, 0.0, 0.0]), "_MatCapBlendMask": tex([1.0, 1.0, 0.0, 0.0])},
    }));
    assert_eq!(m.name, "m");
    assert_eq!(
        (m.alpha, m.render_queue, m.double_sided),
        (AlphaMode::Blend, 3000, true)
    );
    let t = toon(&m);
    let half = 0.21404114; // sRGB 0.5 in linear
    assert!(
        close(t.base_color[0], half) && t.base_color[3] == 1.0,
        "{:?}",
        t.base_color
    );
    // Unity's V-up tiling becomes V-down: offset 1 - 3 - 0.5.
    assert_eq!(t.base_color_tex.unwrap().scale_offset, [2.0, 3.0, 0.25, -2.5]);
    // Shadow: albedo times shadow colour by strength, ramp from border - blur/2, toony 1 - blur.
    assert!(
        close(t.shade_color[0], half * (1.0 + (half - 1.0) * 0.5)),
        "{:?}",
        t.shade_color
    );
    assert!(close(t.shade_shift, 0.5) && close(t.shade_toony, 0.8));
    assert_eq!(t.shade_tex, t.base_color_tex, "no shadow texture: the main one");
    // The tint's alpha scales the strength; its colour tints the matcap.
    assert!(t.matcap_tex.is_some() && close(t.matcap_strength, 0.35) && close(t.matcap_color[0], half));
    assert!(t.matcap_mask_tex.is_some());
    assert_eq!(
        (t.light_min, t.light_max),
        (0.05, 0.8),
        "lilToon's light limits, the lower one by default"
    );
    assert_eq!(t.matcap_blend, MatcapBlend::Add, "lilToon's default blend mode adds");
    let rim_shade = t.rim_shade.expect("rim shade on");
    assert!(close(rim_shade.color[0], half) && rim_shade.border == 0.6 && rim_shade.blur == 1.0);
    assert_eq!(
        (t.rim_color, t.emissive),
        ([0.0; 3], [0.0; 3]),
        "rim and emission are off"
    );
    assert!(t.normal_tex.is_none(), "normal map off");
    assert!(close(t.outline_width, 0.001) && t.outline_color == [1.0, 0.0, 0.0]);
}

#[test]
fn liltoon_by_markers_takes_transparency_from_the_queue_and_leaves_the_outline_off() {
    let imported = import_material(json!({
        "shader": "Hidden/InternalErrorShader", "shader_guid": "", "render_queue": 2450,
        "floats": {"_lilToonVersion": 33, "_utsVersion": 2.08, "_Cutoff": 0.3, "_OutlineWidth": 0.1, "_Cull": 2},
    }));
    let m = &imported.avatar.materials[0];
    assert_eq!(
        (m.alpha, m.render_queue, m.double_sided),
        (AlphaMode::Mask { cutoff: 0.3 }, 2450, false)
    );
    assert_eq!(toon(m).outline_width, 0.0);
    assert!(
        imported
            .warnings
            .iter()
            .any(|w| w.contains("lilToon shader") && w.contains("outline left off"))
    );
    // The multi shader reads `_TransparentMode` when the queue is the shader's own.
    let m = material(
        json!({"shader": "Hidden/InternalErrorShader", "shader_guid": "9294844b15dca184d914a632279b24e1",
        "floats": {"_TransparentMode": 2}}),
    );
    assert_eq!(m.alpha, AlphaMode::Blend);
}

#[test]
fn a_shader_name_wins_over_leftover_markers() {
    // A Poiyomi material that once used lilToon keeps `_lilToonVersion`.
    let m = material(json!({
        "shader": "Hidden/.poiyomi/• Poiyomi Toon •/Body-Body", "render_queue": 3000,
        "floats": {"_lilToonVersion": 45, "_ShaderOptimizerEnabled": 1, "_EnableEmission": 1, "_EmissionStrength": 2,
            "_MatcapEnable": 0, "_Cull": 2},
        "colors": {"_EmissionColor": [1, 1, 1, 1]},
    }));
    assert_eq!(m.alpha, AlphaMode::Blend);
    let t = toon(&m);
    assert_eq!(
        (t.emissive, t.matcap_tex.is_none(), t.outline_width),
        ([2.0; 3], true, 0.0)
    );
}

#[test]
fn unity_toon_families_clamp_the_light_while_mtoon_and_mobile_add_the_ambient() {
    // ADR 0050.
    let light = |shader: &str| toon(&material(json!({"shader": shader, "render_queue": -1}))).light;
    assert_eq!(light("VRM/MToon"), ToonLight::Additive);
    assert_eq!(light("VRChat/Mobile/Toon Lit"), ToonLight::Additive);
    for shader in [
        "Hidden/.poiyomi/• Poiyomi Toon •/Body",
        "UnityChanToonShader/Toon_DoubleShadeWithFeather",
    ] {
        assert_eq!(light(shader), ToonLight::Clamped, "{shader}");
    }
}

#[test]
fn uts2_maps_shade_step_feather_and_outline() {
    let m = material(json!({
        "shader": "Hidden/InternalErrorShader", "keywords": ["_OUTLINE_NML"],
        "floats": {"_utsVersion": 2.08, "_BaseColor_Step": 0.7, "_BaseShade_Feather": 0.2, "_Outline_Width": 2,
            "_CullMode": 0, "_RimLight": 1},
        "colors": {"_BaseColor": [1, 1, 1, 1], "_1st_ShadeColor": [0, 0, 1, 1], "_RimLightColor": [1, 0, 0, 1],
            "_Color": [0, 1, 0, 1]},
        "textures": {"_MainTex": tex([1.0, 1.0, 0.0, 0.0])},
    }));
    let t = toon(&m);
    assert_eq!(t.base_color, [1.0; 4], "UTS2 reads _BaseColor, not _Color");
    assert_eq!(t.shade_color, [0.0, 0.0, 1.0]);
    assert!(close(t.shade_shift, 0.5) && close(t.shade_toony, 0.8));
    assert!(close(t.outline_width, 0.002) && t.rim_color == [1.0, 0.0, 0.0] && m.double_sided);
}

#[test]
fn standard_vrchat_mobile_unlit_and_unknown_shaders() {
    let m = material(json!({
        "shader": "Standard", "render_queue": -1, "keywords": [],
        "floats": {"_Metallic": 0.25, "_Glossiness": 0.75, "_Mode": 1, "_Cutoff": 0.4},
        "colors": {"_EmissionColor": [1, 1, 1, 1]},
        "textures": {"_EmissionMap": tex([1.0, 1.0, 0.0, 0.0])},
    }));
    let MaterialModel::Pbr(p) = &m.model else {
        panic!("{m:?}")
    };
    assert_eq!((p.metallic, p.roughness), (0.25, 0.25));
    assert_eq!(
        (p.emissive, p.emissive_tex.is_none()),
        ([0.0; 3], true),
        "emission needs the _EMISSION keyword"
    );
    assert_eq!(m.alpha, AlphaMode::Mask { cutoff: 0.4 });

    let m = material(json!({"shader": "VRChat/Mobile/MatCap Lit", "textures": {"_MatCap": tex([1.0, 1.0, 0.0, 0.0])}}));
    assert!(toon(&m).matcap_tex.is_some() && toon(&m).matcap_strength == 1.0);
    assert_eq!(toon(&m).matcap_blend, MatcapBlend::Multiply, "MatCap Lit multiplies");

    let m = material(json!({"shader": "VRChat/Mobile/Particles/Additive"}));
    assert!(matches!(m.model, MaterialModel::Unlit(_)) && m.alpha == AlphaMode::Blend);

    let imported = import_material(json!({"shader": "TSF/Base1", "floats": {"_Cull": 0}}));
    let m = &imported.avatar.materials[0];
    assert!(m.double_sided && toon(m).shade_color == [0.75; 3]);
    assert!(
        imported.warnings.iter().any(|w| w.contains("TSF/Base1")),
        "{:?}",
        imported.warnings
    );
}

/// Malformed extensions fail the import; a texture index past the textures drops the slot with a warning.
#[test]
fn hostile_toon_materials() {
    for bad in [
        json!({"floats": {}}),
        json!({"shader": "x", "floats": {"_A": "one"}}),
        json!({"shader": "x", "extra": 1}),
    ] {
        let e = import(&gltf(bad), &Limits::default(), |_| None).expect_err("malformed");
        assert!(e.to_string().contains("X_toon_material"), "{e}");
    }
    let imported = import_material(
        json!({"shader": "Standard", "textures": {"_MainTex": {"index": 9, "scale_offset": [1, 1, 0, 0]}}}),
    );
    assert!(
        imported.warnings.iter().any(|w| w.contains("texture 9")),
        "{:?}",
        imported.warnings
    );
}

#[test]
fn liltoon_emission_gradient_scales_the_emission_by_its_start() {
    // A 2x1 gradient, white then black: a repeating sampler at u = 0 averages them.
    let mut png = Vec::new();
    let img = image::RgbaImage::from_raw(2, 1, vec![255, 255, 255, 255, 0, 0, 0, 255]).unwrap();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let x = json!({
        "shader": "lilToon", "render_queue": -1,
        "floats": {"_UseEmission": 1, "_EmissionUseGrad": 1},
        "colors": {"_EmissionColor": [2, 2, 2, 1]},
        "textures": {"_EmissionGradTex": tex([1.0, 1.0, 0.0, 0.0])},
    });
    let imported = import(&gltf_png(x, &png), &Limits::default(), |_| None).unwrap();
    let t = toon(&imported.avatar.materials[0]);
    // HDR 2 is 2^2.2 in linear (Unity's GammaToLinearSpace), times the averaged texel 0.5.
    let want = 2f32.powf(2.2) * 0.5;
    assert!(close(t.emissive[0], want), "{:?} != {want}", t.emissive);
}

#[test]
fn poiyomi_matcap_takes_its_add_or_multiply_weight_and_tint() {
    let matcap = |floats: Value| {
        let mut f = json!({"_MatcapEnable": 1, "_MatcapIntensity": 2});
        f.as_object_mut().unwrap().extend(floats.as_object().unwrap().clone());
        toon(&material(json!({
            "shader": "Hidden/.poiyomi/• Poiyomi Toon •/Body", "floats": f,
            "colors": {"_MatcapColor": [0.5, 0.5, 0.5, 1]},
            "textures": {"_Matcap": tex([1.0, 1.0, 0.0, 0.0]), "_MatcapMask": tex([1.0, 1.0, 0.0, 0.0])},
        })))
        .clone()
    };
    // Mostly replaced (no counterpart): next to nothing is added.
    let t = matcap(json!({"_MatcapReplace": 0.02}));
    assert_eq!((t.matcap_blend, t.matcap_strength), (MatcapBlend::Add, 0.0));
    assert!(close(t.matcap_color[0], 0.21404114 * 2.0) && t.matcap_mask_tex.is_some());
    let t = matcap(json!({"_MatcapMultiply": 0.5}));
    assert_eq!((t.matcap_blend, t.matcap_strength), (MatcapBlend::Multiply, 0.5));
    let t = matcap(json!({"_MatcapAdd": 0.25, "_MatcapEmissionStrength": 0.5}));
    assert_eq!((t.matcap_blend, t.matcap_strength), (MatcapBlend::Add, 0.75));
}

#[test]
fn liltoon_emission_main_strength_tints_the_emission_with_the_base_colour() {
    let t = toon(&material(json!({
        "shader": "lilToon",
        "floats": {"_UseEmission": 1, "_EmissionMainStrength": 1},
        "colors": {"_Color": [1, 0.5, 0, 1], "_EmissionColor": [1, 1, 1, 1]},
    })))
    .clone();
    assert!(close(t.emissive[0], 1.0) && close(t.emissive[1], 0.21404114) && t.emissive[2] == 0.0);
}
