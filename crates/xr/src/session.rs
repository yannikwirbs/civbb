//! The OpenXR session on a Vulkan device (ADR 0122), split for two threads (ADR 0123): [`Session`] handles the
//! runtime's events and waits for each frame (the simulation thread), [`Frames`] begins, draws into and ends them
//! (the render thread). Stage or local space; one colour swapchain with a layer per eye.

use crate::xr::sys::platform::{VkDevice, VkFormat, VkImage, VkInstance, VkPhysicalDevice};
use crate::{Error, Result, VIEWS, Xr, xr};

/// What the frame loop should do after [`Session::poll`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The session runs: wait for frames and hand each one to [`Frames`].
    Running,
    /// Not begun yet, or stopped: poll again later, no frames.
    Idle,
    /// The runtime stops the session: finish the frames handed over, then [`Session::end`].
    Stopping,
    /// The session is over or the runtime is going away: tear down.
    Exit,
}

/// The session's events and frame pacing, for the thread that simulates frames.
pub struct Session {
    waiter: xr::FrameWaiter,
    session: xr::Session<xr::Vulkan>,
    instance: xr::Instance,
    events: xr::EventDataBuffer,
    running: bool,
    /// The last state the runtime reported.
    pub state: xr::SessionState,
}

/// The session's frames and swapchain, for the thread that renders them.
pub struct Frames {
    // The swapchain and the space go before the session (the `openxr` handles keep it alive until then).
    chain: xr::Swapchain<xr::Vulkan>,
    space: xr::Space,
    stream: xr::FrameStream<xr::Vulkan>,
    session: xr::Session<xr::Vulkan>,
    blend: xr::EnvironmentBlendMode,
    /// The swapchain's `VkImage`s, two array layers each (left eye, right eye).
    pub images: Vec<VkImage>,
    /// Their `VkFormat`.
    pub format: VkFormat,
    /// Each eye's image size in pixels.
    pub size: [u32; 2],
}

impl Session {
    /// Creates the session on `device` (its queue 0 of `queue_family`), a stage space (local without one) and a
    /// colour swapchain of two layers in the first of `formats` the runtime offers.
    ///
    /// # Safety
    ///
    /// The handles are valid, `instance` and `device` were created through `xr` on `physical`, and the queue is
    /// used only by the thread that owns the [`Frames`] (the runtime submits on it in [`Frames::release`] and
    /// [`Frames::end`]). Both halves are dropped before the device.
    pub unsafe fn new(
        xr: &Xr,
        instance: VkInstance,
        physical: VkPhysicalDevice,
        device: VkDevice,
        queue_family: u32,
        formats: &[VkFormat],
    ) -> Result<(Self, Frames)> {
        let info = xr::vulkan::SessionCreateInfo {
            instance,
            physical_device: physical,
            device,
            queue_family_index: queue_family,
            queue_index: 0,
        };
        // SAFETY: per this function's contract.
        let (session, waiter, stream) = unsafe { xr.instance.create_session::<xr::Vulkan>(xr.system, &info) }?;
        let kind = if session
            .enumerate_reference_spaces()?
            .contains(&xr::ReferenceSpaceType::STAGE)
        {
            xr::ReferenceSpaceType::STAGE
        } else {
            xr::ReferenceSpaceType::LOCAL
        };
        let space = session.create_reference_space(kind, xr::Posef::IDENTITY)?;
        let offered = session.enumerate_swapchain_formats()?;
        let format = (formats.iter().copied())
            .find(|f| offered.contains(f))
            .ok_or_else(|| Error::Unsupported("a colour format the renderer writes".into()))?;
        let size = [
            (xr.views.iter())
                .map(|v| v.recommended_image_rect_width)
                .max()
                .unwrap_or(0),
            (xr.views.iter())
                .map(|v| v.recommended_image_rect_height)
                .max()
                .unwrap_or(0),
        ];
        // A layer per eye for multiview (ADR 0120).
        let chain = session
            .create_swapchain(&xr::SwapchainCreateInfo {
                create_flags: xr::SwapchainCreateFlags::EMPTY,
                usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::TRANSFER_DST,
                format,
                sample_count: 1,
                width: size[0],
                height: size[1],
                face_count: 1,
                array_size: 2,
                mip_count: 1,
            })
            .map_err(|e| Error::Unsupported(format!("a swapchain with a layer per eye ({e})")))?;
        let frames = Frames {
            images: chain.enumerate_images()?,
            chain,
            space,
            stream,
            session: session.clone(),
            blend: xr.blend,
            format,
            size,
        };
        let session = Self {
            waiter,
            session,
            instance: xr.instance.clone(),
            events: xr::EventDataBuffer::new(),
            running: false,
            state: xr::SessionState::UNKNOWN,
        };
        Ok((session, frames))
    }

    /// Handles the runtime's events: begins the session when it is ready; reports [`Status::Stopping`] when the
    /// runtime stops it, leaving later events to the next poll.
    pub fn poll(&mut self) -> Result<Status> {
        while let Some(event) = self.instance.poll_event(&mut self.events)? {
            match event {
                xr::Event::SessionStateChanged(e) => {
                    self.state = e.state();
                    match self.state {
                        xr::SessionState::READY => {
                            self.session.begin(VIEWS)?;
                            self.running = true;
                        }
                        xr::SessionState::STOPPING => return Ok(Status::Stopping),
                        xr::SessionState::EXITING | xr::SessionState::LOSS_PENDING => return Ok(Status::Exit),
                        _ => {}
                    }
                }
                xr::Event::InstanceLossPending(_) => return Ok(Status::Exit),
                _ => {}
            }
        }
        Ok(if self.running { Status::Running } else { Status::Idle })
    }

    /// Ends the session after [`Status::Stopping`], once no frame is between [`Frames::begin`] and
    /// [`Frames::end`].
    pub fn end(&mut self) -> Result<()> {
        self.running = false;
        self.session.end()?;
        Ok(())
    }

    /// Asks the runtime to stop the session; [`Session::poll`] then reports [`Status::Stopping`] and later
    /// [`Status::Exit`].
    pub fn request_exit(&self) -> Result<()> {
        Ok(self.session.request_exit()?)
    }

    /// Blocks until the runtime wants the next frame (and the last one was begun): its predicted display time, the
    /// display period and whether to render it. Hand every frame waited for to [`Frames::begin`].
    pub fn wait(&mut self) -> Result<xr::FrameState> {
        Ok(self.waiter.wait()?)
    }
}

impl Frames {
    /// Starts the frame [`Session::wait`] returned.
    pub fn begin(&mut self) -> Result<()> {
        Ok(self.stream.begin()?)
    }

    /// Each eye's pose and field of view in the session's space at `time`; `None` while the runtime cannot orient
    /// them (tracking lost). A view without a valid position is at the space's origin.
    pub fn views(&self, time: xr::Time) -> Result<Option<Vec<xr::View>>> {
        let (flags, views) = self.session.locate_views(VIEWS, time, &self.space)?;
        Ok(flags.contains(xr::ViewStateFlags::ORIENTATION_VALID).then_some(views))
    }

    /// Acquires the next swapchain image and waits until it can be written: its index in [`Frames::images`].
    pub fn acquire(&mut self) -> Result<u32> {
        let i = self.chain.acquire_image()?;
        self.chain.wait_image(xr::Duration::INFINITE)?;
        Ok(i)
    }

    /// Hands the acquired image back once the commands writing it are submitted.
    pub fn release(&mut self) -> Result<()> {
        Ok(self.chain.release_image()?)
    }

    /// Ends the frame begun for `time`, showing `views` (as [`Frames::views`] gave them, rendered into the layers
    /// of the released image) or, with `None`, nothing.
    pub fn end(&mut self, time: xr::Time, views: Option<&[xr::View]>) -> Result<()> {
        let Some(views) = views.filter(|v| v.len() == 2) else {
            return Ok(self.stream.end(time, self.blend, &[])?);
        };
        let rect = xr::Rect2Di {
            offset: xr::Offset2Di { x: 0, y: 0 },
            extent: xr::Extent2Di {
                width: self.size[0] as i32,
                height: self.size[1] as i32,
            },
        };
        let projection = [0, 1].map(|eye| {
            xr::CompositionLayerProjectionView::new()
                .pose(views[eye].pose)
                .fov(views[eye].fov)
                .sub_image(
                    xr::SwapchainSubImage::new()
                        .swapchain(&self.chain)
                        .image_array_index(eye as u32)
                        .image_rect(rect),
                )
        });
        let layer = xr::CompositionLayerProjection::new()
            .space(&self.space)
            .views(&projection);
        Ok(self.stream.end(time, self.blend, &[&layer])?)
    }
}
