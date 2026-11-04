//! Bindless textures: one descriptor array of sampled images that every draw indexes by number, filled as
//! textures load: RGBA8 with mips made on the CPU, or BC7 with its own mips (design doc §6, ADRs 0023, 0029, 0035).

use ash::vk;

use crate::memory::Image;
use crate::{Context, MemoryLocation, Result};

/// Size of the descriptor array: textures the scene can hold at once.
pub const MAX_TEXTURES: u32 = 4096;

/// The bindless texture array: its set layout, the one set, a shared sampler and the images in it.
pub struct Textures {
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    sampler: vk::Sampler,
    images: Vec<Image>,
}

impl Textures {
    /// Creates the empty array.
    pub(crate) fn new(ctx: &Context) -> Result<Self> {
        let d = ctx.device();
        let mut t = Textures {
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            sampler: vk::Sampler::null(),
            images: Vec::new(),
        };
        // SAFETY: valid create infos; on failure the caller drops the partly built value after `destroy`.
        let built = unsafe {
            (|| -> Result<()> {
                let bindings = [vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(MAX_TEXTURES)
                    // Outline widths are sampled per vertex (ADR 0034).
                    .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
                let flags = [vk::DescriptorBindingFlags::PARTIALLY_BOUND
                    | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND
                    | vk::DescriptorBindingFlags::UPDATE_UNUSED_WHILE_PENDING];
                let mut binding_flags = vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&flags);
                t.layout = d.create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default()
                        .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                        .bindings(&bindings)
                        .push_next(&mut binding_flags),
                    None,
                )?;
                let sizes = [vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    descriptor_count: MAX_TEXTURES,
                }];
                t.pool = d.create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND)
                        .max_sets(1)
                        .pool_sizes(&sizes),
                    None,
                )?;
                let layouts = [t.layout];
                t.set = d.allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(t.pool)
                        .set_layouts(&layouts),
                )?[0];
                let limits = ctx.instance().get_physical_device_properties(ctx.physical()).limits;
                t.sampler = d.create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                        .address_mode_u(vk::SamplerAddressMode::REPEAT)
                        .address_mode_v(vk::SamplerAddressMode::REPEAT)
                        .address_mode_w(vk::SamplerAddressMode::REPEAT)
                        .anisotropy_enable(true)
                        .max_anisotropy(limits.max_sampler_anisotropy.min(16.0))
                        .max_lod(vk::LOD_CLAMP_NONE),
                    None,
                )?;
                Ok(())
            })()
        };
        match built {
            Ok(()) => Ok(t),
            Err(e) => {
                // SAFETY: nothing was used on the GPU yet; null handles are ignored.
                unsafe { t.destroy(ctx) };
                Err(e)
            }
        }
    }

    /// The set layout, for pipeline layouts.
    pub fn layout(&self) -> vk::DescriptorSetLayout {
        self.layout
    }

    /// The descriptor set, bound once per pass.
    pub fn set(&self) -> vk::DescriptorSet {
        self.set
    }

    /// Number of textures loaded.
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Whether no texture is loaded.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Uploads an RGBA8 image (`srgb` for colour, linear for data such as normals and masks) with its mips from
    /// [`civbb_core::mips::mips`], the same the compiler encodes (ADR 0035), and returns its index in the array.
    ///
    /// # Panics
    ///
    /// If `rgba` is not `width * height * 4` bytes, a side is 0 or over 16384, or the array is full.
    pub(crate) fn add(&mut self, ctx: &Context, width: u32, height: u32, rgba: &[u8], srgb: bool) -> Result<u32> {
        assert_eq!(
            rgba.len() as u64,
            u64::from(width) * u64::from(height) * 4,
            "RGBA8 texel count"
        );
        let format = if srgb {
            vk::Format::R8G8B8A8_SRGB
        } else {
            vk::Format::R8G8B8A8_UNORM
        };
        let mut levels = vec![rgba.to_vec()];
        levels.extend(civbb_core::mips::mips(rgba, width, height, srgb));
        let image = self.create(ctx, width, height, levels.len() as u32, format)?;
        let uploaded = upload_levels(ctx, &image, width, height, &levels);
        self.insert(ctx, image, uploaded)
    }

    /// Uploads a BC7 image with all its mips, `levels[0]` the full size (ADR 0029), and returns its index in the
    /// array.
    ///
    /// # Panics
    ///
    /// If a level is not the size of its 4x4 blocks at 16 bytes each, there are no levels or more than the image
    /// can have, a side is 0 or over 16384, or the array is full.
    pub(crate) fn add_bc7(
        &mut self,
        ctx: &Context,
        width: u32,
        height: u32,
        levels: &[Vec<u8>],
        srgb: bool,
    ) -> Result<u32> {
        let count = levels.len() as u32;
        assert!(
            (1..=32 - width.max(height).leading_zeros()).contains(&count),
            "{count} BC7 levels"
        );
        for (i, level) in levels.iter().enumerate() {
            let (w, h) = ((width >> i).max(1), (height >> i).max(1));
            assert_eq!(
                level.len() as u64,
                u64::from(w.div_ceil(4) * h.div_ceil(4)) * 16,
                "BC7 level {i} size"
            );
        }
        let format = if srgb {
            vk::Format::BC7_SRGB_BLOCK
        } else {
            vk::Format::BC7_UNORM_BLOCK
        };
        let image = self.create(ctx, width, height, count, format)?;
        let uploaded = upload_levels(ctx, &image, width, height, levels);
        self.insert(ctx, image, uploaded)
    }

    /// Creates a sampled 2D image that transfers can write.
    fn create(&self, ctx: &Context, width: u32, height: u32, levels: u32, format: vk::Format) -> Result<Image> {
        assert!(
            (1..=16384).contains(&width) && (1..=16384).contains(&height),
            "texture size {width}x{height}"
        );
        assert!((self.images.len() as u32) < MAX_TEXTURES, "bindless texture array full");
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(levels)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST);
        ctx.create_image("texture", &info, vk::ImageAspectFlags::COLOR)
    }

    /// Puts an uploaded image into the array, or destroys it if `uploaded` failed.
    fn insert(&mut self, ctx: &Context, image: Image, uploaded: Result<()>) -> Result<u32> {
        if let Err(e) = uploaded {
            // SAFETY: the upload waited for the GPU or never submitted.
            unsafe { ctx.destroy_image(image) };
            return Err(e);
        }
        let index = self.images.len() as u32;
        let infos = [vk::DescriptorImageInfo {
            sampler: self.sampler,
            image_view: image.view,
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        }];
        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(0)
            .dst_array_element(index)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&infos);
        // SAFETY: the element is unused by pending work (update-unused-while-pending) and the image is ready.
        unsafe { ctx.device().update_descriptor_sets(&[write], &[]) };
        self.images.push(image);
        Ok(index)
    }

    /// Destroys the images and the array.
    ///
    /// # Safety
    ///
    /// The GPU no longer uses them.
    pub(crate) unsafe fn destroy(&mut self, ctx: &Context) {
        let d = ctx.device();
        // SAFETY: per the contract; null handles are ignored.
        unsafe {
            for image in self.images.drain(..) {
                ctx.destroy_image(image);
            }
            d.destroy_sampler(self.sampler, None);
            d.destroy_descriptor_pool(self.pool, None);
            d.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}

/// Copies every mip level from one staging buffer and leaves them ready for fragment shaders.
fn upload_levels(ctx: &Context, image: &Image, width: u32, height: u32, levels: &[Vec<u8>]) -> Result<()> {
    let total: usize = levels.iter().map(Vec::len).sum();
    let mut staging = ctx.create_buffer(
        "texture staging",
        total as u64,
        vk::BufferUsageFlags::TRANSFER_SRC,
        MemoryLocation::CpuToGpu,
    )?;
    let mapped = staging.mapped_mut().expect("CpuToGpu memory is mapped");
    let mut regions = Vec::with_capacity(levels.len());
    let mut offset = 0;
    for (i, level) in levels.iter().enumerate() {
        mapped[offset..offset + level.len()].copy_from_slice(level);
        regions.push(
            vk::BufferImageCopy::default()
                .buffer_offset(offset as u64)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: i as u32,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_extent(vk::Extent3D {
                    width: (width >> i).max(1),
                    height: (height >> i).max(1),
                    depth: 1,
                }),
        );
        offset += level.len();
    }
    use vk::AccessFlags2 as A;
    use vk::ImageLayout as L;
    use vk::PipelineStageFlags2 as S;
    let result = ctx.one_shot(|d, cmd| {
        let barrier = |(src_stage, src_access, old), (dst_stage, dst_access, new)| {
            vk::ImageMemoryBarrier2::default()
                .src_stage_mask(src_stage)
                .src_access_mask(src_access)
                .dst_stage_mask(dst_stage)
                .dst_access_mask(dst_access)
                .old_layout(old)
                .new_layout(new)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image.handle)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: levels.len() as u32,
                    base_array_layer: 0,
                    layer_count: 1,
                })
        };
        let written = (S::ALL_TRANSFER, A::TRANSFER_WRITE, L::TRANSFER_DST_OPTIMAL);
        let read = (S::FRAGMENT_SHADER, A::SHADER_SAMPLED_READ, L::SHADER_READ_ONLY_OPTIMAL);
        // SAFETY: the image and staging buffer are alive and sized for these commands.
        unsafe {
            let to_write = [barrier((S::NONE, A::NONE, L::UNDEFINED), written)];
            d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&to_write));
            d.cmd_copy_buffer_to_image(cmd, staging.handle, image.handle, L::TRANSFER_DST_OPTIMAL, &regions);
            let to_read = [barrier(written, read)];
            d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().image_memory_barriers(&to_read));
        }
    });
    // SAFETY: `one_shot` waited for the copy, or never submitted it.
    unsafe { ctx.destroy_buffer(staging) };
    result
}
