//! Compute skinning and sparse blendshapes (ADR 0022): the active shape list without a device, and on a device the
//! GPU output for every mesh of the bloated corpus avatar against a dense CPU reference. Device tests skip without one.

use civbb_format::math::Mat4;
use civbb_format::mesh::Mesh;
use civbb_render::skin::{Active, Job, SkinnedMesh, Skinning, active_shapes};
use civbb_render::{Capacity, Context, Error, MemoryLocation, Scene, vk};

#[test]
fn only_shapes_with_weight_and_deltas_are_active() {
    let shapes = [(0, 3), (3, 0), (3, 2), (5, 4), (9, 1)];
    let weights = [0.5, 1.0, 0.0, -0.25, 1.0];
    let mut out = vec![Active {
        first: 99,
        count: 99,
        start: 99,
        weight: 9.0,
    }];
    let threads = active_shapes(&shapes, &weights, &mut out);
    assert_eq!(
        out,
        vec![
            Active {
                first: 0,
                count: 3,
                start: 0,
                weight: 0.5
            },
            Active {
                first: 5,
                count: 4,
                start: 3,
                weight: -0.25
            },
            Active {
                first: 9,
                count: 1,
                start: 7,
                weight: 1.0
            },
        ]
    );
    assert_eq!(threads, 8);
    assert_eq!(active_shapes(&shapes, &[0.0; 5], &mut out), 0);
    assert!(out.is_empty());
}

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

/// A column-major rotation about a tilted axis by `angle`, then a translation.
fn matrix(angle: f32, t: [f32; 3]) -> Mat4 {
    let [x, y, z] = {
        let a = [0.3_f32, 1.0, 0.2];
        let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
        [a[0] / l, a[1] / l, a[2] / l]
    };
    let (s, c) = angle.sin_cos();
    let k = 1.0 - c;
    [
        c + x * x * k,
        y * x * k + z * s,
        z * x * k - y * s,
        0.0,
        x * y * k - z * s,
        c + y * y * k,
        z * y * k + x * s,
        0.0,
        x * z * k + y * s,
        y * z * k - x * s,
        c + z * z * k,
        0.0,
        t[0],
        t[1],
        t[2],
        1.0,
    ]
}

fn apply(m: &Mat4, v: [f32; 3], w: f32) -> [f32; 3] {
    std::array::from_fn(|r| m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] + m[12 + r] * w)
}

fn unit(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l > 0.0 { v.map(|x| x / l) } else { v }
}

/// Dense reference: every shape times its weight, then linear blend skinning. Per vertex: position, normal, tangent.
fn reference(mesh: &Mesh, joints: &[Mat4], weights: &[f32]) -> Vec<[[f32; 4]; 3]> {
    let mut p = mesh.positions.clone();
    let mut n = mesh.normals.clone();
    for (s, &w) in mesh.blend_shapes.iter().zip(weights) {
        for d in &s.deltas {
            for k in 0..3 {
                p[d.vertex as usize][k] += w * d.position[k];
                n[d.vertex as usize][k] += w * d.normal[k];
            }
        }
    }
    (0..p.len())
        .map(|v| {
            let (js, ws) = match &mesh.skin {
                Some(s) => (s.vertex_joints[v], s.vertex_weights[v]),
                None => ([0; 4], [1.0, 0.0, 0.0, 0.0]),
            };
            let t = mesh.tangents.get(v).copied().unwrap_or_default();
            let (mut sp, mut sn, mut st) = ([0.0; 3], [0.0; 3], [0.0; 3]);
            for i in 0..4 {
                let m = &joints[js[i] as usize];
                let (a, b, c) = (
                    apply(m, p[v], 1.0),
                    apply(m, n[v], 0.0),
                    apply(m, [t[0], t[1], t[2]], 0.0),
                );
                for k in 0..3 {
                    sp[k] += ws[i] * a[k];
                    sn[k] += ws[i] * b[k];
                    st[k] += ws[i] * c[k];
                }
            }
            let (sn, st) = (unit(sn), unit(st));
            [
                [sp[0], sp[1], sp[2], 1.0],
                [sn[0], sn[1], sn[2], 0.0],
                [st[0], st[1], st[2], t[3]],
            ]
        })
        .collect()
}

/// Skins every job in one recording and reads each output back.
fn run(ctx: &Context, scene: &Scene, skinning: &Skinning, jobs: &[Job]) -> Vec<Vec<[[f32; 4]; 3]>> {
    let readbacks: Vec<_> = jobs
        .iter()
        .map(|j| {
            ctx.create_buffer(
                "readback",
                u64::from(j.mesh.vertex_count()) * 48,
                vk::BufferUsageFlags::TRANSFER_DST,
                MemoryLocation::GpuToCpu,
            )
            .unwrap()
        })
        .collect();
    ctx.one_shot(|d, cmd| unsafe {
        skinning.record(d, cmd, jobs);
        let b = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COMPUTE_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_STORAGE_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::COPY)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ);
        d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[b]));
        for (j, r) in jobs.iter().zip(&readbacks) {
            let region = vk::BufferCopy {
                src_offset: u64::from(j.mesh.base()) * 48,
                dst_offset: 0,
                size: u64::from(j.mesh.vertex_count()) * 48,
            };
            d.cmd_copy_buffer(cmd, scene.skinned().handle, r.handle, &[region]);
        }
        let b = vk::MemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::COPY)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::HOST)
            .dst_access_mask(vk::AccessFlags2::HOST_READ);
        d.cmd_pipeline_barrier2(cmd, &vk::DependencyInfo::default().memory_barriers(&[b]));
    })
    .unwrap();
    readbacks
        .into_iter()
        .zip(jobs)
        .map(|(r, j)| {
            let floats: Vec<f32> = r.mapped().unwrap()[..j.mesh.vertex_count() as usize * 48]
                .as_chunks()
                .0
                .iter()
                .map(|&c| f32::from_le_bytes(c))
                .collect();
            unsafe { ctx.destroy_buffer(r) };
            floats
                .as_chunks::<12>()
                .0
                .iter()
                .map(|v| std::array::from_fn(|i| std::array::from_fn(|k| v[i * 4 + k])))
                .collect()
        })
        .collect()
}

fn assert_close(mesh: &str, got: &[[[f32; 4]; 3]], want: &[[[f32; 4]; 3]]) {
    assert_eq!(got.len(), want.len());
    for (v, (g, w)) in got.iter().zip(want).enumerate() {
        for (attr, tol) in [(0, 1e-4), (1, 1e-3), (2, 1e-3)] {
            for k in 0..4 {
                let (a, b) = (g[attr][k], w[attr][k]);
                assert!(
                    (a - b).abs() <= tol,
                    "{mesh} vertex {v} attribute {attr}: gpu {:?} cpu {:?}",
                    g[attr],
                    w[attr]
                );
            }
        }
    }
}

#[test]
fn matches_the_dense_reference_on_the_bloated_avatar() {
    let Some(ctx) = context() else { return };
    let mut avatar = civbb_corpus::generate(&civbb_corpus::bloated());
    // The corpus moves positions only; give some shapes normal deltas too.
    for s in avatar.meshes[0].blend_shapes.iter_mut().step_by(5) {
        for d in &mut s.deltas {
            d.normal = [0.05, -0.02, 0.01];
        }
    }
    assert!(avatar.meshes[0].blend_shapes.len() >= 1000);
    let skinning = Skinning::new(&ctx).unwrap();
    let mut scene = Scene::new(&ctx, Capacity::for_meshes(&avatar.meshes, 1, 0)).unwrap();
    let meshes: Vec<_> = avatar
        .meshes
        .iter()
        .map(|m| {
            let data = scene.add_mesh(&ctx, m).unwrap();
            SkinnedMesh::new(&ctx, &mut scene, &data).unwrap()
        })
        .collect();
    let joints: Vec<Vec<Mat4>> = meshes
        .iter()
        .map(|m| {
            (0..m.joint_count())
                .map(|j| matrix(0.1 * j as f32, [0.01 * j as f32, 0.5, -0.2]))
                .collect()
        })
        .collect();
    // Some shapes on, overlapping and negative weights among them; zero weights for the rest.
    let frames: [&dyn Fn(usize) -> f32; 2] = [
        &|i| if i % 7 == 0 { (i % 10) as f32 / 10.0 - 0.3 } else { 0.0 },
        &|_| 0.0,
    ];
    for weight in frames {
        let weights: Vec<Vec<f32>> = meshes
            .iter()
            .map(|m| (0..m.shape_count()).map(weight).collect())
            .collect();
        let jobs: Vec<Job> = meshes
            .iter()
            .zip(&joints)
            .zip(&weights)
            .map(|((mesh, joints), weights)| Job { mesh, joints, weights })
            .collect();
        let outputs = run(&ctx, &scene, &skinning, &jobs);
        for ((m, out), (j, w)) in avatar.meshes.iter().zip(&outputs).zip(joints.iter().zip(&weights)) {
            assert_close(&m.name, out, &reference(m, j, w));
        }
    }
    unsafe {
        for m in meshes {
            m.destroy(&ctx);
        }
        skinning.destroy(ctx.device());
        scene.destroy(&ctx);
    }
    assert_eq!(ctx.validation_errors(), 0, "validation reported errors (see stderr)");
}
