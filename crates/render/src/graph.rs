//! Render graph: passes declare the images they draw to and the buffers they read and write; the graph plans the
//! layout transitions and barriers between them and records the passes with dynamic rendering (ADRs 0021, 0023).

use ash::vk;

use crate::memory::Image as Allocated;
use crate::{Context, FRAMES_IN_FLIGHT, Result};

/// An image in a [`Graph`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageId(usize);

/// A buffer in a [`Graph`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferId(usize);

/// What an image is used for at one point in the frame: a layout plus the stages and accesses that touch it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    /// Contents don't matter: a new image, or a swapchain image just acquired (its semaphore is waited on at colour
    /// output, so that is where the first barrier starts).
    Undefined,
    /// Drawn to as a colour attachment.
    ColorAttachment,
    /// Tested and written as a depth attachment.
    DepthAttachment,
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
            Use::DepthAttachment => vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            Use::TransferSrc => vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            Use::Present => vk::ImageLayout::PRESENT_SRC_KHR,
        }
    }

    fn stage(self) -> vk::PipelineStageFlags2 {
        match self {
            Use::Undefined | Use::ColorAttachment => vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            Use::DepthAttachment => {
                vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS
            }
            Use::TransferSrc => vk::PipelineStageFlags2::COPY,
            // The submit's semaphore signal (all commands) orders presentation after the transition.
            Use::Present => vk::PipelineStageFlags2::NONE,
        }
    }

    fn access(self) -> vk::AccessFlags2 {
        match self {
            Use::Undefined | Use::Present => vk::AccessFlags2::NONE,
            Use::ColorAttachment => vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            Use::DepthAttachment => {
                vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE
            }
            Use::TransferSrc => vk::AccessFlags2::TRANSFER_READ,
        }
    }

    fn writes(self) -> bool {
        matches!(self, Use::ColorAttachment | Use::DepthAttachment)
    }
}

/// How a pass touches a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// Written by transfer commands (`vkCmdUpdateBuffer`, fills, copies).
    TransferWrite,
    /// Read by compute shaders.
    ComputeRead,
    /// Written (and possibly read) by compute shaders.
    ComputeWrite,
    /// Read by vertex shaders.
    VertexRead,
    /// Read by fragment shaders.
    FragmentRead,
    /// Read as indirect draw commands.
    IndirectRead,
    /// Read as an index buffer.
    IndexRead,
}

impl Access {
    fn stage(self) -> vk::PipelineStageFlags2 {
        use vk::PipelineStageFlags2 as S;
        match self {
            Access::TransferWrite => S::ALL_TRANSFER,
            Access::ComputeRead | Access::ComputeWrite => S::COMPUTE_SHADER,
            Access::VertexRead => S::VERTEX_SHADER,
            Access::FragmentRead => S::FRAGMENT_SHADER,
            Access::IndirectRead => S::DRAW_INDIRECT,
            Access::IndexRead => S::INDEX_INPUT,
        }
    }

    fn access(self) -> vk::AccessFlags2 {
        use vk::AccessFlags2 as A;
        match self {
            Access::TransferWrite => A::TRANSFER_WRITE,
            Access::ComputeRead | Access::VertexRead | Access::FragmentRead => A::SHADER_STORAGE_READ,
            Access::ComputeWrite => A::SHADER_STORAGE_READ | A::SHADER_STORAGE_WRITE,
            Access::IndirectRead => A::INDIRECT_COMMAND_READ,
            Access::IndexRead => A::INDEX_READ,
        }
    }

    fn writes(self) -> bool {
        matches!(self, Access::TransferWrite | Access::ComputeWrite)
    }
}

/// What a pass does with an attachment's previous contents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Load {
    /// Clear to this colour; a depth attachment clears to the first component.
    Clear([f32; 4]),
    /// Keep them.
    Load,
    /// Don't care: the pass overwrites every pixel.
    DontCare,
}

type Record<'a> = Box<dyn FnOnce(&ash::Device, vk::CommandBuffer) + 'a>;

/// One pass: the attachments it draws to, the buffers it touches and, optionally, the commands it records. A pass
/// with attachments records inside a dynamic rendering scope over them; one without (compute) records outside any.
pub struct Pass<'a> {
    name: &'static str,
    colors: Vec<(ImageId, Load)>,
    depth: Option<(ImageId, Load)>,
    buffers: Vec<(BufferId, Access)>,
    record: Option<Record<'a>>,
}

impl<'a> Pass<'a> {
    /// A pass with no attachments and nothing to record yet.
    pub fn new(name: &'static str) -> Self {
        Pass {
            name,
            colors: Vec::new(),
            depth: None,
            buffers: Vec::new(),
            record: None,
        }
    }

    /// Adds a colour attachment.
    pub fn color(mut self, image: ImageId, load: Load) -> Self {
        self.colors.push((image, load));
        self
    }

    /// Sets the depth attachment.
    pub fn depth(mut self, image: ImageId, load: Load) -> Self {
        self.depth = Some((image, load));
        self
    }

    /// Declares an access to a buffer; declare each one the pass makes, in order.
    pub fn buffer(mut self, buffer: BufferId, access: Access) -> Self {
        self.buffers.push((buffer, access));
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

/// One buffer barrier: the earlier accesses that must finish (and whose writes must be visible) before the later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BufferBarrier {
    /// The buffer.
    pub buffer: BufferId,
    /// Stages of the earlier accesses.
    pub src_stage: vk::PipelineStageFlags2,
    /// The earlier write, if any (reads need no making available).
    pub src_access: vk::AccessFlags2,
    /// Stage of the later access.
    pub dst_stage: vk::PipelineStageFlags2,
    /// The later access.
    pub dst_access: vk::AccessFlags2,
}

/// The barriers a graph records: before each pass, and after the last to put images in their final use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Image transitions before each pass, by pass index.
    pub before: Vec<Vec<Transition>>,
    /// Buffer barriers before each pass, by pass index.
    pub buffers: Vec<Vec<BufferBarrier>>,
    /// Image transitions after the last pass.
    pub after: Vec<Transition>,
}

/// A graph-owned image: what [`Graph::transient`] asks the pool for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageDesc {
    /// Texel format; depth formats get a depth view.
    pub format: vk::Format,
    /// Size in pixels.
    pub extent: vk::Extent2D,
    /// How passes use it.
    pub usage: vk::ImageUsageFlags,
}

impl ImageDesc {
    fn aspect(&self) -> vk::ImageAspectFlags {
        match self.format {
            vk::Format::D16_UNORM | vk::Format::D32_SFLOAT | vk::Format::X8_D24_UNORM_PACK32 => {
                vk::ImageAspectFlags::DEPTH
            }
            _ => vk::ImageAspectFlags::COLOR,
        }
    }
}

struct Image {
    image: vk::Image,
    view: vk::ImageView,
    extent: vk::Extent2D,
    aspect: vk::ImageAspectFlags,
    initial: Use,
    /// `None` for transients: they stay in their last use, which the pool remembers.
    last: Option<Use>,
    transient: Option<ImageDesc>,
}

#[derive(Clone, Copy, Default)]
struct BufferState {
    /// The last write, if any.
    write: Option<Access>,
    /// Stages that read since it.
    reads: vk::PipelineStageFlags2,
    /// Read stages already made to wait for it.
    synced: vk::PipelineStageFlags2,
}

impl BufferState {
    /// Moves to `access`, returning the barrier it needs.
    fn step(&mut self, buffer: BufferId, access: Access) -> Option<BufferBarrier> {
        let barrier = |src_stage, src_access| BufferBarrier {
            buffer,
            src_stage,
            src_access,
            dst_stage: access.stage(),
            dst_access: access.access(),
        };
        if access.writes() {
            // Write after write and write after read: every earlier access finishes first.
            let src_stage = self.write.map_or(vk::PipelineStageFlags2::NONE, Access::stage) | self.reads;
            let src_access = self.write.map_or(vk::AccessFlags2::NONE, Access::access);
            *self = BufferState {
                write: Some(access),
                ..BufferState::default()
            };
            (src_stage != vk::PipelineStageFlags2::NONE).then(|| barrier(src_stage, src_access))
        } else {
            self.reads |= access.stage();
            let write = self.write.filter(|_| !self.synced.contains(access.stage()))?;
            self.synced |= access.stage();
            Some(barrier(write.stage(), write.access()))
        }
    }
}

/// One frame's passes over imported and transient images and imported buffers, run in the order added (ADR 0021).
#[derive(Default)]
pub struct Graph<'a> {
    images: Vec<Image>,
    buffers: Vec<(vk::Buffer, BufferState)>,
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
            aspect: vk::ImageAspectFlags::COLOR,
            initial,
            last: Some(last),
            transient: None,
        });
        ImageId(self.images.len() - 1)
    }

    /// Adds an image the graph takes from the [`Transients`] pool when it records: reused across frames, its
    /// contents are undefined at the first pass (load it as `Clear` or `DontCare`).
    pub fn transient(&mut self, desc: ImageDesc) -> ImageId {
        self.images.push(Image {
            image: vk::Image::null(),
            view: vk::ImageView::null(),
            extent: desc.extent,
            aspect: desc.aspect(),
            initial: Use::Undefined,
            last: None,
            transient: Some(desc),
        });
        ImageId(self.images.len() - 1)
    }

    /// An image's size.
    pub fn extent(&self, image: ImageId) -> vk::Extent2D {
        self.images[image.0].extent
    }

    /// Adds a buffer owned elsewhere. `prior` lists how earlier work (a previous frame) touched it, so the first
    /// access waits for that.
    pub fn import_buffer(&mut self, buffer: vk::Buffer, prior: &[Access]) -> BufferId {
        let id = BufferId(self.buffers.len());
        let mut state = BufferState::default();
        for &a in prior {
            state.step(id, a);
        }
        self.buffers.push((buffer, state));
        id
    }

    /// Adds a pass after the ones already added.
    pub fn add(&mut self, pass: Pass<'a>) {
        self.passes.push(pass);
    }

    /// Plans the barriers: an image changes use where the next use differs, and after a write even where it does
    /// not (write after write still needs a barrier); a buffer waits for the last write before a read, and for
    /// every earlier access before a write.
    pub fn plan(&self) -> Plan {
        self.plan_with_finals().0
    }

    /// The plan and every image's use after the graph.
    fn plan_with_finals(&self) -> (Plan, Vec<Use>) {
        let mut state: Vec<Use> = self.images.iter().map(|i| i.initial).collect();
        let mut step = |image: ImageId, to: Use, out: &mut Vec<Transition>| {
            let from = state[image.0];
            if from != to || from.writes() {
                out.push(Transition { image, from, to });
            }
            state[image.0] = to;
        };
        let mut buffers: Vec<BufferState> = self.buffers.iter().map(|b| b.1).collect();
        let mut before = Vec::with_capacity(self.passes.len());
        let mut buffer_barriers = Vec::with_capacity(self.passes.len());
        for p in &self.passes {
            let mut out = Vec::new();
            for &(image, _) in &p.colors {
                step(image, Use::ColorAttachment, &mut out);
            }
            if let Some((image, _)) = p.depth {
                step(image, Use::DepthAttachment, &mut out);
            }
            before.push(out);
            buffer_barriers.push(
                p.buffers
                    .iter()
                    .filter_map(|&(b, access)| buffers[b.0].step(b, access))
                    .collect(),
            );
        }
        let mut after = Vec::new();
        for (i, image) in self.images.iter().enumerate() {
            if let Some(last) = image.last {
                step(ImageId(i), last, &mut after);
            }
        }
        let plan = Plan {
            before,
            buffers: buffer_barriers,
            after,
        };
        (plan, state)
    }

    /// Takes the transient images from `pool` and records every pass with its barriers into `cmd`.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording on `ctx`'s device; the imported images, views and buffers must be valid and the
    /// images in their initial use; `pool` must only be used with `ctx`.
    pub unsafe fn record(mut self, ctx: &Context, pool: &mut Transients, cmd: vk::CommandBuffer) -> Result<()> {
        let mut slots = Vec::new();
        for (i, image) in self.images.iter_mut().enumerate() {
            if let Some(desc) = image.transient {
                let slot = pool.take(ctx, desc)?;
                let pooled = &pool.images[slot];
                (image.image, image.view, image.initial) = (pooled.image.handle, pooled.image.view, pooled.last);
                slots.push((i, slot));
            }
        }
        let (plan, finals) = self.plan_with_finals();
        for (i, slot) in slots {
            pool.images[slot].last = finals[i];
        }
        let device = ctx.device();
        // SAFETY: per this function's contract; barriers and rendering infos only reference the graph's resources.
        unsafe {
            for ((pass, before), buffers) in self.passes.into_iter().zip(&plan.before).zip(&plan.buffers) {
                barriers(device, cmd, &self.images, &self.buffers, before, buffers);
                let attachment = |(image, load): (ImageId, Load)| {
                    let (load_op, clear) = match load {
                        Load::Clear(c) => (vk::AttachmentLoadOp::CLEAR, c),
                        Load::Load => (vk::AttachmentLoadOp::LOAD, [0.0; 4]),
                        Load::DontCare => (vk::AttachmentLoadOp::DONT_CARE, [0.0; 4]),
                    };
                    let image = &self.images[image.0];
                    let (layout, clear_value) = if image.aspect == vk::ImageAspectFlags::DEPTH {
                        let depth = vk::ClearDepthStencilValue {
                            depth: clear[0],
                            stencil: 0,
                        };
                        (Use::DepthAttachment.layout(), vk::ClearValue { depth_stencil: depth })
                    } else {
                        let color = vk::ClearColorValue { float32: clear };
                        (Use::ColorAttachment.layout(), vk::ClearValue { color })
                    };
                    vk::RenderingAttachmentInfo::default()
                        .image_view(image.view)
                        .image_layout(layout)
                        .load_op(load_op)
                        .store_op(vk::AttachmentStoreOp::STORE)
                        .clear_value(clear_value)
                };
                let colors: Vec<_> = pass.colors.iter().copied().map(attachment).collect();
                let depth = pass.depth.map(attachment);
                let first = pass.colors.first().or(pass.depth.as_ref()).map(|&(image, _)| image);
                if let Some(first) = first {
                    let area = vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent: self.images[first.0].extent,
                    };
                    let mut info = vk::RenderingInfo::default()
                        .render_area(area)
                        .layer_count(1)
                        .color_attachments(&colors);
                    if let Some(depth) = &depth {
                        info = info.depth_attachment(depth);
                    }
                    device.cmd_begin_rendering(cmd, &info);
                }
                if let Some(record) = pass.record {
                    record(device, cmd);
                }
                if first.is_some() {
                    device.cmd_end_rendering(cmd);
                }
            }
            barriers(device, cmd, &self.images, &self.buffers, &plan.after, &[]);
        }
        Ok(())
    }
}

/// # Safety
///
/// As [`Graph::record`].
unsafe fn barriers(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    images: &[Image],
    buffers: &[(vk::Buffer, BufferState)],
    transitions: &[Transition],
    buffer_barriers: &[BufferBarrier],
) {
    if transitions.is_empty() && buffer_barriers.is_empty() {
        return;
    }
    let image_barriers: Vec<_> = transitions
        .iter()
        .map(|t| {
            let image = &images[t.image.0];
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
                .image(image.image)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: image.aspect,
                    ..COLOR
                })
        })
        .collect();
    let buffer_barriers: Vec<_> = buffer_barriers
        .iter()
        .map(|b| {
            vk::BufferMemoryBarrier2::default()
                .src_stage_mask(b.src_stage)
                .src_access_mask(b.src_access)
                .dst_stage_mask(b.dst_stage)
                .dst_access_mask(b.dst_access)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(buffers[b.buffer.0].0)
                .size(vk::WHOLE_SIZE)
        })
        .collect();
    let info = vk::DependencyInfo::default()
        .image_memory_barriers(&image_barriers)
        .buffer_memory_barriers(&buffer_barriers);
    // SAFETY: per the caller's contract.
    unsafe { device.cmd_pipeline_barrier2(cmd, &info) };
}

struct Pooled {
    desc: ImageDesc,
    image: Allocated,
    /// Its use after the last graph that took it.
    last: Use,
    /// The frame that last took it.
    frame: u64,
}

/// The images behind [`Graph::transient`], kept across frames and reused by description; one no frame took for
/// [`FRAMES_IN_FLIGHT`] frames is destroyed. Not aliased yet (ADR 0023).
#[derive(Default)]
pub struct Transients {
    images: Vec<Pooled>,
    /// The current frame.
    frame: u64,
}

impl Transients {
    /// Starts a frame: destroys images no frame took for [`FRAMES_IN_FLIGHT`] frames.
    ///
    /// # Safety
    ///
    /// Frames older than [`FRAMES_IN_FLIGHT`] have finished on the GPU; `ctx` is the pool's context.
    pub unsafe fn next_frame(&mut self, ctx: &Context) {
        self.frame += 1;
        let frame = self.frame;
        let (keep, old): (Vec<_>, Vec<_>) = std::mem::take(&mut self.images)
            .into_iter()
            .partition(|p| frame - p.frame < FRAMES_IN_FLIGHT as u64);
        self.images = keep;
        for p in old {
            // SAFETY: per the contract, no frame that took it still runs.
            unsafe { ctx.destroy_image(p.image) };
        }
    }

    /// Takes an image matching `desc` that this frame has not taken yet, creating one if needed.
    fn take(&mut self, ctx: &Context, desc: ImageDesc) -> Result<usize> {
        let frame = self.frame;
        if let Some(i) = self.images.iter().position(|p| p.desc == desc && p.frame != frame) {
            self.images[i].frame = frame;
            return Ok(i);
        }
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(desc.format)
            .extent(desc.extent.into())
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(desc.usage);
        let image = ctx.create_image("transient", &info, desc.aspect())?;
        self.images.push(Pooled {
            desc,
            image,
            last: Use::Undefined,
            frame,
        });
        Ok(self.images.len() - 1)
    }

    /// Number of images in the pool.
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Whether the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Destroys every image.
    ///
    /// # Safety
    ///
    /// The GPU no longer uses them; `ctx` is the pool's context.
    pub unsafe fn destroy(&mut self, ctx: &Context) {
        for p in self.images.drain(..) {
            // SAFETY: per the contract.
            unsafe { ctx.destroy_image(p.image) };
        }
    }
}

/// The one mip and layer of a colour image.
pub(crate) const COLOR: vk::ImageSubresourceRange = vk::ImageSubresourceRange {
    aspect_mask: vk::ImageAspectFlags::COLOR,
    base_mip_level: 0,
    level_count: 1,
    base_array_layer: 0,
    layer_count: 1,
};
