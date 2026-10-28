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
    write("gltf_import", "vrm1.gltf", VRM.as_bytes());
    write("avatar_validate", "lean.cbor", &cbor::to_vec(&lean));
    let imported = import(&glb, &Limits::default(), |_| None).expect("lean imports");
    let signed = package(imported, Platform::Desktop).encode_signed(&SigningKey::from_bytes(&[7; 32]));
    write("package_decode", "lean.civpkg", &signed);
    println!("seeds written to {}", dir.display());
}
