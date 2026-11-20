//! Unity's humanoid body pose (#52) against `HumanPoseHandler` recordings on the generated test skeleton, plain and
//! with rolled bone axes (`tests/unity/body_pose.json`, `tools/unity-truth/Assets/Editor/BodyPose.cs`): the centre
//! of mass over the human scale is the body position, the orientation relative to the rest pose's the body rotation.

use std::collections::HashMap;
use std::path::Path;

use civbb_anim::body::{center_of_mass, orientation};
use civbb_core::math::{conj, length, mul, scale, sub};
use civbb_format::hierarchy::HumanBone;
use civbb_format::math::{Quat, Vec3};
use serde::Deserialize;

#[derive(Deserialize)]
struct Recording {
    skeletons: Vec<Skeleton>,
}

#[derive(Deserialize)]
struct Skeleton {
    rolled: bool,
    human_scale: f32,
    nodes: Vec<RecordedNode>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct RecordedNode {
    name: String,
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
        .filter_map(|(n, (p, _))| {
            let bone = serde_json::from_value(serde_json::Value::String(n.name.clone())).ok()?;
            Some((bone, mirror(*p)))
        })
        .collect()
}

#[test]
fn body_pose_matches_unity() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/unity/body_pose.json");
    let r: Recording = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(r.skeletons.len(), 2);
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
                "rolled {} {}: body rotation off by {a} degrees",
                s.rolled,
                c.name
            );
            assert!(d < 0.006, "rolled {} {}: centre of mass off by {d} m", s.rolled, c.name);
            worst_angle = worst_angle.max(a);
            worst_com = worst_com.max(d);
        }
        // The residuals are real (no point-mass model fits better than 1.7 mm): keep the bounds honest.
        assert!(
            worst_angle > 0.2 && worst_com > 0.003,
            "{worst_angle} {worst_com}: tighten the bounds"
        );
    }
}
