//! Retained-mode UI drawn by the renderer as a screen-space overlay (design doc §4, ADR 0117).
//!
//! Engine crate: depends only on `civbb-core` and `civbb-format`. Widgets keep their state, read input and
//! parameter values from the caller and emit a [`draw::DrawList`]; they never touch the GPU or the animator.

pub mod draw;
pub mod radial;
pub mod text;
