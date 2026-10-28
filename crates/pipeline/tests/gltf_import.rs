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

    for bytes in [&b"glTF\x02\x00\x00\x00\xff\xff\xff\xff"[..], b"{", b""] {
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
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/local/khronos");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipped: {} not present", dir.display());
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .flat_map(|e| walk(&e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "gltf" || x == "glb"))
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
                eprintln!(
                    "{}: {} nodes, {} meshes ({skinned} skinned), {verts} verts, {shapes} shapes, {} textures, {} clips, warnings {:?}",
                    f.file_name().unwrap().display(),
                    a.hierarchy.nodes.len(),
                    a.meshes.len(),
                    a.textures.len(),
                    a.clips.len(),
                    i.warnings
                );
            }
            Err(e) => failures.push(format!("{}: {e}", f.display())),
        }
    }
    eprintln!(
        "imported {} of {} Khronos samples",
        files.len() - failures.len(),
        files.len()
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

fn walk(p: &std::path::Path) -> Vec<std::path::PathBuf> {
    match std::fs::read_dir(p) {
        Ok(entries) => entries.flatten().flat_map(|e| walk(&e.path())).collect(),
        Err(_) => vec![p.to_path_buf()],
    }
}
