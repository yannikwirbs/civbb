//! `civbb-client measure` (ADR 0032): debug builds refuse, release builds print CPU times and, with a device, GPU
//! times as JSON; and the viewer's frames run headless on a device without validation errors (skips without one).

use std::process::Command;

use civbb_client::view;
use civbb_format::cost::Measured;
use civbb_format::package::{ContentKind, Header, Platform};
use civbb_format::{BlobId, FORMAT_VERSION, Package, SigningKey};

#[test]
fn measure_prints_times_in_release_and_refuses_in_debug() {
    let dir = std::env::temp_dir().join(format!("civbb-measure-{}", std::process::id()));
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
    .with_avatar(&civbb_corpus::generate(&civbb_corpus::lean()));
    std::fs::write(
        dir.join("lean.civpkg"),
        package.encode_signed(&SigningKey::from_bytes(&seed)),
    )
    .unwrap();
    std::fs::write(dir.join("key"), seed).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_civbb-client"))
        .args([
            "measure",
            "lean.civpkg",
            "--key",
            "key",
            "--avatars",
            "2",
            "--frames",
            "5",
            "--warmup",
            "2",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
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
        if let Some(g) = m.gpu {
            assert_eq!((g.copies, g.frames, g.width, g.height), (2, 5, 1280, 720));
            assert!(g.gpu_ms_per_frame > 0.0 && g.draws > 0);
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn headless_frames_draw_without_validation_errors() {
    let cfg = view::Config {
        avatar: civbb_corpus::generate(&civbb_corpus::bloated()),
        textures: Vec::new(),
        copies: 2,
        frames: Some(4),
        warmup: 2,
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
