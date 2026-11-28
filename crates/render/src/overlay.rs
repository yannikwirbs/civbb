//! The overlay pass: screen-space UI triangles drawn over the frame after the forward pass, vertices pulled by
//! address, textures from the bindless array (ADR 0117).

use ash::vk;

use crate::graph::{Access, Graph, ImageId, Load, Pass};
use crate::memory::{bytes, update};
use crate::scene::Scene;
use crate::{Buffer, Context, MemoryLocation, Result};

const SPIRV: &[u8] = include_bytes!("../shaders/spv/overlay.spv");
/// Push constants: the vertex buffer's address and 2 / extent.
const PUSH_SIZE: u32 = 16;
/// Words per vertex (`Vertex` in `overlay.slang`).
const VERTEX_WORDS: usize = 12;

/// No texture: the vertex colour alone.
pub const NO_TEXTURE: u32 = u32::MAX;

/// One vertex: window pixels (origin top left), texture coordinates, linear colour with straight alpha, and a
/// bindless texture index or [`NO_TEXTURE`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    /// Position in window pixels.
    pub pos: [f32; 2],
    /// Texture coordinates.
    pub uv: [f32; 2],
    /// Multiplies the texel.
    pub color: [f32; 4],
    /// Bindless index from [`Scene`], or [`NO_TEXTURE`].
    pub texture: u32,
}

/// The overlay pipeline and its vertex buffer.
pub struct Overlay {
    layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    vertices: Buffer,
    max_vertices: u32,
}

impl Overlay {
    /// Creates the pipeline for drawing over `color` images with `scene`'s textures, and room for `max_vertices`
    /// vertices a frame.
    pub fn new(ctx: &Context, scene: &Scene, color: vk::Format, max_vertices: u32) -> Result<Self> {
        use vk::BufferUsageFlags as U;
        let d = ctx.device();
        let size = u64::from(max_vertices) * VERTEX_WORDS as u64 * 4;
        let mut o = Overlay {
            layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            vertices: ctx.create_buffer(
                "overlay vertices",
                size,
                U::STORAGE_BUFFER | U::TRANSFER_DST,
                MemoryLocation::GpuOnly,
            )?,
            max_vertices,
        };
        let code: Vec<u32> = SPIRV.as_chunks().0.iter().map(|&c| u32::from_le_bytes(c)).collect();
        let range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX,
            offset: 0,
            size: PUSH_SIZE,
        };
        let sets = [scene.textures().layout()];
        // SAFETY: valid create infos; on failure everything created so far is destroyed.
        let built = unsafe {
            (|| -> Result<()> {
                o.layout = d.create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&sets)
                        .push_constant_ranges(&[range]),
                    None,
                )?;
                let module = d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
                let pipeline = create_pipeline(d, o.layout, module, color);
                d.destroy_shader_module(module, None);
                o.pipeline = pipeline?;
                Ok(())
            })()
        };
        match built {
            Ok(()) => Ok(o),
            Err(e) => {
                // SAFETY: nothing was used on the GPU; null handles are ignored.
                unsafe { o.destroy(ctx) };
                Err(e)
            }
        }
    }

    /// Adds the frame's upload and overlay passes to `graph`, drawing `vertices` (three per triangle, beyond
    /// `max_vertices` dropped) over `target`. Nothing is added for no triangles.
    pub fn add<'a>(&'a self, graph: &mut Graph<'a>, scene: &'a Scene, target: ImageId, vertices: &[Vertex]) {
        let n = vertices.len().min(self.max_vertices as usize) / 3 * 3;
        if n == 0 {
            return;
        }
        let words: Vec<u32> = (vertices[..n].iter())
            .flat_map(|v| {
                let [x, y] = v.pos.map(f32::to_bits);
                let [u, w] = v.uv.map(f32::to_bits);
                let [r, g, b, a] = v.color.map(f32::to_bits);
                [x, y, u, w, r, g, b, a, v.texture, 0, 0, 0]
            })
            .collect();
        let data = bytes(&words);
        let buf = graph.import_buffer(self.vertices.handle, &[Access::VertexRead]);
        graph.add(
            Pass::new("overlay upload")
                .buffer(buf, Access::TransferWrite)
                .record(move |d, cmd| {
                    // SAFETY: outside a rendering scope; `n` vertices fit the buffer.
                    unsafe { update(d, cmd, &self.vertices, 0, &data) };
                }),
        );
        let extent = graph.extent(target);
        let mut push = Vec::with_capacity(PUSH_SIZE as usize);
        push.extend(self.vertices.address.to_le_bytes());
        push.extend((2.0 / extent.width.max(1) as f32).to_le_bytes());
        push.extend((2.0 / extent.height.max(1) as f32).to_le_bytes());
        graph.add(
            Pass::new("overlay")
                .color(target, Load::Load)
                .buffer(buf, Access::VertexRead)
                .record(move |d, cmd| {
                    let viewport = vk::Viewport {
                        x: 0.0,
                        y: 0.0,
                        width: extent.width as f32,
                        height: extent.height as f32,
                        min_depth: 0.0,
                        max_depth: 1.0,
                    };
                    let scissor = vk::Rect2D {
                        offset: vk::Offset2D::default(),
                        extent,
                    };
                    let graphics = vk::PipelineBindPoint::GRAPHICS;
                    // SAFETY: inside the pass's rendering scope; every handle is alive for the frame.
                    unsafe {
                        d.cmd_set_viewport(cmd, 0, &[viewport]);
                        d.cmd_set_scissor(cmd, 0, &[scissor]);
                        d.cmd_bind_pipeline(cmd, graphics, self.pipeline);
                        d.cmd_bind_descriptor_sets(cmd, graphics, self.layout, 0, &[scene.textures().set()], &[]);
                        d.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::VERTEX, 0, &push);
                        d.cmd_draw(cmd, n as u32, 1, 0, 0);
                    }
                }),
        );
    }

    /// Destroys the pipeline and buffer.
    ///
    /// # Safety
    ///
    /// The GPU no longer uses them.
    pub unsafe fn destroy(self, ctx: &Context) {
        let d = ctx.device();
        // SAFETY: per the contract; null handles are ignored.
        unsafe {
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_pipeline_layout(self.layout, None);
            ctx.destroy_buffer(self.vertices);
        }
    }
}

/// The one overlay pipeline: no culling, no depth, straight alpha blending.
///
/// # Safety
///
/// `layout` and `module` are alive on `d`.
unsafe fn create_pipeline(
    d: &ash::Device,
    layout: vk::PipelineLayout,
    module: vk::ShaderModule,
    color: vk::Format,
) -> Result<vk::Pipeline> {
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(module)
            .name(c"vertex"),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(module)
            .name(c"fragment"),
    ];
    let raster = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .line_width(1.0);
    let attachments = [vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(vk::ColorComponentFlags::RGBA)];
    let blend = vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let multisample =
        vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let depth = vk::PipelineDepthStencilStateCreateInfo::default();
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let formats = [color];
    let mut rendering = vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&formats);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&assembly)
        .viewport_state(&viewport)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .depth_stencil_state(&depth)
        .color_blend_state(&blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .push_next(&mut rendering);
    // SAFETY: per the contract; every create info and what it points to is alive for the call.
    match unsafe { d.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) } {
        Ok(p) => Ok(p[0]),
        Err((p, e)) => {
            for p in p {
                // SAFETY: just created, never used.
                unsafe { d.destroy_pipeline(p, None) };
            }
            Err(e.into())
        }
    }
}
