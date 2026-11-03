//! Headless CPU benchmark: N avatar islands stepped for a number of frames at each worker count (design doc §21).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use civbb_core::math::Pose;
use civbb_core::{Counters, System, jobs};
use civbb_format::Avatar;
use serde::Serialize;

use crate::frame;
use crate::island::{Island, Model};

/// Simulation step, seconds: one frame at the 90 Hz target of design doc §1.
pub const DT: f32 = 1.0 / 90.0;
/// Frames between parameter flips, so animator transitions keep firing.
const FLIP_EVERY: u32 = 30;

/// What to run.
#[derive(Clone, Debug)]
pub struct Config {
    /// Corpus avatar every island shows: `lean` or `bloated`.
    pub avatar: String,
    /// Number of avatars.
    pub avatars: u32,
    /// Worker counts to measure, in order.
    pub threads: Vec<usize>,
    /// Measured frames per worker count.
    pub frames: u32,
    /// Frames run before measuring, per worker count.
    pub warmup: u32,
}

/// The benchmark's result, written as JSON.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    /// Corpus avatar.
    pub avatar: String,
    /// Number of avatars.
    pub avatars: u32,
    /// Measured frames per run.
    pub frames: u32,
    /// Warm-up frames per run.
    pub warmup: u32,
    /// Simulation step, seconds.
    pub dt: f32,
    /// Logical cores the OS reports.
    pub logical_cores: usize,
    /// `release` or `debug`; debug numbers are not meaningful.
    pub profile: &'static str,
    /// One run per worker count.
    pub runs: Vec<Run>,
}

/// One worker count's measurements.
#[derive(Clone, Debug, Serialize)]
pub struct Run {
    /// Workers.
    pub threads: usize,
    /// Mean wall time per frame, milliseconds.
    pub ms_per_frame: f64,
    /// Median wall time per frame, milliseconds.
    pub ms_per_frame_p50: f64,
    /// 99th percentile wall time per frame, milliseconds.
    pub ms_per_frame_p99: f64,
    /// Mean wall time per frame divided by the number of avatars, milliseconds.
    pub ms_per_avatar: f64,
    /// The first run's mean frame time over this one's (the first run is 1 worker unless configured otherwise).
    pub speedup: f64,
    /// Mean CPU time per frame and avatar by system, milliseconds (the contact pass, which runs once over all
    /// avatars, is divided by the avatar count too).
    pub cpu_ms_per_avatar: BTreeMap<&'static str, f64>,
}

/// The corpus avatar called `name`.
pub fn avatar(name: &str) -> Option<Avatar> {
    let spec = match name {
        "lean" => civbb_corpus::lean(),
        "bloated" => civbb_corpus::bloated(),
        _ => return None,
    };
    Some(civbb_corpus::generate(&spec))
}

/// 1, 2, 4 ... up to `max`, and `max` itself.
pub fn doubling(max: usize) -> Vec<usize> {
    let mut v: Vec<usize> = std::iter::successors(Some(1), |&t| Some(t * 2))
        .take_while(|&t| t < max)
        .collect();
    v.push(max.max(1));
    v
}

/// Runs the benchmark; `None` if the avatar name is unknown or a worker pool cannot be built.
pub fn run(cfg: &Config) -> Option<Report> {
    let model = Arc::new(Model::new(&avatar(&cfg.avatar)?));
    let mut runs: Vec<Run> = Vec::with_capacity(cfg.threads.len());
    for &threads in &cfg.threads {
        let pool = jobs::pool(threads).ok()?;
        let (mut frame_ms, counters) = pool.install(|| measure(&model, cfg));
        frame_ms.sort_by(f64::total_cmp);
        let n = f64::from(cfg.frames.max(1));
        let mean = frame_ms.iter().sum::<f64>() / n;
        let pick = |q: f64| {
            frame_ms
                .get(((frame_ms.len() as f64 - 1.0) * q).round() as usize)
                .copied()
        };
        let per_avatar = n * f64::from(cfg.avatars.max(1));
        let reference = runs.first().map_or(mean, |r| r.ms_per_frame);
        runs.push(Run {
            threads,
            ms_per_frame: mean,
            ms_per_frame_p50: pick(0.5).unwrap_or(0.0),
            ms_per_frame_p99: pick(0.99).unwrap_or(0.0),
            ms_per_avatar: mean / f64::from(cfg.avatars.max(1)),
            speedup: if mean > 0.0 { reference / mean } else { 0.0 },
            cpu_ms_per_avatar: System::ALL
                .iter()
                .map(|&s| (s.name(), counters.seconds(s) * 1e3 / per_avatar))
                .collect(),
        });
    }
    Some(Report {
        avatar: cfg.avatar.clone(),
        avatars: cfg.avatars,
        frames: cfg.frames,
        warmup: cfg.warmup,
        dt: DT,
        logical_cores: std::thread::available_parallelism().map_or(1, |n| n.get()),
        profile: if cfg!(debug_assertions) { "debug" } else { "release" },
        runs,
    })
}

/// Fresh islands on a square grid 1 m apart, so neighbours share contact cells; avatar 0 is the local one.
fn scene(model: &Arc<Model>, avatars: u32) -> Vec<Island> {
    let side = f64::from(avatars).sqrt().ceil().max(1.0) as u32;
    (0..avatars)
        .map(|id| {
            let origin = Pose {
                pos: [(id % side) as f32, 0.0, (id / side) as f32],
                ..Pose::IDENTITY
            };
            Island::new(Arc::clone(model), u64::from(id), id == 0, origin)
        })
        .collect()
}

/// Warm-up then measured frames on a fresh scene; returns each measured frame's wall time in milliseconds and the
/// summed counters. Each avatar sways sideways so its chains keep moving, and flips one parameter every
/// [`FLIP_EVERY`] frames.
fn measure(model: &Arc<Model>, cfg: &Config) -> (Vec<f64>, Counters) {
    let mut islands = scene(model, cfg.avatars);
    let homes: Vec<Pose> = islands.iter().map(|i| i.origin).collect();
    let params = model.controller().map_or(0, |c| c.animator.parameters.len());
    let mut frame_ms = Vec::with_capacity(cfg.frames as usize);
    let mut total = Counters::default();
    for f in 0..cfg.warmup + cfg.frames {
        let t = f as f32 * DT;
        for (k, island) in islands.iter_mut().enumerate() {
            island.origin.pos[0] = homes[k].pos[0] + 0.1 * (t * 3.0 + k as f32).sin();
            if params > 0 && f % FLIP_EVERY == 0 {
                let p = (f / FLIP_EVERY) as usize % params;
                if let Some(a) = island.animator_mut() {
                    a.params[p] = 1.0 - a.params[p];
                }
            }
        }
        let start = Instant::now();
        let c = frame::step(&mut islands, |_, _| true, DT);
        let ms = start.elapsed().as_secs_f64() * 1e3;
        if f >= cfg.warmup {
            frame_ms.push(ms);
            total.merge(&c);
        }
    }
    (frame_ms, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubling_ends_at_the_core_count() {
        assert_eq!(doubling(1), [1]);
        assert_eq!(doubling(6), [1, 2, 4, 6]);
        assert_eq!(doubling(16), [1, 2, 4, 8, 16]);
    }
}
