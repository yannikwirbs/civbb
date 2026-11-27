//! Desktop viewer: N copies of an avatar in a grid with animator and dynamics running, drawn in a window with an
//! orbit camera, CPU and GPU frame times in the title (design doc §21 Phase 0 deliverable 4, ADRs 0024 and 0025).

use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;

use civbb_core::math::{Pose, mat4_mul};
use civbb_format::Document;
use civbb_format::material::{ColorSpace, ImageFormat};
use civbb_format::math::{Mat4, Vec3};
use civbb_format::mesh::Mesh;
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

use crate::cull::{Frustum, MeshBounds};
use crate::extract::{Camera, Snapshot};
use crate::frame::Runtime;
use crate::island::{Island, Model};
use crate::render_thread::{Drawn, RenderThread};

/// Largest texture side the viewer decodes; larger textures are left out.
const MAX_TEXTURE_SIDE: u32 = 8192;

/// What to show and for how long.
pub struct Config {
    /// The avatar, validated.
    pub avatar: Document,
    /// Its texture images, decoded by [`decode_textures`].
    pub textures: Vec<Option<TextureImage>>,
    /// Copies in the grid.
    pub copies: u32,
    /// Exit after this many presented frames past the warm-up (for automated runs); `None` runs until the window
    /// closes.
    pub frames: Option<u32>,
    /// Frames presented before timing starts: the first one creates the swapchain (~170 ms on the reference PC).
    pub warmup: u32,
}

/// What a run measured, past the warm-up.
#[derive(Clone, Copy, Debug, Default)]
pub struct Summary {
    /// Frames presented.
    pub frames: u32,
    /// Mean CPU time per frame in milliseconds: the busier of the simulation and render threads, which overlap
    /// (ADR 0103).
    pub cpu_ms: f32,
    /// Mean CPU time of the simulation thread: the frame schedule and the hand-over.
    pub main_ms: f32,
    /// Mean CPU time of the frame schedule alone: the avatar systems and the extract step.
    pub systems_ms: f32,
    /// Mean CPU time of the render thread: culling, draw preparation, recording and submitting.
    pub render_ms: f32,
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
    avatar: &Document,
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
    // Data format descriptor, key/value data and supercompression global data inside the file: `basisu` adds the
    // last one's offset unchecked.
    for (offset, length) in [
        (u32_at(48).map(u64::from), u32_at(52).map(u64::from)),
        (u32_at(56).map(u64::from), u32_at(60).map(u64::from)),
        (u64_at(64), u64_at(72)),
    ] {
        let (Some(offset), Some(length)) = (offset, length) else {
            return Err("truncated header".into());
        };
        if offset.checked_add(length).is_none_or(|end| end > bytes.len() as u64) {
            return Err("header range out of bounds".into());
        }
    }
    // Key/value entries are padded to 4 bytes; `basisu` steps its padded cursor past an unaligned end and underflows.
    if [u32_at(56), u32_at(60)].iter().any(|v| v.is_none_or(|v| v % 4 != 0)) {
        return Err("key/value data not 4-byte aligned".into());
    }
    // One 2D image: `basisu` multiplies the level, layer and face counts unchecked (XUASTC, found by the
    // damaged-file test).
    let (depth, layers, faces) = (header(28)?, header(32)?, header(36)?);
    if depth != 0 || layers > 1 || faces != 1 {
        return Err("not a single 2D image".into());
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
    if let Err(e) = viewer.shutdown() {
        viewer.error.get_or_insert(e.to_string());
    }
    match viewer.error.take() {
        Some(e) => Err(e),
        None => Ok(viewer.summary()),
    }
}

/// Runs the viewer's frames without a window on a `width` by `height` offscreen target, back to back: the warm-up,
/// then `cfg.frames` measured ones (ADR 0032). Returns the summary and the device's name; fails with
/// [`civbb_render::Error::NoDevice`] or [`civbb_render::Error::Load`] where there is no device.
pub fn run_headless(cfg: Config, width: u32, height: u32) -> civbb_render::Result<(Summary, String)> {
    let frames = cfg.frames.unwrap_or(0);
    let mut viewer = Viewer::new(cfg);
    let result = Renderer::headless(cfg!(debug_assertions), width, height)
        .and_then(|r| viewer.start(r))
        .and_then(|()| {
            // An offscreen target never skips a frame; stop if it does rather than loop.
            while viewer.timing.frames < frames && viewer.timing.skipped == 0 {
                viewer.frame()?;
            }
            Ok(())
        });
    let ended = viewer.shutdown();
    result
        .and(ended)
        .map(|()| (viewer.summary(), std::mem::take(&mut viewer.device)))
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

/// The matrices that skin `mesh` given the nodes' world placements, into `out`: joint world times inverse bind
/// per skin joint, or the mesh node's placement for an unskinned mesh.
pub fn skin_matrices(mesh: &Mesh, world: &[Pose], out: &mut Vec<Mat4>) {
    out.clear();
    match &mesh.skin {
        Some(skin) if !skin.joints.is_empty() => out.extend(
            skin.joints
                .iter()
                .zip(&skin.inverse_bind)
                .map(|(&j, ib)| mat4_mul(&matrix(&world[j as usize]), ib)),
        ),
        _ => out.push(matrix(&world[mesh.node as usize])),
    }
}

/// The avatar's bind-pose bounds: lowest and highest corner over every mesh's vertices.
pub(crate) fn bounds(avatar: &Document) -> (Vec3, Vec3) {
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
pub(crate) struct Gpu {
    scene: Scene,
    meshes: Vec<MeshData>,
    /// Per mesh, its bind-space spheres for culling (ADR 0077).
    bounds: Vec<MeshBounds>,
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
    pub(crate) fn new(
        renderer: &Renderer,
        avatar: &Document,
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
        // Each submesh may add an outline draw (ADR 0034).
        let max_draws = (2 * submeshes * copies as usize).max(1) as u32;
        Ok(Gpu {
            skinning: Skinning::new(ctx)?,
            forward: Forward::new(ctx, &scene, renderer.color_format()?, max_draws)?,
            scene,
            meshes,
            bounds: avatar.meshes.iter().map(MeshBounds::new).collect(),
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
    pub(crate) unsafe fn destroy(self, renderer: &Renderer) {
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
pub(crate) struct Scratch {
    draws: Vec<Draw>,
    batches: Batches,
    /// Per island, the records last written for its animated materials.
    packed: Vec<Vec<[u32; MATERIAL_WORDS]>>,
    updates: Vec<(u32, [u32; MATERIAL_WORDS])>,
}

/// Running totals for the title and the summary.
#[derive(Default)]
struct Timing {
    frames: u32,
    /// Sums over the frames: CPU per frame (the busier thread), simulation thread, schedule, render thread.
    cpu: f32,
    main: f32,
    systems: f32,
    render: f32,
    gpu: f32,
    gpu_frames: u32,
    /// Draws and skinned meshes in the last frame.
    draws: usize,
    skinned: usize,
    /// Frames the render thread skipped.
    skipped: u32,
    /// Simulation thread and schedule times of the frames handed to the render thread and not yet drawn.
    pending: VecDeque<(f32, f32)>,
    /// Since the title was last updated: start, frames, simulation thread, schedule, render thread.
    window: Option<(Instant, u32, f32, f32, f32)>,
}

struct Viewer {
    // Dropped in this order: the render thread (joined, destroying the renderer), then the window.
    render: Option<RenderThread>,
    window: Option<winit::window::Window>,
    /// The snapshot not in the world or on the render thread, if any: the next frame's extract target.
    free: Option<Snapshot>,
    device: String,
    avatar: Arc<Document>,
    textures: Vec<Option<TextureImage>>,
    model: Arc<Model>,
    runtime: Runtime,
    copies: u32,
    limit: Option<u32>,
    /// Warm-up frames still to present.
    warmup: u32,
    camera: Orbit,
    dragging: bool,
    cursor: Option<(f64, f64)>,
    last: Option<Instant>,
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
        let avatar = Arc::new(cfg.avatar);
        let mut runtime = Runtime::default();
        runtime.extract_to_snapshot(avatar.clone());
        for (i, &pos) in origins.iter().enumerate() {
            let origin = Pose { pos, ..Pose::IDENTITY };
            runtime.spawn(Island::new(model.clone(), i as u64, i == 0, origin));
        }
        let cols = (cfg.copies as f32).sqrt().ceil();
        let rows = (cfg.copies as f32 / cols).ceil();
        let camera = Orbit {
            target: [0.0, lo[1] + height * 0.55, -(rows - 1.0) * spacing / 2.0],
            yaw: 0.0,
            pitch: 0.15,
            distance: (height * 1.6).max(cols * spacing * 1.2),
        };
        Viewer {
            render: None,
            window: None,
            free: None,
            device: String::new(),
            avatar,
            textures: cfg.textures,
            model,
            runtime,
            copies: cfg.copies,
            limit: cfg.frames,
            warmup: cfg.warmup,
            camera,
            dragging: false,
            cursor: None,
            last: None,
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
            main_ms: t.main / n,
            systems_ms: t.systems / n,
            render_ms: t.render / n,
            gpu_ms: (t.gpu_frames > 0).then(|| t.gpu / t.gpu_frames as f32),
            validation_errors: self.validation_errors,
            draws: t.draws,
            skinned: t.skinned,
        }
    }

    /// Uploads the avatar to `renderer`'s device and hands both to a new render thread.
    fn start(&mut self, renderer: Renderer) -> civbb_render::Result<()> {
        self.device = renderer.context().device_name();
        let gpu = Gpu::new(&renderer, &self.avatar, &self.model, &self.textures, self.copies)?;
        self.textures = Vec::new();
        self.render = Some(RenderThread::spawn(renderer, gpu));
        self.free = Some(Snapshot::default());
        Ok(())
    }

    /// Ends the render thread after the frames handed to it; fails if one of them failed.
    fn shutdown(&mut self) -> civbb_render::Result<()> {
        let Some(render) = self.render.take() else {
            return Ok(());
        };
        self.validation_errors = render.end()?;
        Ok(())
    }

    /// Steps the avatars and extracts frame N while the render thread draws frame N-1, then hands N over.
    fn frame(&mut self) -> civbb_render::Result<()> {
        if self.render.is_none() {
            return Ok(());
        }
        let start = Instant::now();
        let dt = self
            .last
            .map_or(0.0, |l| start.duration_since(l).as_secs_f32())
            .min(0.1);
        self.last = Some(start);
        self.runtime.world.insert_resource(Camera {
            eye: self.camera.eye(),
            target: self.camera.target,
            fov_y: FOV_Y,
            clear: BACKGROUND,
        });
        self.runtime.step(dt);
        let systems = start.elapsed().as_secs_f32() * 1000.0;
        // Frame N-1's snapshot comes back once drawn; waiting for it is not this thread's work.
        let mut waited = 0.0;
        let mut snapshot = match self.free.take() {
            Some(s) => s,
            None => {
                let wait = Instant::now();
                let (s, drawn) = self.render.as_mut().expect("checked above").drawn();
                waited = wait.elapsed().as_secs_f32() * 1000.0;
                self.drawn(&drawn?);
                s
            }
        };
        std::mem::swap(&mut snapshot, &mut *self.runtime.world.resource_mut::<Snapshot>());
        self.render.as_mut().expect("checked above").draw(snapshot);
        let main = start.elapsed().as_secs_f32() * 1000.0 - waited;
        self.timing.pending.push_back((main, systems));
        Ok(())
    }

    /// Counts a frame the render thread finished.
    fn drawn(&mut self, drawn: &Drawn) {
        let (main, systems) = self.timing.pending.pop_front().unwrap_or_default();
        let t = &mut self.timing;
        let Some((draws, skinned)) = drawn.counts else {
            t.skipped += 1;
            return;
        };
        (t.draws, t.skinned) = (draws, skinned);
        if self.warmup > 0 {
            self.warmup -= 1;
            return;
        }
        let render = drawn.cpu_ms;
        t.frames += 1;
        t.cpu += main.max(render);
        t.main += main;
        t.systems += systems;
        t.render += render;
        if let Some(gpu_ms) = drawn.gpu_ms {
            t.gpu += gpu_ms;
            t.gpu_frames += 1;
        }
        let now = Instant::now();
        let w = t.window.get_or_insert((now, 0, 0.0, 0.0, 0.0));
        (w.1, w.2, w.3, w.4) = (w.1 + 1, w.2 + main, w.3 + systems, w.4 + render);
        if w.0.elapsed().as_secs_f32() >= 0.5 {
            let n = w.1 as f32;
            let fps = n / w.0.elapsed().as_secs_f32();
            let gpu = drawn.gpu_ms.map_or("n/a".into(), |g| format!("{g:.2} ms"));
            let title = format!(
                "civbb-client view: {} x {}, simulation {:.2} ms (schedule {:.2} ms), render thread {:.2} ms, GPU {gpu}, {fps:.0} fps, {} draws, {} skinned meshes, {} triangles per copy",
                self.copies,
                self.avatar.name,
                w.2 / n,
                w.3 / n,
                w.4 / n,
                draws,
                skinned,
                self.avatar.meshes.iter().map(|m| m.indices.len() / 3).sum::<usize>()
            );
            if let Some(window) = &self.window {
                window.set_title(&title);
            }
            t.window = None;
        }
    }
}

/// The viewer's vertical field of view in radians.
const FOV_Y: f32 = 0.9;

/// The viewer's background, linear.
pub(crate) const BACKGROUND: [f32; 4] = [0.08, 0.09, 0.11, 1.0];

/// Skins and draws the islands of `snapshot`, each with its own instances in `gpu`, from the snapshot's camera;
/// returns the draws and skinned meshes, or `None` if the frame was skipped.
pub(crate) fn draw(
    renderer: &mut Renderer,
    gpu: &Gpu,
    snapshot: &Snapshot,
    s: &mut Scratch,
) -> civbb_render::Result<Option<(usize, usize)>> {
    let Camera {
        eye,
        target,
        fov_y,
        clear,
    } = snapshot.camera;
    s.packed.resize_with(snapshot.islands.len(), Vec::new);
    s.draws.clear();
    s.updates.clear();
    let extent = renderer.extent().unwrap_or_default();
    let aspect = extent.width.max(1) as f32 / extent.height.max(1) as f32;
    let view = View::look_at(eye, target, fov_y, aspect, 0.05);
    let frustum = Frustum::new(&view.view_proj);
    let mut jobs = Vec::new();
    for (i, island) in snapshot.islands.iter().enumerate() {
        // An island's records are rewritten only when they change; the first frame writes them all.
        s.packed[i].resize(island.materials.len(), [0; MATERIAL_WORDS]);
        for ((m, last), id) in island.materials.iter().zip(&mut s.packed[i]).zip(&gpu.own[i]) {
            let record = pack(m, &gpu.bindless);
            if record != *last {
                *last = record;
                s.updates.push((id.index, record));
            }
        }
        let o = island.origin;
        let distance = ((o[0] - eye[0]).powi(2) + (o[1] - eye[1]).powi(2) + (o[2] - eye[2]).powi(2)).sqrt();
        for (m, joints) in island.joints.iter().enumerate() {
            let Some(joints) = joints else { continue };
            // A mesh wholly outside the view is neither skinned nor drawn (ADR 0077).
            let seen = gpu.bounds[m].world_box(joints, &island.weights[m]);
            if !seen.is_some_and(|(lo, hi)| frustum.sees(lo, hi)) {
                continue;
            }
            let materials = gpu.maps[i][m].as_deref().unwrap_or(&gpu.materials);
            gpu.meshes[m].draws(
                &gpu.instances[i][m],
                materials,
                &island.slots[m],
                distance,
                &mut s.draws,
            );
            jobs.push(Job {
                mesh: &gpu.instances[i][m],
                joints,
                weights: &island.weights[m],
            });
        }
    }
    batch(&mut s.draws, &mut s.batches);
    let (draws, skinned) = (s.draws.len(), jobs.len());
    let (batches, updates) = (&s.batches, &s.updates);
    let status = renderer.frame(|graph, target| {
        let skinned = graph.import_buffer(gpu.scene.skinned().handle, &[Access::VertexRead]);
        graph.add(
            Pass::new("skin")
                .buffer(skinned, Access::ComputeWrite)
                // SAFETY: the graph records this outside a rendering scope on the renderer's device.
                .record(|d, cmd| unsafe { gpu.skinning.record(d, cmd, &jobs) }),
        );
        gpu.forward
            .add(graph, &gpu.scene, target, skinned, &view, batches, updates, clear);
    })?;
    Ok((status == FrameStatus::Presented).then_some((draws, skinned)))
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
        if let Err(e) = self.start(renderer) {
            return self.fail(event_loop, e);
        }
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.render {
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
