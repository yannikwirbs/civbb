//! Compute skinning and sparse blendshapes: once per mesh per frame, applying only shapes with non-zero weight
//! (design doc §6, ADR 0022).

use ash::vk;
use civbb_format::math::Mat4;
use civbb_format::mesh::Mesh;

use crate::memory::bytes;
use crate::{Buffer, Context, MemoryLocation, Result};

const SPIRV: &[u8] = include_bytes!("../shaders/spv/skin.spv");
/// Fixed-point units per metre in the blendshape accumulator: 2^20, about a micrometre, range ±2 km.
pub const SCALE: f32 = 1_048_576.0;
const GROUP: u32 = 64;
const PUSH_SIZE: u32 = 64;
/// `vkCmdUpdateBuffer`'s limit per call.
const UPDATE_LIMIT: usize = 65_536;

/// A shape with non-zero weight as the morph pass reads it: deltas `first .. first + count`, threads
/// `start .. start + count`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Active {
    /// First delta in the mesh's delta buffer.
    pub first: u32,
    /// Number of deltas.
    pub count: u32,
    /// First thread of the morph dispatch working on it.
    pub start: u32,
    /// Its weight.
    pub weight: f32,
}

/// Lists the shapes to apply into `out`: those with non-zero weight and at least one delta. `shapes` holds each
/// shape's first delta and delta count. Returns the number of threads (deltas) the morph pass runs.
pub fn active_shapes(shapes: &[(u32, u32)], weights: &[f32], out: &mut Vec<Active>) -> u32 {
    out.clear();
    let mut start = 0;
    for (&(first, count), &weight) in shapes.iter().zip(weights) {
        if weight != 0.0 && count > 0 {
            out.push(Active {
                first,
                count,
                start,
                weight,
            });
            start += count;
        }
    }
    start
}

/// One mesh's GPU data: rest vertices and shape deltas uploaded once, per-frame joints and active shapes, the
/// blendshape accumulator and the skinned output.
pub struct SkinnedMesh {
    vertices: Buffer,
    deltas: Buffer,
    shapes: Vec<(u32, u32)>,
    joints: Buffer,
    active: Buffer,
    accum: Buffer,
    output: Buffer,
    vertex_count: u32,
    joint_count: u32,
}

impl SkinnedMesh {
    /// Uploads a mesh. A mesh without a skin is skinned to one joint: pass its node's world matrix.
    ///
    /// # Panics
    ///
    /// If a joint or delta index is out of range ([`civbb_format::validate`] rules them out).
    pub fn new(ctx: &Context, mesh: &Mesh) -> Result<Self> {
        let n = mesh.positions.len();
        let joint_count = mesh.skin.as_ref().map_or(1, |s| s.joints.len().max(1));
        let mut words = Vec::with_capacity(n * 20);
        for v in 0..n {
            let p = mesh.positions[v];
            let nrm = mesh.normals.get(v).copied().unwrap_or_default();
            let t = mesh.tangents.get(v).copied().unwrap_or_default();
            let (j, w) = match &mesh.skin {
                Some(s) => (s.vertex_joints[v], s.vertex_weights[v]),
                None => ([0; 4], [1.0, 0.0, 0.0, 0.0]),
            };
            assert!(
                j.iter().all(|&j| (j as usize) < joint_count),
                "joint index out of range"
            );
            words.extend([p[0], p[1], p[2], 0.0, nrm[0], nrm[1], nrm[2], 0.0].map(f32::to_bits));
            words.extend(t.map(f32::to_bits));
            words.extend(j.map(u32::from));
            words.extend(w.map(f32::to_bits));
        }
        let mut shapes = Vec::with_capacity(mesh.blend_shapes.len());
        let mut deltas = Vec::new();
        for s in &mesh.blend_shapes {
            shapes.push(((deltas.len() / 8) as u32, s.deltas.len() as u32));
            for d in &s.deltas {
                assert!((d.vertex as usize) < n, "delta vertex out of range");
                let (p, nrm) = (d.position, d.normal);
                deltas.extend([p[0].to_bits(), p[1].to_bits(), p[2].to_bits(), d.vertex]);
                deltas.extend([nrm[0], nrm[1], nrm[2], 0.0].map(f32::to_bits));
            }
        }

        use vk::BufferUsageFlags as U;
        let gpu = MemoryLocation::GpuOnly;
        let storage_dst = U::STORAGE_BUFFER | U::TRANSFER_DST;
        let n = n as u64;
        let vertices = ctx.create_buffer("skin vertices", n * 80, storage_dst, gpu)?;
        let mut mesh_gpu = SkinnedMesh {
            deltas: ctx.create_buffer("skin deltas", deltas.len() as u64 * 4, storage_dst, gpu)?,
            joints: ctx.create_buffer("skin joints", joint_count as u64 * 64, storage_dst, gpu)?,
            active: ctx.create_buffer("skin active", shapes.len() as u64 * 16, storage_dst, gpu)?,
            accum: ctx.create_buffer("skin accum", n * 24, storage_dst, gpu)?,
            output: ctx.create_buffer(
                "skinned",
                n * 48,
                U::STORAGE_BUFFER | U::VERTEX_BUFFER | U::TRANSFER_SRC,
                gpu,
            )?,
            vertices,
            shapes,
            vertex_count: n as u32,
            joint_count: joint_count as u32,
        };
        // Buffers created above leak to the allocator on these errors; the device is unusable by then anyway.
        ctx.upload(&mesh_gpu.vertices, &bytes(&words))?;
        ctx.upload(&mesh_gpu.deltas, &bytes(&deltas))?;
        mesh_gpu.shapes.shrink_to_fit();
        Ok(mesh_gpu)
    }

    /// The skinned vertices: per vertex, position (w = 1), normal and tangent (w = handedness) as four floats each.
    pub fn output(&self) -> &Buffer {
        &self.output
    }

    /// Number of vertices.
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    /// Number of joint matrices a frame passes (1 for a mesh without a skin).
    pub fn joint_count(&self) -> u32 {
        self.joint_count
    }

    /// Number of blendshapes, the length of a frame's weights.
    pub fn shape_count(&self) -> usize {
        self.shapes.len()
    }

    /// Destroys the mesh's buffers.
    ///
    /// # Safety
    ///
    /// The GPU no longer uses them.
    pub unsafe fn destroy(self, ctx: &Context) {
        for b in [
            self.vertices,
            self.deltas,
            self.joints,
            self.active,
            self.accum,
            self.output,
        ] {
            // SAFETY: per the contract.
            unsafe { ctx.destroy_buffer(b) };
        }
    }
}

/// One mesh to skin this frame.
pub struct Job<'a> {
    /// The mesh.
    pub mesh: &'a SkinnedMesh,
    /// Skinning matrices (joint world matrix times inverse bind), [`SkinnedMesh::joint_count`] of them.
    pub joints: &'a [Mat4],
    /// Blendshape weights, [`SkinnedMesh::shape_count`] of them.
    pub weights: &'a [f32],
}

/// The skinning pipelines: `morph` and `skin` from one Slang module, buffers passed by address in push constants.
pub struct Skinning {
    layout: vk::PipelineLayout,
    morph: vk::Pipeline,
    skin: vk::Pipeline,
}

impl Skinning {
    /// Creates the pipelines.
    pub fn new(ctx: &Context) -> Result<Self> {
        let d = ctx.device();
        let code: Vec<u32> = SPIRV.as_chunks().0.iter().map(|&c| u32::from_le_bytes(c)).collect();
        let range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::COMPUTE,
            offset: 0,
            size: PUSH_SIZE,
        };
        // SAFETY: valid create infos; the module is only needed while the pipelines are created.
        unsafe {
            let module = d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
            let layout = d.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default().push_constant_ranges(&[range]),
                None,
            );
            let layout = match layout {
                Ok(l) => l,
                Err(e) => {
                    d.destroy_shader_module(module, None);
                    return Err(e.into());
                }
            };
            let info = |name| {
                vk::ComputePipelineCreateInfo::default().layout(layout).stage(
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::COMPUTE)
                        .module(module)
                        .name(name),
                )
            };
            let pipelines =
                d.create_compute_pipelines(vk::PipelineCache::null(), &[info(c"morph"), info(c"skin")], None);
            d.destroy_shader_module(module, None);
            match pipelines {
                Ok(p) => Ok(Skinning {
                    layout,
                    morph: p[0],
                    skin: p[1],
                }),
                Err((p, e)) => {
                    for p in p {
                        d.destroy_pipeline(p, None);
                    }
                    d.destroy_pipeline_layout(layout, None);
                    Err(e.into())
                }
            }
        }
    }

    /// Destroys the pipelines.
    ///
    /// # Safety
    ///
    /// The GPU no longer uses them.
    pub unsafe fn destroy(&self, device: &ash::Device) {
        // SAFETY: per the contract.
        unsafe {
            device.destroy_pipeline(self.morph, None);
            device.destroy_pipeline(self.skin, None);
            device.destroy_pipeline_layout(self.layout, None);
        }
    }

    /// Records skinning for every job: uploads joints and active shapes, clears the accumulators of meshes with
    /// active shapes, runs `morph` for those and `skin` for all, with four barriers in total. Afterwards the
    /// outputs are readable by vertex input, vertex and compute shaders.
    ///
    /// # Safety
    ///
    /// `cmd` is recording on the device the pipelines and meshes were created on, outside a rendering scope.
    ///
    /// # Panics
    ///
    /// If a job's joint or weight count does not match its mesh.
    pub unsafe fn record(&self, device: &ash::Device, cmd: vk::CommandBuffer, jobs: &[Job]) {
        let mut active = Vec::new();
        let plans: Vec<(Vec<Active>, u32)> = jobs
            .iter()
            .map(|j| {
                assert_eq!(j.joints.len(), j.mesh.joint_count as usize, "joint matrix count");
                assert_eq!(j.weights.len(), j.mesh.shapes.len(), "blendshape weight count");
                let threads = active_shapes(&j.mesh.shapes, j.weights, &mut active);
                (active.clone(), threads)
            })
            .collect();
        use vk::AccessFlags2 as A;
        use vk::PipelineStageFlags2 as S;
        let readers = S::VERTEX_ATTRIBUTE_INPUT | S::VERTEX_SHADER | S::COMPUTE_SHADER;
        // SAFETY: per the contract; every buffer belongs to a live mesh and the dispatches stay within them.
        unsafe {
            // Last frame's reads and writes of these buffers finish before they are overwritten.
            barrier(
                device,
                cmd,
                readers,
                A::SHADER_STORAGE_WRITE,
                S::ALL_TRANSFER,
                A::TRANSFER_WRITE,
            );
            for (job, (active, threads)) in jobs.iter().zip(&plans) {
                let m = job.mesh;
                update(
                    device,
                    cmd,
                    &m.joints,
                    &bytes(&job.joints.iter().flatten().map(|x| x.to_bits()).collect::<Vec<_>>()),
                );
                if *threads > 0 {
                    let words: Vec<u32> = active
                        .iter()
                        .flat_map(|a| [a.first, a.count, a.start, a.weight.to_bits()])
                        .collect();
                    update(device, cmd, &m.active, &bytes(&words));
                    device.cmd_fill_buffer(cmd, m.accum.handle, 0, vk::WHOLE_SIZE, 0);
                }
            }
            let rw = A::SHADER_STORAGE_READ | A::SHADER_STORAGE_WRITE;
            barrier(device, cmd, S::ALL_TRANSFER, A::TRANSFER_WRITE, S::COMPUTE_SHADER, rw);
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.morph);
            for (job, (active, threads)) in jobs.iter().zip(&plans) {
                if *threads > 0 {
                    self.push(device, cmd, job.mesh, active.len() as u32, *threads);
                    device.cmd_dispatch(cmd, threads.div_ceil(GROUP), 1, 1);
                }
            }
            barrier(
                device,
                cmd,
                S::COMPUTE_SHADER,
                A::SHADER_STORAGE_WRITE,
                S::COMPUTE_SHADER,
                rw,
            );
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.skin);
            for (job, (active, threads)) in jobs.iter().zip(&plans) {
                self.push(device, cmd, job.mesh, active.len() as u32, *threads);
                device.cmd_dispatch(cmd, job.mesh.vertex_count.div_ceil(GROUP), 1, 1);
            }
            let read = A::VERTEX_ATTRIBUTE_READ | A::SHADER_STORAGE_READ;
            barrier(device, cmd, S::COMPUTE_SHADER, A::SHADER_STORAGE_WRITE, readers, read);
        }
    }

    /// # Safety
    ///
    /// As [`Skinning::record`].
    unsafe fn push(&self, device: &ash::Device, cmd: vk::CommandBuffer, m: &SkinnedMesh, active: u32, threads: u32) {
        let mut push = Vec::with_capacity(PUSH_SIZE as usize);
        for b in [&m.vertices, &m.deltas, &m.active, &m.joints, &m.accum, &m.output] {
            push.extend(b.address.to_le_bytes());
        }
        push.extend(bytes(&[m.vertex_count, active, threads, SCALE.to_bits()]));
        // SAFETY: per the contract; the layout's range covers the 64 bytes.
        unsafe { device.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::COMPUTE, 0, &push) };
    }
}

/// # Safety
///
/// `cmd` is recording outside a rendering scope.
unsafe fn barrier(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    src_stage: vk::PipelineStageFlags2,
    src_access: vk::AccessFlags2,
    dst_stage: vk::PipelineStageFlags2,
    dst_access: vk::AccessFlags2,
) {
    let b = vk::MemoryBarrier2::default()
        .src_stage_mask(src_stage)
        .src_access_mask(src_access)
        .dst_stage_mask(dst_stage)
        .dst_access_mask(dst_access);
    // SAFETY: per the contract.
    unsafe { device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[b])) };
}

/// Writes `data` to the start of `buffer` from the command buffer, in chunks `vkCmdUpdateBuffer` accepts.
///
/// # Safety
///
/// As [`barrier`]; `buffer` holds at least `data.len()` bytes.
unsafe fn update(device: &ash::Device, cmd: vk::CommandBuffer, buffer: &Buffer, data: &[u8]) {
    for (i, chunk) in data.chunks(UPDATE_LIMIT).enumerate() {
        // SAFETY: per the contract; chunks are multiples of 4 bytes (whole words) at most 65536 long.
        unsafe { device.cmd_update_buffer(cmd, buffer.handle, (i * UPDATE_LIMIT) as u64, chunk) };
    }
}
