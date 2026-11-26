//! Frustum culling of mesh instances (ADR 0077): the bounds hold every skinned vertex in every parity state, and
//! the frustum keeps what is in view.

use civbb_client::cull::{Frustum, MeshBounds};
use civbb_client::parity::{placed, states, toggles};
use civbb_client::view::skin_matrices;
use civbb_format::Avatar;
use civbb_render::View;

/// Every vertex the GPU would place, in every parity state (toggles move meshes, shapes and bones), lies in its
/// mesh's world box. The blendshape padding is checked on its own below.
#[test]
fn bounds_hold_every_skinned_vertex() {
    for (name, spec) in [("lean", civbb_corpus::lean()), ("bloated", civbb_corpus::bloated())] {
        let avatar: Avatar = civbb_corpus::generate(&spec);
        let bounds: Vec<MeshBounds> = avatar.meshes.iter().map(MeshBounds::new).collect();
        let shaped = avatar.meshes.iter().filter(|m| !m.blend_shapes.is_empty()).count();
        assert!(shaped > 0, "{name}: no blendshapes to check the padding with");
        let mut checked = 0;
        states(&[&avatar], &toggles(&avatar), |_, state, islands| {
            let island = &islands[0];
            let mut weights = Vec::new();
            island.shape_weights(&mut weights);
            let positions = placed(&avatar, island);
            let mut matrices = Vec::new();
            for (m, mesh) in avatar.meshes.iter().enumerate() {
                skin_matrices(mesh, island.world(), &mut matrices);
                let (lo, hi) = bounds[m]
                    .world_box(&matrices, &weights[m])
                    .expect("a mesh with vertices");
                for p in &positions[m] {
                    let slack = 1e-4 * (1.0 + p.iter().fold(0.0f32, |a, x| a.max(x.abs())));
                    assert!(
                        (0..3).all(|k| p[k] >= lo[k] - slack && p[k] <= hi[k] + slack),
                        "{name} {state} mesh {m}: {p:?} outside {lo:?}..{hi:?}"
                    );
                }
                checked += positions[m].len();
            }
        });
        assert!(checked > 0);
    }
}

/// Every shape at weight -3 (weights are unclamped): each displaced vertex lies in the padded box, and some leave
/// the unpadded one, so the padding is needed. Every skinning matrix scales by 2.5 (scaled bones), so the box must
/// grow with the matrix too.
#[test]
fn shape_padding_covers_deltas() {
    let avatar: Avatar = civbb_corpus::generate(&civbb_corpus::bloated());
    const S: f32 = 2.5;
    let identity = [S, 0.0, 0.0, 0.0, 0.0, S, 0.0, 0.0, 0.0, 0.0, S, 0.0, 0.0, 0.0, 0.0, 1.0];
    let mut escaped = 0;
    for mesh in avatar.meshes.iter().filter(|m| !m.blend_shapes.is_empty()) {
        let bounds = MeshBounds::new(mesh);
        let matrices = vec![identity; mesh.skin.as_ref().map_or(1, |s| s.joints.len().max(1))];
        let mut weights = vec![0.0; mesh.blend_shapes.len()];
        let rest = bounds.world_box(&matrices, &weights).unwrap();
        weights.fill(-3.0);
        let (lo, hi) = bounds.world_box(&matrices, &weights).unwrap();
        let mut moved = mesh.positions.clone();
        for d in mesh.blend_shapes.iter().flat_map(|s| &s.deltas) {
            (0..3).for_each(|k| moved[d.vertex as usize][k] -= 3.0 * d.position[k]);
        }
        moved.iter_mut().for_each(|p| *p = p.map(|x| S * x));
        let inside = |p: &[f32; 3], (lo, hi): ([f32; 3], [f32; 3])| (0..3).all(|k| p[k] >= lo[k] && p[k] <= hi[k]);
        for p in &moved {
            assert!(inside(p, (lo, hi)), "{}: {p:?} outside {lo:?}..{hi:?}", mesh.name);
            escaped += usize::from(!inside(p, rest));
        }
    }
    assert!(escaped > 0, "no vertex needed the padding");
}

#[test]
fn frustum_keeps_what_is_in_view() {
    // At the origin looking down -Z, 90 degrees vertically, square.
    let view = View::look_at([0.0; 3], [0.0, 0.0, -1.0], std::f32::consts::FRAC_PI_2, 1.0, 0.05);
    let f = Frustum::new(&view.view_proj);
    let at = |c: [f32; 3], r: f32| (c.map(|x| x - r), c.map(|x| x + r));
    let sees = |c, r| {
        let (lo, hi) = at(c, r);
        f.sees(lo, hi)
    };
    assert!(sees([0.0, 0.0, -5.0], 0.5), "ahead");
    assert!(sees([0.0, 0.0, -1e6], 0.5), "far ahead: the far plane is infinite");
    assert!(!sees([0.0, 0.0, 5.0], 0.5), "behind");
    assert!(!sees([0.0, 0.0, -0.01], 0.005), "in front of the near plane");
    assert!(!sees([7.5, 0.0, -5.0], 1.0), "right of the 45 degree side");
    assert!(sees([5.5, 0.0, -5.0], 1.0), "straddling the right side");
    assert!(!sees([0.0, -7.5, -5.0], 1.0), "below");
    assert!(sees([0.0, 0.0, 0.0], 0.5), "around the eye");
}
