//! Client: wires the avatar systems into per-avatar islands and runs simulation frames (design doc §5, ADR 0020),
//! and shows them in a desktop viewer (ADR 0024).

pub mod bench;
pub mod frame;
pub mod island;
pub mod view;

pub use island::{Island, Model};
