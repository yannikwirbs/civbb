//! The headset (ADRs 0122 to 0124): the OpenXR runtime creates the Vulkan instance and device through
//! [`civbb_render::Create`], and `view --xr` runs the viewer in the headset: frames paced by `xrWaitFrame` on the
//! simulation thread, which samples the head and hands for the player's trackers, both eyes drawn by multiview into
//! the swapchain on the render thread.

use std::ffi::c_void;
use std::time::{Duration, Instant};

use civbb_core::math::{Pose, mul};
use civbb_render::ash::vk::Handle as _;
use civbb_render::{Context, Create, Error, External, Renderer, Use, View, ash, vk};
use civbb_xr::xr::FrameState;
use civbb_xr::xr::sys::platform::VkGetInstanceProcAddr;
use civbb_xr::{Frames, Session, Status, Xr, xr};

use crate::extract::{Camera, Snapshot};
use crate::gpu::{Gpu, NEAR, Scratch, draw_views};
use crate::render_thread::Present;
use crate::view::{Config, Summary, Viewer};
use crate::vr::Vr;

/// Creates the Vulkan instance and device through an OpenXR runtime (XR_KHR_vulkan_enable2).
pub struct ByRuntime<'a> {
    xr: &'a Xr,
    /// The Vulkan loader's `vkGetInstanceProcAddr`, kept from the instance's creation for the device's.
    gipa: Option<VkGetInstanceProcAddr>,
}

impl<'a> ByRuntime<'a> {
    /// Creation through `xr`'s runtime.
    pub fn new(xr: &'a Xr) -> Self {
        Self { xr, gipa: None }
    }
}

fn failed(e: civbb_xr::Error) -> Error {
    match e {
        civbb_xr::Error::Vulkan(r) => Error::Vk(vk::Result::from_raw(r)),
        e => Error::Create(e.to_string()),
    }
}

impl Create for ByRuntime<'_> {
    unsafe fn instance(
        &mut self,
        entry: &ash::Entry,
        info: &vk::InstanceCreateInfo<'_>,
    ) -> civbb_render::Result<ash::Instance> {
        // SAFETY: both are `vkGetInstanceProcAddr`'s signature; dispatchable handles are pointers either way.
        let gipa: VkGetInstanceProcAddr = unsafe { std::mem::transmute(entry.static_fn().get_instance_proc_addr) };
        self.gipa = Some(gipa);
        // SAFETY: the info is valid per the trait's contract; the function is the loader's.
        let raw = unsafe { self.xr.create_vulkan_instance(gipa, std::ptr::from_ref(info).cast()) }.map_err(failed)?;
        // SAFETY: the runtime created the instance with this entry's loader.
        Ok(unsafe { ash::Instance::load(entry.static_fn(), vk::Instance::from_raw(raw as u64)) })
    }

    unsafe fn physical(&mut self, instance: &ash::Instance) -> civbb_render::Result<Option<vk::PhysicalDevice>> {
        let raw = instance.handle().as_raw() as *const c_void;
        // SAFETY: the instance came from `instance` above, i.e. from the runtime.
        let physical = unsafe { self.xr.vulkan_physical_device(raw) }.map_err(failed)?;
        Ok(Some(vk::PhysicalDevice::from_raw(physical as u64)))
    }

    unsafe fn device(
        &mut self,
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        info: &vk::DeviceCreateInfo<'_>,
    ) -> civbb_render::Result<ash::Device> {
        let gipa = (self.gipa).ok_or_else(|| Error::Create("the device before the instance".into()))?;
        let physical = physical.as_raw() as *const c_void;
        // SAFETY: the physical device is the runtime's and the info valid, per the trait's contract.
        let raw = unsafe {
            self.xr
                .create_vulkan_device(gipa, physical, std::ptr::from_ref(info).cast())
        }
        .map_err(failed)?;
        // SAFETY: the runtime created the device on this instance.
        Ok(unsafe { ash::Device::load(instance.fp_v1_0(), vk::Device::from_raw(raw as u64)) })
    }
}

/// Where the headset's play space sits in the world without a VR player placing it (ADR 0125): on the ground below
/// the camera, its forward (-Z) along the camera's look (ADR 0123).
pub fn stage(camera: &Camera) -> Pose {
    let [x, _, z] = camera.eye;
    let (dx, dz) = (camera.target[0] - x, camera.target[2] - z);
    // A turn by `yaw` about +Y takes -Z to (-sin, 0, -cos).
    let half = (-dx).atan2(-dz) / 2.0;
    Pose {
        pos: [x, 0.0, z],
        rot: [0.0, half.sin(), 0.0, half.cos()],
        scale: 1.0,
    }
}

/// Each eye's view in the world: `located` (in the play space) placed by `stage`.
pub fn eye_views(stage: &Pose, located: &[xr::View]) -> [View; 2] {
    std::array::from_fn(|k| {
        let (o, p, f) = (located[k].pose.orientation, located[k].pose.position, located[k].fov);
        View::eye(
            stage.point([p.x, p.y, p.z]),
            mul(stage.rot, [o.x, o.y, o.z, o.w]),
            [f.angle_left, f.angle_right, f.angle_up, f.angle_down],
            NEAR,
        )
    })
}

/// Display periods that passed without a new frame between frames predicted `gap` nanoseconds apart, `period`
/// apart when none is missed; jitter under half a period is not a miss.
pub fn missed(gap: i64, period: i64) -> u32 {
    if period <= 0 {
        return 0;
    }
    ((gap as f64 / period as f64).round() as i64 - 1).max(0) as u32
}

/// How the runtime paced a run, past the viewer's warm-up.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pacing {
    /// Frames waited for.
    pub frames: u32,
    /// Of those, frames the runtime said not to render (`shouldRender` false).
    pub unrendered: u32,
    /// Display periods without a new frame (see [`missed`]).
    pub missed: u32,
    /// The last display period in milliseconds.
    pub period_ms: f32,
}

/// Frames into the headset: begins each frame, locates the eyes for its display time, draws both into the layers of
/// the acquired swapchain image by multiview and ends it with the projection layer (ADR 0123).
struct Headset {
    frames: Frames,
    /// An array view of both layers per swapchain image.
    views: Vec<vk::ImageView>,
    blocked_ms: f32,
}

fn target(e: civbb_xr::Error) -> Error {
    Error::Target(e.to_string())
}

impl Present for Headset {
    fn present(
        &mut self,
        renderer: &mut Renderer,
        gpu: &mut Gpu,
        snapshot: &Snapshot,
        pace: Option<FrameState>,
        scratch: &mut Scratch,
    ) -> civbb_render::Result<Option<(usize, usize)>> {
        let state = pace.expect("the XR loop hands every frame with its state");
        let time = state.predicted_display_time;
        self.blocked_ms = 0.0;
        self.frames.begin().map_err(target)?;
        let located = match state.should_render {
            true => self.frames.views(time).map_err(target)?,
            false => None,
        };
        let Some(located) = located else {
            self.frames.end(time, None).map_err(target)?;
            return Ok(None);
        };
        let camera = &snapshot.camera;
        let eyes = eye_views(&camera.stage.unwrap_or_else(|| stage(camera)), &located);
        let wait = Instant::now();
        let i = self.frames.acquire().map_err(target)? as usize;
        self.blocked_ms = wait.elapsed().as_secs_f32() * 1000.0;
        let into = External {
            image: vk::Image::from_raw(self.frames.images[i]),
            view: self.views[i],
            layers: 2,
            last: Use::ColorAttachment,
        };
        let drawn = draw_views(renderer, gpu, snapshot, scratch, &eyes, Some(into));
        // The image goes back even when drawing failed, so the runtime's swapchain stays consistent.
        self.frames.release().map_err(target)?;
        let counts = drawn?;
        self.frames.end(time, Some(&located)).map_err(target)?;
        Ok(counts)
    }

    fn blocked_ms(&self) -> f32 {
        self.blocked_ms
    }

    unsafe fn destroy(&mut self, renderer: &Renderer) {
        for view in self.views.drain(..) {
            // SAFETY: the GPU is idle per the contract; each view is destroyed once.
            unsafe { renderer.context().device().destroy_image_view(view, None) };
        }
    }
}

/// Opens a session and runs the viewer of `cfg` in the headset until the runtime ends the session, or for
/// `cfg.frames` frames past the warm-up: what the viewer measured and how the runtime paced it.
pub fn run(cfg: Config) -> Result<(Summary, Pacing), String> {
    if cfg.trackers.is_some() {
        return Err("scripted trackers and the headset both drive the player: use one".into());
    }
    let xr = Xr::new().map_err(|e| e.to_string())?;
    println!(
        "OpenXR: {}, {} x {} per eye",
        xr.runtime().map_err(|e| e.to_string())?,
        xr.views[0].recommended_image_rect_width,
        xr.views[0].recommended_image_rect_height
    );
    if xr.vulkan.0 > vk::API_VERSION_1_3 {
        return Err(format!(
            "the OpenXR runtime needs Vulkan {:#x}, the renderer uses 1.3",
            xr.vulkan.0
        ));
    }
    // SAFETY: no window; the runtime creates from the infos it is given.
    let ctx = unsafe { Context::create(cfg!(debug_assertions), None, &mut ByRuntime::new(&xr)) }
        .map_err(|e| e.to_string())?;
    let formats = [vk::Format::R8G8B8A8_SRGB, vk::Format::B8G8R8A8_SRGB].map(|f| f.as_raw() as u32);
    // SAFETY: the context's handles came from this runtime; the queue is used only by the render thread, which owns
    // the frames and drops them before the renderer; the session half is dropped before the render thread ends.
    let (mut session, frames) = unsafe {
        Session::new(
            &xr,
            ctx.instance().handle().as_raw() as *const c_void,
            ctx.physical().as_raw() as *const c_void,
            ctx.device().handle().as_raw() as *const c_void,
            ctx.queue_family(),
            &formats,
        )
    }
    .map_err(|e| e.to_string())?;
    let format = vk::Format::from_raw(frames.format as i32);
    let renderer = Renderer::external(ctx, format, frames.size[0], frames.size[1]).map_err(|e| e.to_string())?;
    let mut headset = Headset {
        frames,
        views: Vec::new(),
        blocked_ms: 0.0,
    };
    for &image in &headset.frames.images {
        let info = vk::ImageViewCreateInfo::default()
            .image(vk::Image::from_raw(image))
            .view_type(vk::ImageViewType::TYPE_2D_ARRAY)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 2,
            });
        // SAFETY: the image is the runtime's swapchain image on this device, with two layers.
        match unsafe { renderer.context().device().create_image_view(&info, None) } {
            Ok(view) => headset.views.push(view),
            Err(e) => {
                // SAFETY: the views were never used.
                unsafe { headset.destroy(&renderer) };
                return Err(e.to_string());
            }
        }
    }
    let vr = Vr::new(cfg.turning, cfg.steer);
    let mut viewer = Viewer::new(cfg);
    viewer.add_headset(vr);
    let mut pacing = Pacing::default();
    let mut result = (viewer.start_on(renderer, 2, Box::new(headset)))
        .map_err(Into::into)
        .and_then(|()| frame_loop(&mut session, &mut viewer, &mut pacing));
    // No frame may be in flight when the session goes; then the render thread ends, dropping the swapchain before
    // the device.
    let settled = viewer.settle();
    drop(session);
    let ended = viewer.shutdown();
    if result.is_ok() {
        result = settled.and(ended).map_err(Into::into);
    }
    result.map_err(|e| e.to_string())?;
    Ok((viewer.summary(), pacing))
}

/// Polls the session and hands each frame the runtime waits for to the viewer with the head and hands at its
/// predicted display time, until the session exits; asks it to
/// once the viewer's frames are done.
fn frame_loop(
    session: &mut Session,
    viewer: &mut Viewer,
    pacing: &mut Pacing,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut asked = false;
    let mut last: Option<i64> = None;
    loop {
        if let Some(e) = viewer.error.take() {
            return Err(e.into());
        }
        match session.poll()? {
            Status::Exit => return Ok(()),
            Status::Stopping => {
                viewer.settle()?;
                session.end()?;
                continue;
            }
            Status::Idle => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Status::Running => {}
        }
        if viewer.done() && !asked {
            session.request_exit()?;
            asked = true;
        }
        let state = session.wait()?;
        let (time, period) = (
            state.predicted_display_time.as_nanos(),
            state.predicted_display_period.as_nanos(),
        );
        if viewer.warm() {
            pacing.frames += 1;
            pacing.unrendered += u32::from(!state.should_render);
            pacing.missed += last.map_or(0, |l| missed(time - l, period));
            pacing.period_ms = period as f32 / 1e6;
        }
        last = Some(time);
        viewer.headset(session.poses(state.predicted_display_time)?);
        viewer.pace = Some(state);
        viewer.frame()?;
    }
}

#[cfg(test)]
mod tests {
    use civbb_render::HEADLESS_FORMAT;

    use super::*;
    use crate::frame::Runtime;
    use crate::package::Package;

    fn located(x: f32, y: f32, tan: f32) -> xr::View {
        let a = tan.atan();
        xr::View {
            pose: xr::Posef {
                orientation: xr::Quaternionf::IDENTITY,
                position: xr::Vector3f { x, y, z: 0.0 },
            },
            fov: xr::Fovf {
                angle_left: -a,
                angle_right: a,
                angle_up: a,
                angle_down: -a,
            },
        }
    }

    #[test]
    fn the_stage_is_on_the_ground_below_the_camera_facing_its_look() {
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5);
        let ahead = |camera: &Camera| civbb_core::math::rotate(stage(camera).rot, [0.0, 0.0, -1.0]);
        let mut camera = Camera {
            eye: [1.0, 1.4, 3.0],
            target: [1.0, 1.0, 0.0],
            ..Camera::default()
        };
        assert!(close(stage(&camera).pos, [1.0, 0.0, 3.0]));
        assert!(close(ahead(&camera), [0.0, 0.0, -1.0]));
        camera.target = [5.0, 1.0, 3.0];
        assert!(close(ahead(&camera), [1.0, 0.0, 0.0]));
        // An eye 1.6 m up in the play space, the stage turned to face +X: the eye is above the stage's origin.
        let eyes = eye_views(&stage(&camera), &[located(0.0, 1.6, 1.0), located(0.0, 1.6, 1.0)]);
        assert!(close(eyes[0].eye, [1.0, 1.6, 3.0]));
    }

    #[test]
    fn missed_periods_round_away_jitter() {
        let p = 11_111_111;
        assert_eq!(missed(p, p), 0);
        assert_eq!(missed(p * 14 / 10, p), 0);
        assert_eq!(missed(2 * p, p), 1);
        assert_eq!(missed(p * 26 / 10, p), 2);
        assert_eq!(missed(p, 0), 0);
    }

    /// The headset's drawing without a headset: eyes 64 mm apart as the runtime locates them, drawn by multiview
    /// into a two-layer image as the swapchain's. The avatar sits further right in the left eye's layer than in the
    /// right eye's, by the disparity `focal * ipd / distance`. Skips without a device.
    #[test]
    fn stereo_layers_differ_by_the_eye_offset() {
        const SIZE: u32 = 256;
        let ctx = match Context::headless(cfg!(debug_assertions)) {
            Ok(c) => c,
            Err(e @ (Error::Load(_) | Error::NoDevice)) => return eprintln!("skipped: {e}"),
            Err(e) => panic!("{e}"),
        };
        let mut renderer = Renderer::external(ctx, HEADLESS_FORMAT, SIZE, SIZE).unwrap();
        let mut gpu = Gpu::new(&renderer, 1, 2).unwrap();
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(HEADLESS_FORMAT)
            .extent(vk::Extent3D {
                width: SIZE,
                height: SIZE,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(2)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC);
        let ctx = renderer.context();
        let image = ctx.create_image("eyes", &info, vk::ImageAspectFlags::COLOR).unwrap();
        let into = External {
            image: image.handle,
            view: image.view,
            layers: 2,
            last: Use::TransferSrc,
        };

        let lean = Package::new(civbb_corpus::generate(&civbb_corpus::lean()), Vec::new());
        let (lo, hi) = crate::view::bounds(&lean.avatar);
        let mut runtime = Runtime::default();
        runtime.extract_to_snapshot();
        runtime.spawn_package(&lean, 0, true, Pose::IDENTITY);
        runtime.step(1.0 / 60.0);
        // 90 degrees across: the focal length is half the image.
        let (distance, ipd, height) = (1.5, 0.064, (lo[1] + hi[1]) / 2.0);
        let views = [located(-ipd / 2.0, height, 1.0), located(ipd / 2.0, height, 1.0)];
        let place = Pose {
            pos: [(lo[0] + hi[0]) / 2.0, 0.0, (lo[2] + hi[2]) / 2.0 + distance],
            ..Pose::IDENTITY
        };
        let eyes = eye_views(&place, &views);
        let snapshot = runtime.world.resource::<Snapshot>();
        let mut scratch = Scratch::default();
        let counts = draw_views(&mut renderer, &mut gpu, snapshot, &mut scratch, &eyes, Some(into)).unwrap();
        assert!(counts.is_some_and(|(draws, _)| draws > 0));
        let ctx = renderer.context();
        let texels = ctx.read_image(image.handle, SIZE, SIZE, 2).unwrap();

        // Each layer's mean column of the pixels the avatar covers (anything brighter than the clear).
        let layer = SIZE as usize * SIZE as usize * 4;
        let centroid = |l: usize| {
            let (mut sum, mut n) = (0.0, 0.0);
            for (i, px) in texels[l * layer..][..layer].as_chunks::<4>().0.iter().enumerate() {
                if px[..3].iter().any(|&c| c > 8) {
                    sum += (i % SIZE as usize) as f64;
                    n += 1.0;
                }
            }
            assert!(n > 500.0, "layer {l} shows the avatar ({n} pixels)");
            sum / n
        };
        let disparity = centroid(0) - centroid(1);
        let expected = f64::from(SIZE / 2) * f64::from(ipd / distance);
        assert!(
            (disparity - expected).abs() < expected * 0.25,
            "disparity {disparity:.2} px, expected about {expected:.2}"
        );

        // SAFETY: after the wait nothing is in flight.
        unsafe {
            ctx.device().device_wait_idle().unwrap();
            gpu.destroy(&renderer);
            renderer.context().destroy_image(image);
        }
        assert_eq!(renderer.context().validation_errors(), 0);
    }
}
