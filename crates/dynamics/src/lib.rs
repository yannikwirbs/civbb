//! Bone dynamics: a PhysBone-compatible XPBD chain solver with grab and pose, and contacts (design doc §7, ADRs 0015 to
//! 0017).

pub mod contact;
pub mod math;
pub mod rig;
pub mod solver;

pub use contact::{ContactRig, ContactState};
pub use rig::Rig;
pub use solver::{ChainParams, State};
