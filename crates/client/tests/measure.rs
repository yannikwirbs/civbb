//! `civbb-client measure` (ADR 0032): debug builds refuse, release builds print CPU times and, with a device, GPU
//! times as JSON; and the viewer's frames run headless on a device without validation errors (skips without one).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use civbb_client::package;
use civbb_client::view;
use civbb_format::cost::Measured;
use civbb_format::package::{ContentKind, Header, Platform};
use civbb_format::{BlobId, FORMAT_VERSION, Package, SigningKey};

/// A fresh directory holding `spec`'s avatar as `avatar.civpkg`, signed with the key in `key`.
fn package_dir(name: &str, spec: &civbb_corpus::Spec) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("civbb-measure-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let seed = [3u8; 32];
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
    .with_document(&civbb_corpus::generate(spec));
    std::fs::write(
        dir.join("avatar.civpkg"),
        package.encode_signed(&SigningKey::from_bytes(&seed)),
    )
    .unwrap();
    std::fs::write(dir.join("key"), seed).unwrap();
    dir
}

fn measure(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_civbb-client"))
        .args(["measure", "avatar.civpkg", "--key", "key"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

#[test]
fn measure_prints_times_in_release_and_refuses_in_debug() {
    let dir = package_dir("lean", &civbb_corpus::lean());
    let out = measure(&dir, &["--avatars", "2", "--frames", "5", "--warmup", "2"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if cfg!(debug_assertions) {
        assert!(!out.status.success());
        assert!(stderr.contains("needs a release build"), "{stderr}");
    } else {
        assert!(out.status.success(), "{stderr}");
        let m: Measured = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(m.profile, "release");
        assert_eq!((m.cpu.avatars, m.cpu.threads, m.cpu.frames), (2, 1, 5));
        assert!(m.cpu.ms_per_avatar > 0.0);
        assert!(m.cpu.systems_ms_per_avatar.contains_key("animator"));
        if cfg!(windows) {
            let p = m.cpu.pinned.expect("Windows pins the worker (ADR 0080)");
            assert_eq!(p.ran_on, p.logical_cpu);
        }
        if let Some(g) = m.gpu {
            assert_eq!((g.copies, g.frames, g.width, g.height), (2, 5, 1280, 720));
            assert!(g.gpu_ms_per_frame > 0.0 && g.draws > 0);
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// ADR 0080: pinned, consecutive runs of one package agree. Unpinned they differed by up to 1.8x on the reference
/// PC (#103).
#[test]
#[ignore = "two full release measure runs (about 15 s): run alone with --release --ignored"]
fn two_pinned_runs_agree_within_ten_percent() {
    if cfg!(debug_assertions) {
        panic!("timings need --release");
    }
    let dir = package_dir("repeat", &civbb_corpus::bloated());
    let runs: Vec<Measured> = (0..2)
        .map(|_| {
            let out = measure(&dir, &[]);
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            serde_json::from_slice(&out.stdout).unwrap()
        })
        .collect();
    let (a, b) = (runs[0].cpu.ms_per_avatar, runs[1].cpu.ms_per_avatar);
    eprintln!("ms per avatar: {a:.4}, {b:.4}; pinned {:?}", runs[0].cpu.pinned);
    assert!((a - b).abs() <= 0.1 * a.min(b), "{a} against {b}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn headless_frames_draw_two_packages_in_the_test_world_without_validation_errors() {
    let load = |spec: civbb_corpus::Spec| {
        package::spawn(move || Ok(package::Package::new(civbb_corpus::generate(&spec), vec![])))
    };
    let cfg = view::Config {
        packages: vec![load(civbb_corpus::lean()), load(civbb_corpus::bloated())],
        copies: 2,
        frames: Some(4),
        warmup: 2,
        world: Some(package::Package::new(civbb_corpus::world(), vec![])),
        stations: Vec::new(),
        trackers: None,
        turning: Default::default(),
    };
    match view::run_headless(cfg, 320, 180) {
        Ok((s, device)) => {
            assert!(!device.is_empty());
            assert_eq!(s.frames, 4);
            assert_eq!(s.validation_errors, 0);
            assert!(s.draws > 0 && s.skinned > 0);
        }
        Err(e @ (civbb_render::Error::Load(_) | civbb_render::Error::NoDevice)) => eprintln!("skipped: {e}"),
        Err(e) => panic!("{e}"),
    }
}
