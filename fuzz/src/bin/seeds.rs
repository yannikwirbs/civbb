//! Writes seed inputs for every fuzz target into `fuzz/corpus/<target>/`, generated from the CC0 corpus.

use std::fs;
use std::path::Path;

use civbb_format::package::Platform;
use civbb_format::{SigningKey, cbor};
use civbb_pipeline::{Limits, import, package};

/// A minimal VRM 1.0 document with a humanoid, an expression and a spring, so the VRM paths get seeded too.
const VRM: &str = r#"{"asset":{"version":"2.0"},"extensionsUsed":["VRMC_vrm","VRMC_springBone"],
"scenes":[{"nodes":[0]}],"nodes":[{"name":"Root","children":[1]},{"name":"Hips","children":[2]},
{"name":"Head","translation":[0,1.5,0]}],"extensions":{"VRMC_vrm":{"specVersion":"1.0",
"humanoid":{"humanBones":{"hips":{"node":1},"head":{"node":2}}},"expressions":{"preset":{"happy":{}}}},
"VRMC_springBone":{"specVersion":"1.0","springs":[{"joints":[{"node":1},{"node":2}]}]}}}"#;

/// One triangle drawn once per material, each material an `X_toon_material` of another shader family (ADR 0043):
/// lilToon by GUID, Poiyomi, UTS2, MToon, Standard with emission, VRChat Mobile and unlit; their textures name a
/// texture the document lacks.
const TOON: &str = r#"{"asset":{"version":"2.0"},"extensionsUsed":["X_toon_material"],"scenes":[{"nodes":[0]}],
"nodes":[{"mesh":0}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0},
{"attributes":{"POSITION":0},"material":1},{"attributes":{"POSITION":0},"material":2},
{"attributes":{"POSITION":0},"material":3},{"attributes":{"POSITION":0},"material":4},
{"attributes":{"POSITION":0},"material":5},{"attributes":{"POSITION":0},"material":6}]}],
"materials":[
{"name":"lil","extensions":{"X_toon_material":{"shader":"Hidden/InternalErrorShader",
"shader_guid":"3c79b10c7e0b2784aaa4c2f8dd17d55e","render_queue":-1,"keywords":["EFFECT_HUE_VARIATION"],
"floats":{"_UseShadow":1,"_ShadowStrength":0.5,"_UseMatCap":1,"_MatCapBlend":0.7,"_OutlineWidth":0.1,"_UseRim":1},
"colors":{"_Color":[0.5,0.5,0.5,1],"_MatCapColor":[0.5,0.5,0.5,0.5],"_RimColor":[1,1,1,1]},
"textures":{"_MainTex":{"index":0,"scale_offset":[2,3,0.25,0.5]},"_MatCapTex":{"index":0,"scale_offset":[1,1,0,0]}}}}},
{"name":"poi","extensions":{"X_toon_material":{"shader":".poiyomi/Poiyomi 7.3/Poiyomi Toon","render_queue":2450,
"floats":{"_Mode":1,"_MatcapEnable":1,"_MatcapAdd":0.5},"colors":{"_Color":[1,1,1,1],"_EmissionColor":[2,1,0,1]}}}},
{"name":"uts","extensions":{"X_toon_material":{"shader":"UnityChanToonShader/Toon_DoubleShadeWithFeather",
"floats":{"_BaseColor_Step":0.5,"_BaseShade_Feather":0.1},"colors":{"_BaseColor":[1,0.9,0.8,1],"_1st_ShadeColor":[0.5,0.4,0.4,1]}}}},
{"name":"mtoon","extensions":{"X_toon_material":{"shader":"VRM/MToon","floats":{"_ShadeShift":-0.2,"_ShadeToony":0.9,
"_BlendMode":2},"colors":{"_Color":[1,1,1,0.5],"_ShadeColor":[0.6,0.5,0.5,1]}}}},
{"name":"std","extensions":{"X_toon_material":{"shader":"Standard","keywords":["_EMISSION"],
"floats":{"_Glossiness":0.7,"_Metallic":0.2},"colors":{"_Color":[0.2,0.4,0.6,1],"_EmissionColor":[1,1,1,1]},
"textures":{"_MetallicGlossMap":{"index":0,"scale_offset":[1,1,0,0]}}}}},
{"name":"mobile","extensions":{"X_toon_material":{"shader":"VRChat/Mobile/Toon Lit","colors":{"_Color":[1,1,1,1]}}}},
{"name":"unlit","extensions":{"X_toon_material":{"shader":"Unlit/Texture"}}}],
"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
"bufferViews":[{"buffer":0,"byteLength":36}],
"buffers":[{"byteLength":36,"uri":"data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAA"}]}"#;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let write = |target: &str, name: &str, bytes: &[u8]| {
        let d = dir.join(target);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(name), bytes).unwrap();
    };
    let lean = civbb_corpus::generate(&civbb_corpus::lean());
    let glb = civbb_corpus::gltf::to_glb(&lean);
    write("gltf_import", "lean.glb", &glb);
    write("gltf_import", "lean-source.glb", &civbb_corpus::gltf::to_source_glb(&lean));
    write("gltf_import", "vrm1.gltf", VRM.as_bytes());
    let toon = import(TOON.as_bytes(), &Limits::default(), |_| None).expect("the toon seed imports");
    assert_eq!(toon.avatar.materials.len(), 7, "every toon seed material maps");
    write("gltf_import", "toon-materials.gltf", TOON.as_bytes());
    write("avatar_validate", "lean.cbor", &cbor::to_vec(&lean));
    let imported = import(&glb, &Limits::default(), |_| None).expect("lean imports");
    let signed = package(imported, Platform::Desktop).encode_signed(&SigningKey::from_bytes(&[7; 32]));
    write("package_decode", "lean.civpkg", &signed);
    let image = image::RgbaImage::from_fn(16, 8, |x, y| image::Rgba([x as u8 * 16, y as u8 * 32, 128, 255]));
    use civbb_pipeline::texture::Codec;
    for (name, codec) in [("uastc.ktx2", Codec::Uastc), ("xuastc.ktx2", Codec::Xuastc(75))] {
        let ktx2 = civbb_pipeline::texture::encode_ktx2(&image, true, codec).expect("encodes");
        write("ktx2_transcode", name, &ktx2);
    }
    println!("seeds written to {}", dir.display());
}
