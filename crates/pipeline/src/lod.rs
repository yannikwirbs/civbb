//! Compiler stage 3 LOD chain: up to three simplified levels per mesh from meshoptimizer, weighing where skinning and
//! shapes move each vertex, with submesh borders locked, in a vertex order that makes each level a prefix (ADR 0031).

use civbb_format::Avatar;
use civbb_format::math::Mat4;
use civbb_format::mesh::{Lod, Mesh, Submesh};
use meshopt::{SimplifyOptions, VertexDataAdapter, simplify_scale, simplify_with_attributes_and_locks, typed_to_bytes};

/// Per lower level: the share of the full mesh's triangles aimed for, and the largest error allowed relative to the
/// mesh's extent (meshoptimizer's scale). Simplification stops at whichever it reaches first.
pub const LEVELS: [(f32, f32); 3] = [(0.5, 0.005), (0.25, 0.015), (0.125, 0.04)];

/// A level is kept only with at most this share of the level above's triangles; the chain ends at the first that
/// is not.
pub const KEEP: f32 = 0.8;

/// Attribute weights: normal (3), UV0 (2), skin anchor (3), shape motion (1). Anchor and motion are in units of the
/// mesh's extent, like positions.
const WEIGHTS: [f32; 9] = [0.5, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];

/// What the chain holds, summed over meshes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lods {
    /// Meshes with at least one lower level.
    pub meshes: usize,
    /// Triangles drawn at each level: full, then the lower ones; a mesh without that level counts its lowest.
    pub triangles: [usize; 4],
    /// Largest error at each lower level relative to its mesh's extent.
    pub error: [f32; 3],
}

/// Builds the chain for every mesh of a validated, merged avatar in place; level 0 looks as before.
pub fn lod(a: &mut Avatar) -> Lods {
    let mut out = Lods::default();
    for m in &mut a.meshes {
        let scale = build(m);
        let full = m.indices.len() / 3;
        out.triangles[0] += full;
        if !m.lods.is_empty() {
            out.meshes += 1;
        }
        let mut lowest = full;
        for k in 0..LEVELS.len() {
            if let Some(l) = m.lods.get(k) {
                lowest = l.indices.len() / 3;
                out.error[k] = out.error[k].max(l.error / scale);
            }
            out.triangles[k + 1] += lowest;
        }
    }
    out
}

/// Simplifies one mesh and reorders its vertices; returns meshoptimizer's scale of it.
fn build(m: &mut Mesh) -> f32 {
    m.lods.clear();
    let n = m.positions.len();
    let bytes = typed_to_bytes(&m.positions);
    let Ok(adapter) = VertexDataAdapter::new(bytes, 12, 0) else {
        return 1.0;
    };
    let scale = simplify_scale(&adapter);
    if m.indices.is_empty() || scale.is_nan() || scale <= 0.0 {
        return 1.0;
    }
    let attributes = attributes(m, scale);
    let locks = vec![false; n];
    let options = SimplifyOptions::LockBorder | SimplifyOptions::ErrorAbsolute | SimplifyOptions::Sparse;
    let full: Vec<&[u32]> = m
        .submeshes
        .iter()
        .map(|s| &m.indices[s.first_index as usize..(s.first_index + s.index_count) as usize])
        .collect();
    let mut current: Vec<Vec<u32>> = full.iter().map(|s| s.to_vec()).collect();
    let mut error = vec![0.0f32; full.len()];
    let mut above = m.indices.len();
    for (share, cap) in LEVELS {
        let mut next = Vec::with_capacity(current.len());
        for (s, idx) in current.iter().enumerate() {
            let target = (full[s].len() as f32 * share) as usize / 3 * 3;
            let mut e = 0.0;
            next.push(simplify_with_attributes_and_locks(
                idx,
                &adapter,
                &attributes,
                &WEIGHTS,
                WEIGHTS.len() * 4,
                &locks,
                target,
                (cap * scale - error[s]).max(0.0),
                options,
                Some(&mut e),
            ));
            error[s] += e;
        }
        let count: usize = next.iter().map(Vec::len).sum();
        if count as f32 > KEEP * above as f32 {
            break;
        }
        let mut lod = Lod {
            indices: Vec::with_capacity(count),
            error: error.iter().copied().fold(0.0, f32::max),
            ..Lod::default()
        };
        for (s, idx) in m.submeshes.iter().zip(&next) {
            lod.submeshes.push(Submesh {
                first_index: lod.indices.len() as u32,
                index_count: idx.len() as u32,
                material: s.material,
            });
            lod.indices.extend_from_slice(idx);
        }
        m.lods.push(lod);
        current = next;
        above = count;
    }
    if !m.lods.is_empty() {
        reorder(m);
    }
    scale
}

/// Per vertex: normal, UV0, the skin anchor (the weighted bind positions of its joints) and how far all shapes
/// together move it, the last two divided by `scale`. Non-finite values become 0.
fn attributes(m: &Mesh, scale: f32) -> Vec<f32> {
    let n = m.positions.len();
    let mut out = vec![0.0f32; n * WEIGHTS.len()];
    if let Some(s) = &m.skin {
        let joints: Vec<[f32; 3]> = s.inverse_bind.iter().map(bind_position).collect();
        for (v, (js, ws)) in s.vertex_joints.iter().zip(&s.vertex_weights).enumerate() {
            for (&j, &w) in js.iter().zip(ws) {
                if let Some(p) = joints.get(j as usize) {
                    (0..3).for_each(|k| out[v * 9 + 5 + k] += w * p[k] / scale);
                }
            }
        }
    }
    for b in &m.blend_shapes {
        for d in &b.deltas {
            let [x, y, z] = d.position;
            out[d.vertex as usize * 9 + 8] += (x * x + y * y + z * z).sqrt() / scale;
        }
    }
    for v in 0..n {
        out[v * 9..v * 9 + 3].copy_from_slice(&m.normals[v]);
        if let Some(uv) = m.uv0.get(v) {
            out[v * 9 + 3..v * 9 + 5].copy_from_slice(uv);
        }
    }
    out.iter_mut().filter(|x| !x.is_finite()).for_each(|x| *x = 0.0);
    out
}

/// Where a joint sits in the mesh's space at bind time: the point its column-major inverse bind matrix maps to the
/// origin, `-R⁻¹ t`. Non-finite for a singular matrix.
fn bind_position(m: &Mat4) -> [f32; 3] {
    let r = |row: usize, col: usize| m[col * 4 + row];
    let t = [m[12], m[13], m[14]];
    let det = r(0, 0) * (r(1, 1) * r(2, 2) - r(1, 2) * r(2, 1)) - r(0, 1) * (r(1, 0) * r(2, 2) - r(1, 2) * r(2, 0))
        + r(0, 2) * (r(1, 0) * r(2, 1) - r(1, 1) * r(2, 0));
    // Cramer's rule for R p = -t.
    std::array::from_fn(|c| {
        let col = |row: usize, k: usize| if k == c { -t[row] } else { r(row, k) };
        let d = col(0, 0) * (col(1, 1) * col(2, 2) - col(1, 2) * col(2, 1))
            - col(0, 1) * (col(1, 0) * col(2, 2) - col(1, 2) * col(2, 0))
            + col(0, 2) * (col(1, 0) * col(2, 1) - col(1, 1) * col(2, 0));
        d / det
    })
}

/// Orders vertices by the lowest level that uses them, lowest first and otherwise as before, so each level uses a
/// prefix; every stream, skin, shape and index list follows.
fn reorder(m: &mut Mesh) {
    let n = m.positions.len();
    // Levels only drop vertices, so the lowest level using a vertex is the deepest one it appears in.
    let mut depth = vec![0usize; n];
    for (k, l) in m.lods.iter().enumerate() {
        l.indices.iter().for_each(|&i| depth[i as usize] = k + 1);
    }
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_by_key(|&v| std::cmp::Reverse(depth[v as usize]));
    let mut to = vec![0u32; n];
    for (new, &old) in order.iter().enumerate() {
        to[old as usize] = new as u32;
    }
    fn permute<T: Copy>(v: &mut Vec<T>, order: &[u32]) {
        if !v.is_empty() {
            *v = order.iter().map(|&o| v[o as usize]).collect();
        }
    }
    permute(&mut m.positions, &order);
    permute(&mut m.normals, &order);
    permute(&mut m.tangents, &order);
    permute(&mut m.uv0, &order);
    permute(&mut m.uv1, &order);
    if let Some(s) = &mut m.skin {
        permute(&mut s.vertex_joints, &order);
        permute(&mut s.vertex_weights, &order);
    }
    for b in &mut m.blend_shapes {
        b.deltas.iter_mut().for_each(|d| d.vertex = to[d.vertex as usize]);
        b.deltas.sort_by_key(|d| d.vertex);
    }
    m.indices.iter_mut().for_each(|i| *i = to[*i as usize]);
    for (k, l) in m.lods.iter_mut().enumerate() {
        l.vertices = depth.iter().filter(|&&d| d > k).count() as u32;
        l.indices.iter_mut().for_each(|i| *i = to[*i as usize]);
    }
}
