//! `civbb-client bench` end to end: a tiny run prints a JSON report with one run per worker count; bad arguments
//! print usage.

use std::process::Command;

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
fn bad_arguments_print_usage() {
    for args in [
        &["bench", "--threads", "0"][..],
        &["bench", "--avatars"],
        &["bench", "--fast", "1"],
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
