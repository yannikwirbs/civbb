//! The GPU-driven forward pass on a real device (ADR 0023): quads through skinning, shared buffers, bindless
//! textures, indirect multi-draws and a material rewritten in the frame (ADR 0025), checked pixel by pixel; toon
//! outlines around a cube (ADR 0034); and the bloated corpus avatar drawn without validation errors. Skips without a
//! Vulkan device.

use civbb_format::Avatar;
use civbb_format::material::{AlphaMode, Material, MaterialModel, Pbr, TextureSlot, Toon, Unlit};
use civbb_format::math::Mat4;
use civbb_format::mesh::{Mesh, Submesh};
use civbb_render::material::{MATERIAL_WORDS, pack};
use civbb_render::skin::{Job, SkinnedMesh, Skinning};
use civbb_render::{
    Access, Batches, Capacity, Context, Error, Forward, Graph, MaterialId, MemoryLocation, Pass, Scene, Transients,
    Use, View, batch, vk,
};

const SIZE: u32 = 64;
const FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const IDENTITY: Mat4 = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

fn context() -> Option<Context> {
    match Context::headless(cfg!(debug_assertions)) {
        Ok(ctx) => Some(ctx),
        Err(e @ (Error::Load(_) | Error::NoDevice)) => {
            eprintln!("skipped: {e}");
            None
        }
        Err(e) => panic!("{e}"),
    }
}

/// A quad facing +Z over `x` by `y` at depth `z`, counter-clockwise seen from the front, drawn with `material`.
fn quad(x: [f32; 2], y: [f32; 2], z: f32, material: u32) -> Mesh {
    Mesh {
        name: "quad".into(),
        positions: vec![[x[0], y[0], z], [x[1], y[0], z], [x[1], y[1], z], [x[0], y[1], z]],
        normals: vec![[0.0, 0.0, 1.0]; 4],
        tangents: vec![[1.0, 0.0, 0.0, 1.0]; 4],
        uv0: vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
        indices: vec![0, 1, 2, 0, 2, 3],
        submeshes: vec![Submesh {
            first_index: 0,
            index_count: 6,
            material,
        }],
        ..Mesh::default()
    }
}

fn material(model: MaterialModel, alpha: AlphaMode) -> Material {
    Material {
        name: "m".into(),
        model,
        alpha,
        double_sided: false,
        render_queue: 2000,
    }
}

fn unlit(color: [f32; 4], color_tex: Option<TextureSlot>) -> MaterialModel {
    MaterialModel::Unlit(Unlit { color, color_tex })
}

fn toon(base_color: [f32; 4], shade_color: [f32; 3], shade_shift: f32) -> MaterialModel {
    MaterialModel::Toon(Toon {
        base_color,
        base_color_tex: None,
        normal_tex: None,
        normal_scale: 1.0,
        shade_color,
        shade_tex: None,
        shade_shift,
        shade_toony: 0.9,
        rim_color: [0.0; 3],
        rim_power: 0.0,
        matcap_tex: None,
        matcap_strength: 0.0,
        emissive: [0.0; 3],
        emissive_tex: None,
        outline_width: 0.0,
        outline_color: [0.0; 3],
        outline_width_tex: None,
    })
}

/// Skins every mesh with identity joints and draws them into a `SIZE` square image after rewriting `updates`;
/// returns its RGBA8 pixels.
fn render(
    ctx: &Context,
    scene: &mut Scene,
    meshes: &[Mesh],
    materials: &[MaterialId],
    updates: &[(u32, [u32; MATERIAL_WORDS])],
    view: &View,
) -> Vec<u8> {
    let data: Vec<_> = meshes.iter().map(|m| scene.add_mesh(ctx, m).unwrap()).collect();
    let instances: Vec<_> = data.iter().map(|d| SkinnedMesh::new(ctx, scene, d).unwrap()).collect();
    let joints: Vec<Vec<Mat4>> = data.iter().map(|d| vec![IDENTITY; d.joint_count() as usize]).collect();
    let weights: Vec<Vec<f32>> = data.iter().map(|d| vec![0.0; d.shape_count()]).collect();
    let jobs: Vec<Job> = instances
        .iter()
        .zip(&joints)
        .zip(&weights)
        .map(|((mesh, joints), weights)| Job { mesh, joints, weights })
        .collect();
    let mut draws = Vec::new();
    for (d, i) in data.iter().zip(&instances) {
        d.draws(i, materials, 1.0, &mut draws);
    }
    let mut batches = Batches::default();
    batch(&mut draws, &mut batches);

    let skinning = Skinning::new(ctx).unwrap();
    let forward = Forward::new(ctx, scene, FORMAT, 1024).unwrap();
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
    let scene = &*scene;
    ctx.one_shot(|d, cmd| unsafe {
        let mut graph = Graph::new();
        let extent = vk::Extent2D {
            width: SIZE,
            height: SIZE,
        };
        let image = graph.import(target.handle, target.view, extent, Use::Undefined, Use::TransferSrc);
        let skinned = graph.import_buffer(scene.skinned().handle, &[Access::VertexRead]);
        graph.add(
            Pass::new("skin")
                .buffer(skinned, Access::ComputeWrite)
                .record(|d, cmd| skinning.record(d, cmd, &jobs)),
        );
        forward.add(
            &mut graph,
            scene,
            image,
            skinned,
            view,
            &batches,
            updates,
            [0.0, 0.0, 0.0, 1.0],
        );
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
    assert_eq!(pool.len(), 1, "one transient depth buffer");
    unsafe {
        pool.destroy(ctx);
        ctx.destroy_buffer(readback);
        ctx.destroy_image(target);
        forward.destroy(ctx);
        skinning.destroy(ctx.device());
        for i in instances {
            i.destroy(ctx);
        }
    }
    pixels
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    pixels[((y * SIZE + x) * 4) as usize..][..4].try_into().unwrap()
}

fn assert_near(got: [u8; 4], want: [u8; 3], tolerance: u8, what: &str) {
    assert!(
        got.iter().zip(&want).all(|(&g, &w)| g.abs_diff(w) <= tolerance),
        "{what}: got {got:?}, want about {want:?}"
    );
}

#[test]
fn draws_quads_with_each_shader_depth_textures_and_blending() {
    let Some(ctx) = context() else { return };
    let mut scene = Scene::new(
        &ctx,
        Capacity {
            vertices: 20,
            indices: 30,
            deltas: 0,
            skinned: 20,
            materials: 5,
        },
    )
    .unwrap();
    let white = scene.add_texture(&ctx, 1, 1, &[255; 4], true).unwrap();
    let green = scene
        .add_texture(&ctx, 2, 2, &[0, 255, 0, 255].repeat(4), true)
        .unwrap();
    assert_eq!((white, green), (0, 1));
    let tex = Some(TextureSlot {
        texture: 0,
        uv_set: 0,
        scale_offset: [1.0, 1.0, 0.0, 0.0],
    });
    let mut add = |model, alpha| scene.add_material(&ctx, &material(model, alpha), &[green]).unwrap();
    let opaque = AlphaMode::Opaque;
    let green_unlit = add(unlit([1.0; 4], tex), opaque);
    let red = add(unlit([1.0, 0.0, 0.0, 1.0], None), opaque);
    // Shifted far into shadow: the shade colour shows although the light faces the quad.
    let toon_shaded = add(toon([1.0, 1.0, 0.0, 1.0], [0.0, 0.0, 1.0], -1.5), opaque);
    let pbr = add(
        MaterialModel::Pbr(Pbr {
            base_color: [1.0; 4],
            base_color_tex: None,
            metallic: 0.0,
            roughness: 1.0,
            metallic_roughness_tex: None,
            normal_tex: None,
            normal_scale: 1.0,
            occlusion_tex: None,
            emissive: [0.0; 3],
            emissive_tex: None,
        }),
        opaque,
    );
    let half_white = add(unlit([1.0, 1.0, 1.0, 0.5], None), AlphaMode::Blend);
    let materials = [green_unlit, red, toon_shaded, pbr, half_white];
    let all = [-3.0, 3.0];
    let meshes = [
        // Nearer than the red quad behind it and drawn first: depth decides, not order.
        quad([-3.0, 0.0], all, 0.5, 0),
        quad(all, all, 0.0, 1),
        quad([0.875, 3.0], [0.0, 3.0], 0.25, 2),
        quad([0.875, 3.0], [-3.0, 0.0], 0.25, 3),
        quad([-3.0, 0.0], [0.0, 3.0], 0.75, 4),
    ];
    let mut view = View::look_at([0.0, 0.0, 2.0], [0.0; 3], std::f32::consts::FRAC_PI_2, 1.0, 0.1);
    view.light_dir = [0.0, 0.0, 1.0];
    view.ambient = [0.0; 3];
    // The red material is rewritten to blue in the frame that draws it.
    let blue = pack(&material(unlit([0.0, 0.0, 1.0, 1.0], None), opaque), &[]);
    let pixels = render(&ctx, &mut scene, &meshes, &materials, &[(red.index, blue)], &view);
    // Screen left is world -X and screen top world +Y.
    assert_near(pixel(&pixels, 16, 48), [0, 255, 0], 0, "textured unlit, bottom left");
    assert_near(
        pixel(&pixels, 40, 32),
        [0, 0, 255],
        0,
        "rewritten unlit behind, middle right",
    );
    assert_near(pixel(&pixels, 56, 16), [0, 0, 255], 1, "toon in shade, top right");
    assert_near(pixel(&pixels, 56, 48), [242, 242, 242], 12, "lit PBR, bottom right");
    assert_near(
        pixel(&pixels, 16, 16),
        [128, 255, 128],
        2,
        "blended over textured, top left",
    );
    unsafe { scene.destroy(&ctx) };
    assert_eq!(ctx.validation_errors(), 0, "validation reported errors (see stderr)");
}

#[test]
fn draws_the_bloated_avatar() {
    let Some(ctx) = context() else { return };
    let avatar: Avatar = civbb_corpus::generate(&civbb_corpus::bloated());
    let mut scene = Scene::new(
        &ctx,
        Capacity::for_meshes(&avatar.meshes, 1, avatar.materials.len() as u32),
    )
    .unwrap();
    let materials: Vec<MaterialId> = avatar
        .materials
        .iter()
        .map(|m| scene.add_material(&ctx, m, &[]).unwrap())
        .collect();
    let view = View::look_at([0.0, 1.0, 3.0], [0.0, 1.0, 0.0], 0.8, 1.0, 0.1);
    let pixels = render(&ctx, &mut scene, &avatar.meshes, &materials, &[], &view);
    let covered = pixels.chunks(4).filter(|p| p[..3] != [0, 0, 0]).count();
    assert!(covered > 100, "only {covered} pixels drawn");
    unsafe { scene.destroy(&ctx) };
    assert_eq!(ctx.validation_errors(), 0, "validation reported errors (see stderr)");
}

/// A cube of half-size `h` at the origin with normals pointing out of its corners (smooth, so a pushed-out hull
/// stays closed), drawn with material 0.
fn cube(h: f32) -> Mesh {
    let corners: Vec<[f32; 3]> = (0..8)
        .map(|i| [i & 1, (i >> 1) & 1, (i >> 2) & 1].map(|b| if b == 1 { h } else { -h }))
        .collect();
    let normals = corners.iter().map(|c| c.map(|v| v.signum() / 3f32.sqrt())).collect();
    // Two counter-clockwise triangles per face, seen from outside.
    let faces = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    let indices: Vec<u32> = faces
        .iter()
        .flat_map(|f| [f[0], f[1], f[2], f[0], f[2], f[3]])
        .collect();
    Mesh {
        name: "cube".into(),
        tangents: vec![[1.0, 0.0, 0.0, 1.0]; 8],
        uv0: vec![[0.5, 0.5]; 8],
        submeshes: vec![Submesh {
            first_index: 0,
            index_count: indices.len() as u32,
            material: 0,
        }],
        positions: corners,
        normals,
        indices,
        ..Mesh::default()
    }
}

#[test]
fn draws_toon_outlines_around_the_silhouette() {
    let Some(ctx) = context() else { return };
    let mut scene = Scene::new(
        &ctx,
        Capacity {
            vertices: 8,
            indices: 36,
            deltas: 0,
            skinned: 8,
            materials: 1,
        },
    )
    .unwrap();
    // A white width texture: sampled in the vertex shader, it keeps the full width.
    let white = scene.add_texture(&ctx, 1, 1, &[255; 4], false).unwrap();
    let mut model = toon([1.0, 1.0, 0.0, 1.0], [1.0, 1.0, 0.0], 0.0);
    if let MaterialModel::Toon(t) = &mut model {
        t.outline_width = 0.3;
        t.outline_color = [1.0, 0.0, 0.0];
        t.outline_width_tex = Some(TextureSlot {
            texture: 0,
            uv_set: 0,
            scale_offset: [1.0, 1.0, 0.0, 0.0],
        });
    }
    let outlined = scene
        .add_material(&ctx, &material(model, AlphaMode::Opaque), &[white])
        .unwrap();
    assert!(outlined.outline.is_some());
    let mut view = View::look_at([0.0, 0.0, 2.0], [0.0; 3], std::f32::consts::FRAC_PI_2, 1.0, 0.1);
    view.light_dir = [0.0, 0.0, 1.0];
    view.ambient = [0.0; 3];
    let pixels = render(&ctx, &mut scene, &[cube(0.5)], &[outlined], &[], &view);
    // The front face spans 32 +- 10.7 pixels; the hull's sides, pushed out by 0.3 / sqrt(3) per axis, reach 48.
    assert_near(pixel(&pixels, 32, 32), [255, 255, 0], 1, "the cube's front face");
    assert_near(pixel(&pixels, 46, 32), [255, 0, 0], 0, "outline right of the face");
    assert_near(pixel(&pixels, 32, 18), [255, 0, 0], 0, "outline above the face");
    assert_near(pixel(&pixels, 56, 32), [0, 0, 0], 0, "background past the outline");
    unsafe { scene.destroy(&ctx) };
    assert_eq!(ctx.validation_errors(), 0, "validation reported errors (see stderr)");
}
