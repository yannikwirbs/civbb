//! Unity's humanoid body pose (#52) against `HumanPoseHandler` recordings on the generated test skeleton, plain and
//! with rolled bone axes (`tests/unity/body_pose.json`, `tools/unity-truth/Assets/Editor/BodyPose.cs`): the centre
//! of mass over the human scale is the body position, the orientation relative to the rest pose's the body rotation.

use std::collections::HashMap;
use std::path::Path;

use civbb_anim::body::{BodyRig, center_of_mass, orientation};
use civbb_core::math::{Pose, conj, length, mul, rotate, scale, sub};
use civbb_format::hierarchy::{BoneNode, Hierarchy, HumanBone, Humanoid, Node};
use civbb_format::math::{Quat, Transform, Vec3};
use serde::Deserialize;

#[derive(Deserialize)]
struct Recording {
    skeletons: Vec<Skeleton>,
}

#[derive(Deserialize)]
struct Skeleton {
    rolled: bool,
    /// Human bones left unmapped (their nodes stay).
    unmapped: Vec<String>,
    human_scale: f32,
    nodes: Vec<RecordedNode>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct RecordedNode {
    name: String,
    parent: Option<u32>,
    position: Vec3,
    rotation: Quat,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    /// Each node's world position and rotation after `SetHumanPose`, in node order.
    world: Vec<(Vec3, Quat)>,
    /// `GetHumanPose` of the result.
    got_position: Vec3,
    got_rotation: Quat,
}

/// Unity's left-handed axes to ours: mirror X (ADR 0006).
fn mirror(v: Vec3) -> Vec3 {
    [-v[0], v[1], v[2]]
}

fn mirror_q(q: Quat) -> Quat {
    [q[0], -q[1], -q[2], q[3]]
}

/// The angle between two rotations in degrees, robust near zero.
fn angle(a: Quat, b: Quat) -> f32 {
    let d = mul(conj(a), b);
    2.0 * length([d[0], d[1], d[2]]).atan2(d[3].abs()).to_degrees()
}

fn positions(s: &Skeleton, c: &Case) -> HashMap<HumanBone, Vec3> {
    s.nodes
        .iter()
        .zip(&c.world)
        .filter(|(n, _)| !s.unmapped.contains(&n.name))
        .filter_map(|(n, (p, _))| {
            let bone = civbb_anim::muscle::unity_bone(&n.name)?;
            Some((bone, mirror(*p)))
        })
        .collect()
}

#[test]
fn body_pose_matches_unity() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/unity/body_pose.json");
    let r: Recording = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(r.skeletons.len(), 4);
    for s in &r.skeletons {
        let rest = s.cases.iter().find(|c| c.name == "rest").expect("rest case");
        let rest = positions(s, rest);
        let rest_axes = orientation(|b| rest.get(&b).copied()).unwrap();
        let (mut worst_angle, mut worst_com) = (0.0f32, 0.0f32);
        for c in &s.cases {
            let pos = positions(s, c);
            let body = mul(orientation(|b| pos.get(&b).copied()).unwrap(), conj(rest_axes));
            let a = angle(body, mirror_q(c.got_rotation));
            let com = center_of_mass(|b| pos.get(&b).copied());
            let d = length(sub(com, scale(mirror(c.got_position), s.human_scale)));
            assert!(
                a < 0.35,
                "rolled {} unmapped {:?} {}: body rotation off by {a} degrees",
                s.rolled,
                s.unmapped,
                c.name
            );
            assert!(
                d < 0.0015,
                "rolled {} unmapped {:?} {}: centre of mass off by {d} m",
                s.rolled,
                s.unmapped,
                c.name
            );
            worst_angle = worst_angle.max(a);
            worst_com = worst_com.max(d);
        }
        // The residuals are real (Unity's own axis lengths would fit closer, ADR 0073): keep the bounds honest.
        assert!(
            worst_angle > 0.15 && worst_com > 0.0004,
            "{worst_angle} {worst_com}: tighten the bounds"
        );
    }
}

/// `BodyRig::place` puts the hips where Unity does for the body pose it reads back, on Unity's own posed bones.
#[test]
fn hips_placement_matches_unity() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/unity/body_pose.json");
    let r: Recording = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for s in &r.skeletons {
        let hierarchy = Hierarchy {
            nodes: (s.nodes.iter())
                .map(|n| Node {
                    name: n.name.clone(),
                    parent: n.parent,
                    rest: Transform {
                        translation: mirror(n.position),
                        rotation: mirror_q(n.rotation),
                        scale: [1.0; 3],
                    },
                    active: true,
                })
                .collect(),
        };
        let bones: Vec<BoneNode> = (s.nodes.iter().enumerate())
            .filter(|(_, n)| !s.unmapped.contains(&n.name))
            .filter_map(|(i, n)| {
                let bone = civbb_anim::muscle::unity_bone(&n.name)?;
                Some(BoneNode { bone, node: i as u32 })
            })
            .collect();
        let hips = bones.iter().find(|b| b.bone == HumanBone::Hips).unwrap().node as usize;
        // Anchored to Unity's rest centre of mass, as exports are: the rest case is the rest pose moved to body
        // position (0, 1, 0), so move its centre back by the hips' offset.
        let rest = s.cases.iter().find(|c| c.name == "rest").expect("rest case");
        let rest_hips = hierarchy.nodes[hips].rest.translation;
        let moved = sub(mirror(rest.world[hips].0), rest_hips);
        let humanoid = Humanoid {
            bones,
            human_scale: s.human_scale,
            center_of_mass: Some(sub(scale(mirror(rest.got_position), s.human_scale), moved)),
            ..Humanoid::default()
        };
        let rig = BodyRig::new(&hierarchy, &humanoid).unwrap();
        let (mut worst_d, mut worst_a) = (0.0f32, 0.0f32);
        for c in &s.cases {
            let world: Vec<Pose> = (c.world.iter())
                .map(|(p, q)| Pose {
                    pos: mirror(*p),
                    rot: mirror_q(*q),
                    scale: 1.0,
                })
                .collect();
            let mut locals: Vec<Transform> = (hierarchy.nodes.iter().enumerate())
                .map(|(i, n)| {
                    let w = world[i];
                    let p = n.parent.map_or(Pose::IDENTITY, |p| world[p as usize]);
                    Transform {
                        translation: rotate(conj(p.rot), sub(w.pos, p.pos)),
                        rotation: mul(conj(p.rot), w.rot),
                        scale: [1.0; 3],
                    }
                })
                .collect();
            locals[hips] = hierarchy.nodes[hips].rest;
            let (p, q) = (mirror(c.got_position), mirror_q(c.got_rotation));
            rig.place([p[0], p[1], p[2], q[0], q[1], q[2], q[3]], &mut locals);
            let placed = Pose::IDENTITY.then(&locals[0]).then(&locals[hips]);
            let d = length(sub(placed.pos, world[hips].pos));
            let a = angle(placed.rot, world[hips].rot);
            assert!(
                d < 0.007 && a < 0.35,
                "rolled {} {}: hips off by {d} m, {a} degrees",
                s.rolled,
                c.name
            );
            worst_d = worst_d.max(d);
            worst_a = worst_a.max(a);
        }
        eprintln!("rolled {}: hips within {worst_d} m, {worst_a} degrees", s.rolled);
    }
}
