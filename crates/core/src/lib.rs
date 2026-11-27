//! Engine core: math, the frame schedule, the job runner for per-avatar islands and per-system timing counters
//! (design doc §5, ADRs 0020 and 0103).

pub mod counters;
pub mod jobs;
pub mod math;
pub mod mips;
pub mod profile;
pub mod schedule;

pub use counters::{Counters, System};
