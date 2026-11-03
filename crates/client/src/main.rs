//! `civbb-client`: the client binary; in Phase 0 a headless benchmark of N avatar islands (`bench`).

use std::process::ExitCode;

use civbb_client::bench::{self, Config};

const USAGE: &str = "usage:
  civbb-client bench [--avatars N] [--threads T[,T...]] [--frames F] [--warmup W] [--avatar lean|bloated]

Steps N avatars (default 100, bloated) for W + F frames (default 60 + 300) at each worker count (default 1, 2, 4 ...
up to the logical core count) and prints a JSON report to stdout. Build with --release for meaningful numbers.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
