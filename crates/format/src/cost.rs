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
