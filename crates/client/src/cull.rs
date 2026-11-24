//! Frustum culling of mesh instances before skinning (design doc §6, ADR 0077): conservative world bounds from
//! per-joint spheres and the frame's skinning matrices.

use civbb_core::math::{length, sub};
use civbb_format::math::{Mat4, Vec3};
use civbb_format::mesh::Mesh;

/// A mesh's bind-space spheres, one per joint that carries weight (or one over every vertex when unskinned),
/// and how far its blendshapes can move a vertex at weight 1.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshBounds {
    /// Per skinning matrix: centre and radius, or `None` when no vertex depends on it.
    spheres: Vec<Option<(Vec3, f32)>>,
    /// The largest sum over shapes of a vertex's delta lengths.
    shape_reach: f32,
}

impl MeshBounds {
    /// Bounds for `mesh`; the spheres follow the skinning matrices the client builds (one per skin joint, or the
    /// node's for an unskinned mesh).
    pub fn new(mesh: &Mesh) -> Self {
        let n = mesh.positions.len();
        let joints = match &mesh.skin {
            Some(s) if !s.joints.is_empty() => s.joints.len(),
            _ => 1,
        };
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); joints];
        match &mesh.skin {
            Some(s) if !s.joints.is_empty() => {
                for (v, (js, ws)) in s.vertex_joints.iter().zip(&s.vertex_weights).enumerate() {
                    for (&j, &w) in js.iter().zip(ws) {
                        if w != 0.0
                            && let Some(m) = members.get_mut(j as usize)
                            && m.last() != Some(&v)
                        {
                            m.push(v);
                        }
                    }
                }
            }
            _ => members[0].extend(0..n),
        }
        let spheres = members
            .iter()
            .map(|vs| {
                let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for &v in vs {
                    let p = mesh.positions[v];
                    for k in 0..3 {
                        lo[k] = lo[k].min(p[k]);
                        hi[k] = hi[k].max(p[k]);
                    }
                }
                if vs.is_empty() {
                    return None;
                }
                let c = std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0);
                let r = vs
                    .iter()
                    .map(|&v| length(sub(mesh.positions[v], c)))
                    .fold(0.0, f32::max);
                Some((c, r))
            })
            .collect();
        let mut reach = vec![0.0f32; n];
        for s in &mesh.blend_shapes {
            for d in &s.deltas {
                if let Some(r) = reach.get_mut(d.vertex as usize) {
                    *r += length(d.position);
                }
            }
        }
        MeshBounds {
            spheres,
            shape_reach: reach.into_iter().fold(0.0, f32::max),
        }
    }

    /// The world-space box holding every vertex skinned by `matrices` with blendshape weights `weights`, or
    /// `None` when no sphere applies. Holds for skin weights that sum to 1 (the importer normalises them).
    pub fn world_box(&self, matrices: &[Mat4], weights: &[f32]) -> Option<(Vec3, Vec3)> {
        let pad = self.shape_reach * weights.iter().fold(0.0f32, |m, w| m.max(w.abs()));
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for (sphere, m) in self.spheres.iter().zip(matrices) {
            let Some((c, r)) = sphere else { continue };
            let centre: Vec3 = std::array::from_fn(|k| m[k] * c[0] + m[4 + k] * c[1] + m[8 + k] * c[2] + m[12 + k]);
            // The largest column length bounds how far the matrix stretches any direction.
            let stretch = (0..3)
                .map(|col| length([m[col * 4], m[col * 4 + 1], m[col * 4 + 2]]))
                .fold(0.0, f32::max);
            let radius = (r + pad) * stretch;
            for k in 0..3 {
                lo[k] = lo[k].min(centre[k] - radius);
                hi[k] = hi[k].max(centre[k] + radius);
            }
        }
        (lo[0] <= hi[0]).then_some((lo, hi))
    }
}

/// The planes of a view frustum with an infinite far plane: left, right, bottom, top and near, each `(n, d)` with
/// `dot(n, p) + d >= 0` inside.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum([[f32; 4]; 5]);

impl Frustum {
    /// From a column-major world-to-clip matrix with reverse Z (clip depth `z / w` is 1 at the near plane).
    pub fn new(view_proj: &Mat4) -> Self {
        let row = |r: usize| [view_proj[r], view_proj[4 + r], view_proj[8 + r], view_proj[12 + r]];
        let (x, y, z, w) = (row(0), row(1), row(2), row(3));
        let add = |a: [f32; 4], b: [f32; 4]| std::array::from_fn(|i| a[i] + b[i]);
        let sub = |a: [f32; 4], b: [f32; 4]| std::array::from_fn(|i| a[i] - b[i]);
        Frustum([add(w, x), sub(w, x), add(w, y), sub(w, y), sub(w, z)])
    }

    /// Whether any part of the box `lo..hi` may be inside: false only when it lies wholly outside one plane.
    pub fn sees(&self, lo: Vec3, hi: Vec3) -> bool {
        self.0.iter().all(|p| {
            // The box corner furthest along the plane's normal.
            let far: f32 = (0..3).map(|k| p[k] * if p[k] >= 0.0 { hi[k] } else { lo[k] }).sum();
            far + p[3] >= 0.0
        })
    }
}
