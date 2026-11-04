//! `civbb-client`: the client binary; in Phase 0 a headless benchmark of N avatar islands (`bench`), a desktop
//! viewer (`view`) and both run on a package for its report (`measure`).

use std::path::Path;
use std::process::ExitCode;

use civbb_client::bench::{self, Config};
use civbb_client::view;
use civbb_format::cost::{CpuTimes, GpuTimes, Measured};
use civbb_format::{Caps, Package, ReadLimits, SigningKey, validate};

/// The offscreen target `measure` draws to: the viewer window's default size.
const MEASURE_SIZE: (u32, u32) = (1280, 720);

const USAGE: &str = "usage:
  civbb-client bench [--avatars N] [--threads T[,T...]] [--frames F] [--warmup W]
                     [--avatar lean|bloated | --package <package.civpkg> --key <key-file>]
  civbb-client view [<package.civpkg> --key <key-file> | --avatar lean|bloated] [--copies N] [--frames F] [--warmup W]
                    [--lod L]
  civbb-client measure <package.civpkg> --key <key-file> [--avatars N] [--frames F] [--warmup W]

bench steps N avatars (default 100, bloated, or a verified runtime package) for W + F frames (default 60 + 300) at each worker count (default 1,
2, 4 ... up to the logical core count) and prints a JSON report to stdout. Build with --release for meaningful
numbers.

view shows N copies (default 1) of a runtime package, verified against the key that signed it (`civbb build`
makes one from glTF or VRM), or of a corpus avatar (default bloated), with animator and dynamics running. Drag to
orbit, scroll to zoom; CPU and GPU frame times are in the title. With --frames it exits after W + F frames
(default W 60; the first creates the swapchain) and prints a summary of the last F. --lod draws every mesh at level L of its LOD chain (0 is full, default; past a mesh's last level, its
last).

measure runs a package's avatar through bench on one worker (N avatars, default 100, W + F frames, default 60 +
300) and through the viewer's frames on an offscreen 1280x720 target (N copies, same frames) and prints the times
as JSON for `civbb build --measure` (ADR 0032). GPU times are null without a Vulkan device. Release builds only.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("view") => return run_view(&args[1..]),
        Some("measure") => return run_measure(&args[1..]),
        _ => {}
    }
    let Some((mut cfg, package)) = parse(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let avatar = match package {
        Some((package, key)) => match load_package(Path::new(&package), Path::new(&key)) {
            Ok((avatar, _)) => avatar,
            Err(e) => {
                eprintln!("civbb-client: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => match bench::avatar(&cfg.avatar) {
            Some(avatar) => avatar,
            None => {
                eprintln!("civbb-client: unknown avatar {:?}", cfg.avatar);
                return ExitCode::FAILURE;
            }
        },
    };
    cfg.avatar = avatar.name.clone();
    let Some(report) = bench::run(&cfg, &avatar) else {
        eprintln!("civbb-client: worker pool failed");
        return ExitCode::FAILURE;
    };
    for r in &report.runs {
        eprintln!(
            "{:>3} threads: {:8.3} ms/frame (p99 {:8.3}), {:.4} ms/avatar, speedup {:.2}",
            r.threads, r.ms_per_frame, r.ms_per_frame_p99, r.ms_per_avatar, r.speedup
        );
    }
    match serde_json::to_string_pretty(&report) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("civbb-client: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The benchmark's configuration and, if given, the package and key file to run instead of a corpus avatar.
fn parse(args: &[String]) -> Option<(Config, Option<(String, String)>)> {
    let (cmd, rest) = args.split_first()?;
    if cmd != "bench" {
        return None;
    }
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut cfg = Config {
        avatar: "bloated".into(),
        avatars: 100,
        threads: bench::doubling(cores),
        frames: 300,
        warmup: 60,
    };
    let (mut package, mut key) = (None, None);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it.next()?;
        match flag.as_str() {
            "--package" => package = Some(value.clone()),
            "--key" => key = Some(value.clone()),
            "--avatars" => cfg.avatars = value.parse().ok()?,
            "--threads" => {
                cfg.threads = value.split(',').map(|t| t.parse().ok()).collect::<Option<_>>()?;
                if cfg.threads.contains(&0) {
                    return None;
                }
            }
            "--frames" => cfg.frames = value.parse().ok()?,
            "--warmup" => cfg.warmup = value.parse().ok()?,
            "--avatar" => cfg.avatar = value.clone(),
            _ => return None,
        }
    }
    match (package, key) {
        (Some(p), Some(k)) => Some((cfg, Some((p, k)))),
        (None, None) => Some((cfg, None)),
        _ => None,
    }
}

fn run_view(args: &[String]) -> ExitCode {
    let (mut package, mut key, mut corpus, mut copies, mut frames, mut lod) = (None, None, None, 1, None, 0);
    let mut warmup = 60;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let parsed = match arg.as_str() {
            "--key" => it.next().map(|v| key = Some(v.clone())),
            "--avatar" => it.next().map(|v| corpus = Some(v.clone())),
            "--copies" => it.next().and_then(|v| v.parse().ok()).map(|n| copies = n),
            "--frames" => it.next().and_then(|v| v.parse().ok()).map(|n| frames = Some(n)),
            "--lod" => it.next().and_then(|v| v.parse().ok()).map(|n| lod = n),
            "--warmup" => it.next().and_then(|v| v.parse().ok()).map(|n| warmup = n),
            s if !s.starts_with('-') && package.is_none() => {
                package = Some(s.to_owned());
                Some(())
            }
            _ => None,
        };
        if parsed.is_none() || copies == 0 {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    }
    let loaded = match (package, key, corpus) {
        (Some(p), Some(k), None) => load_package(Path::new(&p), Path::new(&k)),
        (None, None, corpus) => match corpus.as_deref().unwrap_or("bloated") {
            "lean" => Ok((civbb_corpus::generate(&civbb_corpus::lean()), Vec::new())),
            "bloated" => Ok((civbb_corpus::generate(&civbb_corpus::bloated()), Vec::new())),
            other => Err(format!("unknown avatar {other:?}")),
        },
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let (mut avatar, textures) = match loaded {
        Ok(l) => l,
        Err(e) => {
            eprintln!("civbb-client: {e}");
            return ExitCode::FAILURE;
        }
    };
    if lod > 0 {
        avatar.meshes = avatar.meshes.iter().map(|m| m.level(lod)).collect();
        let triangles: usize = avatar.meshes.iter().map(|m| m.indices.len() / 3).sum();
        println!("LOD {lod}: {triangles} triangles");
    }
    let cfg = view::Config {
        avatar,
        textures,
        copies,
        frames,
        warmup,
    };
    match view::run(cfg) {
        Ok(s) => {
            if frames.is_some() {
                let gpu = s.gpu_ms.map_or("n/a".into(), |g| format!("{g:.3} ms"));
                println!(
                    "{} frames: CPU {:.3} ms/frame (avatar systems {:.3} ms), GPU {gpu}, {} draws, {} skinned meshes, {} validation errors",
                    s.frames, s.cpu_ms, s.systems_ms, s.draws, s.skinned, s.validation_errors
                );
            }
            if s.validation_errors > 0 {
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("civbb-client: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_measure(args: &[String]) -> ExitCode {
    let (mut package, mut key, mut avatars, mut frames, mut warmup) = (None, None, 100, 300, 60);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let parsed = match arg.as_str() {
            "--key" => it.next().map(|v| key = Some(v.clone())),
            "--avatars" => it.next().and_then(|v| v.parse().ok()).map(|n| avatars = n),
            "--frames" => it.next().and_then(|v| v.parse().ok()).map(|n| frames = n),
            "--warmup" => it.next().and_then(|v| v.parse().ok()).map(|n| warmup = n),
            s if !s.starts_with('-') && package.is_none() => {
                package = Some(s.to_owned());
                Some(())
            }
            _ => None,
        };
        if parsed.is_none() || avatars == 0 || frames == 0 {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    }
    let (Some(package), Some(key)) = (package, key) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if cfg!(debug_assertions) {
        eprintln!("civbb-client: measure needs a release build (debug CPU times are ~30x off)");
        return ExitCode::FAILURE;
    }
    match measure(Path::new(&package), Path::new(&key), avatars, frames, warmup) {
        Ok(m) => match serde_json::to_string_pretty(&m) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("civbb-client: {e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("civbb-client: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `bench` on one worker, then the viewer offscreen, on the package's avatar (ADR 0032).
fn measure(package: &Path, key: &Path, avatars: u32, frames: u32, warmup: u32) -> Result<Measured, String> {
    let (avatar, textures) = load_package(package, key)?;
    let cfg = Config {
        avatar: avatar.name.clone(),
        avatars,
        threads: vec![1],
        frames,
        warmup,
    };
    let report = bench::run(&cfg, &avatar).ok_or("worker pool failed")?;
    let run = &report.runs[0];
    let cpu = CpuTimes {
        avatars,
        frames,
        warmup,
        threads: run.threads,
        logical_cores: report.logical_cores,
        ms_per_frame: run.ms_per_frame,
        ms_per_frame_p99: run.ms_per_frame_p99,
        ms_per_avatar: run.ms_per_avatar,
        systems_ms_per_avatar: run.cpu_ms_per_avatar.iter().map(|(&k, &v)| (k.to_owned(), v)).collect(),
    };
    let (width, height) = MEASURE_SIZE;
    let view = view::Config {
        avatar,
        textures,
        copies: avatars,
        frames: Some(frames),
        warmup,
    };
    let gpu = match view::run_headless(view, width, height) {
        Ok((s, device)) => match s.gpu_ms {
            Some(gpu_ms) => Some(GpuTimes {
                device,
                copies: avatars,
                frames: s.frames,
                width,
                height,
                gpu_ms_per_frame: f64::from(gpu_ms),
                gpu_ms_per_avatar: f64::from(gpu_ms) / f64::from(avatars),
                cpu_ms_per_frame: f64::from(s.cpu_ms),
                systems_ms_per_frame: f64::from(s.systems_ms),
                draws: s.draws,
                skinned_meshes: s.skinned,
            }),
            None => {
                eprintln!("civbb-client: GPU not measured: {device} has no timestamps");
                None
            }
        },
        Err(e @ (civbb_render::Error::Load(_) | civbb_render::Error::NoDevice)) => {
            eprintln!("civbb-client: GPU not measured: {e}");
            None
        }
        Err(e) => return Err(e.to_string()),
    };
    Ok(Measured {
        profile: report.profile.into(),
        cpu,
        gpu,
    })
}

/// Reads, verifies and validates a package, and decodes its textures.
#[allow(clippy::type_complexity)]
fn load_package(path: &Path, key: &Path) -> Result<(civbb_format::Avatar, Vec<Option<view::TextureImage>>), String> {
    let seed: [u8; 32] = std::fs::read(key)
        .map_err(|e| format!("{}: {e}", key.display()))?
        .try_into()
        .map_err(|_| format!("{}: a key file holds exactly 32 bytes", key.display()))?;
    let limits = ReadLimits::default();
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let key = SigningKey::from_bytes(&seed).verifying_key();
    let pkg = Package::decode_verified(&bytes, &key, &limits).map_err(|e| e.to_string())?;
    let avatar = pkg.avatar(&limits).map_err(|e| e.to_string())?;
    validate(&avatar, &Caps::default(), |id| pkg.blob(id).is_some()).map_err(|errors| {
        let list: Vec<String> = errors.iter().map(ToString::to_string).collect();
        format!("invalid avatar: {}", list.join("; "))
    })?;
    let textures = view::decode_textures(&avatar, |id| pkg.blob(id));
    Ok((avatar, textures))
}
