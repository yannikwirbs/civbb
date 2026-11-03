//! Renders frames to a window: two frames in flight, acquire, record a frame's graph, submit and present,
//! recreating the swapchain when the window resizes or the surface says it is out of date (ADR 0021).

use ash::vk;

use crate::graph::{Graph, ImageId, Use};
use crate::swapchain::Swapchain;
use crate::{Context, Result, Window};

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

/// A window's renderer: its context, swapchain and frames in flight.
pub struct Renderer {
    frames: Vec<Frame>,
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
        let mut r = Renderer {
            frames: Vec::new(),
            swapchain: None,
            extent: vk::Extent2D { width, height },
            stale: true,
            current: 0,
            ctx,
        };
        let device = r.ctx.device();
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

    /// The window's new size in pixels; the swapchain follows on the next frame.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.extent = vk::Extent2D { width, height };
        self.stale = true;
    }

    /// The swapchain's current size, once it exists.
    pub fn extent(&self) -> Option<vk::Extent2D> {
        self.swapchain.as_ref().map(|s| s.extent)
    }

    /// Renders one frame: `build` adds passes to the frame's graph, given the swapchain image as an imported
    /// image (undefined contents, presented after the graph).
    pub fn frame<'a>(&mut self, build: impl FnOnce(&mut Graph<'a>, ImageId)) -> Result<FrameStatus> {
        if self.extent.width == 0 || self.extent.height == 0 {
            return Ok(FrameStatus::Skipped);
        }
        if self.stale || self.swapchain.is_none() {
            self.recreate()?;
        }
        let sc = self.swapchain.as_ref().expect("recreated above");
        let frame = &self.frames[self.current];
        let device = self.ctx.device();
        // SAFETY: every handle belongs to this renderer's device; the fence wait guarantees the frame's command
        // buffer and semaphore are no longer in use before they are reused.
        unsafe {
            device.wait_for_fences(&[frame.done], true, u64::MAX)?;
            let index = match sc
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
            // Only after a successful acquire, so the fence is signalled again by this frame's submit.
            device.reset_fences(&[frame.done])?;
            device.reset_command_pool(frame.pool, vk::CommandPoolResetFlags::empty())?;
            device.begin_command_buffer(
                frame.cmd,
                &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            let mut graph = Graph::new();
            let target = graph.import(
                sc.images[index],
                sc.views[index],
                sc.extent,
                Use::Undefined,
                Use::Present,
            );
            build(&mut graph, target);
            graph.record(device, frame.cmd);
            device.end_command_buffer(frame.cmd)?;

            let waits = [vk::SemaphoreSubmitInfo::default()
                .semaphore(frame.acquired)
                .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)];
            let signals = [vk::SemaphoreSubmitInfo::default()
                .semaphore(sc.rendered[index])
                .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)];
            let cmds = [vk::CommandBufferSubmitInfo::default().command_buffer(frame.cmd)];
            let submit = vk::SubmitInfo2::default()
                .wait_semaphore_infos(&waits)
                .command_buffer_infos(&cmds)
                .signal_semaphore_infos(&signals);
            device.queue_submit2(self.ctx.queue(), &[submit], frame.done)?;

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
            for f in self.frames.drain(..) {
                device.destroy_fence(f.done, None);
                device.destroy_semaphore(f.acquired, None);
                device.destroy_command_pool(f.pool, None);
            }
        }
    }
}
