//! `civbb-client bench` end to end: a tiny run prints a JSON report with one run per worker count, of a corpus
//! avatar or a verified runtime package (#57); bad arguments print usage.

use std::process::Command;

use civbb_format::package::{ContentKind, Header, Platform};
use civbb_format::{BlobId, FORMAT_VERSION, Package, SigningKey};

fn client(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_civbb-client"))
        .args(args)
        .output()
        .expect("runs")
}

#[test]
fn bench_reports_every_worker_count_as_json() {
    let out = client(&[
        "bench",
        "--avatars",
        "3",
        "--threads",
        "1,2",
        "--frames",
        "4",
        "--warmup",
        "1",
        "--avatar",
        "lean",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON on stdout");
    assert_eq!(report["avatars"], 3);
    assert_eq!(report["avatar"], "lean");
    let runs = report["runs"].as_array().unwrap();
    assert_eq!(
        runs.iter().map(|r| r["threads"].as_u64().unwrap()).collect::<Vec<_>>(),
        [1, 2]
    );
    for r in runs {
        assert!(r["ms_per_frame"].as_f64().unwrap() > 0.0);
        let systems = r["cpu_ms_per_avatar"].as_object().unwrap();
        let names: Vec<&str> = systems.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            ["animator", "constraints", "contacts", "dynamics", "ik", "world"]
        );
    }
    assert_eq!(runs[0]["speedup"].as_f64(), Some(1.0));
}

#[test]
fn bench_runs_a_verified_package() {
    let dir = std::env::temp_dir().join(format!("civbb-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let package = Package {
        header: Header {
            format_version: FORMAT_VERSION,
            compiler: "test".into(),
            kind: ContentKind::Avatar,
            platform: Platform::Desktop,
            root: BlobId([0; 32]),
            cost: None,
        },
        blobs: Default::default(),
    }
    .with_avatar(&civbb_corpus::generate(&civbb_corpus::lean()));
    std::fs::write(
        dir.join("lean.civpkg"),
        package.encode_signed(&SigningKey::from_bytes(&[5; 32])),
    )
    .unwrap();
    std::fs::write(dir.join("key"), [5u8; 32]).unwrap();
    std::fs::write(dir.join("other"), [6u8; 32]).unwrap();
    let run = |key: &str| {
        let pkg = dir.join("lean.civpkg");
        let key = dir.join(key);
        Command::new(env!("CARGO_BIN_EXE_civbb-client"))
            .args([
                "bench",
                "--threads",
                "1",
                "--avatars",
                "2",
                "--frames",
                "2",
                "--warmup",
                "0",
            ])
            .arg("--package")
            .arg(&pkg)
            .arg("--key")
            .arg(&key)
            .output()
            .unwrap()
    };
    let out = run("key");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["avatar"], "lean");
    assert!(!run("other").status.success(), "a package signed by another key");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn bad_arguments_print_usage() {
    for args in [
        &["bench", "--threads", "0"][..],
        &["bench", "--avatars"],
        &["bench", "--fast", "1"],
        &["bench", "--package", "x.civpkg"],
        &["run"],
    ] {
        let out = client(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("usage"));
    }
    assert!(
        !client(&["bench", "--avatar", "nobody", "--frames", "1"])
            .status
            .success()
    );
}
