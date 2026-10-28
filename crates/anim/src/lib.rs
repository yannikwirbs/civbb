//! Animation runtime: compile-time curve binding, clip sampling, Mecanim state machines and blend trees (design doc §7).

pub mod bind;
pub mod clip;
pub mod controller;
pub mod curve;
pub mod machine;
mod mix;
pub mod motion;

pub use bind::{Bindings, Bound, Slot, Target, Unbound, bind};
pub use clip::BoundClip;
pub use controller::{Controller, Instance};
