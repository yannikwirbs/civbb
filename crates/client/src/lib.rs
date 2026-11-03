//! Client: wires the avatar systems into per-avatar islands and runs simulation frames (design doc §5, ADR 0020).

pub mod frame;
pub mod island;

pub use island::{Island, Model};
