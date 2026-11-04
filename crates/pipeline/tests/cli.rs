//! `civbb` end to end: keygen, build (pruning, atlasing, merging, LOD chain, compression; ADRs 0026, 0027, 0029, 0031)
//! with its report (ADR 0032), validate and inspect on a corpus avatar, plus tamper and wrong-key rejection.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn civbb(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_civbb"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run civbb")
}

fn stdout(o: &Output) -> String {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("civbb-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn build_validate_inspect() {
    let dir = scratch("roundtrip");
    let glb = civbb_corpus::gltf::to_glb(&civbb_corpus::generate(&civbb_corpus::lean()));
    std::fs::write(dir.join("lean.glb"), glb).unwrap();

    assert!(stdout(&civbb(&dir, &["keygen", "dev.key"])).contains("public key"));
    assert!(
        !civbb(&dir, &["keygen", "dev.key"]).status.success(),
        "keygen overwrote a key"
    );
    // The GLB carries no animator or face, so nothing drives the lean avatar's 15 visemes or toggles its meshes:
    // all three merge, and the body and prop share a material; the merged mesh gets a LOD chain. It has no
    // textures to atlas or compress.
    let built = stdout(&civbb(
        &dir,
        &["build", "lean.glb", "--key", "dev.key", "--report", "lean.report.json"],
    ));
    assert!(
        built.starts_with(
            "pruned 0 nodes, 0 skin joints, 15 blendshapes\natlased 0 atlases, 0 fewer textures, 0 fewer materials\n\
             merged 2 meshes, 1 submeshes\nlod chain on 1 meshes: "
        ),
        "{built}"
    );
    assert!(
        built.contains("\ncollapsed 0 toggle layers\ncompressed 0 textures"),
        "{built}"
    );
    assert!(
        built.ends_with(
            "wrote lean.report.json and lean.report.md
"
        ),
        "{built}"
    );
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("lean.report.json")).unwrap()).unwrap();
    assert_eq!(report["report"], 1);
    assert_eq!(report["stages"].as_array().unwrap().len(), 7);
    assert_eq!(
        report["package_bytes"].as_u64(),
        Some(std::fs::metadata(dir.join("lean.civpkg")).unwrap().len())
    );
    assert!(report["timings"].is_null());
    let md = std::fs::read_to_string(dir.join("lean.report.md")).unwrap();
    assert!(
        md.starts_with(
            "# Report: lean
"
        ),
        "{md}"
    );
    assert!(stdout(&civbb(&dir, &["validate", "lean.civpkg", "--key", "dev.key"])).starts_with("ok"));

    let from_source = stdout(&civbb(&dir, &["inspect", "lean.glb"]));
    let from_package = stdout(&civbb(&dir, &["inspect", "lean.civpkg", "--key", "dev.key"]));
    assert!(from_source.contains("name        lean"), "{from_source}");
    assert!(from_package.contains("compiler    civbb-pipeline"), "{from_package}");
    let without_shapes = |s: &str| {
        s.lines()
            .filter(|l| !l.starts_with("shapes") && !l.starts_with("meshes"))
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    };
    assert!(
        without_shapes(&from_package).ends_with(&without_shapes(&from_source)),
        "package and source disagree"
    );
    assert!(
        from_package.contains("shapes      0 blend shapes, 0 deltas"),
        "{from_package}"
    );
    assert!(from_package.contains("meshes      1 meshes"), "{from_package}");

    stdout(&civbb(&dir, &["keygen", "other.key"]));
    assert!(
        !civbb(&dir, &["validate", "lean.civpkg", "--key", "other.key"])
            .status
            .success()
    );
    let mut bytes = std::fs::read(dir.join("lean.civpkg")).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(dir.join("tampered.civpkg"), bytes).unwrap();
    assert!(
        !civbb(&dir, &["validate", "tampered.civpkg", "--key", "dev.key"])
            .status
            .success()
    );

    assert_eq!(civbb(&dir, &["frobnicate"]).status.code(), Some(2));
    assert_eq!(
        civbb(&dir, &["build", "lean.glb"]).status.code(),
        Some(2),
        "build without a key"
    );
    assert_eq!(
        civbb(&dir, &["build", "lean.glb", "--key", "dev.key", "--measure"])
            .status
            .code(),
        Some(2),
        "measure without a report"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
