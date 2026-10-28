//! `civbb` end to end: keygen, build, validate and inspect on a corpus avatar, plus tamper and wrong-key rejection.

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
    stdout(&civbb(&dir, &["build", "lean.glb", "--key", "dev.key"]));
    assert!(stdout(&civbb(&dir, &["validate", "lean.civpkg", "--key", "dev.key"])).starts_with("ok"));

    let from_source = stdout(&civbb(&dir, &["inspect", "lean.glb"]));
    let from_package = stdout(&civbb(&dir, &["inspect", "lean.civpkg", "--key", "dev.key"]));
    assert!(from_source.contains("name        lean"), "{from_source}");
    assert!(from_package.contains("compiler    civbb-pipeline"), "{from_package}");
    assert!(from_package.ends_with(&from_source), "package and source disagree");

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
    std::fs::remove_dir_all(&dir).unwrap();
}
