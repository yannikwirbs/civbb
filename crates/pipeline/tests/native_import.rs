//! The native `CIVBB_*` extensions (ADRs 0086, 0101): the generated corpus round-trips through them, and
//! malformed or dangling ones fail the import with the extension's name.

use base64::Engine;
use civbb_format::animator::{Action, EntryOp, PosePart, PoseSource, Unwritten};
use civbb_format::state::{Entry, EntryKind, Platform};
use civbb_pipeline::gltf_import::{Limits, import};
use serde_json::{Value, json};

fn no_files(_: &str) -> Option<Vec<u8>> {
    None
}

/// A `.gltf` of one triangle on node 1 below root node 0, with one material; `f` edits the JSON first.
fn triangle(f: &dyn Fn(&mut Value)) -> Vec<u8> {
    let bin: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    let data = base64::engine::general_purpose::STANDARD.encode(&bin);
    let mut doc = json!({
        "asset": {"version": "2.0"},
        "scenes": [{"nodes": [0]}],
        "nodes": [{"children": [1]}, {"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "materials": [{}],
        "buffers": [{"byteLength": bin.len(), "uri": format!("data:application/octet-stream;base64,{data}")}],
        "bufferViews": [{"buffer": 0, "byteLength": 36}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]}],
    });
    f(&mut doc);
    serde_json::to_vec(&doc).unwrap()
}

/// The native `CIVBB_*` extensions (ADRs 0086, 0101) carry everything glTF lacks: the corpus avatars import equal
/// to the original.
#[test]
fn corpus_round_trips_through_native_extensions() {
    for spec in [civbb_corpus::lean(), civbb_corpus::bloated()] {
        let mut original = civbb_corpus::generate(&spec);
        // The generator makes no muscle frames; one with distinct values checks the field travels.
        let h = original.avatar_mut().and_then(|r| r.humanoid.as_mut()).unwrap();
        h.muscle_frames.push(civbb_format::hierarchy::MuscleFrame {
            bone: h.bones[0].0,
            pre: [0.0, 0.6, 0.0, 0.8],
            post: [0.6, 0.0, 0.0, 0.8],
            sign: [1.0, -1.0, 1.0],
            min: [-40.0, -30.0, -20.0],
            max: [40.0, 30.0, 20.0],
            twist: 1.0,
        });
        // The generator makes no actions; every kind and both unwritten rules travel.
        let states = &mut original.animator.as_mut().unwrap().layers[0].state_machine.states;
        assert!(states.iter().any(|s| s.unwritten == Unwritten::Hold), "{}", spec.name);
        states[0].unwritten = Unwritten::Rest;
        states[0].actions = vec![
            Action::Entries {
                owner_only: true,
                ops: vec![
                    EntryOp::Set { param: 0, value: 1.0 },
                    EntryOp::Copy {
                        src: 0,
                        dst: 0,
                        remap: Some(([0.0, 1.0], [1.0, 0.0])),
                    },
                ],
            },
            Action::PoseSource(vec![(PosePart::LeftHand, PoseSource::Animation)]),
            Action::LayerWeight {
                layer: 0,
                goal: 0.5,
                duration: 0.25,
            },
            Action::GroupWeight {
                group: 0,
                goal: 0.0,
                duration: 1.0,
            },
            Action::Locomotion { disabled: true },
        ];
        // A platform value travels (ADR 0096).
        let grounded = Platform::Grounded.name();
        (original.state.entries).push(Entry::new(grounded, EntryKind::Bool, 1.0));
        assert!(
            original.hierarchy.nodes.iter().any(|n| !n.active),
            "{}: no inactive node",
            spec.name
        );
        let glb = civbb_corpus::gltf::to_source_glb(&original);
        let imported = import(&glb, &Limits::default(), no_files).unwrap_or_else(|e| panic!("{}: {e}", spec.name));
        let a = imported.avatar;
        assert_eq!(a.hierarchy.nodes, original.hierarchy.nodes, "{}", spec.name);
        assert_eq!(a.meshes, original.meshes, "{}", spec.name);
        assert_eq!(a.materials, original.materials, "{}", spec.name);
        let (r, o) = (a.avatar().unwrap(), original.avatar().unwrap());
        assert!(
            o.humanoid.is_some() && o.face.is_some(),
            "{}: no humanoid or face",
            spec.name
        );
        assert_eq!(r, o, "{}", spec.name);
        assert_eq!(a.animator, original.animator, "{}", spec.name);
        assert_eq!(a.clips, original.clips, "{}", spec.name);
        assert_eq!(a.constraints, original.constraints, "{}", spec.name);
        assert_eq!(a.dynamics, original.dynamics, "{}", spec.name);
        assert_eq!(a.contacts, original.contacts, "{}", spec.name);
        assert_eq!(a.state, original.state, "{}", spec.name);
        assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    }
}

/// `CIVBB_*` extensions are checked: a missing or newer version, malformed JSON, node indices outside the scene,
/// mesh references to nodes without a mesh and textures that do not exist fail the import with the extension's name.
#[test]
fn hostile_native_extensions_are_rejected() {
    let reject = |expected: &str, f: &dyn Fn(&mut Value)| {
        let e = import(&triangle(f), &Limits::default(), no_files).expect_err(expected);
        assert!(e.to_string().contains(expected), "expected `{expected}`, got `{e}`");
    };
    let blink = |mesh: u32| {
        let face = json!({"visemes": [], "blink": [{"mesh": mesh, "shape": 0, "weight": 1.0}]});
        json!({"version": 1, "viewpoint": [0.0, 1.5, 0.0], "face": face})
    };
    reject("CIVBB_avatar: no version", &|d| {
        d["extensions"] = json!({"CIVBB_avatar": {}})
    });
    reject("CIVBB_state: this importer reads version 1", &|d| {
        d["extensions"] = json!({"CIVBB_state": {"version": 2, "entries": []}})
    });
    reject("CIVBB_dynamics", &|d| d["extensions"] = json!({"CIVBB_dynamics": []}));
    reject("CIVBB_avatar: node 0 has no mesh", &|d| {
        d["extensions"] = json!({"CIVBB_avatar": blink(0)})
    });
    reject("CIVBB_avatar: node 5 is not in the scene", &|d| {
        d["extensions"] = json!({"CIVBB_avatar": blink(5)})
    });
    reject("CIVBB_node: node 0 has no mesh", &|d| {
        d["nodes"][0]["extensions"] = json!({"CIVBB_node": {"version": 1, "first_person": "Both"}})
    });
    reject("CIVBB_materials_toon: texture 3 does not exist", &|d| {
        let lean = civbb_corpus::generate(&civbb_corpus::lean());
        let mut toon = match &lean.materials[0].model {
            civbb_format::material::MaterialModel::Toon(t) => serde_json::to_value(t).unwrap(),
            m => panic!("corpus material {m:?}"),
        };
        toon["version"] = json!(1);
        toon["base_color_tex"] = json!({"texture": 3, "uv_set": 0, "scale_offset": [1.0, 1.0, 0.0, 0.0]});
        d["materials"][0]["extensions"] = json!({"CIVBB_materials_toon": toon});
    });
    reject(
        "more than one of VRM, X_* and CIVBB_*",
        &|d| {
            d["extensions"] =
                json!({"CIVBB_state": {"version": 1, "entries": []}, "X_contacts": {"senders": [], "receivers": []}})
        },
    );
}
