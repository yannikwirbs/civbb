//! The render thread's GPU resources, kept in step with each snapshot: one upload per package shown and one skinned
//! instance per island, keyed by the island; the scene is rebuilt larger when a snapshot needs more (ADR 0104).

use std::collections::HashMap;

use civbb_format::material::ColorSpace;
use civbb_render::material::{MATERIAL_WORDS, pack};
use civbb_render::overlay::{NO_TEXTURE, Vertex};
use civbb_render::skin::{Job, SkinnedMesh, Skinning};
use civbb_render::{
    Access, Batches, Capacity, Context, Draw, Forward, FrameStatus, MaterialId, MeshData, Overlay, Pass, Renderer,
    Result, Scene, View, batch,
};
use civbb_ui::draw::Texture;

use crate::cull::{Frustum, MeshBounds};
use crate::extract::{Camera, Snapshot};
use crate::package::Package;
use crate::view::Texels;

/// One package on the GPU: its meshes, textures and shared materials, uploaded once.
struct Upload {
    meshes: Vec<MeshData>,
    /// Per mesh, its bind-space spheres for culling (ADR 0077).
    bounds: Vec<MeshBounds>,
    /// The avatar's materials, shared by every island showing it.
    materials: Vec<MaterialId>,
    /// Bindless index of each avatar texture.
    bindless: Vec<u32>,
    /// Instances of islands that went away, for the next islands showing this package.
    free: Vec<Instance>,
}

/// One island on the GPU.
struct Instance {
    /// The package's id.
    package: u64,
    /// Per mesh, its skinned instance.
    meshes: Vec<SkinnedMesh>,
    /// Its own records for animated renderers, in [`crate::Island::materials`] order (ADR 0025).
    own: Vec<MaterialId>,
    /// Per mesh, the material map of an animated renderer: the shared one with its own records.
    maps: Vec<Option<Vec<MaterialId>>>,
    /// The records last written for its own materials; cleared for a new island, so its first frame writes them all.
    packed: Vec<[u32; MATERIAL_WORDS]>,
    /// The last snapshot that showed it.
    seen: u64,
}

/// Overlay vertices a frame can draw (ADR 0117).
const MAX_OVERLAY: u32 = 1 << 16;

/// A scene and what is in it.
struct Built {
    scene: Scene,
    forward: Forward,
    overlay: Overlay,
    /// The glyph atlas's bindless slot and the version uploaded into it (ADR 0116).
    atlas: Option<(u32, u64)>,
    capacity: Capacity,
    max_draws: u32,
    /// By package id.
    uploads: HashMap<u64, Upload>,
    /// By island key.
    instances: HashMap<u64, Instance>,
}

impl Built {
    /// # Safety
    ///
    /// The GPU is idle.
    unsafe fn destroy(self, ctx: &Context) {
        let free = self.uploads.into_values().flat_map(|u| u.free);
        // SAFETY: per the contract.
        unsafe {
            self.forward.destroy(ctx);
            self.overlay.destroy(ctx);
            for m in self.instances.into_values().chain(free).flat_map(|i| i.meshes) {
                m.destroy(ctx);
            }
            self.scene.destroy(ctx);
        }
    }
}

/// Everything on the GPU: the scene with the packages and islands of the last snapshot, and the pipelines.
pub(crate) struct Gpu {
    skinning: Skinning,
    /// `None` until the first snapshot.
    built: Option<Built>,
    /// Snapshots synced so far.
    synced: u64,
}

impl Gpu {
    pub(crate) fn new(renderer: &Renderer) -> Result<Self> {
        Ok(Gpu {
            skinning: Skinning::new(renderer.context())?,
            built: None,
            synced: 0,
        })
    }

    /// # Safety
    ///
    /// The GPU is idle.
    pub(crate) unsafe fn destroy(self, renderer: &Renderer) {
        let ctx = renderer.context();
        // SAFETY: per the contract.
        unsafe {
            self.skinning.destroy(ctx.device());
            if let Some(b) = self.built {
                b.destroy(ctx);
            }
        }
    }

    /// Gives every island of `snapshot` an instance and returns those of islands it lacks to their package's free
    /// list, rebuilding the scene first when it cannot hold what is new.
    fn sync(&mut self, renderer: &Renderer, snapshot: &Snapshot) -> Result<()> {
        self.synced += 1;
        let seen = self.synced;
        if let Some(b) = &mut self.built {
            for i in &snapshot.islands {
                if let Some(instance) = b.instances.get_mut(&i.key) {
                    instance.seen = seen;
                }
            }
            for (_, instance) in b.instances.extract_if(|_, i| i.seen != seen) {
                let upload = b.uploads.get_mut(&instance.package);
                upload.expect("instances' packages stay uploaded").free.push(instance);
            }
        }
        let (extra, draws) = need(snapshot, self.built.as_ref());
        if !self.built.as_ref().is_some_and(|b| fits(b, extra, draws)) {
            self.rebuild(renderer, snapshot)?;
        }
        let b = self.built.as_mut().expect("built above");
        let ctx = renderer.context();
        for i in &snapshot.islands {
            if b.instances.contains_key(&i.key) {
                continue;
            }
            let p = &*i.package;
            if !b.uploads.contains_key(&p.id) {
                let u = upload(ctx, &mut b.scene, p)?;
                b.uploads.insert(p.id, u);
            }
            let u = b.uploads.get_mut(&p.id).expect("uploaded above");
            let mut instance = match u.free.pop() {
                Some(instance) => instance,
                None => new_instance(ctx, &mut b.scene, u, p)?,
            };
            instance.packed.clear();
            instance.seen = seen;
            b.instances.insert(i.key, instance);
        }
        Ok(())
    }

    /// Replaces the scene with one that holds `snapshot`: exactly the first time, after that at least twice the old.
    fn rebuild(&mut self, renderer: &Renderer, snapshot: &Snapshot) -> Result<()> {
        let ctx = renderer.context();
        let (need, draws) = need(snapshot, None);
        let (capacity, max_draws) = match self.built.take() {
            Some(old) => {
                let (mut c, mut need, max_draws) = (old.capacity, need, old.max_draws);
                // SAFETY: after the wait nothing is in flight.
                unsafe {
                    ctx.device().device_wait_idle()?;
                    old.destroy(ctx);
                }
                for (c, n) in fields(&mut c).into_iter().zip(fields(&mut need)) {
                    *c = c.saturating_mul(2).max(*n);
                }
                (c, max_draws.saturating_mul(2).max(draws))
            }
            None => (need, draws),
        };
        let scene = Scene::new(ctx, capacity)?;
        self.built = Some(Built {
            forward: Forward::new(ctx, &scene, renderer.color_format()?, max_draws)?,
            overlay: Overlay::new(ctx, &scene, renderer.color_format()?, MAX_OVERLAY)?,
            atlas: None,
            scene,
            capacity,
            max_draws,
            uploads: HashMap::new(),
            instances: HashMap::new(),
        });
        Ok(())
    }
}

/// A capacity's fields.
fn fields(c: &mut Capacity) -> [&mut u32; 5] {
    [
        &mut c.vertices,
        &mut c.indices,
        &mut c.deltas,
        &mut c.skinned,
        &mut c.materials,
    ]
}

/// What `snapshot` adds to `built` (everything without one), and the draws it may make (at least 1).
fn need(snapshot: &Snapshot, built: Option<&Built>) -> (Capacity, u32) {
    let mut new: HashMap<u64, (&Package, u32)> = HashMap::new();
    let mut draws = 0;
    for i in &snapshot.islands {
        let p = &*i.package;
        // Each submesh may add an outline draw (ADR 0034).
        draws += 2 * p.avatar.meshes.iter().map(|m| m.submeshes.len() as u32).sum::<u32>();
        if built.is_none_or(|b| !b.instances.contains_key(&i.key)) {
            new.entry(p.id).or_insert((p, 0)).1 += 1;
        }
    }
    let mut sum = Capacity::default();
    for (id, (p, n)) in new {
        let upload = built.and_then(|b| b.uploads.get(&id));
        let fresh = n.saturating_sub(upload.map_or(0, |u| u.free.len() as u32));
        let own: u32 = p.model.animated_materials().map(|(_, m)| m.len() as u32).sum();
        let mut add = Capacity::for_meshes(&p.avatar.meshes, fresh, own * fresh);
        match upload {
            Some(_) => (add.vertices, add.indices, add.deltas) = (0, 0, 0),
            None => add.materials += p.avatar.materials.len() as u32,
        }
        for (s, a) in fields(&mut sum).into_iter().zip(fields(&mut add)) {
            *s += *a;
        }
    }
    (sum, draws.max(1))
}

/// Whether `built` has room for `extra` more and `draws` draws.
fn fits(b: &Built, mut extra: Capacity, draws: u32) -> bool {
    let (mut used, mut capacity) = (b.scene.used(), b.capacity);
    let room = fields(&mut used)
        .into_iter()
        .zip(fields(&mut capacity))
        .zip(fields(&mut extra));
    draws <= b.max_draws
        && room
            .into_iter()
            .all(|((u, c), e)| u64::from(*u) + u64::from(*e) <= u64::from(*c))
}

/// Uploads `p`'s textures, materials and meshes.
fn upload(ctx: &Context, scene: &mut Scene, p: &Package) -> Result<Upload> {
    let avatar = &p.avatar;
    let mut bindless = Vec::with_capacity(p.textures.len());
    for (t, decoded) in avatar.textures.iter().zip(&p.textures) {
        bindless.push(match decoded {
            Some(image) => {
                let srgb = t.color_space == ColorSpace::Srgb;
                match &image.texels {
                    Texels::Rgba8(rgba) => scene.add_texture(ctx, image.width, image.height, rgba, srgb)?,
                    Texels::Bc7(levels) => scene.add_bc7_texture(ctx, image.width, image.height, levels, srgb)?,
                }
            }
            None => civbb_render::material::NONE,
        });
    }
    let materials = (avatar.materials.iter())
        .map(|m| scene.add_material(ctx, m, &bindless))
        .collect::<Result<Vec<_>>>()?;
    let meshes = (avatar.meshes.iter())
        .map(|m| scene.add_mesh(ctx, m))
        .collect::<Result<Vec<_>>>()?;
    Ok(Upload {
        meshes,
        bounds: avatar.meshes.iter().map(MeshBounds::new).collect(),
        materials,
        bindless,
        free: Vec::new(),
    })
}

/// A new instance of `p`, uploaded as `u`.
fn new_instance(ctx: &Context, scene: &mut Scene, u: &Upload, p: &Package) -> Result<Instance> {
    let mut own = Vec::new();
    let mut maps = vec![None; p.avatar.meshes.len()];
    for (mesh, indices) in p.model.animated_materials() {
        let mut map = u.materials.clone();
        for &m in indices {
            let id = scene.add_material(ctx, &p.avatar.materials[m as usize], &u.bindless)?;
            map[m as usize] = id;
            own.push(id);
        }
        maps[mesh as usize] = Some(map);
    }
    let meshes = (u.meshes.iter())
        .map(|m| SkinnedMesh::new(ctx, scene, m))
        .collect::<Result<Vec<_>>>()?;
    Ok(Instance {
        package: p.id,
        meshes,
        own,
        maps,
        packed: Vec::new(),
        seen: 0,
    })
}

/// Per-frame scratch reused across frames.
#[derive(Default)]
pub(crate) struct Scratch {
    draws: Vec<Draw>,
    batches: Batches,
    updates: Vec<(u32, [u32; MATERIAL_WORDS])>,
    overlay: Vec<Vertex>,
}

/// Skins and draws the islands of `snapshot`, each with its own instance in `gpu`, from the snapshot's camera;
/// returns the draws and skinned meshes, or `None` if the frame was skipped.
pub(crate) fn draw(
    renderer: &mut Renderer,
    gpu: &mut Gpu,
    snapshot: &Snapshot,
    s: &mut Scratch,
) -> Result<Option<(usize, usize)>> {
    gpu.sync(renderer, snapshot)?;
    let Camera {
        eye,
        target,
        fov_y,
        clear,
    } = snapshot.camera;
    s.draws.clear();
    s.updates.clear();
    let extent = renderer.extent().unwrap_or_default();
    let aspect = extent.width.max(1) as f32 / extent.height.max(1) as f32;
    let view = View::look_at(eye, target, fov_y, aspect, 0.05);
    let frustum = Frustum::new(&view.view_proj);
    let b = gpu.built.as_mut().expect("synced above");
    // An island's records are rewritten only when they change; the first frame writes them all.
    for island in &snapshot.islands {
        let instance = b.instances.get_mut(&island.key).expect("synced above");
        let bindless = &b.uploads[&instance.package].bindless;
        instance.packed.resize(island.materials.len(), [0; MATERIAL_WORDS]);
        for ((m, last), id) in island.materials.iter().zip(&mut instance.packed).zip(&instance.own) {
            let record = pack(m, bindless);
            if record != *last {
                *last = record;
                s.updates.push((id.index, record));
            }
        }
    }
    overlay(renderer.context(), b, snapshot, &mut s.overlay)?;
    let b = &*b;
    let mut jobs = Vec::new();
    for island in &snapshot.islands {
        let instance = &b.instances[&island.key];
        let u = &b.uploads[&instance.package];
        let o = island.origin;
        let distance = ((o[0] - eye[0]).powi(2) + (o[1] - eye[1]).powi(2) + (o[2] - eye[2]).powi(2)).sqrt();
        for (m, joints) in island.joints.iter().enumerate() {
            let Some(joints) = joints else { continue };
            // A mesh wholly outside the view is neither skinned nor drawn (ADR 0077).
            let seen = u.bounds[m].world_box(joints, &island.weights[m]);
            if !seen.is_some_and(|(lo, hi)| frustum.sees(lo, hi)) {
                continue;
            }
            let materials = instance.maps[m].as_deref().unwrap_or(&u.materials);
            u.meshes[m].draws(&instance.meshes[m], materials, &island.slots[m], distance, &mut s.draws);
            jobs.push(Job {
                mesh: &instance.meshes[m],
                joints,
                weights: &island.weights[m],
            });
        }
    }
    batch(&mut s.draws, &mut s.batches);
    let (draws, skinned) = (s.draws.len(), jobs.len());
    let (batches, updates, overlay, skinning) = (&s.batches, &s.updates, &s.overlay, &gpu.skinning);
    let status = renderer.frame(|graph, target| {
        let skinned = graph.import_buffer(b.scene.skinned().handle, &[Access::VertexRead]);
        graph.add(
            Pass::new("skin")
                .buffer(skinned, Access::ComputeWrite)
                // SAFETY: the graph records this outside a rendering scope on the renderer's device.
                .record(|d, cmd| unsafe { skinning.record(d, cmd, &jobs) }),
        );
        b.forward
            .add(graph, &b.scene, target, skinned, &view, batches, updates, clear);
        b.overlay.add(graph, &b.scene, target, overlay);
    })?;
    Ok((status == FrameStatus::Presented).then_some((draws, skinned)))
}

/// Uploads the snapshot's glyph atlas when its version changed and writes its overlay's vertices into `out`, with
/// bindless slots for the atlas and the icons' package textures.
fn overlay(ctx: &Context, b: &mut Built, snapshot: &Snapshot, out: &mut Vec<Vertex>) -> Result<()> {
    let o = &snapshot.overlay;
    if let Some(atlas) = (o.atlas.as_ref()).filter(|a| b.atlas.is_none_or(|(_, version)| version != a.version)) {
        let slot = b.atlas.map(|(slot, _)| slot);
        if slot.is_some() {
            // Rare (new glyphs): the frames in flight may still sample the old atlas.
            // SAFETY: this thread is the only one using the device (ADR 0103 point 5).
            unsafe { ctx.device().device_wait_idle()? };
        }
        // SAFETY: a new slot, or the GPU is idle after the wait above.
        let slot = unsafe { (b.scene).put_flat_texture(ctx, slot, atlas.width, atlas.height, &atlas.rgba)? };
        b.atlas = Some((slot, atlas.version));
    }
    let icons = (o.icons)
        .and_then(|key| b.instances.get(&key))
        .map(|i| &b.uploads[&i.package].bindless);
    let atlas = b.atlas.map_or(NO_TEXTURE, |(slot, _)| slot);
    out.clear();
    out.extend(o.list.vertices.iter().map(|v| Vertex {
        pos: v.pos,
        uv: v.uv,
        color: v.color,
        texture: match v.texture {
            Texture::None => NO_TEXTURE,
            Texture::Atlas => atlas,
            Texture::Icon(i) => icons.and_then(|t| t.get(i as usize)).copied().unwrap_or(NO_TEXTURE),
        },
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use civbb_core::math::Pose;

    use super::*;
    use crate::frame::Runtime;

    /// Islands that go away give their instance to the next island of their package without growing the scene; a new
    /// package that does not fit rebuilds it larger (ADR 0104). Skips without a device.
    #[test]
    fn freed_instances_are_reused_and_a_new_package_rebuilds_the_scene() {
        let mut renderer = match Renderer::headless(cfg!(debug_assertions), 64, 64) {
            Ok(r) => r,
            Err(e @ (civbb_render::Error::Load(_) | civbb_render::Error::NoDevice)) => {
                return eprintln!("skipped: {e}");
            }
            Err(e) => panic!("{e}"),
        };
        let mut gpu = Gpu::new(&renderer).unwrap();
        let mut scratch = Scratch::default();
        let lean = Package::new(civbb_corpus::generate(&civbb_corpus::lean()), Vec::new());
        let bloated = Package::new(civbb_corpus::generate(&civbb_corpus::bloated()), Vec::new());
        let mut runtime = Runtime::default();
        runtime.extract_to_snapshot();
        let mut frame = |runtime: &mut Runtime, gpu: &mut Gpu| {
            runtime.step(1.0 / 60.0);
            let snapshot = runtime.world.resource::<Snapshot>();
            draw(&mut renderer, gpu, snapshot, &mut scratch).unwrap();
            let b = gpu.built.as_ref().unwrap();
            let free: usize = b.uploads.values().map(|u| u.free.len()).sum();
            (b.capacity, b.scene.used(), b.instances.len(), free)
        };

        runtime.spawn_package(&lean, 0, true, Pose::IDENTITY);
        let second = runtime.spawn_package(&lean, 1, false, Pose::IDENTITY);
        let (capacity, used, ..) = frame(&mut runtime, &mut gpu);
        assert_eq!(used, capacity, "the first snapshot sizes the scene exactly");

        runtime.despawn(second);
        assert_eq!(frame(&mut runtime, &mut gpu), (capacity, used, 1, 1));
        runtime.spawn_package(&lean, 2, false, Pose::IDENTITY);
        assert_eq!(frame(&mut runtime, &mut gpu), (capacity, used, 2, 0));

        runtime.spawn_package(&bloated, 3, false, Pose::IDENTITY);
        let (grown, _, instances, free) = frame(&mut runtime, &mut gpu);
        assert!(grown.vertices > capacity.vertices && grown.skinned >= 2 * capacity.skinned);
        assert_eq!((instances, free), (3, 0));

        // SAFETY: after the wait nothing is in flight.
        unsafe {
            renderer.context().device().device_wait_idle().unwrap();
            gpu.destroy(&renderer);
        }
        assert_eq!(renderer.context().validation_errors(), 0);
    }
}
