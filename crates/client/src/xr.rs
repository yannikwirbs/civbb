//! The XR session on the renderer's device (ADR 0122): the OpenXR runtime creates the Vulkan instance and device
//! through [`civbb_render::Create`], and `view --xr` submits cleared frames until the runtime ends the session.

use std::ffi::c_void;
use std::time::Duration;

use civbb_render::ash::vk::Handle as _;
use civbb_render::{Context, Create, Error, ash, vk};
use civbb_xr::xr::sys::platform::VkGetInstanceProcAddr;
use civbb_xr::{Session, Status, Target, Xr};

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

/// Each eye's clear colour: tinted apart, so a headset check sees that each eye gets its own layer.
const CLEAR: [[f32; 4]; 2] = [[0.08, 0.04, 0.02, 1.0], [0.02, 0.04, 0.08, 1.0]];

/// Opens a session and submits cleared frames until the runtime ends it, or for `frames` frames; returns how many
/// were shown.
pub fn run(frames: Option<u32>) -> Result<u32, String> {
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
    // SAFETY: the context's handles came from this runtime; the queue is used only on this thread; the session is
    // dropped before the context.
    let mut session = unsafe {
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
    let clear = Clear::new(&ctx).map_err(|e| e.to_string())?;
    let shown = frame_loop(&mut session, &ctx, &clear, frames).map_err(|e| e.to_string());
    drop(session);
    clear.destroy(&ctx);
    shown
}

fn frame_loop(
    session: &mut Session,
    ctx: &Context,
    clear: &Clear,
    frames: Option<u32>,
) -> Result<u32, Box<dyn std::error::Error>> {
    let mut shown = 0;
    loop {
        match session.poll()? {
            Status::Exit => return Ok(shown),
            Status::Idle => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Status::Running => {}
        }
        let state = session.wait()?;
        session.begin()?;
        if !state.should_render {
            session.end(state.predicted_display_time, None)?;
            continue;
        }
        let views = session.views(state.predicted_display_time)?;
        let targets = session.acquire()?;
        clear.record_and_wait(ctx, &targets)?;
        session.release()?;
        session.end(state.predicted_display_time, Some(&views))?;
        shown += 1;
        if frames == Some(shown) {
            session.request_exit()?;
        }
    }
}

/// A command buffer that clears the eyes' swapchain layers, and its fence.
struct Clear {
    pool: vk::CommandPool,
    commands: vk::CommandBuffer,
    fence: vk::Fence,
}

impl Clear {
    fn new(ctx: &Context) -> civbb_render::Result<Self> {
        let d = ctx.device();
        // SAFETY: valid create infos on the context's device; destroyed in `destroy`.
        unsafe {
            let pool = d.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(ctx.queue_family())
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )?;
            let commands = d.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .command_buffer_count(1),
            )?[0];
            let fence = d.create_fence(&vk::FenceCreateInfo::default(), None)?;
            Ok(Self { pool, commands, fence })
        }
    }

    /// Clears each eye's layer and waits for the GPU; the images end in the layout the runtime expects.
    fn record_and_wait(&self, ctx: &Context, targets: &[Target; 2]) -> civbb_render::Result<()> {
        let d = ctx.device();
        let range = |layer| vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: layer,
            layer_count: 1,
        };
        let barrier = |t: &Target, from, to| {
            vk::ImageMemoryBarrier2::default()
                .image(vk::Image::from_raw(t.image))
                .subresource_range(range(t.layer))
                .old_layout(from)
                .new_layout(to)
                .src_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .src_access_mask(vk::AccessFlags2::MEMORY_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
                .dst_access_mask(vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE)
        };
        let to_clear = targets.map(|t| barrier(&t, vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL));
        let to_runtime = targets.map(|t| {
            barrier(
                &t,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            )
        });
        // SAFETY: the command buffer is idle (the last submit was waited for); the images are acquired from the
        // runtime and not in use; everything is submitted on the context's queue from this thread only.
        unsafe {
            d.begin_command_buffer(
                self.commands,
                &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            d.cmd_pipeline_barrier2(
                self.commands,
                &vk::DependencyInfo::default().image_memory_barriers(&to_clear),
            );
            for (t, color) in targets.iter().zip(CLEAR) {
                d.cmd_clear_color_image(
                    self.commands,
                    vk::Image::from_raw(t.image),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue { float32: color },
                    &[range(t.layer)],
                );
            }
            d.cmd_pipeline_barrier2(
                self.commands,
                &vk::DependencyInfo::default().image_memory_barriers(&to_runtime),
            );
            d.end_command_buffer(self.commands)?;
            let commands = [self.commands];
            d.queue_submit(
                ctx.queue(),
                &[vk::SubmitInfo::default().command_buffers(&commands)],
                self.fence,
            )?;
            d.wait_for_fences(&[self.fence], true, u64::MAX)?;
            d.reset_fences(&[self.fence])?;
        }
        Ok(())
    }

    fn destroy(self, ctx: &Context) {
        // SAFETY: the last submit was waited for; each object is destroyed once.
        unsafe {
            ctx.device().destroy_fence(self.fence, None);
            ctx.device().destroy_command_pool(self.pool, None);
        }
    }
}
