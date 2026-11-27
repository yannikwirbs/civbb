//! `civbb-client`: the client binary; in Phase 0 a headless benchmark of N avatar islands (`bench`), a desktop
//! viewer (`view`), both run on a package for its report (`measure`), and FLIP parity of two packages (`parity`).

use std::path::Path;
use std::process::ExitCode;

use civbb_client::bench::{self, Config};
use civbb_client::gamepad::{self, Turning};
use civbb_client::package::{self, Package, read as load_package};
use civbb_client::station::Station;
use civbb_client::tracking::Scripted;
use civbb_client::{parity, view};
use civbb_format::cost::{CpuTimes, GpuTimes, Measured};

/// The offscreen target `measure` draws to: the viewer window's default size.
const MEASURE_SIZE: (u32, u32) = (1280, 720);

const USAGE: &str = "usage:
  civbb-client bench [--avatars N] [--threads T[,T...]] [--frames F] [--warmup W]
                     [--avatar lean|bloated | --package <package.civpkg> --key <key-file>]
  civbb-client view [<package.civpkg>... --key <key-file> | --avatar lean|bloated] [--copies N] [--frames F]
                    [--warmup W] [--lod L] [--world] [--trackers 3|6|10] [--turn smooth|snap]
  civbb-client measure <package.civpkg> --key <key-file> [--avatars N] [--frames F] [--warmup W]
  civbb-client parity <reference.civpkg | unity-dir> <test.civpkg> --key <key-file> [--images <dir>]
  civbb-client rig <package.civpkg> --key <key-file>

bench steps N avatars (default 100, bloated, or a verified runtime package) for W + F frames (default 60 + 300) at each worker count (default 1,
2, 4 ... up to the logical core count) and prints a JSON report to stdout. Build with --release for meaningful
numbers.

view shows N copies (default 1) of each runtime package, verified against the key that signed them (`civbb build`
makes one from glTF or VRM), or of a corpus avatar (default bloated), with animator and dynamics running. Packages
load in the background and appear as they are ready. Drag to
orbit, scroll to zoom; CPU and GPU frame times are in the title. With --frames it exits after W + F frames
(default W 60; the first creates the swapchain) and prints a summary of the last F. --lod draws every mesh at level L of its LOD chain (0 is full, default; past a mesh's last level, its
last). --world shows the generated test world (ground, stairs, ramps, a platform, a wall) and the player walks
through it (ADR 0110); E sits on its bench and gets up again, = and - scale the avatar (ADR 0114). --trackers drives the player's avatar with scripted 3-, 6- or 10-point trackers, calibrated on
the first frame (ADR 0113). A gamepad moves (left stick, click to run), turns and looks (right stick; --turn
smooth, the default, or snap in 30-degree steps), jumps (A), crouches (B), lies prone (Y), uses the bench (X) and
cycles the camera (View/Back) alongside keyboard and mouse (ADR 0115).

measure runs a package's avatar through bench on one worker (N avatars, default 100, W + F frames, default 60 +
300) and through the viewer's frames on an offscreen 1280x720 target (N copies, same frames) and prints the times
as JSON for `civbb build --measure` (ADR 0032). The bench worker is pinned to a performance core where the OS
allows it (ADR 0080; cpu.pinned says which). GPU times are null without a Vulkan device. Release builds only.

parity renders both packages' avatars with the reference's camera rig in every state (rest, then each parameter
set in turn), compares each view with NVIDIA FLIP and prints the scores and traffic lights as JSON (ADR 0033).
--images writes reference, test and error map side by side for the rest state and every state that is not green.
Prints null without a Vulkan device. A directory as the reference holds Unity's renders, <state>-<view>.png, and
the rig.json they were rendered from (ADR 0048).

rig prints the hand-off for Unity reference renders of a package's avatar as JSON: camera rig, light, frame step,
parameters and states (ADR 0048).";

fn main() -> ExitCode {
    // With the `profile` feature: Tracy zones from here on (#136).
    civbb_core::profile::start();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("view") => return run_view(&args[1..]),
        Some("measure") => return run_measure(&args[1..]),
        Some("parity") => return run_parity(&args[1..]),
        Some("rig") => return run_rig(&args[1..]),
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
        pin: false,
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
    let (mut packages, mut key, mut corpus, mut copies, mut frames, mut lod) = (Vec::new(), None, None, 1, None, 0);
    let mut warmup = 60;
    let mut world = false;
    let mut trackers = None;
    let mut turning = Turning::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let parsed = match arg.as_str() {
            "--key" => it.next().map(|v| key = Some(v.clone())),
            "--avatar" => it.next().map(|v| corpus = Some(v.clone())),
            "--copies" => it.next().and_then(|v| v.parse().ok()).map(|n| copies = n),
            "--frames" => it.next().and_then(|v| v.parse().ok()).map(|n| frames = Some(n)),
            "--lod" => it.next().and_then(|v| v.parse().ok()).map(|n| lod = n),
            "--warmup" => it.next().and_then(|v| v.parse().ok()).map(|n| warmup = n),
            "--world" => {
                world = true;
                Some(())
            }
            "--trackers" => (it.next().and_then(|v| v.parse().ok()))
                .and_then(Scripted::new)
                .map(|s| trackers = Some(s)),
            "--turn" => (match it.next().map(String::as_str) {
                Some("smooth") => Some(Turning::Smooth(gamepad::SMOOTH)),
                Some("snap") => Some(Turning::Snap(gamepad::SNAP)),
                _ => None,
            })
            .map(|t| turning = t),
            s if !s.starts_with('-') => {
                packages.push(s.to_owned());
                Some(())
            }
            _ => None,
        };
        if parsed.is_none() || copies == 0 {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    }
    // Each package loads on the I/O pool while the window opens and frames run (ADR 0104).
    let loads = match (packages.is_empty(), key, corpus) {
        (false, Some(key), None) => (packages.into_iter())
            .map(|p| {
                let key = key.clone();
                package::spawn(move || {
                    let (mut avatar, textures) = load_package(Path::new(&p), Path::new(&key))?;
                    // Built-in locomotion and proxy substitutes (ADR 0108); parity and `measure` show packages as built.
                    civbb_anim::defaults::apply(&mut avatar);
                    if lod > 0 {
                        avatar.meshes = avatar.meshes.iter().map(|m| m.level(lod)).collect();
                        let triangles: usize = avatar.meshes.iter().map(|m| m.indices.len() / 3).sum();
                        println!("{}: LOD {lod}, {triangles} triangles", avatar.name);
                    }
                    Ok(Package::new(avatar, textures))
                })
            })
            .collect(),
        (true, None, corpus) => {
            let avatar = match corpus.as_deref().unwrap_or("bloated") {
                "lean" => civbb_corpus::lean(),
                "bloated" => civbb_corpus::bloated(),
                other => {
                    eprintln!("civbb-client: unknown avatar {other:?}");
                    return ExitCode::FAILURE;
                }
            };
            vec![package::spawn(move || {
                Ok(Package::new(civbb_corpus::generate(&avatar), Vec::new()))
            })]
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let cfg = view::Config {
        packages: loads,
        copies,
        frames,
        warmup,
        world: world.then(|| Package::new(civbb_corpus::world(), Vec::new())),
        stations: if world { vec![test_station()] } else { Vec::new() },
        trackers,
        turning,
    };
    match view::run(cfg) {
        Ok(s) => {
            if frames.is_some() {
                let gpu = s.gpu_ms.map_or("n/a".into(), |g| format!("{g:.3} ms"));
                println!(
                    "{} frames: CPU {:.3} ms/frame (simulation {:.3} ms, schedule {:.3} ms, render thread {:.3} ms), GPU {gpu}, {} draws, {} skinned meshes, {} validation errors",
                    s.frames, s.cpu_ms, s.main_ms, s.systems_ms, s.render_ms, s.draws, s.skinned, s.validation_errors
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

/// The test world's bench (ADR 0114).
fn test_station() -> Station {
    use civbb_corpus::world::{EXIT, SEAT};
    Station {
        seat: SEAT,
        yaw: 0.0,
        exit: EXIT,
        seated: true,
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

fn run_rig(args: &[String]) -> ExitCode {
    let [package, flag, key] = args else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if flag != "--key" {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let json = load_package(Path::new(package), Path::new(key))
        .and_then(|(avatar, _)| serde_json::to_string_pretty(&parity::handoff(&avatar)).map_err(|e| e.to_string()));
    match json {
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

fn run_parity(args: &[String]) -> ExitCode {
    let (mut packages, mut key, mut images) = (Vec::new(), None, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let parsed = match arg.as_str() {
            "--key" => it.next().map(|v| key = Some(v.clone())),
            "--images" => it.next().map(|v| images = Some(v.clone())),
            s if !s.starts_with('-') && packages.len() < 2 => {
                packages.push(s.to_owned());
                Some(())
            }
            _ => None,
        };
        if parsed.is_none() {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    }
    let (Some(key), [reference, test]) = (key, packages.as_slice()) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let dir = Path::new(reference);
    // A directory holds Unity's renders and the hand-off they used (ADR 0048).
    let unity = dir.is_dir();
    let loaded = (|| {
        let handoff = if unity {
            let path = dir.join("rig.json");
            let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            Some(serde_json::from_str::<parity::Handoff>(&text).map_err(|e| format!("{}: {e}", path.display()))?)
        } else {
            None
        };
        let source = if unity {
            None
        } else {
            Some(load_package(dir, Path::new(&key))?)
        };
        Ok::<_, String>((handoff, source, load_package(Path::new(test), Path::new(&key))?))
    })();
    let (handoff, source, (ta, tt)) = match loaded {
        Ok(l) => l,
        Err(e) => {
            eprintln!("civbb-client: {e}");
            return ExitCode::FAILURE;
        }
    };
    let reference = match (&handoff, &source) {
        (Some(handoff), _) => parity::Reference::Unity { handoff, dir },
        (_, Some((ra, rt))) => parity::Reference::Package(parity::Side {
            avatar: ra,
            textures: rt,
        }),
        _ => unreachable!("one of them is loaded"),
    };
    let test = parity::Side {
        avatar: &ta,
        textures: &tt,
    };
    let result = match parity::run(reference, test, images.as_deref().map(Path::new)) {
        Ok(p) => Some(p),
        Err(e @ parity::Error::Render(civbb_render::Error::Load(_) | civbb_render::Error::NoDevice)) => {
            eprintln!("civbb-client: parity not rendered: {e}");
            None
        }
        Err(e) => {
            eprintln!("civbb-client: {e}");
            return ExitCode::FAILURE;
        }
    };
    match serde_json::to_string_pretty(&result) {
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

/// `bench` on one worker, then the viewer offscreen, on the package's avatar (ADR 0032).
fn measure(package: &Path, key: &Path, avatars: u32, frames: u32, warmup: u32) -> Result<Measured, String> {
    let (avatar, textures) = load_package(package, key)?;
    let cfg = Config {
        avatar: avatar.name.clone(),
        avatars,
        threads: vec![1],
        frames,
        warmup,
        pin: true,
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
        pinned: run.pinned.clone(),
    };
    let (width, height) = MEASURE_SIZE;
    let handle = Package::new(avatar, textures);
    let view = view::Config {
        packages: vec![package::spawn(move || Ok(handle))],
        copies: avatars,
        frames: Some(frames),
        warmup,
        world: None,
        stations: Vec::new(),
        trackers: None,
        turning: Turning::default(),
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
