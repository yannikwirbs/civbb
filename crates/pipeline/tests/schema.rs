//! The `CIVBB_*` JSON Schemas (ADR 0086 point 2): `docs/spec/schema/*.schema.json` equal what the format types
//! generate (`cargo xtask schema` rewrites them), and the corpus's native extensions validate against them.

use std::path::{Path, PathBuf};

use civbb_pipeline::gltf_import::schemas;
use serde_json::Value;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/schema")
}

#[test]
fn checked_in_schemas_are_fresh() {
    let bless = std::env::var_os("CIVBB_BLESS").is_some();
    let mut stale = Vec::new();
    for (name, schema) in schemas() {
        let path = dir().join(format!("{name}.schema.json"));
        let text = serde_json::to_string_pretty(&schema).unwrap() + "\n";
        if bless {
            std::fs::create_dir_all(dir()).unwrap();
            std::fs::write(&path, &text).unwrap();
        }
        let file = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
        if file != text {
            stale.push(name);
        }
    }
    assert!(stale.is_empty(), "stale schemas {stale:?}: run `cargo xtask schema`");
}

/// The JSON chunk of a GLB.
fn gltf_json(glb: &[u8]) -> Value {
    let len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    assert_eq!(&glb[16..20], b"JSON");
    serde_json::from_slice(&glb[20..20 + len]).unwrap()
}

#[test]
fn corpus_extensions_validate() {
    let validators: Vec<_> = schemas()
        .into_iter()
        .map(|(name, s)| {
            (
                name,
                jsonschema::draft202012::new(&s).unwrap_or_else(|e| panic!("{name}: {e}")),
            )
        })
        .collect();
    for spec in [civbb_corpus::lean(), civbb_corpus::bloated()] {
        let doc = gltf_json(&civbb_corpus::gltf::to_source_glb(&civbb_corpus::generate(&spec)));
        let holders = std::iter::once(&doc)
            .chain(doc["nodes"].as_array().into_iter().flatten())
            .chain(doc["materials"].as_array().into_iter().flatten());
        let mut seen = Vec::new();
        for (name, v) in holders.filter_map(|h| h["extensions"].as_object()).flatten() {
            let Some((_, validator)) = validators.iter().find(|(n, _)| n == name) else {
                assert!(!name.starts_with("CIVBB_"), "{name}: no schema");
                continue;
            };
            let errors: Vec<String> = validator
                .iter_errors(v)
                .map(|e| format!("{}: {e}", e.instance_path()))
                .collect();
            assert!(errors.is_empty(), "{} {name}:\n{}", spec.name, errors.join("\n"));
            let mut unknown = v.clone();
            unknown["unknown"] = true.into();
            assert!(!validator.is_valid(&unknown), "{name}: an unknown field passes");
            seen.push(name.as_str());
        }
        for (name, _) in &validators {
            assert!(seen.contains(name), "{}: the corpus writes no {name}", spec.name);
        }
    }
}
