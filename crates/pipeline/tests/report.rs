//! The per-avatar report (ADR 0032): the lean corpus avatar's counts per stage against a golden JSON, the package
//! size against the signed bytes, and the Markdown with and without timings. Set `CIVBB_BLESS=1` to rewrite the
//! golden after an intended change.

use std::path::Path;

use civbb_format::cost::{CpuTimes, GpuTimes, Measured};
use civbb_format::package::Platform;
use civbb_format::{Package, ReadLimits, SigningKey};
use civbb_pipeline::{Imported, Report, compile};

const KEY: [u8; 32] = [7; 32];

fn lean() -> (Vec<u8>, Report) {
    let imported = Imported {
        avatar: civbb_corpus::generate(&civbb_corpus::lean()),
        blobs: Default::default(),
        warnings: Vec::new(),
    };
    compile(imported, Platform::Desktop, &SigningKey::from_bytes(&KEY), |_| {}).unwrap()
}

#[test]
fn lean_counts_match_the_golden() {
    let (bytes, report) = lean();
    assert_eq!(report.package_bytes, bytes.len());
    let key = SigningKey::from_bytes(&KEY).verifying_key();
    Package::decode_verified(&bytes, &key, &ReadLimits::default()).unwrap();
    assert_eq!(
        report.stages.iter().map(|s| s.stage).collect::<Vec<_>>(),
        ["source", "prune", "atlas", "merge", "lod", "collapse", "compress"]
    );
    assert!(report.timings.is_none());

    let json = serde_json::to_string_pretty(&report).unwrap() + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/report-lean.json");
    if std::env::var_os("CIVBB_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &json).unwrap();
    }
    let golden = std::fs::read_to_string(&path).expect("golden missing: run with CIVBB_BLESS=1");
    let (got, want): (serde_json::Value, serde_json::Value) = (
        serde_json::from_str(&json).unwrap(),
        serde_json::from_str(&golden).unwrap(),
    );
    assert!(got == want, "report differs from {}:\n{json}", path.display());
}

#[test]
fn markdown_has_a_column_per_stage_and_the_timings() {
    let (_, mut report) = lean();
    let md = report.markdown();
    assert!(
        md.contains("| | source | prune | atlas | merge | lod | collapse | compress | after |"),
        "{md}"
    );
    assert!(md.contains("| LOD 1 triangles |"), "{md}");
    assert!(md.contains("Not measured"), "{md}");

    report.timings = Some(Measured {
        profile: "release".into(),
        cpu: CpuTimes {
            avatars: 100,
            threads: 1,
            ms_per_avatar: 0.0123,
            systems_ms_per_avatar: [("dynamics".to_owned(), 0.01)].into(),
            ..CpuTimes::default()
        },
        gpu: Some(GpuTimes {
            device: "Test GPU".into(),
            gpu_ms_per_avatar: 0.0045,
            ..GpuTimes::default()
        }),
    });
    let md = report.markdown();
    assert!(md.contains("| **per avatar** | **0.0123** |"), "{md}");
    assert!(md.contains("| per avatar: dynamics | 0.0100 |"), "{md}");
    assert!(md.contains("## GPU (Test GPU)"), "{md}");
    assert!(md.contains("| **GPU per avatar** | **0.0045** |"), "{md}");
}
