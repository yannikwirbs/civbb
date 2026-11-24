//! VRM spring bones (0.x `secondaryAnimation`, 1.0 `VRMC_springBone`) to chains and colliders (ADR 0006).

use civbb_core::math::{conj, dot};
use civbb_format::dynamics::{
    Chain, Collider, Dynamics, Graded, ImmobileType, Integration, Limit, MultiChild, Shape, Version,
};
use civbb_format::hierarchy::NodeIndex;
use civbb_format::math::Vec3;
use serde_json::Value;

use super::{arr, from_to, index, num, rotate, xyz};
use crate::gltf_import::Importer;

impl Importer<'_> {
    pub(super) fn springs_v0(&mut self, sa: &Value) -> Dynamics {
        let mut d = Dynamics::default();
        let mut groups = Vec::new();
        for g in sa["colliderGroups"].as_array().into_iter().flatten() {
            let mut ids = Vec::new();
            if let Some(node) = self.vrm_node(&g["node"]) {
                for c in g["colliders"].as_array().into_iter().flatten() {
                    ids.push(d.colliders.len() as u32);
                    d.colliders
                        .push(sphere(node, xyz(&c["offset"], [0.0; 3]), num(&c["radius"], 0.0)));
                }
            } else {
                self.warnings
                    .push("VRM collider group without a valid node skipped".into());
            }
            groups.push(ids);
        }
        let children = self.children();
        for bg in sa["boneGroups"].as_array().into_iter().flatten() {
            let colliders = collider_list(&bg["colliderGroups"], &groups);
            let gravity_dir = xyz(&bg["gravityDir"], [0.0, -1.0, 0.0]);
            for b in bg["bones"].as_array().into_iter().flatten() {
                let Some(root) = self.vrm_node(b) else {
                    self.warnings
                        .push("VRM spring bone without a valid node skipped".into());
                    continue;
                };
                // UniVRM 0.x gives each leaf a 7 cm virtual tail continuing the bone; use the first leaf's direction.
                let mut leaf = root;
                while let Some(&c) = children[leaf as usize].first() {
                    leaf = c;
                }
                let rest = &self.nodes[leaf as usize].rest;
                let dir = rotate(conj(rest.rotation), rest.translation);
                let len = dot(dir, dir).sqrt();
                let endpoint = if len > 1e-6 {
                    dir.map(|x| x / len * 0.07)
                } else {
                    [0.0, 0.07, 0.0]
                };
                d.chains.push(chain(
                    root,
                    Vec::new(),
                    endpoint,
                    &Params {
                        stiffness: Graded::constant(num(&bg["stiffiness"], 1.0)),
                        drag: Graded::constant(num(&bg["dragForce"], 0.4)),
                        gravity: Graded::constant(num(&bg["gravityPower"], 0.0) * -gravity_dir[1]),
                        radius: Graded::constant(num(&bg["hitRadius"], 0.02)),
                    },
                    colliders.clone(),
                ));
            }
        }
        d
    }

    pub(super) fn springs_v1(&mut self, sb: &Value) -> Dynamics {
        let mut d = Dynamics::default();
        let mut collider_ids = Vec::new();
        for c in sb["colliders"].as_array().into_iter().flatten() {
            let (Some(node), shape) = (self.vrm_node(&c["node"]), &c["shape"]) else {
                self.warnings.push("VRM collider without a valid node skipped".into());
                collider_ids.push(None);
                continue;
            };
            let collider = if !shape["capsule"].is_null() {
                let s = &shape["capsule"];
                capsule(
                    node,
                    arr(&s["offset"], [0.0; 3]),
                    arr(&s["tail"], [0.0; 3]),
                    num(&s["radius"], 0.0),
                )
            } else {
                let s = &shape["sphere"];
                sphere(node, arr(&s["offset"], [0.0; 3]), num(&s["radius"], 0.0))
            };
            collider_ids.push(Some(d.colliders.len() as u32));
            d.colliders.push(collider);
        }
        let groups: Vec<Vec<u32>> = sb["colliderGroups"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|g| {
                let ids = g["colliders"].as_array().into_iter().flatten();
                ids.filter_map(|i| collider_ids.get(index(i)?).copied().flatten())
                    .collect()
            })
            .collect();
        let children = self.children();
        let cap = self.caps.graded_points;
        for s in sb["springs"].as_array().into_iter().flatten() {
            let joints: Vec<(NodeIndex, &Value)> = s["joints"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|j| Some((self.vrm_node(&j["node"])?, j)))
                .collect();
            let Some(&(root, _)) = joints.first() else {
                self.warnings.push("VRM spring without valid joints skipped".into());
                continue;
            };
            // Only the listed path is simulated: branches off it are ignored, and the last joint is the tail.
            let mut ignore = Vec::new();
            for (i, (node, _)) in joints.iter().enumerate() {
                let next = joints.get(i + 1).map(|j| j.0);
                ignore.extend(children[*node as usize].iter().filter(|&&c| Some(c) != next));
            }
            // Parameters on a joint drive the segment to the next joint, so the tail's are unused.
            let segs = &joints[..joints.len().saturating_sub(1).max(1)];
            let per =
                |key: &str, default: f32| -> Vec<f32> { segs.iter().map(|(_, j)| num(&j[key], default)).collect() };
            let gravity: Vec<f32> = segs
                .iter()
                .map(|(_, j)| num(&j["gravityPower"], 0.0) * -arr(&j["gravityDir"], [0.0, -1.0, 0.0])[1])
                .collect();
            d.chains.push(chain(
                root,
                ignore,
                [0.0; 3],
                &Params {
                    stiffness: graded(&per("stiffness", 1.0), cap),
                    drag: graded(&per("dragForce", 0.5), cap),
                    gravity: graded(&gravity, cap),
                    radius: graded(&per("hitRadius", 0.0), cap),
                },
                collider_list(&s["colliderGroups"], &groups),
            ));
        }
        d
    }
}

struct Params {
    stiffness: Graded,
    drag: Graded,
    gravity: Graded,
    radius: Graded,
}

/// A chain with the ADR 0006 mapping: pull from stiffness, momentum from drag, gravity from its vertical part.
fn chain(root: NodeIndex, ignore: Vec<NodeIndex>, endpoint: Vec3, p: &Params, colliders: Vec<u32>) -> Chain {
    let map = |g: &Graded, f: fn(f32) -> f32| {
        let value = f(g.value);
        // Curves scale the base value; recompute them so the mapped value at each point stays exact.
        let curve = g
            .curve
            .iter()
            .map(|&[d, k]| [d, if value != 0.0 { f(g.value * k) / value } else { 0.0 }])
            .collect();
        Graded { value, curve }
    };
    let constant = Graded::constant;
    Chain {
        root,
        ignore,
        endpoint,
        multi_child: MultiChild::First,
        version: Version::V1_0,
        integration: Integration::Simplified,
        pull: map(&p.stiffness, |s| (s / 4.0).clamp(0.0, 1.0)),
        spring: map(&p.drag, |d| (1.0 - d).clamp(0.0, 1.0)),
        stiffness: constant(0.0),
        gravity: map(&p.gravity, |g| g.clamp(-1.0, 1.0)),
        gravity_falloff: constant(0.0),
        immobile: constant(0.0),
        immobile_type: ImmobileType::AllMotion,
        limit: Limit::None,
        radius: p.radius.clone(),
        colliders,
        max_stretch: constant(0.0),
        max_squish: constant(0.0),
        allow_grab: true,
        allow_posing: true,
        parameter: None,
    }
}

/// Per-joint values as a graded value: the largest magnitude scaled by a curve over depth, at most `cap` points.
fn graded(values: &[f32], cap: usize) -> Graded {
    let max = values.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if max == 0.0 || values.windows(2).all(|w| w[0] == w[1]) {
        return Graded::constant(values.first().copied().unwrap_or(0.0));
    }
    let n = values.len();
    let points = n.min(cap.max(2));
    let curve = (0..points)
        .map(|k| {
            let i = k * (n - 1) / (points - 1);
            [i as f32 / (n - 1) as f32, values[i] / max]
        })
        .collect();
    Graded { value: max, curve }
}

fn collider_list(groups: &Value, resolved: &[Vec<u32>]) -> Vec<u32> {
    let mut out: Vec<u32> = groups
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| resolved.get(index(g)?))
        .flatten()
        .copied()
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn sphere(node: NodeIndex, offset: Vec3, radius: f32) -> Collider {
    Collider {
        node,
        shape: Shape::Sphere { radius },
        position: offset,
        rotation: [0.0, 0.0, 0.0, 1.0],
        inside: false,
    }
}

/// A capsule from `a` to `b`; ours run along local Y, so it is centred and rotated onto that segment.
fn capsule(node: NodeIndex, a: Vec3, b: Vec3, radius: f32) -> Collider {
    let axis = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = dot(axis, axis).sqrt();
    if len < 1e-6 {
        return sphere(node, a, radius);
    }
    let dir = axis.map(|x| x / len);
    Collider {
        node,
        shape: Shape::Capsule {
            radius,
            height: len + 2.0 * radius,
        },
        position: [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0],
        rotation: from_to([0.0, 1.0, 0.0], dir),
        inside: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graded_keeps_per_joint_values() {
        let g = graded(&[0.5, 1.0, 0.25], 64);
        assert_eq!(g.value, 1.0);
        assert_eq!(g.curve, [[0.0, 0.5], [0.5, 1.0], [1.0, 0.25]]);
        assert_eq!(graded(&vec![1.0; 100], 64), Graded::constant(1.0));
        assert_eq!(
            graded(&(0..100).map(|i| i as f32).collect::<Vec<_>>(), 8).curve.len(),
            8
        );
    }
}
