//! The overlay pass on a real device (ADR 0117): plain and textured triangles in window pixels, blended over the
//! frame, and a mip-less texture slot replaced between frames (ADR 0116). Skips without a Vulkan device.

use civbb_render::overlay::{NO_TEXTURE, Vertex};
use civbb_render::{Capacity, Context, Error, Graph, Load, MemoryLocation, Overlay, Pass, Scene, Transients, Use, vk};

const SIZE: u32 = 64;
const FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;

/// Two triangles over `min` to `max` with texture coordinates 0 to 1.
fn rect(min: [f32; 2], max: [f32; 2], color: [f32; 4], texture: u32) -> Vec<Vertex> {
    let v = |x: usize, y: usize| Vertex {
        pos: [[min[0], max[0]][x], [min[1], max[1]][y]],
        uv: [x as f32, y as f32],
        color,
        texture,
    };
    vec![v(0, 0), v(1, 0), v(1, 1), v(0, 0), v(1, 1), v(0, 1)]
}

/// Clears a target to dark grey, draws `vertices` over it and reads it back.
fn render(ctx: &Context, scene: &Scene, overlay: &Overlay, vertices: &[Vertex]) -> Vec<u8> {
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(FORMAT)
        .extent(vk::Extent3D {
            width: SIZE,
            height: SIZE,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC);
    let target = ctx.create_image("target", &info, vk::ImageAspectFlags::COLOR).unwrap();
    let readback = ctx
        .create_buffer(
            "readback",
            u64::from(SIZE * SIZE * 4),
            vk::BufferUsageFlags::TRANSFER_DST,
            MemoryLocation::GpuToCpu,
        )
        .unwrap();
    let mut pool = Transients::default();
    // SAFETY: `cmd` is recording on `ctx`'s device; the target, scene and overlay belong to `ctx`, and the target
    // starts undefined.
    ctx.one_shot(|d, cmd| unsafe {
        let mut graph = Graph::new();
        let extent = vk::Extent2D {
            width: SIZE,
            height: SIZE,
        };
        let image = graph.import(target.handle, target.view, extent, Use::Undefined, Use::TransferSrc);
        graph.add(
            Pass::new("clear")
                .color(image, Load::Clear([0.2, 0.2, 0.2, 1.0]))
                .record(|_, _| {}),
        );
        overlay.add(&mut graph, scene, image, vertices);
        graph.record(ctx, &mut pool, cmd).unwrap();
        let region = vk::BufferImageCopy::default()
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                layer_count: 1,
                ..Default::default()
            })
            .image_extent(extent.into());
        d.cmd_copy_image_to_buffer(
            cmd,
            target.handle,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            readback.handle,
            &[region],
        );
        let to_host = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_access_mask(vk::AccessFlags2::HOST_READ);
        d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[to_host]));
    })
    .unwrap();
    let pixels = readback.mapped().unwrap()[..(SIZE * SIZE * 4) as usize].to_vec();
    // SAFETY: `one_shot` waited for the GPU to finish.
    unsafe {
        pool.destroy(ctx);
        ctx.destroy_buffer(readback);
        ctx.destroy_image(target);
    }
    pixels
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 3] {
    pixels[((y * SIZE + x) * 4) as usize..][..3].try_into().unwrap()
}

fn near(got: [u8; 3], want: [u8; 3]) -> bool {
    got.iter().zip(&want).all(|(&g, &w)| g.abs_diff(w) <= 2)
}

#[test]
fn draws_plain_and_textured_triangles_and_replaces_a_flat_texture() {
    let ctx = match Context::headless(cfg!(debug_assertions)) {
        Ok(ctx) => ctx,
        Err(e @ (Error::Load(_) | Error::NoDevice)) => return eprintln!("skipped: {e}"),
        Err(e) => panic!("{e}"),
    };
    let one = Capacity {
        vertices: 1,
        indices: 1,
        deltas: 1,
        skinned: 1,
        materials: 1,
    };
    let mut scene = Scene::new(&ctx, one).unwrap();
    // The left two texels green, the right two blue: the sampled points sit between two of the same colour.
    let texels = [[0, 255, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [0, 0, 255, 255]].concat();
    // SAFETY: a new slot.
    let slot = unsafe { scene.put_flat_texture(&ctx, None, 4, 1, &texels) }.unwrap();
    let overlay = Overlay::new(&ctx, &scene, FORMAT, 64).unwrap();
    let mut vertices = rect([0.0, 0.0], [32.0, 16.0], [1.0, 0.0, 0.0, 1.0], NO_TEXTURE);
    vertices.extend(rect([0.0, 32.0], [64.0, 64.0], [1.0, 1.0, 1.0, 0.5], slot));
    let pixels = render(&ctx, &scene, &overlay, &vertices);
    assert!(
        near(pixel(&pixels, 8, 8), [255, 0, 0]),
        "red at the top left: {:?}",
        pixel(&pixels, 8, 8)
    );
    assert!(
        near(pixel(&pixels, 48, 8), [51, 51, 51]),
        "background: {:?}",
        pixel(&pixels, 48, 8)
    );
    let half = |c: u8| ((f32::from(c) + 51.0) / 2.0).round() as u8;
    assert!(
        near(pixel(&pixels, 16, 48), [half(0), half(255), half(0)]),
        "green, half: {:?}",
        pixel(&pixels, 16, 48)
    );
    assert!(
        near(pixel(&pixels, 48, 48), [half(0), half(0), half(255)]),
        "blue, half: {:?}",
        pixel(&pixels, 48, 48)
    );

    let white = [255; 16];
    // SAFETY: `one_shot` in `render` waited for the GPU.
    let again = unsafe { scene.put_flat_texture(&ctx, Some(slot), 4, 1, &white) }.unwrap();
    assert_eq!(again, slot);
    let pixels = render(&ctx, &scene, &overlay, &vertices[6..]);
    assert!(
        near(pixel(&pixels, 16, 48), [half(255); 3]),
        "replaced: {:?}",
        pixel(&pixels, 16, 48)
    );
    assert!(
        near(pixel(&pixels, 8, 8), [51, 51, 51]),
        "only what was asked for is drawn"
    );
    // SAFETY: nothing is in flight.
    unsafe {
        overlay.destroy(&ctx);
        scene.destroy(&ctx);
    }
    assert_eq!(ctx.validation_errors(), 0);
}
