//! Unity dialect 1 (ADR 0085): the exporter's `X_*` extensions as format v25 wrote them, frozen in
//! `tests/fixtures/unity-dialect-1*.gltf` (generated content covering every variant of every `X_*` part). The
//! fixtures never change; the native model they import to is pinned by `tests/golden/unity-dialect-1*.json`, which
//! `CIVBB_BLESS=1` rewrites after an intended change to the native model. A native change that the importer does not
//! lower fails here instead of silently changing what existing exports read as.

use std::path::Path;

use civbb_format::animator::{Behaviour, DriverOp};
use civbb_format::state::{EntryKind, SyncMode};
use civbb_pipeline::gltf_import::{Limits, import};
use serde_json::{Value, json};

fn no_files(_: &str) -> Option<Vec<u8>> {
    None
}

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn unity_dialect_1_imports_to_its_golden() {
    for name in ["unity-dialect-1", "unity-dialect-1-eye-bones"] {
        let imported = import(&fixture(&format!("{name}.gltf")), &Limits::default(), no_files)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let got = json!({"avatar": imported.avatar, "warnings": imported.warnings});
        let text = serde_json::to_string_pretty(&got).unwrap() + "\n";
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/{name}.json"));
        if std::env::var_os("CIVBB_BLESS").is_some() {
            std::fs::write(&path, &text).unwrap();
        }
        // Compared as text: parsing floats back is not exact without serde_json's `float_roundtrip`.
        let golden = std::fs::read_to_string(&path).expect("golden missing: run with CIVBB_BLESS=1");
        assert!(
            text == golden.replace("\r\n", "\n"),
            "{name} imports differently from {}:\n{text}",
            path.display()
        );
    }
}

#[test]
fn unity_dialect_versions_are_checked() {
    let mut doc: Value = serde_json::from_slice(&fixture("unity-dialect-1.gltf")).unwrap();
    let with = |doc: &mut Value, extras: Value| {
        doc["asset"]["extras"] = extras;
        import(&serde_json::to_vec(doc).unwrap(), &Limits::default(), no_files)
    };
    with(&mut doc, json!({"civbb_unity_dialect": 1})).expect("dialect 1 named");
    with(&mut doc, json!({"other": true})).expect("absent means 1");
    for bad in [json!(2), json!(0), json!("1")] {
        let err = with(&mut doc, json!({"civbb_unity_dialect": bad}))
            .err()
            .unwrap_or_else(|| panic!("{bad} accepted"));
        assert!(err.to_string().contains("civbb_unity_dialect"), "{err}");
    }
}

/// Dialect 1's parameters lower to state entries (ADR 0088, #115): the animator's first, in order, then expression
/// parameters it lacks; an expression parameter's default starts the entry, the animator's stays its `rest`; the
/// animator's type holds; unknown names in the menu and receivers reference nothing.
#[test]
fn unity_parameters_lower_to_state_entries() {
    let mut doc: Value = serde_json::from_slice(&fixture("unity-dialect-1.gltf")).unwrap();
    let ext = &mut doc["extensions"];
    ext["X_animator"]["animator"]["parameters"][1]["default"] = json!(0.0); // Speed: 0.5 in the expressions
    let expression = &mut ext["X_expressions"]["parameters"];
    expression[1]["kind"] = json!("Float"); // Count: an int in the animator
    expression
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "Extra", "kind": "Int", "default": 2.0, "saved": false, "synced": true}));
    ext["X_expressions"]["menu"]["controls"][0]["parameter"] = json!("Nope");
    ext["X_contacts"]["receivers"][0]["parameter"] = json!("");
    let a = import(&serde_json::to_vec(&doc).unwrap(), &Limits::default(), no_files)
        .unwrap()
        .avatar;
    let e = &a.state.entries;
    let names: Vec<&str> = e.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Toggle", "Speed", "Count", "Fire", "X", "Y", "Mirror", "Extra"]);
    assert_eq!((e[1].default, e[1].rest), (0.5, Some(0.0)));
    assert_eq!((e[0].default, e[0].rest), (1.0, None));
    assert_eq!(e[2].kind, EntryKind::Int);
    assert_eq!(
        (e[7].kind, e[7].default, e[7].sync, e[7].external),
        (EntryKind::Int, 2.0, SyncMode::Owner, true)
    );
    assert!(!e[3].external && e[3].sync == SyncMode::None);
    assert_eq!(a.expressions.unwrap().menu.controls[0].parameter, None);
    assert_eq!(a.contacts.receivers[0].parameter, None);
}

/// VRChat's built-ins lower to platform values (ADR 0096): renamed into `platform/`, keeping the animator's type,
/// never synced, saved or external even when an expression parameter declares one; the menu and drivers no longer
/// write them, and every reference by index still reads them.
#[test]
fn vrchat_builtins_lower_to_platform_values() {
    let mut doc: Value = serde_json::from_slice(&fixture("unity-dialect-1.gltf")).unwrap();
    let ext = &mut doc["extensions"];
    let params = ext["X_animator"]["animator"]["parameters"].as_array_mut().unwrap();
    params.push(json!({"name": "Grounded", "kind": "Float", "default": 0.0})); // 7
    params.push(json!({"name": "GestureLeft", "kind": "Int", "default": 0.0})); // 8
    let ops = &mut ext["X_animator"]["animator"]["layers"][0]["state_machine"]["states"][1]["behaviours"][0]["ParameterDriver"]
        ["ops"];
    let before = ops.as_array().unwrap().len();
    let ops = ops.as_array_mut().unwrap();
    ops.push(json!({"Set": {"param": 7, "value": 1.0}}));
    ops.push(json!({"Copy": {"src": 8, "dst": 0, "remap": null}}));
    (ext["X_expressions"]["parameters"].as_array_mut().unwrap())
        .push(json!({"name": "InStation", "kind": "Bool", "default": 0.0, "saved": true, "synced": true}));
    ext["X_expressions"]["menu"]["controls"][0]["parameter"] = json!("InStation");
    let imported = import(&serde_json::to_vec(&doc).unwrap(), &Limits::default(), no_files).unwrap();
    let a = imported.avatar;
    civbb_format::validate(&a, &civbb_format::Caps::default(), |_| true).unwrap();

    let e = &a.state.entries;
    let names: Vec<&str> = e.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names[7..],
        ["platform/grounded", "platform/gesture_left", "platform/in_station"]
    );
    assert_eq!((e[7].kind, e[8].kind), (EntryKind::Float, EntryKind::Int));
    assert!(
        e[7..]
            .iter()
            .all(|e| e.sync == SyncMode::None && !e.saved && !e.external)
    );
    assert_eq!(a.expressions.unwrap().menu.controls[0].parameter, None);
    let state = &a.animator.unwrap().layers[0].state_machine.states[1];
    let Behaviour::ParameterDriver { ops, .. } = &state.behaviours[0] else {
        panic!("not a driver")
    };
    assert_eq!(
        ops.len(),
        before + 1,
        "the write to Grounded is dropped, the read of GestureLeft kept"
    );
    assert!(matches!(ops[before], DriverOp::Copy { src: 8, dst: 0, .. }));
    assert!(
        imported.warnings.iter().any(|w| w.contains("1 driver operations")),
        "{:?}",
        imported.warnings
    );
}
