//! Client: wires the avatar systems into per-avatar islands and runs simulation frames (design doc §5, ADR 0020),
//! shows them in a desktop viewer (ADR 0024) and scores parity renders (ADR 0033).

pub mod affinity;
pub mod bench;
pub mod controller;
pub mod cull;
pub mod extract;
pub mod frame;
mod gpu;
pub mod island;
pub mod package;
#[cfg(feature = "parity")]
pub mod parity;
pub mod player;
mod render_thread;
pub mod station;
pub mod tracking;
pub mod view;

pub use island::{Island, Model};
