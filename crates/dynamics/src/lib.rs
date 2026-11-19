//! Bone dynamics: PhysBone chains as the VRChat SDK steps them, with grab and pose, and contacts (design doc §7,
//! ADRs 0016, 0017, 0049).

pub mod contact;
pub mod rig;
pub mod solver;

pub use contact::{ContactRig, ContactState};
pub use rig::Rig;
pub use solver::{ChainParams, State};
