//! The GPU-resident data draws reach: shared rest-vertex, attribute, blendshape, index and skinned-vertex buffers,
//! one material buffer and the bindless textures (design doc §6 "GPU-driven path", ADR 0023).

use ash::vk;
use civbb_format::material::Material;
use civbb_format::mesh::{Mesh, Submesh};

use crate::material::{MATERIAL_WORDS, PipelineKey, pack};
use crate::memory::bytes;
use crate::texture::Textures;
use crate::{Buffer, Context, Error, MemoryLocation, Result};

/// Bytes per rest vertex (`Vertex` in `skin.slang`): position, normal, tangent, joints, weights.
const REST: u64 = 80;
/// Bytes per attribute vertex: UV0 and UV1.
const ATTRS: u64 = 16;
/// Bytes per blendshape delta (`Delta` in `skin.slang`).
const DELTA: u64 = 32;
/// Bytes per skinned vertex: position, normal, tangent.
pub(crate) const SKINNED: u64 = 48;

/// How much the shared buffers hold, in elements. Allocation only grows; it is sized up front (ADR 0023).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capacity {
    /// Rest vertices of all meshes (and their attributes).
    pub vertices: u32,
    /// Indices of all meshes.
    pub indices: u32,
    /// Blendshape deltas of all meshes.
    pub deltas: u32,
    /// Skinned vertices of all mesh instances.
    pub skinned: u32,
    /// Materials.
    pub materials: u32,
}

impl Capacity {
    /// What `meshes` need once each, with `instances` skinned copies and `materials` materials.
    pub fn for_meshes<'m>(meshes: impl IntoIterator<Item = &'m Mesh>, instances: u32, materials: u32) -> Self {
        let mut c = Capacity {
            materials,
            ..Capacity::default()
        };
        for m in meshes {
            c.vertices += m.positions.len() as u32;
            c.indices += m.indices.len() as u32;
            c.deltas += m.blend_shapes.iter().map(|s| s.deltas.len() as u32).sum::<u32>();
        }
        c.skinned = c.vertices * instances;
        c
    }
}

/// A mesh in the shared buffers: where its vertices, indices and blendshape deltas start.
#[derive(Clone, Debug)]
pub struct MeshData {
    /// First rest vertex (and attribute vertex).
    pub(crate) vertex_base: u32,
    pub(crate) vertex_count: u32,
    /// First index.
    pub(crate) index_base: u32,
    /// Each shape's first delta in the shared delta buffer and its delta count.
    pub(crate) shapes: Vec<(u32, u32)>,
    pub(crate) joint_count: u32,
    pub(crate) submeshes: Vec<Submesh>,
}

impl MeshData {
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

    /// The index ranges, one per material, as in the source mesh.
    pub fn submeshes(&self) -> &[Submesh] {
        &self.submeshes
    }
}

/// A material in the material buffer and how its draws are grouped and ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialId {
    /// Index in the material buffer.
    pub index: u32,
    /// The pipeline it draws with.
    pub key: PipelineKey,
    /// Unity render queue, ordering blended draws.
    pub queue: i32,
}

/// Every mesh, material and texture the GPU draws from, in shared buffers.
pub struct Scene {
    rest: Buffer,
    attrs: Buffer,
    deltas: Buffer,
    indices: Buffer,
    skinned: Buffer,
    materials: Buffer,
    textures: Textures,
    capacity: Capacity,
    used: Capacity,
}

impl Scene {
    /// Creates the shared buffers and the empty texture array.
    pub fn new(ctx: &Context, capacity: Capacity) -> Result<Self> {
        use vk::BufferUsageFlags as U;
        let gpu = MemoryLocation::GpuOnly;
        let storage = U::STORAGE_BUFFER | U::TRANSFER_DST;
        let c = capacity;
        // Buffers created before an error leak to the allocator; the device is unusable by then anyway.
        Ok(Scene {
            rest: ctx.create_buffer("scene rest vertices", u64::from(c.vertices) * REST, storage, gpu)?,
            attrs: ctx.create_buffer("scene attributes", u64::from(c.vertices) * ATTRS, storage, gpu)?,
            deltas: ctx.create_buffer("scene deltas", u64::from(c.deltas) * DELTA, storage, gpu)?,
            indices: ctx.create_buffer(
                "scene indices",
                u64::from(c.indices) * 4,
                U::INDEX_BUFFER | U::TRANSFER_DST,
                gpu,
            )?,
            skinned: ctx.create_buffer(
                "scene skinned vertices",
                u64::from(c.skinned) * SKINNED,
                U::STORAGE_BUFFER | U::TRANSFER_SRC,
                gpu,
            )?,
            materials: ctx.create_buffer(
                "scene materials",
                u64::from(c.materials) * MATERIAL_WORDS as u64 * 4,
                storage,
                gpu,
            )?,
            textures: Textures::new(ctx)?,
            capacity,
            used: Capacity::default(),
        })
    }

    /// Uploads a mesh's rest vertices, attributes, indices and blendshape deltas.
    ///
    /// # Panics
    ///
    /// If a joint, index or delta vertex is out of range ([`civbb_format::validate`] rules them out).
    pub fn add_mesh(&mut self, ctx: &Context, mesh: &Mesh) -> Result<MeshData> {
        let n = mesh.positions.len();
        let joint_count = mesh.skin.as_ref().map_or(1, |s| s.joints.len().max(1));
        let mut rest = Vec::with_capacity(n * 20);
        let mut attrs = Vec::with_capacity(n * 4);
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
            rest.extend([p[0], p[1], p[2], 0.0, nrm[0], nrm[1], nrm[2], 0.0].map(f32::to_bits));
            rest.extend(t.map(f32::to_bits));
            rest.extend(j.map(u32::from));
            rest.extend(w.map(f32::to_bits));
            let uv0 = mesh.uv0.get(v).copied().unwrap_or_default();
            let uv1 = mesh.uv1.get(v).copied().unwrap_or_default();
            attrs.extend([uv0[0], uv0[1], uv1[0], uv1[1]].map(f32::to_bits));
        }
        assert!(
            mesh.indices.iter().all(|&i| (i as usize) < n),
            "vertex index out of range"
        );
        let delta_count: usize = mesh.blend_shapes.iter().map(|s| s.deltas.len()).sum();
        let delta_base = bump(&mut self.used.deltas, self.capacity.deltas, delta_count, "deltas")?;
        let mut shapes = Vec::with_capacity(mesh.blend_shapes.len());
        let mut deltas = Vec::with_capacity(delta_count * 8);
        for s in &mesh.blend_shapes {
            shapes.push((delta_base + (deltas.len() / 8) as u32, s.deltas.len() as u32));
            for d in &s.deltas {
                assert!((d.vertex as usize) < n, "delta vertex out of range");
                let (p, nrm) = (d.position, d.normal);
                deltas.extend([p[0].to_bits(), p[1].to_bits(), p[2].to_bits(), d.vertex]);
                deltas.extend([nrm[0], nrm[1], nrm[2], 0.0].map(f32::to_bits));
            }
        }
        let vertex_base = bump(&mut self.used.vertices, self.capacity.vertices, n, "vertices")?;
        let index_base = bump(
            &mut self.used.indices,
            self.capacity.indices,
            mesh.indices.len(),
            "indices",
        )?;
        let v = u64::from(vertex_base);
        ctx.upload(&self.rest, v * REST, &bytes(&rest))?;
        ctx.upload(&self.attrs, v * ATTRS, &bytes(&attrs))?;
        ctx.upload(&self.deltas, u64::from(delta_base) * DELTA, &bytes(&deltas))?;
        ctx.upload(&self.indices, u64::from(index_base) * 4, &bytes(&mesh.indices))?;
        Ok(MeshData {
            vertex_base,
            vertex_count: n as u32,
            index_base,
            shapes,
            joint_count: joint_count as u32,
            submeshes: mesh.submeshes.clone(),
        })
    }

    /// Uploads an RGBA8 texture (`srgb` for colour, linear for data) and returns its bindless index.
    ///
    /// # Panics
    ///
    /// If `rgba` is not `width * height * 4` bytes, a side is 0 or over 16384, or the array is full.
    pub fn add_texture(&mut self, ctx: &Context, width: u32, height: u32, rgba: &[u8], srgb: bool) -> Result<u32> {
        self.textures.add(ctx, width, height, rgba, srgb)
    }

    /// Uploads a BC7 texture with all its mips, full size first, and returns its bindless index (ADR 0029).
    ///
    /// # Panics
    ///
    /// If a level's size does not match its blocks, the level count is 0 or too many, a side is 0 or over 16384, or
    /// the array is full.
    pub fn add_bc7_texture(
        &mut self,
        ctx: &Context,
        width: u32,
        height: u32,
        levels: &[Vec<u8>],
        srgb: bool,
    ) -> Result<u32> {
        self.textures.add_bc7(ctx, width, height, levels, srgb)
    }

    /// Uploads a material; `textures` maps its texture indices to bindless indices from [`Scene::add_texture`].
    pub fn add_material(&mut self, ctx: &Context, material: &Material, textures: &[u32]) -> Result<MaterialId> {
        let index = bump(&mut self.used.materials, self.capacity.materials, 1, "materials")?;
        let record = MATERIAL_WORDS as u64 * 4;
        ctx.upload(
            &self.materials,
            u64::from(index) * record,
            &bytes(&pack(material, textures)),
        )?;
        Ok(MaterialId {
            index,
            key: PipelineKey::of(material),
            queue: material.render_queue,
        })
    }

    /// Takes `count` skinned vertices for one mesh instance; returns the first.
    pub(crate) fn alloc_skinned(&mut self, count: u32) -> Result<u32> {
        bump(
            &mut self.used.skinned,
            self.capacity.skinned,
            count as usize,
            "skinned vertices",
        )
    }

    /// Rest vertices, read by skinning.
    pub(crate) fn rest(&self) -> &Buffer {
        &self.rest
    }

    /// UV0 and UV1 per rest vertex, read by draws.
    pub(crate) fn attrs(&self) -> &Buffer {
        &self.attrs
    }

    /// Blendshape deltas, read by skinning.
    pub(crate) fn deltas(&self) -> &Buffer {
        &self.deltas
    }

    /// Mesh-relative indices, the one index buffer of every draw.
    pub(crate) fn indices(&self) -> &Buffer {
        &self.indices
    }

    /// Material records.
    pub(crate) fn materials(&self) -> &Buffer {
        &self.materials
    }

    /// The bindless texture array.
    pub(crate) fn textures(&self) -> &Textures {
        &self.textures
    }

    /// The skinned vertex buffer every mesh instance writes into and draws read from: per vertex, position
    /// (w = 1), normal and tangent (w = handedness) as four floats each. Frames declare it as a graph buffer.
    pub fn skinned(&self) -> &Buffer {
        &self.skinned
    }

    /// How much of each buffer is in use.
    pub fn used(&self) -> Capacity {
        self.used
    }

    /// Destroys the buffers and textures.
    ///
    /// # Safety
    ///
    /// The GPU no longer uses them.
    pub unsafe fn destroy(mut self, ctx: &Context) {
        // SAFETY: per the contract.
        unsafe {
            self.textures.destroy(ctx);
            for b in [
                self.rest,
                self.attrs,
                self.deltas,
                self.indices,
                self.skinned,
                self.materials,
            ] {
                ctx.destroy_buffer(b);
            }
        }
    }
}

/// Takes `n` elements from a buffer with `cap` in total and `used` taken; returns the first.
fn bump(used: &mut u32, cap: u32, n: usize, what: &str) -> Result<u32> {
    let first = *used;
    match u32::try_from(n).ok().and_then(|n| first.checked_add(n)) {
        Some(end) if end <= cap => {
            *used = end;
            Ok(first)
        }
        _ => Err(Error::Memory(format!(
            "scene {what} full: {first} of {cap} used, {n} more asked"
        ))),
    }
}
