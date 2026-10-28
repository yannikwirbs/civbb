//! Bone dynamics: a PhysBone-compatible XPBD chain solver with grab and pose (design doc §7, ADR 0015).

pub mod math;
pub mod rig;
pub mod solver;

pub use rig::Rig;
pub use solver::{ChainParams, State};
