//! Frozen `CIVBB_*` fixtures (ADR 0087 point 2): every version of every extension, with the newest of the others,
//! on a frozen host glTF (the lean corpus avatar without its `CIVBB_*` extensions), migrates, deserialises and
//! validates through the importer.

use std::path::{Path, PathBuf};

use civbb_format::migrate::newest;
use civbb_pipeline::gltf_import::{Limits, import, schemas};
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../format/tests/fixtures")
}

fn fixture(extension: &str, version: u64) -> Value {
    let path = fixtures().join(extension).join(format!("v{version}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

/// The host's JSON chunk and the rest of the file after it (the BIN chunk).
fn host() -> (Value, Vec<u8>) {
    let glb = std::fs::read(fixtures().join("host.glb")).unwrap();
    let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    assert_eq!(&glb[16..20], b"JSON");
    (
        serde_json::from_slice(&glb[20..20 + len]).unwrap(),
        glb[20 + len..].to_vec(),
    )
}

fn glb(json: &Value, bin: &[u8]) -> Vec<u8> {
    let mut j = serde_json::to_vec(json).unwrap();
    j.resize(j.len().next_multiple_of(4), b' ');
    let total = 12 + 8 + j.len() + bin.len();
    let mut out = b"glTF".to_vec();
    out.extend(2u32.to_le_bytes());
    out.extend((total as u32).to_le_bytes());
    out.extend((j.len() as u32).to_le_bytes());
    out.extend(b"JSON");
    out.extend(j);
    out.extend(bin);
    out
}

/// The host with `CIVBB_node` on its first node carrying a mesh, `CIVBB_materials_toon` on every material and the
/// rest at the root.
fn with_extensions(extensions: &[(&str, Value)]) -> Vec<u8> {
    let (mut doc, bin) = host();
    let mesh_node = (doc["nodes"].as_array().unwrap().iter())
        .position(|n| n.get("mesh").is_some())
        .unwrap();
    for (name, v) in extensions {
        match *name {
            "CIVBB_node" => doc["nodes"][mesh_node]["extensions"][name] = v.clone(),
            "CIVBB_materials_toon" => {
                for m in doc["materials"].as_array_mut().unwrap() {
                    m["extensions"][name] = v.clone();
                }
            }
            _ => doc["extensions"][name] = v.clone(),
        }
    }
    doc["extensionsUsed"] = extensions.iter().map(|(n, _)| Value::from(*n)).collect();
    glb(&doc, &bin)
}

#[test]
fn every_fixture_imports() {
    let names: Vec<&str> = schemas().into_iter().map(|(n, _)| n).collect();
    let newest_of_each: Vec<(&str, Value)> = names.iter().map(|&n| (n, fixture(n, newest(n)))).collect();
    let (host, _) = host();
    assert!(
        !serde_json::to_string(&host).unwrap().contains("CIVBB_"),
        "the host carries no CIVBB_* extensions"
    );
    for &name in &names {
        for version in 1..=newest(name) {
            let mut set = newest_of_each.clone();
            set.iter_mut().find(|(n, _)| *n == name).unwrap().1 = fixture(name, version);
            let imported = import(&with_extensions(&set), &Limits::default(), |_| None)
                .unwrap_or_else(|e| panic!("{name} v{version}: {e}"));
            assert!(
                imported.warnings.is_empty(),
                "{name} v{version}: {:?}",
                imported.warnings
            );
            let d = imported.avatar;
            assert!(d.avatar().is_some_and(|a| a.humanoid.is_some()), "{name} v{version}");
            assert!(d.animator.is_some() && !d.clips.is_empty() && !d.state.entries.is_empty());
            assert!(!d.constraints.is_empty() && !d.dynamics.chains.is_empty() && !d.contacts.receivers.is_empty());
            assert!(
                d.hierarchy.nodes.iter().any(|n| !n.active),
                "{name} v{version}: CIVBB_node not read"
            );
        }
    }
    // Every extension has fixtures, and every fixture directory is an extension.
    let dirs: Vec<String> = std::fs::read_dir(fixtures())
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    let mut dirs_sorted = dirs.clone();
    dirs_sorted.sort_unstable();
    assert_eq!(dirs_sorted, sorted);
}
