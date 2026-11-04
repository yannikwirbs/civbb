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
