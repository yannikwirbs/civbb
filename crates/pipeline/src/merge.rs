//! Compiler stage 3 mesh merging: meshes that are always visible together become one skinned mesh, and submeshes
//! sharing a material one draw (design doc §9, ADR 0027).

use std::collections::{HashMap, HashSet};

use civbb_format::Avatar;
use civbb_format::animator::Property;
use civbb_format::face::EyeDriver;
use civbb_format::hierarchy::NodeIndex;
use civbb_format::material::AlphaMode;
use civbb_format::math::Mat4;
use civbb_format::mesh::{FirstPerson, Mesh, Skin, Submesh};

use crate::prune::node_paths;

/// How much merging removed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Merged {
    /// Meshes, each one skinning job.
    pub meshes: usize,
    /// Submeshes, each one draw.
    pub submeshes: usize,
}

const IDENTITY: Mat4 = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// What merged meshes share: the animated-activity nodes above them (so they show in exactly the same states),
/// first-person visibility and which optional vertex streams they have.
#[derive(PartialEq, Eq)]
struct Key {
    toggles: Vec<NodeIndex>,
    first_person: FirstPerson,
    streams: [bool; 3],
}

/// Merges meshes of a validated, pruned avatar in place; it stays valid and looks the same in every state.
pub fn merge(a: &mut Avatar) -> Merged {
    let meshes_before = a.meshes.len();
    let submeshes_before: usize = a.meshes.iter().map(|m| m.submeshes.len()).sum();
    let paths = node_paths(&a.hierarchy);
    // Clip paths resolve to the first node with that path, as `civbb-anim` binds them.
    let by_path: HashMap<&str, usize> = paths.iter().enumerate().rev().map(|(i, p)| (p.as_str(), i)).collect();
    let n = a.hierarchy.nodes.len();
    let mut toggled = vec![false; n];
    let mut renderer = vec![false; n];
    let mut shape_curves: Vec<HashSet<&str>> = vec![HashSet::new(); n];
    for c in a.clips.iter().flat_map(|c| &c.curves) {
        let Some(&i) = by_path.get(c.binding.path.as_str()) else {
            continue;
        };
        match &c.binding.property {
            Property::Active => toggled[i] = true,
            Property::RendererEnabled | Property::Material(_) => renderer[i] = true,
            Property::BlendShape(name) => {
                shape_curves[i].insert(name.as_str());
            }
            _ => {}
        }
    }
    let mut per_node = vec![0; n];
    for m in &a.meshes {
        per_node[m.node as usize] += 1;
    }
    // A mesh merges only when it is its node's only mesh, its path is its own, no clip animates its renderer or
    // materials, it draws nothing blended (draw order) and some state can show it.
    let keys: Vec<Option<Key>> = a
        .meshes
        .iter()
        .map(|m| {
            let node = m.node as usize;
            let blended = m
                .submeshes
                .iter()
                .any(|s| matches!(a.materials[s.material as usize].alpha, AlphaMode::Blend));
            if per_node[node] > 1 || by_path[paths[node].as_str()] != node || renderer[node] || blended {
                return None;
            }
            let mut toggles = Vec::new();
            let mut at = Some(m.node);
            while let Some(i) = at {
                let node = &a.hierarchy.nodes[i as usize];
                if toggled[i as usize] {
                    toggles.push(i);
                } else if !node.active {
                    return None;
                }
                at = node.parent;
            }
            Some(Key {
                toggles,
                first_person: m.first_person,
                streams: [!m.tangents.is_empty(), !m.uv0.is_empty(), !m.uv1.is_empty()],
            })
        })
        .collect();

    // In mesh order, a mesh joins the first group with its key when no shape name repeats (curves and face
    // tracking find shapes by name), no curve on the group's path names its shapes (it was unbound before), and
    // joints still fit 16-bit indices.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut of = vec![None; a.meshes.len()];
    {
        let mut names: Vec<HashSet<&str>> = Vec::new();
        let mut joints: Vec<usize> = Vec::new();
        for (i, m) in a.meshes.iter().enumerate() {
            let Some(key) = &keys[i] else { continue };
            let own: Vec<&str> = m.blend_shapes.iter().map(|s| s.name.as_str()).collect();
            let j = m.skin.as_ref().map_or(0, |s| s.joints.len()).max(1);
            let fits = |g: usize| {
                let first = &a.meshes[groups[g][0]];
                keys[groups[g][0]].as_ref() == Some(key)
                    && own
                        .iter()
                        .all(|s| !names[g].contains(s) && !shape_curves[first.node as usize].contains(s))
                    && joints[g] + j <= 1 << 16
            };
            let g = match (0..groups.len()).find(|&g| fits(g)) {
                Some(g) => g,
                None => {
                    groups.push(Vec::new());
                    names.push(HashSet::new());
                    joints.push(0);
                    groups.len() - 1
                }
            };
            groups[g].push(i);
            names[g].extend(&own);
            joints[g] += j;
            of[i] = Some(g);
        }
    }

    // Shape curves on a merged-away mesh's node move to the group's node, if they named one of its shapes.
    let mut moved: HashMap<usize, (usize, HashSet<String>)> = HashMap::new();
    for g in groups.iter().filter(|g| g.len() > 1) {
        let to = a.meshes[g[0]].node as usize;
        for &i in &g[1..] {
            let m = &a.meshes[i];
            let names = m.blend_shapes.iter().map(|s| s.name.clone()).collect();
            moved.insert(m.node as usize, (to, names));
        }
    }
    for c in a.clips.iter_mut().flat_map(|c| &mut c.curves) {
        let Property::BlendShape(name) = &c.binding.property else {
            continue;
        };
        let Some(&i) = by_path.get(c.binding.path.as_str()) else {
            continue;
        };
        if let Some((to, names)) = moved.get(&i)
            && names.contains(name)
        {
            c.binding.path = paths[*to].clone();
        }
    }

    // Old mesh to new mesh and the offset of its shapes there.
    let mut target = vec![(0u32, 0u32); a.meshes.len()];
    let mut old: Vec<Option<Mesh>> = std::mem::take(&mut a.meshes).into_iter().map(Some).collect();
    for i in 0..old.len() {
        let Some(mut m) = old[i].take() else { continue };
        let new = a.meshes.len() as u32;
        if let Some(g) = of[i] {
            let members = &groups[g];
            if members.len() > 1 {
                let mut parts = vec![m];
                parts.extend(members[1..].iter().map(|&j| old[j].take().expect("merged once")));
                let mut shapes = 0;
                for (&j, part) in members.iter().zip(&parts) {
                    target[j] = (new, shapes);
                    shapes += part.blend_shapes.len() as u32;
                }
                m = combine(parts);
            } else {
                target[i] = (new, 0);
            }
            coalesce(&mut m);
        } else {
            target[i] = (new, 0);
        }
        a.meshes.push(m);
    }
    if let Some(face) = &mut a.face {
        let mut binds: Vec<_> = face
            .visemes
            .iter_mut()
            .flat_map(|(_, b)| b)
            .chain(&mut face.blink)
            .collect();
        if let Some(EyeDriver::Shapes { up, down, left, right }) = face.eyes.as_mut().map(|e| &mut e.driver) {
            binds.extend(up.iter_mut().chain(down).chain(left).chain(right));
        }
        for b in binds {
            let (mesh, offset) = target[b.mesh as usize];
            b.mesh = mesh;
            b.shape += offset;
        }
    }
    Merged {
        meshes: meshes_before - a.meshes.len(),
        submeshes: submeshes_before - a.meshes.iter().map(|m| m.submeshes.len()).sum::<usize>(),
    }
}

/// Appends meshes into one skinned mesh on the first one's node. A rigid mesh becomes skinned to its own node with
/// an identity inverse bind, which places it as before; joints with the same node and inverse bind are shared.
fn combine(parts: Vec<Mesh>) -> Mesh {
    let first = &parts[0];
    let mut out = Mesh {
        name: first.name.clone(),
        node: first.node,
        first_person: first.first_person,
        ..Mesh::default()
    };
    let mut skin = Skin::default();
    let mut shared: HashMap<(NodeIndex, [u32; 16]), u16> = HashMap::new();
    for m in parts {
        let base = out.positions.len() as u32;
        let first_index = out.indices.len() as u32;
        let v = m.positions.len();
        let s = match m.skin {
            Some(s) if !s.joints.is_empty() => s,
            _ => Skin {
                joints: vec![m.node],
                inverse_bind: vec![IDENTITY],
                vertex_joints: vec![[0; 4]; v],
                vertex_weights: vec![[1.0, 0.0, 0.0, 0.0]; v],
            },
        };
        let map: Vec<u16> = s
            .joints
            .iter()
            .zip(&s.inverse_bind)
            .map(|(&j, ib)| {
                *shared.entry((j, ib.map(f32::to_bits))).or_insert_with(|| {
                    skin.joints.push(j);
                    skin.inverse_bind.push(*ib);
                    (skin.joints.len() - 1) as u16
                })
            })
            .collect();
        skin.vertex_joints
            .extend(s.vertex_joints.iter().map(|js| js.map(|j| map[j as usize])));
        skin.vertex_weights.extend(s.vertex_weights);
        out.positions.extend(m.positions);
        out.normals.extend(m.normals);
        out.tangents.extend(m.tangents);
        out.uv0.extend(m.uv0);
        out.uv1.extend(m.uv1);
        out.indices.extend(m.indices.iter().map(|i| i + base));
        out.submeshes.extend(m.submeshes.iter().map(|s| Submesh {
            first_index: s.first_index + first_index,
            ..*s
        }));
        out.blend_shapes.extend(m.blend_shapes.into_iter().map(|mut s| {
            s.deltas.iter_mut().for_each(|d| d.vertex += base);
            s
        }));
    }
    out.skin = Some(skin);
    out
}

/// Joins each material's submeshes into one, materials in order of first use. Only for meshes without blended
/// submeshes: opaque and cutout draws are depth-tested, so their order does not change the image.
fn coalesce(m: &mut Mesh) {
    let mut order: Vec<u32> = Vec::new();
    for s in &m.submeshes {
        if !order.contains(&s.material) {
            order.push(s.material);
        }
    }
    if order.len() == m.submeshes.len() {
        return;
    }
    let mut indices = Vec::with_capacity(m.indices.len());
    let mut submeshes = Vec::with_capacity(order.len());
    for material in order {
        let first_index = indices.len() as u32;
        for s in m.submeshes.iter().filter(|s| s.material == material) {
            let start = s.first_index as usize;
            indices.extend_from_slice(&m.indices[start..start + s.index_count as usize]);
        }
        submeshes.push(Submesh {
            first_index,
            index_count: indices.len() as u32 - first_index,
            material,
        });
    }
    m.indices = indices;
    m.submeshes = submeshes;
}
