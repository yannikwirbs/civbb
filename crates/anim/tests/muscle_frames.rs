//! Muscle frames derived from a rest pose against Unity's `AvatarBuilder` (#53, ADR 0064): the generated humanoid
//! reshaped one way at a time (`tests/unity/muscle_frames.json`, `tools/unity-truth/Assets/Editor/MuscleFrames.cs`).

use std::path::Path;

use civbb_anim::frames::derive;
use civbb_core::math::{conj, length, mul};
use civbb_format::hierarchy::{Hierarchy, HumanBone, Humanoid, Node};
use civbb_format::math::{Quat, Transform, Vec3};
use serde::Deserialize;

#[derive(Deserialize)]
struct Recording {
    variants: Vec<Variant>,
}

#[derive(Deserialize)]
struct Variant {
    name: String,
    nodes: Vec<RecordedNode>,
    frames: Vec<RecordedFrame>,
}

#[derive(Deserialize)]
struct RecordedNode {
    name: String,
    parent: Option<u32>,
    position: Vec3,
    rotation: Quat,
}

#[derive(Deserialize)]
struct RecordedFrame {
    bone: HumanBone,
    node: u32,
    pre: Quat,
    post: Quat,
    sign: Vec3,
}

/// Unity's left-handed axes to ours: mirror X (ADR 0006).
fn mirror_q(q: Quat) -> Quat {
    [q[0], -q[1], -q[2], q[3]]
}

/// The angle between two rotations in degrees, robust near zero.
fn angle(a: Quat, b: Quat) -> f32 {
    let d = mul(conj(a), b);
    2.0 * length([d[0], d[1], d[2]]).atan2(d[3].abs()).to_degrees()
}

#[test]
fn derived_frames_match_unity() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/unity/muscle_frames.json");
    let r: Recording = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(r.variants.len() >= 10);
    let mut worst = 0.0f32;
    for v in &r.variants {
        let hierarchy = Hierarchy {
            nodes: (v.nodes.iter())
                .map(|n| Node {
                    name: n.name.clone(),
                    parent: n.parent,
                    rest: Transform {
                        translation: [-n.position[0], n.position[1], n.position[2]],
                        rotation: mirror_q(n.rotation),
                        scale: [1.0; 3],
                    },
                    active: true,
                })
                .collect(),
        };
        let humanoid = Humanoid {
            bones: v.frames.iter().map(|f| (f.bone, f.node)).collect(),
            ..Humanoid::default()
        };
        let derived = derive(&hierarchy, &humanoid);
        assert_eq!(derived.len(), v.frames.len(), "{}: a bone got no frame", v.name);
        for f in &v.frames {
            let d = derived.iter().find(|d| d.bone == f.bone).unwrap();
            let (pre, post) = (angle(d.pre, mirror_q(f.pre)), angle(d.post, mirror_q(f.post)));
            assert!(
                pre < 0.05 && post < 0.05,
                "{} {:?}: pre off by {pre} degrees, post by {post}",
                v.name,
                f.bone
            );
            assert_eq!(
                d.sign,
                [f.sign[0], -f.sign[1], -f.sign[2]],
                "{} {:?} sign",
                v.name,
                f.bone
            );
            worst = worst.max(pre).max(post);
        }
    }
    eprintln!("worst {worst} degrees");
}
