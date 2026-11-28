//! Desktop viewer: N copies of each of several packages in a grid with animator and dynamics running, drawn in a
//! window with an orbit camera, CPU and GPU frame times in the title (design doc §21 Phase 0 deliverable 4, ADRs 0024,
//! 0025 and 0104).

use std::collections::{HashSet, VecDeque};
use std::io::Cursor;
use std::time::Instant;

use bevy_tasks::Task;
use civbb_core::math::{Pose, mat4_mul};
use civbb_format::Document;
use civbb_format::material::ImageFormat;
use civbb_format::math::{Mat4, Vec3};
use civbb_format::mesh::Mesh;
use civbb_render::{Renderer, Window};
use civbb_ui::radial::Pointer;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{CursorGrabMode, WindowId};

use crate::controller::WorldMesh;
use crate::extract::{Camera, Snapshot};
use crate::frame::Runtime;
use crate::gamepad::{Pad, Turning};
use crate::gpu::Gpu;
use crate::menu::ExpressionMenu;
use crate::package::Handle;
use crate::player::{Input, Player, View, WorldRules};
use crate::render_thread::{Drawn, Own, Present, RenderThread};
use crate::station::Station;
use crate::tracking::{HeadsetPoses, Scripted};

/// Largest texture side the viewer decodes; larger textures are left out.
const MAX_TEXTURE_SIDE: u32 = 8192;
/// The factor one press of `=` or `-` multiplies or divides the eye height by (ADR 0114).
const RESIZE: f32 = 1.1;

/// The viewer's samples per pixel unless asked otherwise (ADR 0120).
pub const MSAA: u32 = 4;

/// What to show and for how long.
pub struct Config {
    /// The packages, loading ([`crate::package::spawn`]): each one's copies spawn on the frame its load is done; a
    /// failed load ends the viewer with its error.
    pub packages: Vec<Task<Result<Handle, String>>>,
    /// Copies of each package in the grid.
    pub copies: u32,
    /// Exit after this many presented frames past the warm-up (for automated runs); `None` runs until the window
    /// closes.
    pub frames: Option<u32>,
    /// Frames presented before timing starts: the first one creates the swapchain (~170 ms on the reference PC).
    pub warmup: u32,
    /// A world shown at the origin; the player walks on its rigid meshes (ADR 0110). `None`: a flat ground at y 0.
    pub world: Option<Handle>,
    /// The world's stations (ADR 0114).
    pub stations: Vec<Station>,
    /// Scripted trackers drive the player's avatar (ADR 0113).
    pub trackers: Option<Scripted>,
    /// How a gamepad's right stick turns the player (ADR 0115).
    pub turning: Turning,
    /// Samples per pixel of the forward pass, as far as the device supports them (ADR 0120).
    pub msaa: u32,
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
#[derive(Clone)]
pub struct TextureImage {
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Its texels.
    pub texels: Texels,
}

/// Texels as the renderer uploads them.
#[derive(Clone)]
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
            while viewer.timing.frames < frames && viewer.timing.skipped == 0 && viewer.error.is_none() {
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

pub(crate) struct Viewer {
    // Dropped in this order: the render thread (joined, destroying the renderer), then the window.
    render: Option<RenderThread>,
    window: Option<winit::window::Window>,
    /// The snapshot not in the world or on the render thread, if any: the next frame's extract target.
    free: Option<Snapshot>,
    device: String,
    /// Package loads not done yet.
    loads: Vec<Task<Result<Handle, String>>>,
    /// Packages shown, in the order their loads finished.
    shown: Vec<Handle>,
    /// Grid slots: copies times packages asked for.
    slots: u32,
    /// The grid's spacing, made for the first package shown.
    spacing: f32,
    runtime: Runtime,
    copies: u32,
    limit: Option<u32>,
    /// Warm-up frames still to present.
    warmup: u32,
    camera: Orbit,
    dragging: bool,
    cursor: Option<(f64, f64)>,
    /// Keys held down, for the player's input (ADR 0106).
    held: HashSet<KeyCode>,
    /// Gamepad events, if the platform has them (ADR 0115).
    pads: Option<gilrs::Gilrs>,
    /// The gamepads' sticks and settings.
    pad: Pad,
    /// Samples per pixel asked for (ADR 0120).
    msaa: u32,
    /// The cursor is confined and hidden, and mouse motion turns the player.
    captured: bool,
    last: Option<Instant>,
    timing: Timing,
    validation_errors: u32,
    pub(crate) error: Option<String>,
    /// The XR runtime's frame the next [`Viewer::frame`] is for, if it paces them (ADR 0123).
    pub(crate) pace: Option<civbb_xr::xr::FrameState>,
}

impl Viewer {
    pub(crate) fn new(cfg: Config) -> Self {
        let mut runtime = Runtime::default();
        if let Some(world) = &cfg.world {
            (runtime.world).insert_resource(WorldMesh::from_document(&world.avatar, Pose::IDENTITY));
            runtime.spawn_package(world, u64::MAX, false, Pose::IDENTITY);
            // The generated test world allows flight (ADR 0119).
            runtime.world.insert_resource(WorldRules { flight: true });
        }
        runtime.add_player();
        runtime.add_menu();
        runtime.world.resource_mut::<ExpressionMenu>().load_fonts();
        runtime.world.spawn_batch(cfg.stations.clone());
        if let Some(scripted) = cfg.trackers.clone() {
            runtime.add_trackers(Some(scripted));
        }
        runtime.extract_to_snapshot();
        Viewer {
            render: None,
            window: None,
            free: None,
            device: String::new(),
            slots: cfg.copies * cfg.packages.len() as u32,
            loads: cfg.packages,
            shown: Vec::new(),
            spacing: 1.0,
            runtime,
            copies: cfg.copies,
            limit: cfg.frames,
            warmup: cfg.warmup,
            camera: Orbit {
                target: [0.0, 1.0, 0.0],
                yaw: 0.0,
                pitch: 0.15,
                distance: 3.0,
            },
            dragging: false,
            cursor: None,
            held: HashSet::new(),
            pads: gilrs::Gilrs::new().ok(),
            pad: Pad::new(cfg.turning),
            msaa: cfg.msaa,
            captured: false,
            last: None,
            timing: Timing::default(),
            validation_errors: 0,
            error: None,
            pace: None,
        }
    }

    /// Shows every package whose load is done; a failed load is kept as the viewer's error.
    fn arrive(&mut self) {
        let mut k = 0;
        while k < self.loads.len() {
            if !self.loads[k].is_finished() {
                k += 1;
                continue;
            }
            match bevy_tasks::block_on(self.loads.swap_remove(k)) {
                Ok(p) => self.show(p),
                Err(e) => {
                    self.error.get_or_insert(e);
                }
            }
        }
    }

    /// Spawns `copies` islands of `p` in the next grid slots; the first package shown frames the camera and its
    /// first copy is the player's.
    fn show(&mut self, p: Handle) {
        if self.shown.is_empty() {
            let (lo, hi) = bounds(&p.avatar);
            let width = (hi[0] - lo[0]).max(hi[2] - lo[2]);
            let height = (hi[1] - lo[1]).max(0.5);
            self.spacing = (width * 1.5).max(0.8);
            let cols = (self.slots as f32).sqrt().ceil();
            let rows = (self.slots as f32 / cols).ceil();
            self.camera = Orbit {
                target: [0.0, lo[1] + height * 0.55, -(rows - 1.0) * self.spacing / 2.0],
                yaw: 0.0,
                pitch: 0.15,
                distance: (height * 1.6).max(cols * self.spacing * 1.2),
            };
        }
        let origins = grid(self.slots, self.spacing);
        let first = self.shown.len() * self.copies as usize;
        for (i, &pos) in origins.iter().enumerate().skip(first).take(self.copies as usize) {
            let origin = Pose { pos, ..Pose::IDENTITY };
            let e = self.runtime.spawn_package(&p, i as u64, i == 0, origin);
            if i == 0 {
                let mut player = self.runtime.world.resource_mut::<Player>();
                (player.island, player.pos) = (Some(e), pos);
            }
        }
        self.shown.push(p);
    }

    /// Confines and hides the cursor so mouse motion turns the player, or frees it.
    fn capture(&mut self, on: bool) {
        self.captured = on;
        let Some(window) = &self.window else { return };
        let grab = if on {
            // Windows confines but cannot lock; other platforms may only lock.
            window
                .set_cursor_grab(CursorGrabMode::Confined)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked))
        } else {
            window.set_cursor_grab(CursorGrabMode::None)
        };
        if let Err(e) = grab {
            eprintln!("civbb-client: cursor grab: {e}");
        }
        window.set_cursor_visible(!on);
    }

    /// Cycles the camera mode (V or a pad's Select), capturing the cursor outside orbit mode.
    fn cycle_view(&mut self) {
        let mut player = self.runtime.world.resource_mut::<Player>();
        player.view = player.view.next();
        let view = player.view;
        self.capture(view != View::Orbit);
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, e: impl ToString) {
        self.error = Some(e.to_string());
        event_loop.exit();
    }

    pub(crate) fn summary(&self) -> Summary {
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

    /// Hands `renderer` to a new render thread.
    fn start(&mut self, renderer: Renderer) -> civbb_render::Result<()> {
        self.start_on(renderer, 1, Box::new(Own))
    }

    /// Hands `renderer` to a new render thread drawing `views` at once through `present`.
    pub(crate) fn start_on(
        &mut self,
        renderer: Renderer,
        views: u32,
        present: Box<dyn Present>,
    ) -> civbb_render::Result<()> {
        self.device = renderer.context().device_name();
        let gpu = Gpu::new(&renderer, self.msaa, views)?;
        self.render = Some(RenderThread::spawn(renderer, gpu, present));
        self.free = Some(Snapshot::default());
        Ok(())
    }

    /// Whether the frames asked for are done.
    pub(crate) fn done(&self) -> bool {
        self.limit.is_some_and(|l| self.timing.frames >= l)
    }

    /// Makes the headset the player's tracker source (ADR 0124), its poses set by [`Viewer::headset`].
    pub(crate) fn add_headset(&mut self) {
        self.runtime.add_headset();
    }

    /// The headset's poses for the next [`Viewer::frame`].
    pub(crate) fn headset(&mut self, poses: civbb_xr::Poses) {
        self.runtime.world.insert_resource(HeadsetPoses(poses));
    }

    /// Whether every load is done and the warm-up is over: frames are being measured.
    pub(crate) fn warm(&self) -> bool {
        self.loads.is_empty() && self.warmup == 0
    }

    /// Waits until the render thread has handed back the snapshot it holds, if any: no frame is being drawn.
    pub(crate) fn settle(&mut self) -> civbb_render::Result<()> {
        if self.free.is_none()
            && let Some(render) = &mut self.render
        {
            let (s, drawn) = render.drawn();
            self.drawn(&drawn?);
            self.free = Some(s);
        }
        Ok(())
    }

    /// Ends the render thread after the frames handed to it; fails if one of them failed.
    pub(crate) fn shutdown(&mut self) -> civbb_render::Result<()> {
        let Some(render) = self.render.take() else {
            return Ok(());
        };
        self.validation_errors = render.end()?;
        Ok(())
    }

    /// Steps the avatars and extracts frame N while the render thread draws frame N-1, then hands N over.
    pub(crate) fn frame(&mut self) -> civbb_render::Result<()> {
        if self.render.is_none() {
            return Ok(());
        }
        self.arrive();
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
        let key = |k| f32::from(u8::from(self.held.contains(&k)));
        let (forward, right) = (
            key(KeyCode::KeyW) - key(KeyCode::KeyS),
            key(KeyCode::KeyD) - key(KeyCode::KeyA),
        );
        let run = self.held.contains(&KeyCode::ShiftLeft) || self.held.contains(&KeyCode::ShiftRight);
        let open = self.runtime.world.resource::<ExpressionMenu>().radial.is_open();
        let mut input = self.runtime.world.resource_mut::<Input>();
        (input.forward, input.right, input.run) = (forward, right, run);
        let (mut cycle, mut menu, mut pointer, mut stick) = (false, false, Pointer::default(), false);
        while let Some(gilrs::Event { event, .. }) = self.pads.as_mut().and_then(|p| p.next_event()) {
            use gilrs::{Axis, Button, EventType as E};
            match event {
                E::ButtonPressed(Button::Start, _) => menu = true,
                // The open menu takes the pad's buttons: South acts, East goes back, the rest do nothing (#158).
                E::ButtonPressed(b, _) if open => _ = Pad::menu_button(b, true, &mut pointer),
                E::ButtonReleased(b, _) if open => _ = Pad::menu_button(b, false, &mut pointer),
                E::ButtonPressed(b, _) => cycle |= self.pad.button(b, &mut input),
                E::AxisChanged(a, v, _) => {
                    stick |= matches!(a, Axis::RightStickX | Axis::RightStickY);
                    self.pad.axis(a, v);
                }
                E::Disconnected => self.pad.reset(),
                _ => {}
            }
        }
        self.pad.apply(&mut input, dt, !open);
        if cycle {
            self.cycle_view();
        }
        let size = self.window.as_ref().map(|w| w.inner_size()).unwrap_or_default();
        let right = self.pad.right();
        let mut m = self.runtime.world.resource_mut::<ExpressionMenu>();
        m.toggle |= menu;
        m.screen = [size.width as f32, size.height as f32];
        // The right stick points from its first move until the mouse moves.
        if stick || m.stick.is_some() {
            m.stick = Some(right);
        }
        m.pointer.press |= pointer.press;
        m.pointer.release |= pointer.release;
        m.pointer.back |= pointer.back;
        let was_open = m.radial.is_open();
        self.runtime.step(dt);
        // The open menu frees the cursor; closing it gives look mode back outside orbit view (ADR 0117).
        let open = self.runtime.world.resource::<ExpressionMenu>().radial.is_open();
        if open != was_open {
            self.capture(!open && self.runtime.world.resource::<Player>().view != View::Orbit);
        }
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
        let pace = self.pace.take();
        self.render.as_mut().expect("checked above").draw(snapshot, pace);
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
        if !self.loads.is_empty() {
            return;
        }
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
                self.shown
                    .iter()
                    .map(|p| p.avatar.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                w.2 / n,
                w.3 / n,
                w.4 / n,
                draws,
                skinned,
                (self.shown.iter().flat_map(|p| &p.avatar.meshes))
                    .map(|m| m.indices.len() / 3)
                    .sum::<usize>()
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
            } => {
                let pressed = state == ElementState::Pressed;
                let mut menu = self.runtime.world.resource_mut::<ExpressionMenu>();
                if menu.radial.is_open() {
                    // Both may come in one frame.
                    menu.pointer.press |= pressed;
                    menu.pointer.release |= !pressed;
                    self.dragging = false;
                } else if self.runtime.world.resource::<Player>().view == View::Orbit {
                    self.dragging = pressed;
                } else if pressed {
                    self.capture(true);
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => self.runtime.world.resource_mut::<ExpressionMenu>().pointer.back = true,
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                if state == ElementState::Released {
                    self.held.remove(&code);
                    return;
                }
                self.held.insert(code);
                match code {
                    KeyCode::Space => self.runtime.world.resource_mut::<Input>().jump = true,
                    KeyCode::KeyV => self.cycle_view(),
                    KeyCode::Escape => {
                        self.runtime.world.resource_mut::<ExpressionMenu>().pointer.back = true;
                        self.capture(false);
                    }
                    KeyCode::Tab => self.runtime.world.resource_mut::<ExpressionMenu>().toggle = true,
                    KeyCode::KeyC => self.runtime.world.resource_mut::<Input>().crouch = true,
                    KeyCode::KeyZ => self.runtime.world.resource_mut::<Input>().prone = true,
                    KeyCode::KeyE => self.runtime.world.resource_mut::<Input>().interact = true,
                    KeyCode::KeyX => self.runtime.world.resource_mut::<Input>().pose = true,
                    KeyCode::KeyF => self.runtime.world.resource_mut::<Input>().fly = true,
                    KeyCode::Equal => self.runtime.world.resource_mut::<Input>().resize = Some(RESIZE),
                    KeyCode::Minus => self.runtime.world.resource_mut::<Input>().resize = Some(1.0 / RESIZE),
                    _ => {
                        // Left or right Shift with F1 to F8: that hand's gesture 0 to 7.
                        let f = [
                            KeyCode::F1,
                            KeyCode::F2,
                            KeyCode::F3,
                            KeyCode::F4,
                            KeyCode::F5,
                            KeyCode::F6,
                            KeyCode::F7,
                            KeyCode::F8,
                        ];
                        let hands = [KeyCode::ShiftLeft, KeyCode::ShiftRight].map(|k| self.held.contains(&k));
                        if let Some(g) = f.iter().position(|&k| k == code) {
                            let gestures = &mut self.runtime.world.resource_mut::<Input>().gestures;
                            for (hand, _) in hands.iter().enumerate().filter(|(_, held)| **held) {
                                gestures[hand] = g as u8;
                            }
                        }
                    }
                }
            }
            WindowEvent::Focused(focused) => {
                self.runtime.world.resource_mut::<Input>().afk = !focused;
                if !focused {
                    self.held.clear();
                    self.pad.reset();
                    self.capture(false);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let (true, Some((x, y))) = (self.dragging, self.cursor) {
                    self.camera.drag((position.x - x) as f32, (position.y - y) as f32);
                }
                self.cursor = Some((position.x, position.y));
                let mut menu = self.runtime.world.resource_mut::<ExpressionMenu>();
                menu.pointer.pos = [position.x as f32, position.y as f32];
                menu.stick = None;
            }
            WindowEvent::MouseWheel { delta, .. } => self.camera.zoom(match delta {
                MouseScrollDelta::LineDelta(_, y) => y,
                MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
            }),
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.frame() {
                    return self.fail(event_loop, e);
                }
                if self.error.is_some() {
                    return event_loop.exit();
                }
                if self.done() {
                    event_loop.exit();
                }
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if let (true, DeviceEvent::MouseMotion { delta: (dx, dy) }) = (self.captured, event) {
            let look = &mut self.runtime.world.resource_mut::<Input>().look;
            look[0] -= dx as f32 * LOOK_PER_COUNT;
            look[1] -= dy as f32 * LOOK_PER_COUNT;
        }
    }
}

/// Radians the player turns per raw mouse count.
const LOOK_PER_COUNT: f32 = 0.0025;
