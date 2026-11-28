//! The OpenXR session on a Vulkan device (ADR 0122): its state machine, stage or local space, the colour swapchain
//! with a layer per eye (or one swapchain per eye) and the frame loop.

use crate::xr::sys::platform::{VkDevice, VkFormat, VkImage, VkInstance, VkPhysicalDevice};
use crate::{Error, Result, VIEWS, Xr, xr};

/// What the frame loop should do after [`Session::poll`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The session runs: wait for frames and end each one.
    Running,
    /// Not begun yet, or stopped: poll again later, no frames.
    Idle,
    /// The session is over or the runtime is going away: tear down.
    Exit,
}

/// Where one eye renders this frame: a swapchain image and its array layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    /// The `VkImage`.
    pub image: VkImage,
    /// Its array layer.
    pub layer: u32,
}

/// A session with its space, swapchains and frame loop.
pub struct Session {
    // Swapchains and the space are destroyed before the session.
    chains: Vec<xr::Swapchain<xr::Vulkan>>,
    images: Vec<Vec<VkImage>>,
    acquired: [u32; 2],
    space: xr::Space,
    stream: xr::FrameStream<xr::Vulkan>,
    waiter: xr::FrameWaiter,
    session: xr::Session<xr::Vulkan>,
    instance: xr::Instance,
    events: xr::EventDataBuffer,
    running: bool,
    blend: xr::EnvironmentBlendMode,
    /// The last state the runtime reported.
    pub state: xr::SessionState,
    /// The swapchain images' `VkFormat`.
    pub format: VkFormat,
    /// Each eye's image size in pixels.
    pub size: [u32; 2],
}

impl Session {
    /// Creates the session on `device` (its queue 0 of `queue_family`), a stage space (local without one) and the
    /// colour swapchain in the first of `formats` the runtime offers.
    ///
    /// # Safety
    ///
    /// The handles are valid, `instance` and `device` were created through `xr` on `physical`, and the queue is
    /// externally synchronised with the frame loop's calls (the runtime submits on it in
    /// [`Session::release`] and [`Session::end`]). The session is dropped before the device.
    pub unsafe fn new(
        xr: &Xr,
        instance: VkInstance,
        physical: VkPhysicalDevice,
        device: VkDevice,
        queue_family: u32,
        formats: &[VkFormat],
    ) -> Result<Self> {
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
        let info = |array_size| xr::SwapchainCreateInfo {
            create_flags: xr::SwapchainCreateFlags::EMPTY,
            usage_flags: xr::SwapchainUsageFlags::COLOR_ATTACHMENT | xr::SwapchainUsageFlags::TRANSFER_DST,
            format,
            sample_count: 1,
            width: size[0],
            height: size[1],
            face_count: 1,
            array_size,
            mip_count: 1,
        };
        // A layer per eye for multiview (ADR 0120); a runtime without array swapchains gets one per eye.
        let chains = match session.create_swapchain(&info(2)) {
            Ok(chain) => vec![chain],
            Err(_) => vec![session.create_swapchain(&info(1))?, session.create_swapchain(&info(1))?],
        };
        let images = (chains.iter())
            .map(xr::Swapchain::enumerate_images)
            .collect::<xr::Result<_>>()?;
        Ok(Self {
            chains,
            images,
            acquired: [0; 2],
            space,
            stream,
            waiter,
            session,
            instance: xr.instance.clone(),
            events: xr::EventDataBuffer::new(),
            running: false,
            blend: xr.blend,
            state: xr::SessionState::UNKNOWN,
            format,
            size,
        })
    }

    /// Handles the runtime's events: begins the session when it is ready, ends it when it stops.
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
                        xr::SessionState::STOPPING => {
                            self.session.end()?;
                            self.running = false;
                        }
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

    /// Asks the runtime to stop the session; [`Session::poll`] then reports [`Status::Exit`].
    pub fn request_exit(&self) -> Result<()> {
        Ok(self.session.request_exit()?)
    }

    /// Blocks until the runtime wants the next frame: its predicted display time and whether to render it.
    pub fn wait(&mut self) -> Result<xr::FrameState> {
        Ok(self.waiter.wait()?)
    }

    /// Starts the frame [`Session::wait`] returned.
    pub fn begin(&mut self) -> Result<()> {
        Ok(self.stream.begin()?)
    }

    /// Each eye's pose and field of view in the session's space at `time`.
    pub fn views(&self, time: xr::Time) -> Result<Vec<xr::View>> {
        Ok(self.session.locate_views(VIEWS, time, &self.space)?.1)
    }

    /// Acquires the next image of each swapchain and waits until it can be written: where each eye renders.
    pub fn acquire(&mut self) -> Result<[Target; 2]> {
        for (chain, i) in self.chains.iter_mut().zip(&mut self.acquired) {
            *i = chain.acquire_image()?;
            chain.wait_image(xr::Duration::INFINITE)?;
        }
        let one = self.chains.len() == 1;
        Ok([0, 1].map(|eye| {
            let (chain, layer) = if one { (0, eye as u32) } else { (eye, 0) };
            Target {
                image: self.images[chain][self.acquired[chain] as usize],
                layer,
            }
        }))
    }

    /// Hands the acquired images back once the commands writing them are submitted.
    pub fn release(&mut self) -> Result<()> {
        for chain in &mut self.chains {
            chain.release_image()?;
        }
        Ok(())
    }

    /// Ends the frame begun for `time`, showing `views` (as [`Session::views`] gave them, rendered into the
    /// released images) or, with `None`, nothing.
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
        let one = self.chains.len() == 1;
        let projection = [0, 1].map(|eye| {
            let (chain, layer) = if one { (0, eye as u32) } else { (eye, 0) };
            xr::CompositionLayerProjectionView::new()
                .pose(views[eye].pose)
                .fov(views[eye].fov)
                .sub_image(
                    xr::SwapchainSubImage::new()
                        .swapchain(&self.chains[chain])
                        .image_array_index(layer)
                        .image_rect(rect),
                )
        });
        let layer = xr::CompositionLayerProjection::new()
            .space(&self.space)
            .views(&projection);
        Ok(self.stream.end(time, self.blend, &[&layer])?)
    }
}
