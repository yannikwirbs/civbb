//! On a real device, headless: a two-pass graph clears an image and clears half of it again, the copy back reads
//! the expected pixels, and validation (on in debug builds) reports no errors. Skips without a Vulkan device.

use civbb_render::{Context, Error, Graph, Load, Pass, Use, vk};

const SIZE: u32 = 64;
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn memory_type(ctx: &Context, bits: u32, flags: vk::MemoryPropertyFlags) -> u32 {
    // SAFETY: plain query.
    let props = unsafe { ctx.instance().get_physical_device_memory_properties(ctx.physical()) };
    (0..props.memory_type_count)
        .find(|&i| bits & (1 << i) != 0 && props.memory_types[i as usize].property_flags.contains(flags))
        .expect("a matching memory type")
}

#[test]
fn clears_and_reads_back() {
    let ctx = match Context::headless(cfg!(debug_assertions)) {
        Ok(ctx) => ctx,
        Err(e @ (Error::Load(_) | Error::NoDevice)) => {
            eprintln!("skipped: {e}");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    let d = ctx.device();
    let extent = vk::Extent2D {
        width: SIZE,
        height: SIZE,
    };
    let bytes = u64::from(SIZE * SIZE * 4);
    // SAFETY: valid create infos on the context's device; everything is destroyed after the queue is idle.
    unsafe {
        let image = d
            .create_image(
                &vk::ImageCreateInfo::default()
                    .image_type(vk::ImageType::TYPE_2D)
                    .format(vk::Format::R8G8B8A8_UNORM)
                    .extent(extent.into())
                    .mip_levels(1)
                    .array_layers(1)
                    .samples(vk::SampleCountFlags::TYPE_1)
                    .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC),
                None,
            )
            .unwrap();
        let req = d.get_image_memory_requirements(image);
        let image_mem = d
            .allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(memory_type(
                        &ctx,
                        req.memory_type_bits,
                        vk::MemoryPropertyFlags::DEVICE_LOCAL,
                    )),
                None,
            )
            .unwrap();
        d.bind_image_memory(image, image_mem, 0).unwrap();
        let view = d
            .create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(vk::Format::R8G8B8A8_UNORM)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        level_count: 1,
                        layer_count: 1,
                        ..Default::default()
                    }),
                None,
            )
            .unwrap();
        let buffer = d
            .create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(bytes)
                    .usage(vk::BufferUsageFlags::TRANSFER_DST),
                None,
            )
            .unwrap();
        let req = d.get_buffer_memory_requirements(buffer);
        let host = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let buffer_mem = d
            .allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(memory_type(&ctx, req.memory_type_bits, host)),
                None,
            )
            .unwrap();
        d.bind_buffer_memory(buffer, buffer_mem, 0).unwrap();

        let pool = d
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default().queue_family_index(ctx.queue_family()),
                None,
            )
            .unwrap();
        let cmd = d
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .command_buffer_count(1),
            )
            .unwrap()[0];
        d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
            .unwrap();

        let mut graph = Graph::new();
        let target = graph.import(image, view, extent, Use::Undefined, Use::TransferSrc);
        graph.add(Pass::new("clear").color(target, Load::Clear(RED)));
        graph.add(Pass::new("left half").color(target, Load::Load).record(|d, cmd| {
            let clear = vk::ClearAttachment {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                color_attachment: 0,
                clear_value: vk::ClearValue {
                    color: vk::ClearColorValue { float32: BLUE },
                },
            };
            let rect = vk::ClearRect {
                rect: vk::Rect2D {
                    offset: vk::Offset2D::default(),
                    extent: vk::Extent2D {
                        width: SIZE / 2,
                        height: SIZE,
                    },
                },
                base_array_layer: 0,
                layer_count: 1,
            };
            d.cmd_clear_attachments(cmd, &[clear], &[rect]);
        }));
        graph.record(d, cmd);

        let region = vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                layer_count: 1,
                ..Default::default()
            })
            .image_extent(extent.into());
        d.cmd_copy_image_to_buffer(cmd, image, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, buffer, &[region]);
        let to_host = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_access_mask(vk::AccessFlags2::HOST_READ);
        d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[to_host]));
        d.end_command_buffer(cmd).unwrap();

        let fence = d.create_fence(&vk::FenceCreateInfo::default(), None).unwrap();
        let cmds = [vk::CommandBufferSubmitInfo::default().command_buffer(cmd)];
        d.queue_submit2(
            ctx.queue(),
            &[vk::SubmitInfo2::default().command_buffer_infos(&cmds)],
            fence,
        )
        .unwrap();
        d.wait_for_fences(&[fence], true, u64::MAX).unwrap();

        let ptr = d.map_memory(buffer_mem, 0, bytes, vk::MemoryMapFlags::empty()).unwrap() as *const u8;
        let pixels = std::slice::from_raw_parts(ptr, bytes as usize);
        let at = |x: u32, y: u32| &pixels[((y * SIZE + x) * 4) as usize..][..4];
        assert_eq!(at(0, 0), [0, 0, 255, 255]);
        assert_eq!(at(SIZE / 2 - 1, SIZE - 1), [0, 0, 255, 255]);
        assert_eq!(at(SIZE / 2, 0), [255, 0, 0, 255]);
        assert_eq!(at(SIZE - 1, SIZE - 1), [255, 0, 0, 255]);
        d.unmap_memory(buffer_mem);

        d.destroy_fence(fence, None);
        d.destroy_command_pool(pool, None);
        d.destroy_buffer(buffer, None);
        d.free_memory(buffer_mem, None);
        d.destroy_image_view(view, None);
        d.destroy_image(image, None);
        d.free_memory(image_mem, None);
    }
    assert_eq!(ctx.validation_errors(), 0, "validation reported errors (see stderr)");
}
