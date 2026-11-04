//! The forward pass: draws sorted into runs by pipeline, one indirect multi-draw per run, vertices pulled from the
//! scene's shared buffers and textures from the bindless array (design doc §6 "GPU-driven path", ADR 0023).

use ash::vk;
use civbb_format::math::{Mat4, Vec3};

use crate::graph::{Access, BufferId, Graph, ImageDesc, ImageId, Load, Pass};
use crate::material::{MATERIAL_WORDS, PipelineKey, Shader};
use crate::memory::{bytes, update};
use crate::scene::{MaterialId, MeshData, Scene};
use crate::skin::SkinnedMesh;
use crate::{Buffer, Context, MemoryLocation, Result};

const SPIRV: &[u8] = include_bytes!("../shaders/spv/mesh.spv");
/// Push constants: five buffer addresses.
const PUSH_SIZE: u32 = 40;
/// Words per draw record (`Draw` in `mesh.slang`).
const RECORD_WORDS: usize = 4;
/// Words per `VkDrawIndexedIndirectCommand`.
const COMMAND_WORDS: usize = 5;
/// Words of the view record (`View` in `mesh.slang`).
const VIEW_WORDS: usize = 32;

/// The depth format: 32-bit float, cleared to 0 (reverse Z: near is 1, infinitely far is 0).
pub const DEPTH: vk::Format = vk::Format::D32_SFLOAT;

/// The camera and the light of a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// World to clip space, column-major; clip depth is reverse Z.
    pub view_proj: Mat4,
    /// Camera position, for view-dependent shading.
    pub eye: Vec3,
    /// Unit direction towards the one directional light.
    pub light_dir: Vec3,
    /// The light's colour: what a white Lambert surface facing it shows.
    pub light_color: Vec3,
    /// Uniform ambient light.
    pub ambient: Vec3,
}

impl View {
    /// A camera at `eye` looking at `target` with +Y up, a vertical field of view `fov_y` (radians), `aspect`
    /// (width over height) and a near plane at `near`, with an infinite far plane and reverse Z; lit from the
    /// upper front left.
    pub fn look_at(eye: Vec3, target: Vec3, fov_y: f32, aspect: f32, near: f32) -> Self {
        let sub = |a: Vec3, b: Vec3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let dot = |a: Vec3, b: Vec3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let cross = |a: Vec3, b: Vec3| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let unit = |a: Vec3| {
            let l = dot(a, a).sqrt();
            a.map(|x| x / l)
        };
        let f = unit(sub(target, eye));
        let s = unit(cross(f, [0.0, 1.0, 0.0]));
        let u = cross(s, f);
        let view: Mat4 = [
            s[0],
            u[0],
            -f[0],
            0.0,
            s[1],
            u[1],
            -f[1],
            0.0,
            s[2],
            u[2],
            -f[2],
            0.0,
            -dot(s, eye),
            -dot(u, eye),
            dot(f, eye),
            1.0,
        ];
        let g = 1.0 / (fov_y / 2.0).tan();
        // Y flipped for Vulkan's downward clip Y; depth = near / distance.
        let proj: Mat4 = [
            g / aspect,
            0.0,
            0.0,
            0.0,
            0.0,
            -g,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            -1.0,
            0.0,
            0.0,
            near,
            0.0,
        ];
        View {
            view_proj: mul(&proj, &view),
            eye,
            light_dir: unit([-0.3, 0.8, 0.5]),
            light_color: [1.0; 3],
            ambient: [0.3; 3],
        }
    }

    fn words(&self) -> [u32; VIEW_WORDS] {
        let mut w = [0.0; VIEW_WORDS];
        w[..16].copy_from_slice(&self.view_proj);
        for (i, v) in [self.eye, self.light_dir, self.light_color, self.ambient]
            .iter()
            .enumerate()
        {
            w[16 + i * 4..][..3].copy_from_slice(v);
        }
        w.map(f32::to_bits)
    }
}

/// Column-major `a * b`.
fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|i| {
        let (col, row) = (i / 4, i % 4);
        (0..4).map(|k| a[k * 4 + row] * b[col * 4 + k]).sum()
    })
}

/// One submesh of one mesh instance to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Draw {
    /// First index in the scene's index buffer.
    pub first_index: u32,
    /// Number of indices.
    pub index_count: u32,
    /// The instance's first vertex in the skinned buffer.
    pub skinned: u32,
    /// The mesh's first vertex in the attribute buffer.
    pub attrs: u32,
    /// The material.
    pub material: MaterialId,
    /// Distance from the camera, ordering blended draws back to front.
    pub distance: f32,
}

impl MeshData {
    /// Adds a draw per submesh of `instance` (an instance of this mesh), and an outline draw after it if its
    /// material has one; `materials` maps the avatar's material indices to the scene's. Submeshes whose material is
    /// out of range are skipped.
    pub fn draws(&self, instance: &SkinnedMesh, materials: &[MaterialId], distance: f32, out: &mut Vec<Draw>) {
        for s in &self.submeshes {
            if let Some(&material) = materials.get(s.material as usize) {
                let draw = Draw {
                    first_index: self.index_base + s.first_index,
                    index_count: s.index_count,
                    skinned: instance.base(),
                    attrs: self.vertex_base,
                    material,
                    distance,
                };
                out.push(draw);
                if let Some(key) = material.outline {
                    out.push(Draw {
                        material: MaterialId { key, ..material },
                        ..draw
                    });
                }
            }
        }
    }
}

/// Consecutive draws sharing a pipeline: one indirect multi-draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    /// The pipeline.
    pub key: PipelineKey,
    /// First draw.
    pub first: u32,
    /// Number of draws.
    pub count: u32,
}

/// A frame's draws as the GPU reads them: a record and an indirect command per draw, and the runs over them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Batches {
    /// Per draw: material, first skinned vertex, first attribute vertex, 0.
    pub records: Vec<u32>,
    /// Per draw: index count, instance count 1, first index, vertex offset 0, first instance = the draw's index.
    pub commands: Vec<u32>,
    /// Runs of draws sharing a pipeline, in draw order.
    pub runs: Vec<Run>,
}

impl Batches {
    /// Number of draws.
    pub fn len(&self) -> usize {
        self.records.len() / RECORD_WORDS
    }

    /// Whether there are no draws.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Sorts `draws` and fills `out`: opaque draws first, grouped by pipeline; then blended draws by render queue and
/// back to front, each run of one pipeline merged. Draws with equal keys keep their order.
pub fn batch(draws: &mut [Draw], out: &mut Batches) {
    draws.sort_by(|a, b| {
        let (ka, kb) = (a.material.key, b.material.key);
        ka.blend.cmp(&kb.blend).then_with(|| {
            if ka.blend {
                let by_queue = a.material.queue.cmp(&b.material.queue);
                by_queue.then(b.distance.total_cmp(&a.distance))
            } else {
                ka.index().cmp(&kb.index())
            }
        })
    });
    out.records.clear();
    out.commands.clear();
    out.runs.clear();
    for (i, d) in draws.iter().enumerate() {
        let i = i as u32;
        out.records.extend([d.material.index, d.skinned, d.attrs, 0]);
        out.commands.extend([d.index_count, 1, d.first_index, 0, i]);
        match out.runs.last_mut() {
            Some(run) if run.key == d.material.key => run.count += 1,
            _ => out.runs.push(Run {
                key: d.material.key,
                first: i,
                count: 1,
            }),
        }
    }
}

/// The forward pipelines (one per [`PipelineKey`]) and the per-frame view, draw record and command buffers.
pub struct Forward {
    layout: vk::PipelineLayout,
    pipelines: Vec<vk::Pipeline>,
    view: Buffer,
    records: Buffer,
    commands: Buffer,
    max_draws: u32,
}

impl Forward {
    /// Creates every pipeline for drawing `scene` into `color` images, and room for `max_draws` draws a frame.
    pub fn new(ctx: &Context, scene: &Scene, color: vk::Format, max_draws: u32) -> Result<Self> {
        use vk::BufferUsageFlags as U;
        let gpu = MemoryLocation::GpuOnly;
        let n = u64::from(max_draws);
        let d = ctx.device();
        let code: Vec<u32> = SPIRV.as_chunks().0.iter().map(|&c| u32::from_le_bytes(c)).collect();
        let range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            offset: 0,
            size: PUSH_SIZE,
        };
        let sets = [scene.textures().layout()];
        let mut f = Forward {
            layout: vk::PipelineLayout::null(),
            pipelines: Vec::new(),
            view: ctx.create_buffer("view", VIEW_WORDS as u64 * 4, U::STORAGE_BUFFER | U::TRANSFER_DST, gpu)?,
            records: ctx.create_buffer(
                "draw records",
                n * RECORD_WORDS as u64 * 4,
                U::STORAGE_BUFFER | U::TRANSFER_DST,
                gpu,
            )?,
            commands: ctx.create_buffer(
                "draw commands",
                n * COMMAND_WORDS as u64 * 4,
                U::INDIRECT_BUFFER | U::TRANSFER_DST,
                gpu,
            )?,
            max_draws,
        };
        // SAFETY: valid create infos; on failure everything created so far is destroyed.
        let built = unsafe {
            (|| -> Result<()> {
                f.layout = d.create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&sets)
                        .push_constant_ranges(&[range]),
                    None,
                )?;
                let module = d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
                let pipelines = create_pipelines(d, f.layout, module, color);
                d.destroy_shader_module(module, None);
                f.pipelines = pipelines?;
                Ok(())
            })()
        };
        match built {
            Ok(()) => Ok(f),
            Err(e) => {
                // SAFETY: nothing was used on the GPU; null handles are ignored.
                unsafe { f.destroy(ctx) };
                Err(e)
            }
        }
    }

    /// Adds the frame's two passes to `graph`: one writing the view, draw records, commands and the changed
    /// `materials` (index from [`Scene::add_material`], record from [`crate::material::pack`]), then the forward pass
    /// drawing `batches` into `target` (cleared to `clear`) with a transient depth buffer. `skinned` is
    /// [`Scene::skinned`] in the graph, written by an earlier skinning pass.
    ///
    /// # Panics
    ///
    /// If `batches` holds more draws than the `max_draws` given to [`Forward::new`], or a material index was not
    /// added to the scene.
    #[allow(clippy::too_many_arguments)]
    pub fn add<'a>(
        &'a self,
        graph: &mut Graph<'a>,
        scene: &'a Scene,
        target: ImageId,
        skinned: BufferId,
        view: &View,
        batches: &Batches,
        materials: &[(u32, [u32; MATERIAL_WORDS])],
        clear: [f32; 4],
    ) {
        assert!(
            batches.len() <= self.max_draws as usize,
            "more draws than Forward::new allowed"
        );
        let record = (MATERIAL_WORDS * 4) as u64;
        let material_writes: Vec<(u64, Vec<u8>)> = materials
            .iter()
            .map(|(index, words)| {
                assert!(*index < scene.used().materials, "material {index} is not in the scene");
                (u64::from(*index) * record, bytes(words))
            })
            .collect();
        let view_words = view.words();
        let records = bytes(&batches.records);
        let commands = bytes(&batches.commands);
        let runs = batches.runs.clone();
        let view_buf = graph.import_buffer(self.view.handle, &[Access::VertexRead, Access::FragmentRead]);
        let records_buf = graph.import_buffer(self.records.handle, &[Access::VertexRead]);
        let commands_buf = graph.import_buffer(self.commands.handle, &[Access::IndirectRead]);
        let materials_buf = graph.import_buffer(scene.materials().handle, &[Access::VertexRead, Access::FragmentRead]);
        graph.add(
            Pass::new("draw upload")
                .buffer(view_buf, Access::TransferWrite)
                .buffer(records_buf, Access::TransferWrite)
                .buffer(commands_buf, Access::TransferWrite)
                .buffer(materials_buf, Access::TransferWrite)
                .record(move |d, cmd| {
                    // SAFETY: the graph records this outside a rendering scope; the buffers hold max_draws draws
                    // and every material written was added to the scene.
                    unsafe {
                        update(d, cmd, &self.view, 0, &bytes(&view_words));
                        update(d, cmd, &self.records, 0, &records);
                        update(d, cmd, &self.commands, 0, &commands);
                        for (offset, data) in &material_writes {
                            update(d, cmd, scene.materials(), *offset, data);
                        }
                    }
                }),
        );
        let extent = graph.extent(target);
        let depth = graph.transient(ImageDesc {
            format: DEPTH,
            extent,
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
        });
        let mut push = Vec::with_capacity(PUSH_SIZE as usize);
        for address in [
            scene.skinned().address,
            scene.attrs().address,
            scene.materials().address,
            self.records.address,
            self.view.address,
        ] {
            push.extend(address.to_le_bytes());
        }
        graph.add(
            Pass::new("forward")
                .color(target, Load::Clear(clear))
                .depth(depth, Load::Clear([0.0; 4]))
                .buffer(skinned, Access::VertexRead)
                .buffer(view_buf, Access::VertexRead)
                .buffer(view_buf, Access::FragmentRead)
                .buffer(records_buf, Access::VertexRead)
                .buffer(commands_buf, Access::IndirectRead)
                .buffer(materials_buf, Access::VertexRead)
                .buffer(materials_buf, Access::FragmentRead)
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
                    let stages = vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT;
                    let graphics = vk::PipelineBindPoint::GRAPHICS;
                    let stride = (COMMAND_WORDS * 4) as u32;
                    // SAFETY: inside the pass's rendering scope; every handle is alive for the frame and the runs
                    // stay within the commands written above.
                    unsafe {
                        d.cmd_set_viewport(cmd, 0, &[viewport]);
                        d.cmd_set_scissor(cmd, 0, &[scissor]);
                        d.cmd_bind_descriptor_sets(cmd, graphics, self.layout, 0, &[scene.textures().set()], &[]);
                        d.cmd_push_constants(cmd, self.layout, stages, 0, &push);
                        d.cmd_bind_index_buffer(cmd, scene.indices().handle, 0, vk::IndexType::UINT32);
                        for run in &runs {
                            d.cmd_bind_pipeline(cmd, graphics, self.pipelines[run.key.index()]);
                            let offset = u64::from(run.first) * u64::from(stride);
                            d.cmd_draw_indexed_indirect(cmd, self.commands.handle, offset, run.count, stride);
                        }
                    }
                }),
        );
    }

    /// Destroys the pipelines and buffers.
    ///
    /// # Safety
    ///
    /// The GPU no longer uses them.
    pub unsafe fn destroy(self, ctx: &Context) {
        let d = ctx.device();
        // SAFETY: per the contract; null handles are ignored.
        unsafe {
            for p in &self.pipelines {
                d.destroy_pipeline(*p, None);
            }
            d.destroy_pipeline_layout(self.layout, None);
            ctx.destroy_buffer(self.view);
            ctx.destroy_buffer(self.records);
            ctx.destroy_buffer(self.commands);
        }
    }
}

/// One pipeline per key, in key index order.
///
/// # Safety
///
/// `layout` and `module` are alive on `d`.
unsafe fn create_pipelines(
    d: &ash::Device,
    layout: vk::PipelineLayout,
    module: vk::ShaderModule,
    color: vk::Format,
) -> Result<Vec<vk::Pipeline>> {
    let keys: Vec<PipelineKey> = PipelineKey::all().collect();
    let stages: Vec<[vk::PipelineShaderStageCreateInfo; 2]> = keys
        .iter()
        .map(|k| {
            let (vertex, fragment) = match k.shader {
                Shader::Pbr => (c"vertex", c"pbr"),
                Shader::Toon => (c"vertex", c"toon"),
                Shader::Unlit => (c"vertex", c"unlit"),
                Shader::Outline => (c"outline_vertex", c"outline"),
            };
            [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(module)
                    .name(vertex),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(module)
                    .name(fragment),
            ]
        })
        .collect();
    let rasters: Vec<_> = keys
        .iter()
        .map(|k| {
            vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                // Outlines are inverted hulls: only the pushed-out back faces show, around the mesh.
                .cull_mode(match (k.shader, k.double_sided) {
                    (Shader::Outline, _) => vk::CullModeFlags::FRONT,
                    (_, true) => vk::CullModeFlags::NONE,
                    (_, false) => vk::CullModeFlags::BACK,
                })
                // glTF fronts are counter-clockwise; the projection's Y flip keeps them so on screen.
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .line_width(1.0)
        })
        .collect();
    let depths: Vec<_> = keys
        .iter()
        .map(|k| {
            vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(!k.blend)
                .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL)
        })
        .collect();
    let attachments: Vec<[vk::PipelineColorBlendAttachmentState; 1]> = keys
        .iter()
        .map(|k| {
            [vk::PipelineColorBlendAttachmentState::default()
                .blend_enable(k.blend)
                .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .color_blend_op(vk::BlendOp::ADD)
                .src_alpha_blend_factor(vk::BlendFactor::ONE)
                .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                .alpha_blend_op(vk::BlendOp::ADD)
                .color_write_mask(vk::ColorComponentFlags::RGBA)]
        })
        .collect();
    let blends: Vec<_> = attachments
        .iter()
        .map(|a| vk::PipelineColorBlendStateCreateInfo::default().attachments(a))
        .collect();
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let multisample =
        vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let formats = [color];
    let mut renderings: Vec<_> = keys
        .iter()
        .map(|_| {
            vk::PipelineRenderingCreateInfo::default()
                .color_attachment_formats(&formats)
                .depth_attachment_format(DEPTH)
        })
        .collect();
    let infos: Vec<_> = renderings
        .iter_mut()
        .enumerate()
        .map(|(i, rendering)| {
            vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages[i])
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&assembly)
                .viewport_state(&viewport)
                .rasterization_state(&rasters[i])
                .multisample_state(&multisample)
                .depth_stencil_state(&depths[i])
                .color_blend_state(&blends[i])
                .dynamic_state(&dynamic)
                .layout(layout)
                .push_next(rendering)
        })
        .collect();
    // SAFETY: per the contract; every create info and what it points to is alive for the call.
    match unsafe { d.create_graphics_pipelines(vk::PipelineCache::null(), &infos, None) } {
        Ok(p) => Ok(p),
        Err((p, e)) => {
            for p in p {
                // SAFETY: just created, never used.
                unsafe { d.destroy_pipeline(p, None) };
            }
            Err(e.into())
        }
    }
}
