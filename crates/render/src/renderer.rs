//! Renders frames to a window: two frames in flight, acquire, record a frame's graph, submit and present,
//! recreating the swapchain when the window resizes or the surface says it is out of date (ADR 0021); or, headless,
//! to an offscreen image per frame in flight, for measuring (ADR 0032).

use ash::vk;

use crate::graph::{Graph, ImageId, Transients, Use};
use crate::memory::Image;
use crate::swapchain::{Swapchain, choose_format};
use crate::{Context, Result, Window};

/// The format headless frames are drawn in: the one swapchains usually get (`choose_format`).
pub const HEADLESS_FORMAT: vk::Format = vk::Format::B8G8R8A8_SRGB;

/// Frames the CPU may record ahead of the GPU.
pub const FRAMES_IN_FLIGHT: usize = 2;

/// What happened to a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameStatus {
    /// Rendered and queued for presentation.
    Presented,
    /// Not rendered: the window has no area, or the swapchain was out of date and is recreated next frame.
    Skipped,
}

struct Frame {
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    acquired: vk::Semaphore,
    done: vk::Fence,
}

/// A window's renderer: its context, swapchain, frames in flight and the graph's transient images; or a headless
/// one with an offscreen image per frame in flight instead of the swapchain.
pub struct Renderer {
    frames: Vec<Frame>,
    offscreen: Vec<Image>,
    transients: Transients,
    /// Two timestamps per frame in flight (null if the queue has no timestamps), nanoseconds per tick, and
    /// whether each frame's pair was written.
    timestamps: (vk::QueryPool, f32, [bool; FRAMES_IN_FLIGHT]),
    gpu_ms: Option<f32>,
    blocked_ms: f32,
    swapchain: Option<Swapchain>,
    extent: vk::Extent2D,
    stale: bool,
    current: usize,
    ctx: Context,
}

impl Renderer {
    /// Creates the context for `window` (see [`Context::new`]) and the per-frame objects. The swapchain is created
    /// on the first frame, at `width` by `height` unless the surface decides.
    ///
    /// # Safety
    ///
    /// `window`'s handles must be valid and the window must outlive the renderer.
    pub unsafe fn new(validation: bool, window: Window, width: u32, height: u32) -> Result<Self> {
        // SAFETY: per this function's contract.
        let ctx = unsafe { Context::new(validation, Some(window))? };
        Self::with_context(ctx, width, height)
    }

    /// A renderer without a window: each frame draws to its own `width` by `height` [`HEADLESS_FORMAT`] image,
    /// left in `TransferSrc` and never shown. Timestamps and CPU timing work as for a window.
    pub fn headless(validation: bool, width: u32, height: u32) -> Result<Self> {
        let mut r = Self::with_context(Context::headless(validation)?, width, height)?;
        for i in 0..FRAMES_IN_FLIGHT {
            let info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(HEADLESS_FORMAT)
                .extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC);
            let image = r
                .ctx
                .create_image(&format!("offscreen {i}"), &info, vk::ImageAspectFlags::COLOR)?;
            r.offscreen.push(image);
        }
        Ok(r)
    }

    fn with_context(ctx: Context, width: u32, height: u32) -> Result<Self> {
        let mut r = Renderer {
            frames: Vec::new(),
            offscreen: Vec::new(),
            transients: Transients::default(),
            timestamps: (vk::QueryPool::null(), 0.0, [false; FRAMES_IN_FLIGHT]),
            gpu_ms: None,
            blocked_ms: 0.0,
            swapchain: None,
            extent: vk::Extent2D { width, height },
            stale: true,
            current: 0,
            ctx,
        };
        let device = r.ctx.device();
        // SAFETY: plain queries; a valid create info on the context's device, destroyed in `Drop`.
        unsafe {
            let instance = r.ctx.instance();
            let family =
                instance.get_physical_device_queue_family_properties(r.ctx.physical())[r.ctx.queue_family() as usize];
            if family.timestamp_valid_bits > 0 {
                let period = instance
                    .get_physical_device_properties(r.ctx.physical())
                    .limits
                    .timestamp_period;
                let info = vk::QueryPoolCreateInfo::default()
                    .query_type(vk::QueryType::TIMESTAMP)
                    .query_count(2 * FRAMES_IN_FLIGHT as u32);
                r.timestamps.0 = device.create_query_pool(&info, None)?;
                r.timestamps.1 = period;
            }
        }
        for _ in 0..FRAMES_IN_FLIGHT {
            // SAFETY: valid create infos on the context's device; on failure, `Drop` destroys what exists.
            unsafe {
                let pool = device.create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(r.ctx.queue_family())
                        .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                    None,
                )?;
                let frame = Frame {
                    pool,
                    cmd: vk::CommandBuffer::null(),
                    acquired: vk::Semaphore::null(),
                    done: vk::Fence::null(),
                };
                r.frames.push(frame);
                let frame = r.frames.last_mut().expect("just pushed");
                frame.cmd = device.allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(pool)
                        .command_buffer_count(1),
                )?[0];
                frame.acquired = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
                frame.done = device.create_fence(
                    &vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED),
                    None,
                )?;
            }
        }
        Ok(r)
    }

    /// The context, for creating resources the passes use.
    pub fn context(&self) -> &Context {
        &self.ctx
    }

    /// The window's new size in pixels; the swapchain follows on the next frame. Headless renderers keep theirs.
    pub fn resize(&mut self, width: u32, height: u32) {
        if !self.offscreen.is_empty() {
            return;
        }
        self.extent = vk::Extent2D { width, height };
        self.stale = true;
    }

    /// The format frames are drawn in: the surface format the swapchain uses (for creating pipelines).
    pub fn color_format(&self) -> Result<vk::Format> {
        if !self.offscreen.is_empty() {
            return Ok(HEADLESS_FORMAT);
        }
        let (loader, surface) = self.ctx.surface().expect("renderer contexts have a surface");
        // SAFETY: plain query on handles from the context.
        let formats = unsafe { loader.get_physical_device_surface_formats(self.ctx.physical(), surface)? };
        Ok(choose_format(&formats).format)
    }

    /// GPU time of the last finished frame in milliseconds, from timestamps at the start and end of its commands;
    /// `None` until a frame finished or if the queue has no timestamps.
    pub fn gpu_ms(&self) -> Option<f32> {
        self.gpu_ms
    }

    /// How long the last [`Renderer::frame`] waited for the GPU and the swapchain (frame fence and acquire), in
    /// milliseconds: the part of its time that is not CPU work.
    pub fn blocked_ms(&self) -> f32 {
        self.blocked_ms
    }

    /// The swapchain's current size, once it exists; a headless renderer's size.
    pub fn extent(&self) -> Option<vk::Extent2D> {
        if !self.offscreen.is_empty() {
            return Some(self.extent);
        }
        self.swapchain.as_ref().map(|s| s.extent)
    }

    /// Renders one frame: `build` adds passes to the frame's graph, given the swapchain image as an imported
    /// image (undefined contents, presented after the graph), or headless this frame's offscreen image.
    pub fn frame<'a>(&mut self, build: impl FnOnce(&mut Graph<'a>, ImageId)) -> Result<FrameStatus> {
        if self.extent.width == 0 || self.extent.height == 0 {
            return Ok(FrameStatus::Skipped);
        }
        let headless = !self.offscreen.is_empty();
        if !headless && (self.stale || self.swapchain.is_none()) {
            self.recreate()?;
        }
        let frame = &self.frames[self.current];
        let device = self.ctx.device();
        // SAFETY: every handle belongs to this renderer's device; the fence wait guarantees the frame's command
        // buffer and semaphore are no longer in use before they are reused.
        unsafe {
            let wait = std::time::Instant::now();
            device.wait_for_fences(&[frame.done], true, u64::MAX)?;
            let (queries, period, written) = &mut self.timestamps;
            let first = 2 * self.current as u32;
            if written[self.current] {
                let mut ticks = [0u64; 2];
                let flags = vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT;
                device.get_query_pool_results(*queries, first, &mut ticks, flags)?;
                self.gpu_ms = Some(ticks[1].saturating_sub(ticks[0]) as f32 * *period / 1e6);
            }
            // The fence wait finished the frame before last, the oldest that could use a transient image.
            self.transients.next_frame(&self.ctx);
            // The target, and for a window the swapchain and the image index to present.
            let (image, view, extent, last, present) = match &self.swapchain {
                _ if headless => {
                    let o = &self.offscreen[self.current];
                    (o.handle, o.view, self.extent, Use::TransferSrc, None)
                }
                Some(sc) => {
                    let index =
                        match sc
                            .loader
                            .acquire_next_image(sc.handle, u64::MAX, frame.acquired, vk::Fence::null())
                        {
                            Ok((index, suboptimal)) => {
                                self.stale |= suboptimal;
                                index as usize
                            }
                            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                                self.stale = true;
                                return Ok(FrameStatus::Skipped);
                            }
                            Err(e) => return Err(e.into()),
                        };
                    (
                        sc.images[index],
                        sc.views[index],
                        sc.extent,
                        Use::Present,
                        Some((sc, index)),
                    )
                }
                None => unreachable!("recreated above"),
            };
            self.blocked_ms = wait.elapsed().as_secs_f32() * 1000.0;
            device.reset_command_pool(frame.pool, vk::CommandPoolResetFlags::empty())?;
            device.begin_command_buffer(
                frame.cmd,
                &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            let timed = *queries != vk::QueryPool::null();
            if timed {
                device.cmd_reset_query_pool(frame.cmd, *queries, first, 2);
                device.cmd_write_timestamp2(frame.cmd, vk::PipelineStageFlags2::ALL_COMMANDS, *queries, first);
            }
            let mut graph = Graph::new();
            let target = graph.import(image, view, extent, Use::Undefined, last);
            build(&mut graph, target);
            graph.record(&self.ctx, &mut self.transients, frame.cmd)?;
            if timed {
                device.cmd_write_timestamp2(frame.cmd, vk::PipelineStageFlags2::ALL_COMMANDS, *queries, first + 1);
            }
            device.end_command_buffer(frame.cmd)?;

            let waits = [vk::SemaphoreSubmitInfo::default()
                .semaphore(frame.acquired)
                .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
            let signals = present.map(|(sc, index)| {
                [vk::SemaphoreSubmitInfo::default()
                    .semaphore(sc.rendered[index])
                    .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)]
            });
            let cmds = [vk::CommandBufferSubmitInfo::default().command_buffer(frame.cmd)];
            let mut submit = vk::SubmitInfo2::default().command_buffer_infos(&cmds);
            if let Some(signals) = &signals {
                submit = submit.wait_semaphore_infos(&waits).signal_semaphore_infos(signals);
            }
            // Only right before the submit that signals it again, so an earlier error can't leave it unsignalled.
            device.reset_fences(&[frame.done])?;
            device.queue_submit2(self.ctx.queue(), &[submit], frame.done)?;
            written[self.current] = timed;

            let Some((sc, index)) = present else {
                self.current = (self.current + 1) % FRAMES_IN_FLIGHT;
                return Ok(FrameStatus::Presented);
            };
            let wait = [sc.rendered[index]];
            let swapchains = [sc.handle];
            let indices = [index as u32];
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&wait)
                .swapchains(&swapchains)
                .image_indices(&indices);
            match sc.loader.queue_present(self.ctx.queue(), &present) {
                Ok(suboptimal) => self.stale |= suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.stale = true,
                Err(e) => return Err(e.into()),
            }
        }
        self.current = (self.current + 1) % FRAMES_IN_FLIGHT;
        Ok(FrameStatus::Presented)
    }

    /// Headless only: copies the last frame's image back after it finished, as [`HEADLESS_FORMAT`] texels (BGRA8,
    /// sRGB-encoded), rows tightly packed from the top. Call after at least one [`Renderer::frame`].
    pub fn read_last(&self) -> Result<Vec<u8>> {
        assert!(!self.offscreen.is_empty(), "read_last needs a headless renderer");
        let last = &self.offscreen[(self.current + FRAMES_IN_FLIGHT - 1) % FRAMES_IN_FLIGHT];
        let (width, height) = (self.extent.width, self.extent.height);
        let buffer = self.ctx.create_buffer(
            "read back",
            u64::from(width) * u64::from(height) * 4,
            vk::BufferUsageFlags::TRANSFER_DST,
            crate::memory::MemoryLocation::GpuToCpu,
        )?;
        // Submitted after the frame on the same queue: the graph's final barrier orders its writes before the copy.
        let copied = self.ctx.one_shot(|d, cmd| {
            let region = vk::BufferImageCopy::default()
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                });
            let host = vk::MemoryBarrier2::default()
                .src_stage_mask(vk::PipelineStageFlags2::COPY)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(vk::PipelineStageFlags2::HOST)
                .dst_access_mask(vk::AccessFlags2::HOST_READ);
            // SAFETY: the image is in `TransferSrc` after every headless frame; the buffer holds all its texels.
            unsafe {
                d.cmd_copy_image_to_buffer(
                    cmd,
                    last.handle,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    buffer.handle,
                    &[region],
                );
                d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[host]));
            }
        });
        let texels = copied.map(|()| buffer.mapped().expect("GpuToCpu memory is mapped").to_vec());
        // SAFETY: `one_shot` waited for the copy, or never submitted it.
        unsafe { self.ctx.destroy_buffer(buffer) };
        texels
    }

    fn recreate(&mut self) -> Result<()> {
        let device = self.ctx.device();
        // SAFETY: after the wait no frame uses the old swapchain; it is retired by the new one, then destroyed.
        unsafe {
            device.device_wait_idle()?;
            let old = self.swapchain.take();
            let new = Swapchain::new(
                &self.ctx,
                self.extent,
                old.as_ref().map_or(vk::SwapchainKHR::null(), |s| s.handle),
            );
            if let Some(mut old) = old {
                old.destroy(device);
            }
            self.swapchain = Some(new?);
        }
        self.stale = false;
        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let device = self.ctx.device();
        // SAFETY: after the wait nothing is in use; null handles (a partly built frame) are ignored by Vulkan.
        unsafe {
            let _ = device.device_wait_idle();
            if let Some(mut sc) = self.swapchain.take() {
                sc.destroy(device);
            }
            self.transients.destroy(&self.ctx);
            for image in self.offscreen.drain(..) {
                self.ctx.destroy_image(image);
            }
            device.destroy_query_pool(self.timestamps.0, None);
            for f in self.frames.drain(..) {
                device.destroy_fence(f.done, None);
                device.destroy_semaphore(f.acquired, None);
                device.destroy_command_pool(f.pool, None);
            }
        }
    }
}
