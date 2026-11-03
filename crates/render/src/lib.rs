//! Renderer: a Vulkan 1.3 bootstrap and a render graph of passes that declare their attachments (design doc §6, ADR 0021).
//!
//! [`Context`] owns the instance (with validation in debug builds), the chosen device and its queue. [`Renderer`]
//! adds a window's swapchain, recreated on resize, and two frames in flight; each frame the caller adds passes to a
//! [`Graph`], which plans the layout transitions between them and records them with dynamic rendering. The window
//! loop that drives it lives in the client.

mod context;
pub mod graph;
mod memory;
mod renderer;
pub mod skin;
mod swapchain;

pub use ash::{self, vk};
pub use context::{Candidate, Context, Window, pick_device};
pub use graph::{Graph, ImageId, Load, Pass, Use};
pub use memory::{Buffer, MemoryLocation};
pub use renderer::{FRAMES_IN_FLIGHT, FrameStatus, Renderer};
pub use swapchain::choose_format;

use std::fmt;

/// Why the renderer could not start or render.
#[derive(Debug)]
pub enum Error {
    /// The Vulkan loader is missing or could not be loaded.
    Load(String),
    /// No device supports Vulkan 1.3 with the needed features and a graphics queue (that can present).
    NoDevice,
    /// GPU memory could not be allocated.
    Memory(String),
    /// A Vulkan call failed.
    Vk(vk::Result),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Load(e) => write!(f, "no Vulkan loader: {e}"),
            Error::NoDevice => write!(f, "no Vulkan 1.3 device with the features the renderer needs"),
            Error::Memory(e) => write!(f, "GPU memory: {e}"),
            Error::Vk(r) => write!(f, "Vulkan error: {r}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<vk::Result> for Error {
    fn from(r: vk::Result) -> Self {
        Error::Vk(r)
    }
}

/// Result of renderer calls.
pub type Result<T> = std::result::Result<T, Error>;
