//! Viewer helpers without a window (ADR 0024): placement matrices agree with `Pose::point`, the grid is centred,
//! the orbit camera keeps its distance, islands report blendshape weights, and KTX2 textures transcode to BC7 and
//! reject damaged input without panicking (ADR 0029).

use std::sync::Arc;

use civbb_client::view::{Orbit, Texels, grid, matrix, transcode_ktx2};
use civbb_client::{Island, Model};
use civbb_core::Counters;
use civbb_core::math::{Pose, qnormalize};

#[test]
fn placement_matrix_matches_pose_point() {
    let pose = Pose {
        pos: [1.0, -2.0, 0.5],
        rot: qnormalize([0.2, -0.4, 0.1, 0.9]),
        scale: 1.5,
    };
    let m = matrix(&pose);
    for p in [[0.0, 0.0, 0.0], [1.0, 2.0, 3.0], [-0.5, 0.25, 4.0]] {
        let want = pose.point(p);
        let got: [f32; 3] = std::array::from_fn(|r| m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r]);
        for k in 0..3 {
            assert!((got[k] - want[k]).abs() < 1e-5, "{p:?}: {got:?} vs {want:?}");
        }
    }
    assert_eq!(&m[12..], [1.0, -2.0, 0.5, 1.0]);
}

#[test]
fn grid_is_square_and_centred_on_x() {
    let g = grid(5, 2.0);
    assert_eq!(
        g,
        [
            [-2.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [-2.0, 0.0, -2.0],
            [0.0, 0.0, -2.0]
        ]
    );
    assert_eq!(grid(1, 2.0), [[0.0; 3]]);
}

#[test]
fn orbit_keeps_distance_and_clamps_pitch() {
    let mut o = Orbit {
        target: [0.0, 1.0, 0.0],
        yaw: 0.0,
        pitch: 0.0,
        distance: 3.0,
    };
    assert_eq!(o.eye(), [0.0, 1.0, 3.0]);
    o.drag(300.0, 10_000.0);
    assert_eq!(o.pitch, 1.5);
    let e = o.eye();
    let d = ((e[0]).powi(2) + (e[1] - 1.0).powi(2) + e[2].powi(2)).sqrt();
    assert!((d - 3.0).abs() < 1e-5);
    o.zoom(1.0);
    assert!((o.distance - 2.7).abs() < 1e-5);
}

#[test]
fn shape_weights_are_defaults_without_an_animator_binding() {
    let mut avatar = civbb_corpus::generate(&civbb_corpus::lean());
    avatar.meshes[0].blend_shapes[0].default_weight = 0.25;
    let model = Arc::new(Model::new(&avatar));
    let mut island = Island::new(model, 0, true, Pose::IDENTITY);
    island.step(0.0, &mut Counters::default());
    let mut weights = Vec::new();
    island.shape_weights(&mut weights);
    assert_eq!(weights.len(), avatar.meshes.len());
    for (w, m) in weights.iter().zip(&avatar.meshes) {
        assert_eq!(w.len(), m.blend_shapes.len());
    }
    assert_eq!(weights[0][0], 0.25);
}

/// 16x8 KTX2 textures written by `civbb_pipeline::texture::encode_ktx2` (the fuzz seeds).
const KTX2: [&[u8]; 2] = [include_bytes!("data/etc1s.ktx2"), include_bytes!("data/uastc.ktx2")];

#[test]
fn ktx2_transcodes_every_level_to_bc7() {
    for bytes in KTX2 {
        let image = transcode_ktx2(bytes).unwrap();
        assert_eq!((image.width, image.height), (16, 8));
        let Texels::Bc7(levels) = image.texels else {
            panic!("BC7")
        };
        let sizes: Vec<usize> = levels.iter().map(Vec::len).collect();
        assert_eq!(sizes, [8 * 16, 2 * 16, 16, 16, 16], "4x2, 2x1 then single blocks");
    }
}

#[test]
fn damaged_ktx2_is_an_error_not_a_panic() {
    let mut s = 1u32;
    for bytes in KTX2 {
        for len in 0..bytes.len() {
            assert!(transcode_ktx2(&bytes[..len]).is_err() || len > 80, "truncated at {len}");
        }
        for _ in 0..20_000 {
            let mut b = bytes.to_vec();
            for _ in 0..3 {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let i = (s >> 8) as usize % b.len();
                b[i] ^= 1 << (s & 7);
            }
            let _ = transcode_ktx2(&b);
        }
    }
}
