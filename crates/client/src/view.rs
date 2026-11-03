//! Desktop viewer: N copies of an avatar in a grid with animator and dynamics running, drawn in a window with an
//! orbit camera, CPU and GPU frame times in the title (design doc §21 Phase 0 deliverable 4, ADRs 0024 and 0025).

use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;

use civbb_core::math::Pose;
use civbb_format::Avatar;
use civbb_format::material::{ColorSpace, ImageFormat, Material};
use civbb_format::math::{Mat4, Vec3};
use civbb_render::material::{MATERIAL_WORDS, pack};
use civbb_render::skin::{Job, SkinnedMesh, Skinning};
use civbb_render::{
    Access, Batches, Capacity, Draw, Forward, FrameStatus, MaterialId, MeshData, Pass, Renderer, Scene, View, Window,
    batch,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::WindowId;

use crate::frame;
use crate::island::{Island, Model};

/// Largest texture side the viewer decodes; larger textures are left out.
const MAX_TEXTURE_SIDE: u32 = 8192;

/// What to show and for how long.
pub struct Config {
    /// The avatar, validated.
    pub avatar: Avatar,
    /// Its texture images, decoded by [`decode_textures`].
    pub textures: Vec<Option<TextureImage>>,
    /// Copies in the grid.
    pub copies: u32,
    /// Exit after this many presented frames (for automated runs); `None` runs until the window closes.
    pub frames: Option<u32>,
}

/// What a run measured.
#[derive(Clone, Copy, Debug, Default)]
pub struct Summary {
    /// Frames presented.
    pub frames: u32,
    /// Mean CPU time per frame, simulation and draw preparation, in milliseconds.
    pub cpu_ms: f32,
    /// Mean CPU time of the avatar systems alone.
    pub systems_ms: f32,
    /// Mean GPU time per frame, if the queue has timestamps.
    pub gpu_ms: Option<f32>,
    /// Validation errors reported (debug builds).
    pub validation_errors: u32,
    /// Draws in the last frame, all copies.
    pub draws: usize,
    /// Meshes skinned in the last frame, all copies.
    pub skinned: usize,
}

/// A decoded texture image.
pub struct TextureImage {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Its texels.
    pub texels: Texels,
}

/// Texels as the renderer uploads them.
pub enum Texels {
    /// RGBA8; mips are made on upload.
    Rgba8(Vec<u8>),
    /// BC7 blocks of every mip level, full size first (ADR 0029).
    Bc7(Vec<Vec<u8>>),
}

/// Decodes the avatar's texture blobs with bounded sizes: PNG and JPEG to RGBA8, KTX2 transcoded to BC7; `blob`
/// returns a blob's bytes. Textures that are missing, too large or broken are `None` and draw untextured.
pub fn decode_textures<'b>(
    avatar: &Avatar,
    blob: impl Fn(civbb_format::BlobId) -> Option<&'b [u8]>,
) -> Vec<Option<TextureImage>> {
    avatar
        .textures
        .iter()
        .map(|t| {
            let bytes = blob(t.blob)?;
            let format = match t.format {
                ImageFormat::Png => image::ImageFormat::Png,
                ImageFormat::Jpeg => image::ImageFormat::Jpeg,
                ImageFormat::Ktx2 => {
                    return transcode_ktx2(bytes)
                        .map_err(|e| eprintln!("civbb-client: texture {:?} left out: {e}", t.name))
                        .ok();
                }
            };
            let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(MAX_TEXTURE_SIDE);
            limits.max_image_height = Some(MAX_TEXTURE_SIDE);
            reader.limits(limits);
            match reader.decode() {
                Ok(image) => {
                    let rgba = image.into_rgba8();
                    Some(TextureImage {
                        width: rgba.width(),
                        height: rgba.height(),
                        texels: Texels::Rgba8(rgba.into_raw()),
                    })
                }
                Err(e) => {
                    eprintln!("civbb-client: texture {:?} left out: {e}", t.name);
                    None
                }
            }
        })
        .collect()
}

/// Transcodes a Basis Universal KTX2 texture to BC7, every mip level (ADR 0029). Untrusted input: the container
/// is checked to be a single 2D image of bounded size with a full or partial mip chain before any level is
/// transcoded, and each level's size against its blocks after.
pub fn transcode_ktx2(bytes: &[u8]) -> Result<TextureImage, String> {
    const MAGIC: &[u8] = b"\xabKTX 20\xbb\r\n\x1a\n";
    if !bytes.starts_with(MAGIC) {
        return Err("not KTX2".into());
    }
    // The level index first: `basisu` allocates a Zstandard level's stated uncompressed length unchecked.
    let u32_at = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")))
    };
    let u64_at = |at: usize| {
        bytes
            .get(at..at + 8)
            .map(|b| u64::from_le_bytes(b.try_into().expect("8 bytes")))
    };
    let header = |at| u32_at(at).ok_or("truncated header");
    let (width, height, level_count) = (header(20)?, header(24)?, header(40)?);
    let side = 1..=MAX_TEXTURE_SIDE;
    if !side.contains(&width) || !side.contains(&height) {
        return Err(format!("size {width}x{height}"));
    }
    let most = 32 - width.max(height).leading_zeros();
    if !(1..=most).contains(&level_count) {
        return Err(format!("{level_count} levels"));
    }
    for level in 0..level_count {
        let at = 80 + 24 * level as usize;
        let (Some(offset), Some(length), Some(uncompressed)) = (u64_at(at), u64_at(at + 8), u64_at(at + 16)) else {
            return Err("truncated level index".into());
        };
        let (w, h) = ((width >> level).max(1), (height >> level).max(1));
        let blocks = u64::from(w.div_ceil(4) * h.div_ceil(4)) * 16;
        if offset.checked_add(length).is_none_or(|end| end > bytes.len() as u64) || uncompressed > blocks {
            return Err(format!("level {level} out of bounds"));
        }
    }
    let t = basisu::Transcoder::new(bytes).map_err(|e| format!("{e:?}"))?;
    if t.base_dimensions() != (width, height) {
        return Err("inconsistent size".into());
    }
    if t.layer_count() > 1 || t.face_count() != 1 || t.is_video() || t.level_count() != level_count {
        return Err("not a single 2D image with mips".into());
    }
    let mut levels = Vec::with_capacity(t.level_count() as usize);
    for level in 0..t.level_count() {
        let blocks = t
            .transcode(level, basisu::TargetFormat::Bc7Rgba, basisu::DecodeFlags::NONE)
            .map_err(|e| format!("level {level}: {e:?}"))?;
        let (w, h) = ((width >> level).max(1), (height >> level).max(1));
        if blocks.len() as u64 != u64::from(w.div_ceil(4) * h.div_ceil(4)) * 16 {
            return Err(format!("level {level}: {} bytes", blocks.len()));
        }
        levels.push(blocks);
    }
    Ok(TextureImage {
        width,
        height,
        texels: Texels::Bc7(levels),
    })
}

/// Opens the window and runs until it closes or the frame limit is reached.
pub fn run(cfg: Config) -> Result<Summary, String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut viewer = Viewer::new(cfg);
    event_loop.run_app(&mut viewer).map_err(|e| e.to_string())?;
    viewer.shutdown();
    match viewer.error.take() {
        Some(e) => Err(e),
        None => Ok(viewer.summary()),
    }
}

/// An orbit camera: yaw and pitch around a target at a distance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    /// The point looked at.
    pub target: Vec3,
    /// Rotation about +Y in radians; 0 looks from +Z.
    pub yaw: f32,
    /// Elevation in radians, kept within ±1.5.
    pub pitch: f32,
    /// Distance from the target.
    pub distance: f32,
}

impl Orbit {
    /// The camera position.
    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let t = self.target;
        let d = self.distance;
        [t[0] + d * cp * sy, t[1] + d * sp, t[2] + d * cp * cy]
    }

    /// Turns by a mouse drag of `dx`, `dy` pixels.
    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.005;
        self.pitch = (self.pitch + dy * 0.005).clamp(-1.5, 1.5);
    }

    /// Zooms by `notches` wheel steps (positive moves closer).
    pub fn zoom(&mut self, notches: f32) {
        self.distance = (self.distance * 0.9_f32.powf(notches)).clamp(0.2, 500.0);
    }
}

/// Where each of `copies` avatars stands: a square grid `spacing` apart, centred on X, rows going back along -Z.
pub fn grid(copies: u32, spacing: f32) -> Vec<Vec3> {
    let cols = (copies as f32).sqrt().ceil().max(1.0) as u32;
    (0..copies)
        .map(|i| {
            let (c, r) = (i % cols, i / cols);
            [
                (c as f32 - (cols - 1) as f32 / 2.0) * spacing,
                0.0,
                -(r as f32) * spacing,
            ]
        })
        .collect()
}

/// A placement as a column-major matrix.
pub fn matrix(p: &Pose) -> Mat4 {
    let [x, y, z, w] = p.rot;
    let s = p.scale;
    [
        (1.0 - 2.0 * (y * y + z * z)) * s,
        2.0 * (x * y + z * w) * s,
        2.0 * (x * z - y * w) * s,
        0.0,
        2.0 * (x * y - z * w) * s,
        (1.0 - 2.0 * (x * x + z * z)) * s,
        2.0 * (y * z + x * w) * s,
        0.0,
        2.0 * (x * z + y * w) * s,
        2.0 * (y * z - x * w) * s,
        (1.0 - 2.0 * (x * x + y * y)) * s,
        0.0,
        p.pos[0],
        p.pos[1],
        p.pos[2],
        1.0,
    ]
}

/// Column-major `a * b`.
fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|i| {
        let (col, row) = (i / 4, i % 4);
        (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum()
    })
}

/// The avatar's bind-pose bounds: lowest and highest corner over every mesh's vertices.
fn bounds(avatar: &Avatar) -> (Vec3, Vec3) {
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in avatar.meshes.iter().flat_map(|m| &m.positions) {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    if lo[0] > hi[0] { ([0.0; 3], [0.0; 3]) } else { (lo, hi) }
}

/// Everything on the GPU: the scene, one skinned instance per island and mesh, and the pipelines.
struct Gpu {
    scene: Scene,
    meshes: Vec<MeshData>,
    instances: Vec<Vec<SkinnedMesh>>,
    /// The avatar's materials, shared by every island.
    materials: Vec<MaterialId>,
    /// Bindless index of each avatar texture.
    bindless: Vec<u32>,
    /// Per island, its own records for animated renderers, in [`Island::materials`] order (ADR 0025).
    own: Vec<Vec<MaterialId>>,
    /// Per island and mesh, the material map of an animated renderer: the shared one with its own records.
    maps: Vec<Vec<Option<Vec<MaterialId>>>>,
    skinning: Skinning,
    forward: Forward,
}

impl Gpu {
    fn new(
        renderer: &Renderer,
        avatar: &Avatar,
        model: &Model,
        textures: &[Option<TextureImage>],
        copies: u32,
    ) -> civbb_render::Result<Self> {
        let ctx = renderer.context();
        let per_island: usize = model.animated_materials().map(|(_, m)| m.len()).sum();
        let materials = avatar.materials.len() + per_island * copies as usize;
        let capacity = Capacity::for_meshes(&avatar.meshes, copies, materials as u32);
        let mut scene = Scene::new(ctx, capacity)?;
        let mut bindless = Vec::with_capacity(textures.len());
        for (t, decoded) in avatar.textures.iter().zip(textures) {
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
        let materials = avatar
            .materials
            .iter()
            .map(|m| scene.add_material(ctx, m, &bindless))
            .collect::<civbb_render::Result<Vec<_>>>()?;
        let mut own = Vec::with_capacity(copies as usize);
        let mut maps = Vec::with_capacity(copies as usize);
        for _ in 0..copies {
            let mut records = Vec::with_capacity(per_island);
            let mut island_maps = vec![None; avatar.meshes.len()];
            for (mesh, indices) in model.animated_materials() {
                let mut map = materials.clone();
                for &m in indices {
                    let id = scene.add_material(ctx, &avatar.materials[m as usize], &bindless)?;
                    map[m as usize] = id;
                    records.push(id);
                }
                island_maps[mesh as usize] = Some(map);
            }
            own.push(records);
            maps.push(island_maps);
        }
        let meshes = avatar
            .meshes
            .iter()
            .map(|m| scene.add_mesh(ctx, m))
            .collect::<civbb_render::Result<Vec<_>>>()?;
        let mut instances = Vec::with_capacity(copies as usize);
        for _ in 0..copies {
            instances.push(
                meshes
                    .iter()
                    .map(|m| SkinnedMesh::new(ctx, &mut scene, m))
                    .collect::<civbb_render::Result<Vec<_>>>()?,
            );
        }
        let submeshes: usize = meshes.iter().map(|m| m.submeshes().len()).sum();
        let max_draws = (submeshes * copies as usize).max(1) as u32;
        Ok(Gpu {
            skinning: Skinning::new(ctx)?,
            forward: Forward::new(ctx, &scene, renderer.color_format()?, max_draws)?,
            scene,
            meshes,
            instances,
            materials,
            bindless,
            own,
            maps,
        })
    }

    /// # Safety
    ///
    /// The GPU is idle.
    unsafe fn destroy(self, renderer: &Renderer) {
        let ctx = renderer.context();
        // SAFETY: per the contract.
        unsafe {
            self.forward.destroy(ctx);
            self.skinning.destroy(ctx.device());
            for i in self.instances.into_iter().flatten() {
                i.destroy(ctx);
            }
            self.scene.destroy(ctx);
        }
    }
}

/// Per-frame scratch reused across frames.
#[derive(Default)]
struct Scratch {
    /// Per island and mesh: skinning matrices and blendshape weights; `None` for a hidden mesh.
    joints: Vec<Vec<Option<Vec<Mat4>>>>,
    weights: Vec<Vec<f32>>,
    all_weights: Vec<Vec<Vec<f32>>>,
    draws: Vec<Draw>,
    batches: Batches,
    /// Per island, its animated materials and the records last written for them.
    materials: Vec<Vec<Material>>,
    packed: Vec<Vec<[u32; MATERIAL_WORDS]>>,
    updates: Vec<(u32, [u32; MATERIAL_WORDS])>,
}

/// Running totals for the title and the summary.
#[derive(Default)]
struct Timing {
    frames: u32,
    cpu: f32,
    systems: f32,
    gpu: f32,
    gpu_frames: u32,
    /// Draws and skinned meshes in the last frame.
    draws: usize,
    skinned: usize,
    /// Since the title was last updated: start, frames, CPU, systems.
    window: Option<(Instant, u32, f32, f32)>,
}

struct Viewer {
    // Dropped in this order: GPU resources (destroyed in `shutdown`), the renderer, then the window.
    gpu: Option<Gpu>,
    renderer: Option<Renderer>,
    window: Option<winit::window::Window>,
    avatar: Avatar,
    textures: Vec<Option<TextureImage>>,
    model: Arc<Model>,
    islands: Vec<Island>,
    copies: u32,
    limit: Option<u32>,
    camera: Orbit,
    dragging: bool,
    cursor: Option<(f64, f64)>,
    last: Option<Instant>,
    scratch: Scratch,
    timing: Timing,
    validation_errors: u32,
    error: Option<String>,
}

impl Viewer {
    fn new(cfg: Config) -> Self {
        let (lo, hi) = bounds(&cfg.avatar);
        let width = (hi[0] - lo[0]).max(hi[2] - lo[2]);
        let height = (hi[1] - lo[1]).max(0.5);
        let spacing = (width * 1.5).max(0.8);
        let origins = grid(cfg.copies, spacing);
        let model = Arc::new(Model::new(&cfg.avatar));
        let islands = origins
            .iter()
            .enumerate()
            .map(|(i, &pos)| {
                let origin = Pose { pos, ..Pose::IDENTITY };
                Island::new(model.clone(), i as u64, i == 0, origin)
            })
            .collect();
        let cols = (cfg.copies as f32).sqrt().ceil();
        let rows = (cfg.copies as f32 / cols).ceil();
        let camera = Orbit {
            target: [0.0, lo[1] + height * 0.55, -(rows - 1.0) * spacing / 2.0],
            yaw: 0.0,
            pitch: 0.15,
            distance: (height * 1.6).max(cols * spacing * 1.2),
        };
        Viewer {
            gpu: None,
            renderer: None,
            window: None,
            avatar: cfg.avatar,
            textures: cfg.textures,
            model,
            islands,
            copies: cfg.copies,
            limit: cfg.frames,
            camera,
            dragging: false,
            cursor: None,
            last: None,
            scratch: Scratch::default(),
            timing: Timing::default(),
            validation_errors: 0,
            error: None,
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, e: impl ToString) {
        self.error = Some(e.to_string());
        event_loop.exit();
    }

    fn summary(&self) -> Summary {
        let t = &self.timing;
        let n = t.frames.max(1) as f32;
        Summary {
            frames: t.frames,
            cpu_ms: t.cpu / n,
            systems_ms: t.systems / n,
            gpu_ms: (t.gpu_frames > 0).then(|| t.gpu / t.gpu_frames as f32),
            validation_errors: self.validation_errors,
            draws: t.draws,
            skinned: t.skinned,
        }
    }

    /// Waits for the GPU and destroys what the renderer does not own.
    fn shutdown(&mut self) {
        if let Some(r) = &self.renderer {
            self.validation_errors = r.context().validation_errors();
            if let Some(gpu) = self.gpu.take() {
                // SAFETY: after the wait nothing is in flight.
                unsafe {
                    let _ = r.context().device().device_wait_idle();
                    gpu.destroy(r);
                }
            }
        }
        self.renderer = None;
    }

    /// Steps the avatars, skins and draws them; returns whether a frame was presented.
    fn frame(&mut self) -> civbb_render::Result<bool> {
        let start = Instant::now();
        let dt = self
            .last
            .map_or(0.0, |l| start.duration_since(l).as_secs_f32())
            .min(0.1);
        self.last = Some(start);
        frame::step(&mut self.islands, |_, _| true, dt);
        let systems = start.elapsed().as_secs_f32() * 1000.0;

        let (Some(renderer), Some(gpu)) = (&mut self.renderer, &self.gpu) else {
            return Ok(false);
        };
        let s = &mut self.scratch;
        s.joints.resize_with(self.islands.len(), Vec::new);
        s.all_weights.resize_with(self.islands.len(), Vec::new);
        s.materials.resize_with(self.islands.len(), Vec::new);
        s.packed.resize_with(self.islands.len(), Vec::new);
        let eye = self.camera.eye();
        s.draws.clear();
        s.updates.clear();
        for (i, island) in self.islands.iter().enumerate() {
            island.shape_weights(&mut s.weights);
            std::mem::swap(&mut s.all_weights[i], &mut s.weights);
            // An island's records are rewritten only when they change; the first frame writes them all.
            island.materials(&mut s.materials[i]);
            s.packed[i].resize(s.materials[i].len(), [0; MATERIAL_WORDS]);
            for ((m, last), id) in s.materials[i].iter().zip(&mut s.packed[i]).zip(&gpu.own[i]) {
                let record = pack(m, &gpu.bindless);
                if record != *last {
                    *last = record;
                    s.updates.push((id.index, record));
                }
            }
            let world = island.world();
            let joints = &mut s.joints[i];
            joints.resize(self.avatar.meshes.len(), None);
            let o = island.origin.pos;
            let distance = ((o[0] - eye[0]).powi(2) + (o[1] - eye[1]).powi(2) + (o[2] - eye[2]).powi(2)).sqrt();
            for (m, mesh) in self.avatar.meshes.iter().enumerate() {
                if !island.active()[mesh.node as usize] {
                    joints[m] = None;
                    continue;
                }
                let out = joints[m].get_or_insert_with(Vec::new);
                out.clear();
                match &mesh.skin {
                    Some(skin) if !skin.joints.is_empty() => out.extend(
                        skin.joints
                            .iter()
                            .zip(&skin.inverse_bind)
                            .map(|(&j, ib)| mul(&matrix(&world[j as usize]), ib)),
                    ),
                    _ => out.push(matrix(&world[mesh.node as usize])),
                }
                let materials = gpu.maps[i][m].as_deref().unwrap_or(&gpu.materials);
                gpu.meshes[m].draws(&gpu.instances[i][m], materials, distance, &mut s.draws);
            }
        }
        let jobs: Vec<Job> = s
            .joints
            .iter()
            .zip(&s.all_weights)
            .zip(&gpu.instances)
            .flat_map(|((joints, weights), instances)| {
                joints.iter().zip(weights).zip(instances).filter_map(|((j, w), mesh)| {
                    Some(Job {
                        mesh,
                        joints: j.as_ref()?,
                        weights: w,
                    })
                })
            })
            .collect();
        batch(&mut s.draws, &mut s.batches);
        let (draws, skinned) = (s.draws.len(), jobs.len());
        let extent = renderer.extent().unwrap_or_default();
        let aspect = extent.width.max(1) as f32 / extent.height.max(1) as f32;
        let view = View::look_at(eye, self.camera.target, 0.9, aspect, 0.05);
        let (batches, updates) = (&s.batches, &s.updates);
        let status = renderer.frame(|graph, target| {
            let skinned = graph.import_buffer(gpu.scene.skinned().handle, &[Access::VertexRead]);
            graph.add(
                Pass::new("skin")
                    .buffer(skinned, Access::ComputeWrite)
                    // SAFETY: the graph records this outside a rendering scope on the renderer's device.
                    .record(|d, cmd| unsafe { gpu.skinning.record(d, cmd, &jobs) }),
            );
            gpu.forward.add(
                graph,
                &gpu.scene,
                target,
                skinned,
                &view,
                batches,
                updates,
                [0.08, 0.09, 0.11, 1.0],
            );
        })?;
        if status == FrameStatus::Skipped {
            return Ok(false);
        }
        // Waiting for vsync and the GPU is not CPU work.
        let cpu = start.elapsed().as_secs_f32() * 1000.0 - renderer.blocked_ms();
        let t = &mut self.timing;
        (t.draws, t.skinned) = (draws, skinned);
        t.frames += 1;
        t.cpu += cpu;
        t.systems += systems;
        if let Some(gpu_ms) = renderer.gpu_ms() {
            t.gpu += gpu_ms;
            t.gpu_frames += 1;
        }
        let w = t.window.get_or_insert((start, 0, 0.0, 0.0));
        (w.1, w.2, w.3) = (w.1 + 1, w.2 + cpu, w.3 + systems);
        if w.0.elapsed().as_secs_f32() >= 0.5 {
            let n = w.1 as f32;
            let fps = n / w.0.elapsed().as_secs_f32();
            let gpu = renderer.gpu_ms().map_or("n/a".into(), |g| format!("{g:.2} ms"));
            let title = format!(
                "civbb-client view: {} x {}, CPU {:.2} ms (avatar systems {:.2} ms), GPU {gpu}, {fps:.0} fps, {} draws, {} skinned meshes",
                self.copies,
                self.avatar.name,
                w.2 / n,
                w.3 / n,
                draws,
                skinned
            );
            if let Some(window) = &self.window {
                window.set_title(&title);
            }
            t.window = None;
        }
        Ok(true)
    }
}

impl ApplicationHandler for Viewer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = winit::window::Window::default_attributes()
            .with_title("civbb-client view")
            .with_inner_size(PhysicalSize::new(1280, 720));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => w,
            Err(e) => return self.fail(event_loop, e),
        };
        let handles = match (window.display_handle(), window.window_handle()) {
            (Ok(d), Ok(w)) => Window {
                display: d.as_raw(),
                window: w.as_raw(),
            },
            _ => return self.fail(event_loop, "no window handles"),
        };
        let size = window.inner_size();
        // SAFETY: the window is stored next to the renderer and dropped after it.
        let renderer = unsafe { Renderer::new(cfg!(debug_assertions), handles, size.width, size.height) };
        let renderer = match renderer {
            Ok(r) => r,
            Err(e) => return self.fail(event_loop, e),
        };
        match Gpu::new(&renderer, &self.avatar, &self.model, &self.textures, self.copies) {
            Ok(gpu) => self.gpu = Some(gpu),
            Err(e) => {
                self.renderer = Some(renderer);
                return self.fail(event_loop, e);
            }
        }
        self.textures = Vec::new();
        self.renderer = Some(renderer);
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => self.dragging = state == ElementState::Pressed,
            WindowEvent::CursorMoved { position, .. } => {
                if let (true, Some((x, y))) = (self.dragging, self.cursor) {
                    self.camera.drag((position.x - x) as f32, (position.y - y) as f32);
                }
                self.cursor = Some((position.x, position.y));
            }
            WindowEvent::MouseWheel { delta, .. } => self.camera.zoom(match delta {
                MouseScrollDelta::LineDelta(_, y) => y,
                MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
            }),
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.frame() {
                    return self.fail(event_loop, e);
                }
                if self.limit.is_some_and(|l| self.timing.frames >= l) {
                    event_loop.exit();
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}
