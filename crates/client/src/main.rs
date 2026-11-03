//! `civbb-client`: the client binary; in Phase 0 a headless benchmark of N avatar islands (`bench`) and a desktop
//! viewer (`view`).

use std::path::Path;
use std::process::ExitCode;

use civbb_client::bench::{self, Config};
use civbb_client::view;
use civbb_format::{Caps, Package, ReadLimits, SigningKey, validate};

const USAGE: &str = "usage:
  civbb-client bench [--avatars N] [--threads T[,T...]] [--frames F] [--warmup W] [--avatar lean|bloated]
  civbb-client view [<package.civpkg> --key <key-file> | --avatar lean|bloated] [--copies N] [--frames F]

bench steps N avatars (default 100, bloated) for W + F frames (default 60 + 300) at each worker count (default 1,
2, 4 ... up to the logical core count) and prints a JSON report to stdout. Build with --release for meaningful
numbers.

view shows N copies (default 1) of a runtime package, verified against the key that signed it (`civbb build`
makes one from glTF or VRM), or of a corpus avatar (default bloated), with animator and dynamics running. Drag to
orbit, scroll to zoom; CPU and GPU frame times are in the title. With --frames it exits after F frames and prints
a summary.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "view") {
        return run_view(&args[1..]);
    }
    let Some(cfg) = parse(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let Some(report) = bench::run(&cfg) else {
        eprintln!("civbb-client: unknown avatar {:?} or worker pool failed", cfg.avatar);
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

fn parse(args: &[String]) -> Option<Config> {
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
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it.next()?;
        match flag.as_str() {
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
    Some(cfg)
}

fn run_view(args: &[String]) -> ExitCode {
    let (mut package, mut key, mut corpus, mut copies, mut frames) = (None, None, None, 1, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let parsed = match arg.as_str() {
            "--key" => it.next().map(|v| key = Some(v.clone())),
            "--avatar" => it.next().map(|v| corpus = Some(v.clone())),
            "--copies" => it.next().and_then(|v| v.parse().ok()).map(|n| copies = n),
            "--frames" => it.next().and_then(|v| v.parse().ok()).map(|n| frames = Some(n)),
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
    let (avatar, textures) = match loaded {
        Ok(l) => l,
        Err(e) => {
            eprintln!("civbb-client: {e}");
            return ExitCode::FAILURE;
        }
    };
    let cfg = view::Config {
        avatar,
        textures,
        copies,
        frames,
    };
    match view::run(cfg) {
        Ok(s) => {
            if frames.is_some() {
                let gpu = s.gpu_ms.map_or("n/a".into(), |g| format!("{g:.3} ms"));
                println!(
                    "{} frames: CPU {:.3} ms/frame (avatar systems {:.3} ms), GPU {gpu}, {} validation errors",
                    s.frames, s.cpu_ms, s.systems_ms, s.validation_errors
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

/// Reads, verifies and validates a package, and decodes its textures.
#[allow(clippy::type_complexity)]
fn load_package(path: &Path, key: &Path) -> Result<(civbb_format::Avatar, Vec<Option<(u32, u32, Vec<u8>)>>), String> {
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
