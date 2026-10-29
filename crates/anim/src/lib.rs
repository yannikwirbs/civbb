//! Animation runtime: curve binding, clip sampling, Mecanim state machines, blend trees, muscles and constraints (design
//! doc §7).

pub mod behaviour;
pub mod bind;
pub mod clip;
pub mod constraint;
pub mod controller;
pub mod curve;
pub mod machine;
mod mix;
pub mod motion;
pub mod muscle;

pub use behaviour::Builtin;
pub use bind::{Bindings, Bound, Slot, Target, Unbound, bind};
pub use clip::BoundClip;
pub use constraint::Constraints;
pub use controller::{Controller, Instance};
pub use muscle::Rig;
