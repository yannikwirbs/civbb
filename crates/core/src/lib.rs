//! Engine core: math, the job runner for per-avatar islands and per-system timing counters (design doc §5, ADR 0020).

pub mod counters;
pub mod jobs;
pub mod math;

pub use counters::{Counters, System};
