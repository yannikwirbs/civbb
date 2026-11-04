//! Cost profile: counted and measured CPU, GPU, memory and network cost of content (design doc §8).

use serde::{Deserialize, Serialize};

/// The cost of one piece of content, produced by the compiler and used by the runtime governor.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CostProfile {
    /// CPU inputs.
    pub cpu: CpuCost,
    /// GPU inputs.
    pub gpu: GpuCost,
    /// Memory.
    pub memory: MemoryCost,
    /// Network.
    pub network: NetworkCost,
}

/// Counted CPU work plus the measured time on reference hardware.
#[allow(missing_docs)]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CpuCost {
    pub animator_layers: u32,
    pub animator_states: u32,
    pub animated_curves: u32,
    pub bones: u32,
    pub dynamic_chains: u32,
    pub dynamic_transforms: u32,
    pub colliders: u32,
    pub contacts: u32,
    pub constraints: u32,
    /// Measured microseconds per frame on reference hardware; `None` until measured.
    pub measured_us: Option<f32>,
}

/// Counted GPU work plus the measured time on reference hardware.
#[allow(missing_docs)]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GpuCost {
    pub skinned_vertices: u32,
    pub blend_shapes: u32,
    /// Triangles at each LOD level, highest detail first.
    pub triangles_per_lod: Vec<u32>,
    pub materials: u32,
    pub pipelines: u32,
    /// Measured microseconds per frame on reference hardware; `None` until measured.
    pub measured_us: Option<f32>,
}

/// Memory at full detail.
#[allow(missing_docs)]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryCost {
    pub vram_bytes: u64,
    pub ram_bytes: u64,
}

/// Network cost.
#[allow(missing_docs)]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NetworkCost {
    pub synced_bits: u32,
}

/// Times measured by running a package on this machine (`civbb-client measure`, ADR 0032).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Measured {
    /// `release`; debug builds refuse to measure.
    pub profile: String,
    /// The headless benchmark on one worker.
    pub cpu: CpuTimes,
    /// The viewer drawing offscreen; `None` without a Vulkan device.
    pub gpu: Option<GpuTimes>,
}

/// The benchmark's numbers for one avatar, stepped on one worker.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CpuTimes {
    /// Avatars stepped together.
    pub avatars: u32,
    /// Measured frames, after `warmup` frames.
    pub frames: u32,
    /// Warm-up frames.
    pub warmup: u32,
    /// Workers (1).
    pub threads: usize,
    /// Logical cores the OS reports.
    pub logical_cores: usize,
    /// Mean wall time per frame, milliseconds.
    pub ms_per_frame: f64,
    /// 99th percentile wall time per frame, milliseconds.
    pub ms_per_frame_p99: f64,
    /// Mean wall time per frame and avatar, milliseconds.
    pub ms_per_avatar: f64,
    /// CPU time per frame and avatar by system, milliseconds.
    pub systems_ms_per_avatar: std::collections::BTreeMap<String, f64>,
}

/// The viewer's numbers for copies of one avatar drawn offscreen.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GpuTimes {
    /// The device's name.
    pub device: String,
    /// Copies drawn.
    pub copies: u32,
    /// Measured frames, after the warm-up.
    pub frames: u32,
    /// Target size in pixels.
    pub width: u32,
    /// Target size in pixels.
    pub height: u32,
    /// Mean GPU time per frame from timestamps, milliseconds.
    pub gpu_ms_per_frame: f64,
    /// `gpu_ms_per_frame` divided by the copies.
    pub gpu_ms_per_avatar: f64,
    /// Mean CPU time per frame (simulation and draw preparation), milliseconds.
    pub cpu_ms_per_frame: f64,
    /// Mean CPU time per frame of the avatar systems alone, milliseconds.
    pub systems_ms_per_frame: f64,
    /// Draws in the last frame, all copies.
    pub draws: usize,
    /// Meshes skinned in the last frame, all copies.
    pub skinned_meshes: usize,
}

/// FLIP parity of a compiled avatar against reference images of the same states and views (ADR 0033).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Parity {
    /// What the reference images show: `source`, our renderer on the imported avatar before any compiler stage.
    pub reference: String,
    /// The camera rig both sides are rendered with.
    pub rig: Rig,
    /// FLIP's pixels per degree of visual angle.
    pub pixels_per_degree: f32,
    /// The traffic light's bounds.
    pub thresholds: Thresholds,
    /// Rest, then each parameter set away from its default on top of the state before.
    pub states: Vec<StateParity>,
    /// The worst state's light.
    pub light: Light,
}

/// Fixed cameras around an avatar, framed from its rest bounds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rig {
    /// Image size in pixels.
    pub width: u32,
    /// Image size in pixels.
    pub height: u32,
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// Background colour, linear RGB.
    pub background: [f32; 3],
    /// The views, in the order of each state's scores.
    pub views: Vec<RigView>,
}

/// One camera of the rig, avatar space (+Y up, the avatar faces +Z).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RigView {
    /// `front`, `left` and so on.
    pub name: String,
    /// Camera position.
    pub eye: [f32; 3],
    /// The point looked at.
    pub target: [f32; 3],
}

/// Traffic-light bounds on a view's FLIP error over the avatar's pixels: green when both the mean and the 99th
/// percentile are at most the green bounds, red when either is above the red bounds, yellow between.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Thresholds {
    pub green_mean: f32,
    pub green_p99: f32,
    pub red_mean: f32,
    pub red_p99: f32,
}

/// One state's scores.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StateParity {
    /// `rest`, or the parameter set in this state and its value (`Hat=1`).
    pub name: String,
    /// The worst view's light.
    pub light: Light,
    /// One score per rig view.
    pub views: Vec<ViewScore>,
}

/// FLIP error of one view over the pixels the avatar covers in either image.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ViewScore {
    /// Mean error, 0 (identical) to 1.
    pub mean: f32,
    /// 99th percentile.
    pub p99: f32,
    /// Largest error.
    pub max: f32,
    /// Pixels covered by the avatar in either image.
    pub pixels: u32,
}

/// A parity traffic light (design doc §10): green publishes, yellow publishes with a note, red needs a fix.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[allow(missing_docs)]
pub enum Light {
    #[default]
    Green,
    Yellow,
    Red,
}
