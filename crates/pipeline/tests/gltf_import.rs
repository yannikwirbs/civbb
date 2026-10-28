//! glTF importer conformance: corpus round trips, hand-written edge cases, hostile input, Khronos samples.

use base64::Engine;
use civbb_format::animator::Property;
use civbb_format::material::MaterialModel;
use civbb_pipeline::gltf_import::{Limits, dir_resolver, import};
use serde_json::{Value, json};

fn no_files(_: &str) -> Option<Vec<u8>> {
    None
}

/// A `.gltf` with `bin` embedded as a data URI buffer, unless `doc` brings its own buffers.
fn gltf(mut doc: Value, bin: &[u8]) -> Vec<u8> {
    let data = base64::engine::general_purpose::STANDARD.encode(bin);
    doc["asset"] = json!({"version": "2.0"});
    if doc["buffers"].is_null() {
        doc["buffers"] =
            json!([{"byteLength": bin.len(), "uri": format!("data:application/octet-stream;base64,{data}")}]);
    }
    serde_json::to_vec(&doc).unwrap()
}

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

#[test]
fn corpus_round_trips_through_glb() {
    for spec in [civbb_corpus::lean(), civbb_corpus::bloated()] {
        let original = civbb_corpus::generate(&spec);
        let glb = civbb_corpus::gltf::to_glb(&original);
        let imported = import(&glb, &Limits::default(), no_files).unwrap_or_else(|e| panic!("{}: {e}", spec.name));
        let a = imported.avatar;
        assert_eq!(a.name, original.name);
        // glTF has no active flag; everything imports active.
        let mut nodes = original.hierarchy.nodes.clone();
        nodes.iter_mut().for_each(|n| n.active = true);
        assert_eq!(a.hierarchy.nodes, nodes, "{}", spec.name);
        assert_eq!(a.meshes, original.meshes, "{}", spec.name);
        assert_eq!(a.materials.len(), original.materials.len());
        assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    }
}

/// One triangle as a strip without indices or normals, two scene roots with the same name, an unlit material,
/// a morph target and linear, step and cubic animation channels.
#[test]
fn edge_cases_import() {
    let mut bin = f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]); // 0: positions
    bin.extend(f32s(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0])); // 36: morph positions
    bin.extend(f32s(&[0.0, 1.0])); // 72: times
    bin.extend(f32s(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0])); // 80: translations
    bin.extend(f32s(&[0.0, 1.0])); // 104: weights
    // 112: cubic rotation, (in, value, out) per key.
    bin.extend(f32s(&[
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
        0.0, 0.0,
    ]));
    let view = |offset: usize, len: usize| json!({"buffer": 0, "byteOffset": offset, "byteLength": len});
    let acc = |view: usize, ty: &str, count: usize| json!({"bufferView": view, "componentType": 5126, "count": count, "type": ty});
    let doc = json!({
        "scene": 0,
        "scenes": [{"name": "Scene", "nodes": [0, 1]}],
        "nodes": [{"name": "Twin", "mesh": 0}, {"name": "Twin", "translation": [1.0, 2.0, 3.0]}],
        "meshes": [{
            "primitives": [{"attributes": {"POSITION": 0}, "mode": 5, "material": 0, "targets": [{"POSITION": 1}]}],
            "weights": [0.25],
            "extras": {"targetNames": ["Smile"]},
        }],
        "materials": [{"name": "Flat", "extensions": {"KHR_materials_unlit": {}}}],
        "extensionsUsed": ["KHR_materials_unlit"],
        "bufferViews": [view(0, 36), view(36, 36), view(72, 8), view(80, 24), view(104, 8), view(112, 96)],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]},
            acc(1, "VEC3", 3), acc(2, "SCALAR", 2), acc(3, "VEC3", 2), acc(4, "SCALAR", 2), acc(5, "VEC4", 6),
        ],
        "animations": [{
            "name": "Wave",
            "samplers": [
                {"input": 2, "output": 3},
                {"input": 2, "output": 4, "interpolation": "STEP"},
                {"input": 2, "output": 5, "interpolation": "CUBICSPLINE"},
            ],
            "channels": [
                {"sampler": 0, "target": {"node": 1, "path": "translation"}},
                {"sampler": 1, "target": {"node": 0, "path": "weights"}},
                {"sampler": 2, "target": {"node": 0, "path": "rotation"}},
            ],
        }],
    });
    let imported = import(&gltf(doc, &bin), &Limits::default(), no_files).unwrap();
    let a = imported.avatar;

    let names: Vec<&str> = a.hierarchy.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["Scene", "Twin", "Twin (1)"]);
    assert_eq!(a.hierarchy.nodes[2].rest.translation, [1.0, 2.0, 3.0]);

    let m = &a.meshes[0];
    assert_eq!(m.name, "Twin");
    assert_eq!(m.indices, [0, 1, 2]);
    assert_eq!(m.normals, [[0.0, 0.0, 1.0]; 3]);
    assert_eq!(m.blend_shapes[0].name, "Smile");
    assert_eq!(m.blend_shapes[0].default_weight, 0.25);
    assert_eq!(m.blend_shapes[0].deltas.len(), 1);
    assert_eq!(m.blend_shapes[0].deltas[0].vertex, 2);
    assert!(matches!(a.materials[0].model, MaterialModel::Unlit(_)));

    let clip = &a.clips[0];
    assert_eq!((clip.name.as_str(), clip.length), ("Wave", 1.0));
    assert_eq!(clip.curves.len(), 3 + 1 + 4);
    let tx = &clip.curves[0];
    assert_eq!(
        (tx.binding.path.as_str(), &tx.binding.property),
        ("Twin (1)", &Property::Position(0))
    );
    assert_eq!(
        (tx.keys[0].out_tangent, tx.keys[1].in_tangent, tx.keys[1].value),
        (2.0, 2.0, 2.0)
    );
    let w = &clip.curves[3];
    assert_eq!(w.binding.property, Property::BlendShape("Smile".into()));
    assert_eq!(w.keys[1].value, 100.0);
    assert!(w.keys[0].out_tangent.is_infinite());
    let ry = &clip.curves[4];
    assert_eq!((ry.binding.path.as_str(), ry.keys[0].out_tangent), ("Twin", 0.1));
}

fn triangle_doc() -> (Value, Vec<u8>) {
    let bin = f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    let doc = json!({
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "bufferViews": [{"buffer": 0, "byteLength": 36}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]}],
    });
    (doc, bin)
}

#[test]
fn hostile_input_is_rejected_without_panicking() {
    let reject = |expected: &str, f: &dyn Fn(&mut Value)| {
        let (mut doc, bin) = triangle_doc();
        f(&mut doc);
        let e = import(&gltf(doc, &bin), &Limits::default(), no_files).expect_err(expected);
        assert!(e.to_string().contains(expected), "expected `{expected}`, got `{e}`");
    };
    reject("exceeds limit", &|d| {
        d["accessors"][0] = json!({"componentType": 5126, "count": 4_000_000_000u64, "type": "VEC3",
            "min": [0, 0, 0], "max": [1, 1, 0],
            "sparse": {"count": 1, "indices": {"bufferView": 0, "componentType": 5125}, "values": {"bufferView": 0}}});
    });
    reject("out of bufferView bounds", &|d| d["accessors"][0]["count"] = json!(4));
    reject("out of bufferView bounds", &|d| {
        d["accessors"][0]["byteOffset"] = json!(u64::MAX - 3)
    });
    reject("stride shorter than element", &|d| {
        d["bufferViews"][0]["byteStride"] = json!(4)
    });
    reject("sparse index", &|d| {
        // Index bytes read the float 1.0, i.e. index 0x3f800000.
        d["accessors"][0]["sparse"] = json!({"count": 1,
            "indices": {"bufferView": 0, "byteOffset": 12, "componentType": 5125}, "values": {"bufferView": 0}});
    });
    reject("two parents", &|d| {
        d["nodes"] = json!([{"children": [2]}, {"children": [2]}, {}]);
        d["scenes"][0]["nodes"] = json!([0, 1]);
    });
    reject("has a parent", &|d| {
        d["nodes"] = json!([{"children": [1]}, {"children": [0]}])
    });
    reject("listed twice", &|d| d["scenes"][0]["nodes"] = json!([0, 0]));
    // Fuzz crash 703dba39: gltf-json indexed POSITION accessors before checking the index.
    reject("POSITION accessor", &|d| {
        d["meshes"][0]["primitives"][0]["attributes"]["POSITION"] = json!(5)
    });
    reject("unsigned integer", &|d| {
        d["meshes"][0]["primitives"][0]["indices"] = json!(0)
    });
    reject("Unsupported extension", &|d| {
        d["extensionsRequired"] = json!(["KHR_draco_mesh_compression"]);
        d["extensionsUsed"] = json!(["KHR_draco_mesh_compression"]);
    });
    reject("cannot load", &|d| {
        d["buffers"] = json!([{"byteLength": 36, "uri": "../secret.bin"}])
    });

    // Degenerate but harmless input may import; it must not panic.
    let (mut doc, bin) = triangle_doc();
    doc["accessors"][0]["count"] = json!(0);
    let _ = import(&gltf(doc, &bin), &Limits::default(), no_files);

    // The second is fuzz crash d184ff83: a GLB length below the 12-byte header.
    let short_glb = b"glTF\x02\x00\x00\x00\x02\x00\x00\x00";
    for bytes in [&b"glTF\x02\x00\x00\x00\xff\xff\xff\xff"[..], short_glb, b"{", b""] {
        assert!(import(bytes, &Limits::default(), no_files).is_err());
    }
    let (doc, bin) = triangle_doc();
    let small = Limits {
        bytes: 64,
        ..Limits::default()
    };
    assert!(
        import(&gltf(doc, &bin), &small, no_files).is_err(),
        "byte limit ignored"
    );
}

#[test]
fn dir_resolver_refuses_escapes() {
    let dir = std::env::temp_dir();
    let resolve = dir_resolver(&dir, 1 << 20);
    for uri in [
        "../x.bin",
        "/etc/passwd",
        "C:/Windows/win.ini",
        "a/%2e%2e/b",
        "http://example.com/x",
    ] {
        assert!(resolve(uri).is_none(), "{uri}");
    }
}

/// Imports every Khronos sample in `corpus/local/khronos/` (gitignored; fetch list on issue #9). Skips if absent.
#[test]
fn khronos_samples_import() {
    import_local("khronos");
}

/// Imports every VRM in `corpus/local/vrm/` (gitignored; third-party avatars never go in git). Skips if absent.
#[test]
fn local_vrms_import() {
    import_local("vrm");
}

/// Imports every GLB in `corpus/local/glb/` (gitignored; Booth avatars converted from FBX). Skips if absent.
#[test]
fn local_glbs_import() {
    import_local("glb");
}

fn import_local(sub: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/local")
        .join(sub);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: {} not present", dir.display());
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .flat_map(|e| walk(&e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "gltf" || x == "glb" || x == "vrm"))
        .collect();
    files.sort();
    let mut failures = Vec::new();
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        let resolve = dir_resolver(f.parent().unwrap(), Limits::default().bytes);
        match import(&bytes, &Limits::default(), resolve) {
            Ok(i) => {
                let a = &i.avatar;
                let verts: usize = a.meshes.iter().map(|m| m.positions.len()).sum();
                let shapes: usize = a.meshes.iter().map(|m| m.blend_shapes.len()).sum();
                let skinned = a.meshes.iter().filter(|m| m.skin.is_some()).count();
                let toon = a
                    .materials
                    .iter()
                    .filter(|m| matches!(m.model, MaterialModel::Toon(_)))
                    .count();
                eprintln!(
                    "{}: {} nodes, {} meshes ({skinned} skinned), {verts} verts, {shapes} shapes, {} textures, {} clips, {} human bones, {} visemes, {} chains, {} colliders, {toon} toon, warnings {:?}",
                    f.file_name().unwrap().display(),
                    a.hierarchy.nodes.len(),
                    a.meshes.len(),
                    a.textures.len(),
                    a.clips.len(),
                    a.humanoid.as_ref().map_or(0, |h| h.bones.len()),
                    a.face.as_ref().map_or(0, |f| f.visemes.len()),
                    a.dynamics.chains.len(),
                    a.dynamics.colliders.len(),
                    i.warnings
                );
            }
            Err(e) => failures.push(format!("{}: {e}", f.display())),
        }
    }
    eprintln!("imported {} of {} in {sub}", files.len() - failures.len(), files.len());
    assert!(failures.is_empty(), "{failures:#?}");
}

fn walk(p: &std::path::Path) -> Vec<std::path::PathBuf> {
    match std::fs::read_dir(p) {
        Ok(entries) => entries.flatten().flat_map(|e| walk(&e.path())).collect(),
        Err(_) => vec![p.to_path_buf()],
    }
}

/// A tiny rig: root, hips, spine, head (at z 0.1), a two-bone hair strand on the head and a face mesh with four
/// morph targets (`A`, `Blink`, `Joy`, `Wink`).
fn vrm_rig() -> (Value, Vec<u8>) {
    let mut bin = f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    bin.extend(f32s(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.01, 0.0]));
    let doc = json!({
        "scenes": [{"nodes": [0]}],
        "nodes": [
            {"name": "Root", "children": [1, 6]},
            {"name": "Hips", "translation": [0.0, 1.0, 0.0], "children": [2]},
            {"name": "Spine", "translation": [0.0, 0.1, 0.0], "children": [3]},
            {"name": "Head", "translation": [0.0, 0.5, 0.1], "children": [4]},
            {"name": "Hair", "translation": [0.0, 0.1, 0.0], "children": [5]},
            {"name": "HairEnd", "translation": [0.0, 0.0, -0.1]},
            {"name": "Face", "mesh": 0},
        ],
        "meshes": [{
            "primitives": [{"attributes": {"POSITION": 0}, "material": 0,
                "targets": [{"POSITION": 1}, {"POSITION": 1}, {"POSITION": 1}, {"POSITION": 1}]}],
            "extras": {"targetNames": ["A", "Blink", "Joy", "Wink"]},
        }],
        "materials": [{"name": "Skin"}],
        "bufferViews": [{"buffer": 0, "byteLength": 36}, {"buffer": 0, "byteOffset": 36, "byteLength": 36}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]},
            {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
        ],
    });
    (doc, bin)
}

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
}

#[test]
fn vrm0_imports() {
    use civbb_format::dynamics::Shape;
    use civbb_format::face::{ShapeBind, Viseme};
    use civbb_format::hierarchy::HumanBone;
    use civbb_format::material::AlphaMode;

    let (mut doc, bin) = vrm_rig();
    doc["extensionsUsed"] = json!(["VRM"]);
    doc["extensions"] = json!({"VRM": {
        "meta": {"title": "Vroid"},
        "humanoid": {"humanBones": [
            {"bone": "hips", "node": 1}, {"bone": "spine", "node": 2}, {"bone": "head", "node": 3},
            {"bone": "head", "node": 2}, {"bone": "tail", "node": 4},
        ]},
        "firstPerson": {"firstPersonBone": 3, "firstPersonBoneOffset": {"x": 0, "y": 0.06, "z": 0}},
        "blendShapeMaster": {"blendShapeGroups": [
            {"name": "A", "presetName": "a", "binds": [{"mesh": 0, "index": 0, "weight": 100}]},
            {"name": "Blink", "presetName": "blink", "binds": [{"mesh": 0, "index": 1, "weight": 100}]},
            {"name": "Joy", "presetName": "joy", "binds": [{"mesh": 0, "index": 2, "weight": 50}]},
            {"name": "Broken", "presetName": "unknown", "binds": [{"mesh": 0, "index": 9, "weight": 100}]},
        ]},
        "secondaryAnimation": {
            "boneGroups": [{"stiffiness": 2.0, "gravityPower": 0.5, "gravityDir": {"x": 0, "y": -1, "z": 0},
                "dragForce": 0.4, "hitRadius": 0.05, "bones": [4], "colliderGroups": [0]}],
            "colliderGroups": [{"node": 3, "colliders": [{"offset": {"x": 0, "y": 0.1, "z": 0}, "radius": 0.1}]}],
        },
        "materialProperties": [{"name": "Skin", "shader": "VRM/MToon", "renderQueue": 2450,
            "floatProperties": {"_BlendMode": 1, "_Cutoff": 0.3, "_OutlineWidthMode": 1, "_OutlineWidth": 0.5,
                "_CullMode": 0, "_ShadeToony": 0.7},
            "vectorProperties": {"_Color": [0.5, 0.5, 0.5, 1.0], "_ShadeColor": [1, 1, 1, 1]},
            "textureProperties": {}}],
    }});
    let imported = import(&gltf(doc, &bin), &Limits::default(), no_files).unwrap();
    let a = &imported.avatar;
    assert_eq!(a.name, "Vroid");
    assert_eq!(
        a.hierarchy.nodes[0].rest.rotation,
        [0.0, 1.0, 0.0, 0.0],
        "turned to face +Z"
    );

    let h = a.humanoid.as_ref().unwrap();
    assert_eq!(h.bones.len(), 3, "duplicate and unknown bones skipped");
    assert_eq!(h.node(HumanBone::Head), Some(3));
    assert!(close(h.viewpoint, [0.0, 1.66, -0.1]), "{:?}", h.viewpoint);

    let face = a.face.as_ref().unwrap();
    let bind = |shape| ShapeBind {
        mesh: 0,
        shape,
        weight: 1.0,
    };
    assert_eq!(face.visemes, [(Viseme::Aa, vec![bind(0)])]);
    assert_eq!(face.blink, [bind(1)]);

    let joy = a.clips.iter().find(|c| c.name == "Joy").unwrap();
    assert_eq!(joy.curves[0].binding.path, "Face");
    assert_eq!(joy.curves[0].binding.property, Property::BlendShape("Joy".into()));
    assert_eq!(joy.curves[0].keys[0].value, 50.0);
    assert_eq!(
        a.expressions.as_ref().unwrap().menu.controls.len(),
        1,
        "empty `Broken` gets no toggle"
    );
    assert!(imported.warnings.iter().any(|w| w.contains("missing blendshape")));

    let chain = &a.dynamics.chains[0];
    assert_eq!(chain.root, 4);
    assert_eq!(
        (chain.pull.value, chain.spring.value, chain.gravity.value),
        (0.5, 0.6, 0.5)
    );
    assert_eq!((chain.radius.value, chain.colliders.as_slice()), (0.05, &[0][..]));
    assert!(close(chain.endpoint, [0.0, 0.0, -0.07]), "{:?}", chain.endpoint);
    assert_eq!(a.dynamics.colliders[0].node, 3);
    assert_eq!(a.dynamics.colliders[0].shape, Shape::Sphere { radius: 0.1 });

    let m = &a.materials[0];
    let MaterialModel::Toon(t) = &m.model else {
        panic!("not toon: {m:?}")
    };
    assert_eq!(
        (m.alpha, m.double_sided, m.render_queue),
        (AlphaMode::Mask { cutoff: 0.3 }, true, 2450)
    );
    assert!(
        (t.base_color[0] - 0.214).abs() < 1e-3,
        "sRGB 0.5 linearised: {}",
        t.base_color[0]
    );
    assert_eq!((t.outline_width, t.shade_toony), (0.005, 0.7));
}

#[test]
fn vrm1_imports() {
    use civbb_format::dynamics::Shape;
    use civbb_format::face::Viseme;
    use civbb_format::hierarchy::HumanBone;

    let (mut doc, bin) = vrm_rig();
    doc["extensionsUsed"] = json!(["VRMC_vrm", "VRMC_springBone", "VRMC_materials_mtoon"]);
    doc["materials"][0]["extensions"] = json!({"VRMC_materials_mtoon": {
        "shadeColorFactor": [0.2, 0.3, 0.4], "outlineWidthMode": "worldCoordinates", "outlineWidthFactor": 0.002,
    }});
    doc["extensions"] = json!({
        "VRMC_vrm": {
            "specVersion": "1.0",
            "meta": {"name": "Robo"},
            "humanoid": {"humanBones": {"hips": {"node": 1}, "spine": {"node": 2}, "head": {"node": 3}}},
            "lookAt": {"offsetFromHeadBone": [0.0, 0.05, 0.02]},
            "expressions": {
                "preset": {
                    "aa": {"morphTargetBinds": [{"node": 6, "index": 0, "weight": 1.0}]},
                    "blink": {"morphTargetBinds": [{"node": 6, "index": 1, "weight": 1.0}]},
                    "happy": {"morphTargetBinds": [{"node": 6, "index": 2, "weight": 0.5}]},
                    "lookUp": {"morphTargetBinds": [{"node": 6, "index": 3, "weight": 1.0}]},
                },
                "custom": {"Wink": {"morphTargetBinds": [{"node": 6, "index": 3, "weight": 1.0}]}},
            },
        },
        "VRMC_springBone": {
            "specVersion": "1.0",
            "colliders": [{"node": 3, "shape": {"capsule": {"offset": [0, 0, 0], "tail": [0, 0.2, 0], "radius": 0.05}}}],
            "colliderGroups": [{"colliders": [0]}],
            "springs": [{"joints": [
                {"node": 4, "stiffness": 1.0, "dragForce": 0.5, "hitRadius": 0.02},
                {"node": 5, "stiffness": 2.0},
            ], "colliderGroups": [0]}],
        },
    });
    let imported = import(&gltf(doc, &bin), &Limits::default(), no_files).unwrap();
    let a = &imported.avatar;
    assert_eq!(a.name, "Robo");
    assert_eq!(
        a.hierarchy.nodes[0].rest.rotation,
        [0.0, 0.0, 0.0, 1.0],
        "1.0 already faces +Z"
    );
    let h = a.humanoid.as_ref().unwrap();
    assert_eq!(h.node(HumanBone::Spine), Some(2));
    assert!(close(h.viewpoint, [0.0, 1.65, 0.12]), "{:?}", h.viewpoint);

    let face = a.face.as_ref().unwrap();
    assert_eq!(face.visemes[0].0, Viseme::Aa);
    assert_eq!(face.blink[0].shape, 1);
    let names: Vec<&str> = a.clips.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["happy", "Wink"], "look expressions wait for #44");
    assert_eq!(a.animator.as_ref().unwrap().parameters.len(), 2);

    let chain = &a.dynamics.chains[0];
    assert_eq!(
        (chain.root, chain.ignore.as_slice(), chain.endpoint),
        (4, &[][..], [0.0; 3])
    );
    assert_eq!(
        (chain.pull.value, chain.spring.value, chain.radius.value),
        (0.25, 0.5, 0.02)
    );
    let c = &a.dynamics.colliders[0];
    assert_eq!(
        c.shape,
        Shape::Capsule {
            radius: 0.05,
            height: 0.3
        }
    );
    assert!(close(c.position, [0.0, 0.1, 0.0]));

    let MaterialModel::Toon(t) = &a.materials[0].model else {
        panic!("not toon")
    };
    assert_eq!((t.shade_color, t.outline_width), ([0.2, 0.3, 0.4], 0.002));
}
