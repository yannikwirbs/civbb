//! Animation runtime: curve binding, clip sampling, Mecanim state machines, blend trees, muscles (design doc §7).

pub mod behaviour;
pub mod bind;
pub mod clip;
pub mod controller;
pub mod curve;
pub mod machine;
mod mix;
pub mod motion;
pub mod muscle;

pub use behaviour::Builtin;
pub use bind::{Bindings, Bound, Slot, Target, Unbound, bind};
pub use clip::BoundClip;
pub use controller::{Controller, Instance};
pub use muscle::Rig;
