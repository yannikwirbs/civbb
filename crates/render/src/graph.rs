//! Render graph skeleton: passes declare the images they draw to; the graph plans the layout transitions and
//! barriers between them and records the passes with dynamic rendering (ADR 0021).

use ash::vk;

/// An image in a [`Graph`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageId(usize);

/// What an image is used for at one point in the frame: a layout plus the stages and accesses that touch it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    /// Contents don't matter: a new image, or a swapchain image just acquired (its semaphore is waited on at colour
    /// output, so that is where the first barrier starts).
    Undefined,
    /// Drawn to as a colour attachment.
    ColorAttachment,
    /// Copied from.
    TransferSrc,
    /// Handed to the presentation engine.
    Present,
}

impl Use {
    fn layout(self) -> vk::ImageLayout {
        match self {
            Use::Undefined => vk::ImageLayout::UNDEFINED,
            Use::ColorAttachment => vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            Use::TransferSrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            Use::Present => vk::ImageLayout::PRESENT_SRC_KHR,
        }
    }

    fn stage(self) -> vk::PipelineStageFlags2 {
        match self {
            Use::Undefined | Use::ColorAttachment => vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            Use::TransferSrc => vk::PipelineStageFlags2::COPY,
            // The submit's semaphore signal (all commands) orders presentation after the transition.
            Use::Present => vk::PipelineStageFlags2::NONE,
        }
    }

    fn access(self) -> vk::AccessFlags2 {
        match self {
            Use::Undefined | Use::Present => vk::AccessFlags2::NONE,
            Use::ColorAttachment => vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            Use::TransferSrc => vk::AccessFlags2::TRANSFER_READ,
        }
    }

    fn writes(self) -> bool {
        self == Use::ColorAttachment
    }
}

/// What a pass does with an attachment's previous contents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Load {
    /// Clear to this colour.
    Clear([f32; 4]),
    /// Keep them.
    Load,
    /// Don't care: the pass overwrites every pixel.
    DontCare,
}

type Record<'a> = Box<dyn FnOnce(&ash::Device, vk::CommandBuffer) + 'a>;

/// One pass: the colour attachments it draws to and, optionally, the commands it records. A pass with
/// attachments records inside a dynamic rendering scope over them; one without (compute) records outside any.
pub struct Pass<'a> {
    name: &'static str,
    colors: Vec<(ImageId, Load)>,
    record: Option<Record<'a>>,
}

impl<'a> Pass<'a> {
    /// A pass with no attachments and nothing to record yet.
    pub fn new(name: &'static str) -> Self {
        Pass {
            name,
            colors: Vec::new(),
            record: None,
        }
    }

    /// Adds a colour attachment.
    pub fn color(mut self, image: ImageId, load: Load) -> Self {
        self.colors.push((image, load));
        self
    }

    /// Sets the commands the pass records.
    pub fn record(mut self, f: impl FnOnce(&ash::Device, vk::CommandBuffer) + 'a) -> Self {
        self.record = Some(Box::new(f));
        self
    }

    /// The pass's name.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

/// One image's change of use, recorded as one barrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    /// The image.
    pub image: ImageId,
    /// Its use before.
    pub from: Use,
    /// Its use after.
    pub to: Use,
}

/// The barriers a graph records: before each pass, and after the last to put images in their final use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Transitions before each pass, by pass index.
    pub before: Vec<Vec<Transition>>,
    /// Transitions after the last pass.
    pub after: Vec<Transition>,
}

struct Image {
    image: vk::Image,
    view: vk::ImageView,
    extent: vk::Extent2D,
    initial: Use,
    last: Use,
}

/// One frame's passes over imported images, run in the order added (ADR 0021).
#[derive(Default)]
pub struct Graph<'a> {
    images: Vec<Image>,
    passes: Vec<Pass<'a>>,
}

impl<'a> Graph<'a> {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an image owned elsewhere, in use `initial` now and left in use `last` after the graph runs.
    pub fn import(
        &mut self,
        image: vk::Image,
        view: vk::ImageView,
        extent: vk::Extent2D,
        initial: Use,
        last: Use,
    ) -> ImageId {
        self.images.push(Image {
            image,
            view,
            extent,
            initial,
            last,
        });
        ImageId(self.images.len() - 1)
    }

    /// Adds a pass after the ones already added.
    pub fn add(&mut self, pass: Pass<'a>) {
        self.passes.push(pass);
    }

    /// Plans the transitions: an image changes use where the next use differs, and after a write even where it
    /// does not (write after write still needs a barrier).
    pub fn plan(&self) -> Plan {
        let mut state: Vec<Use> = self.images.iter().map(|i| i.initial).collect();
        let mut step = |image: ImageId, to: Use, out: &mut Vec<Transition>| {
            let from = state[image.0];
            if from != to || from.writes() {
                out.push(Transition { image, from, to });
            }
            state[image.0] = to;
        };
        let before = self
            .passes
            .iter()
            .map(|p| {
                let mut out = Vec::new();
                for &(image, _) in &p.colors {
                    step(image, Use::ColorAttachment, &mut out);
                }
                out
            })
            .collect();
        let mut after = Vec::new();
        for (i, image) in self.images.iter().enumerate() {
            step(ImageId(i), image.last, &mut after);
        }
        Plan { before, after }
    }

    /// Records every pass with its barriers into `cmd`.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording on `device`, and the imported images and views must be valid and in their initial use.
    pub unsafe fn record(self, device: &ash::Device, cmd: vk::CommandBuffer) {
        let plan = self.plan();
        // SAFETY: per this function's contract; barriers and rendering infos only reference imported images.
        unsafe {
            for (pass, before) in self.passes.into_iter().zip(&plan.before) {
                barriers(device, cmd, &self.images, before);
                let attachments: Vec<_> = pass
                    .colors
                    .iter()
                    .map(|&(image, load)| {
                        let (load_op, clear) = match load {
                            Load::Clear(c) => (vk::AttachmentLoadOp::CLEAR, c),
                            Load::Load => (vk::AttachmentLoadOp::LOAD, [0.0; 4]),
                            Load::DontCare => (vk::AttachmentLoadOp::DONT_CARE, [0.0; 4]),
                        };
                        vk::RenderingAttachmentInfo::default()
                            .image_view(self.images[image.0].view)
                            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                            .load_op(load_op)
                            .store_op(vk::AttachmentStoreOp::STORE)
                            .clear_value(vk::ClearValue {
                                color: vk::ClearColorValue { float32: clear },
                            })
                    })
                    .collect();
                if let Some(&(first, _)) = pass.colors.first() {
                    let area = vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: self.images[first.0].extent,
                    };
                    let info = vk::RenderingInfo::default()
                        .render_area(area)
                        .layer_count(1)
                        .color_attachments(&attachments);
                    device.cmd_begin_rendering(cmd, &info);
                }
                if let Some(record) = pass.record {
                    record(device, cmd);
                }
                if !pass.colors.is_empty() {
                    device.cmd_end_rendering(cmd);
                }
            }
            barriers(device, cmd, &self.images, &plan.after);
        }
    }
}

/// # Safety
///
/// As [`Graph::record`].
unsafe fn barriers(device: &ash::Device, cmd: vk::CommandBuffer, images: &[Image], transitions: &[Transition]) {
    if transitions.is_empty() {
        return;
    }
    let barriers: Vec<_> = transitions
        .iter()
        .map(|t| {
            vk::ImageMemoryBarrier2::default()
                .src_stage_mask(t.from.stage())
                // Only writes need making available.
                .src_access_mask(if t.from.writes() {
                    t.from.access()
                } else {
                    vk::AccessFlags2::NONE
                })
                .dst_stage_mask(t.to.stage())
                .dst_access_mask(t.to.access())
                .old_layout(t.from.layout())
                .new_layout(t.to.layout())
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(images[t.image.0].image)
                .subresource_range(COLOR)
        })
        .collect();
    // SAFETY: per the caller's contract.
    unsafe { device.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&barriers)) };
}

/// The one mip and layer of a colour image.
pub(crate) const COLOR: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};
