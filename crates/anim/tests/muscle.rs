//! Humanoid muscles (ADR 0014): the table against Unity's, hand-derived muscle space, poses recorded from Unity's
//! `HumanPoseHandler` on three skeletons (`tests/unity/`, ADR 0013), binding, layer masks and the evaluated pose.

use std::collections::HashMap;
use std::path::Path;

use civbb_anim::muscle::{COUNT, MUSCLES, find};
use civbb_anim::{Controller, Instance, Rig, Target, bind};
use civbb_format::animator::*;
use civbb_format::hierarchy::{HumanBone, Humanoid, MuscleFrame, Node, NodeIndex};
use civbb_format::math::{Quat, Transform};
use civbb_format::{Avatar, Caps, validate};
use serde::Deserialize;

#[derive(Deserialize)]
struct Table {
    bones: Vec<String>,
    muscles: Vec<TableMuscle>,
    clip_properties: Vec<String>,
}

#[derive(Deserialize)]
struct TableMuscle {
    bone: usize,
    dof: u8,
    min: f32,
    max: f32,
}

/// One skeleton recorded by `tools/unity-truth` (Unity coordinates).
#[derive(Deserialize)]
struct Recording {
    twist: [f32; 4],
    nodes: Vec<RecordedNode>,
    axes: Vec<RecordedAxes>,
    rest: RecordedRest,
    poses: Vec<RecordedPose>,
}

#[derive(Deserialize)]
struct RecordedNode {
    name: String,
    parent: Option<u32>,
    position: [f32; 3],
    rotation: Quat,
}

#[derive(Deserialize)]
struct RecordedAxes {
    bone: usize,
    node: NodeIndex,
    pre: Quat,
    post: Quat,
    sign: [f32; 3],
}

#[derive(Deserialize)]
struct RecordedRest {
    muscles: Vec<f32>,
}

#[derive(Deserialize)]
struct RecordedPose {
    muscles: Vec<(usize, f32)>,
    /// `(axes index, local rotation)`; the first pose lists every bone, later ones only those that differ from it.
    local: Vec<(usize, Quat)>,
}

fn load<T: for<'a> Deserialize<'a>>(name: &str) -> T {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/unity").join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// Unity's left-handed axes to ours: mirror X (ADR 0006, 0007).
fn mirror(q: Quat) -> Quat {
    [q[0], -q[1], -q[2], q[3]]
}

fn human_bone(unity_name: &str) -> HumanBone {
    serde_json::from_value(serde_json::Value::String(unity_name.replace(' ', ""))).unwrap()
}

/// Angle between two rotations in degrees.
fn angle(a: Quat, b: Quat) -> f64 {
    let dot: f64 = a.iter().zip(b).map(|(x, y)| f64::from(*x) * f64::from(y)).sum();
    2.0 * dot.abs().min(1.0).acos().to_degrees()
}

/// Unity's default limits of a bone's three axes.
fn default_limits(bone: HumanBone) -> ([f32; 3], [f32; 3]) {
    let (mut min, mut max) = ([0.0; 3], [0.0; 3]);
    for m in MUSCLES.iter().filter(|m| m.bone == bone) {
        min[m.axis as usize] = m.min;
        max[m.axis as usize] = m.max;
    }
    (min, max)
}

#[test]
fn table_matches_unity() {
    let t: Table = load("muscles.json");
    let names: Vec<&String> = t
        .clip_properties
        .iter()
        .filter(|p| !p.ends_with(['x', 'y', 'z', 'w']) && *p != "m_Enabled")
        .collect();
    assert_eq!((names.len(), t.muscles.len()), (COUNT, COUNT));
    for (i, (m, u)) in MUSCLES.iter().zip(&t.muscles).enumerate() {
        assert_eq!(m.name, names[i], "muscle {i}");
        assert_eq!(m.bone, human_bone(&t.bones[u.bone]), "{}", m.name);
        assert_eq!((m.axis, m.min, m.max), (u.dof, u.min, u.max), "{}", m.name);
        assert_eq!(find(m.name), Some(i as u8));
    }
    assert_eq!(find("Left Index 1 Stretched"), None, "fingers bind by clip name only");
}

fn check_recording(file: &str) {
    let r: Recording = load(file);
    let t: Table = load("muscles.json");
    let bone = |a: &RecordedAxes| human_bone(&t.bones[a.bone]);
    let avatar = Avatar {
        hierarchy: civbb_format::hierarchy::Hierarchy {
            nodes: r
                .nodes
                .iter()
                .map(|n| Node {
                    name: n.name.clone(),
                    parent: n.parent,
                    rest: Transform {
                        translation: [-n.position[0], n.position[1], n.position[2]],
                        rotation: mirror(n.rotation),
                        scale: [1.0; 3],
                    },
                    active: true,
                })
                .collect(),
        },
        humanoid: Some(Humanoid {
            bones: r.axes.iter().map(|a| (bone(a), a.node)).collect(),
            viewpoint: [0.0, 1.6, 0.1],
            muscle_frames: r
                .axes
                .iter()
                .map(|a| {
                    let b = bone(a);
                    let (min, max) = default_limits(b);
                    let twist = match b {
                        HumanBone::LeftUpperArm | HumanBone::RightUpperArm => r.twist[0],
                        HumanBone::LeftLowerArm | HumanBone::RightLowerArm => r.twist[1],
                        HumanBone::LeftUpperLeg | HumanBone::RightUpperLeg => r.twist[2],
                        HumanBone::LeftLowerLeg | HumanBone::RightLowerLeg => r.twist[3],
                        _ => 1.0,
                    };
                    // Mirroring X keeps twists and flips Y and Z swings.
                    MuscleFrame {
                        bone: b,
                        pre: mirror(a.pre),
                        post: mirror(a.post),
                        sign: [a.sign[0], -a.sign[1], -a.sign[2]],
                        min,
                        max,
                        twist,
                    }
                })
                .collect(),
        }),
        ..Avatar::default()
    };
    validate(&avatar, &Caps::default(), |_| true).unwrap();
    let rest = |n: NodeIndex| avatar.hierarchy.nodes[n as usize].rest.rotation;
    let rig = Rig::new(avatar.humanoid.as_ref().unwrap(), rest).unwrap();

    for (i, (ours, unity)) in rig.rest.iter().zip(&r.rest.muscles).enumerate() {
        assert!(
            (ours - unity).abs() < 1e-3,
            "{file}: rest muscle {} {ours} != {unity}",
            MUSCLES[i].name
        );
    }

    let zero: Vec<Quat> = r.poses[0].local.iter().map(|(_, q)| *q).collect();
    for pose in &r.poses {
        let mut muscles = [0.0; COUNT];
        for &(m, v) in &pose.muscles {
            muscles[m] = v;
        }
        let mut expected = zero.clone();
        for &(i, q) in &pose.local {
            expected[i] = q;
        }
        let got: HashMap<NodeIndex, Quat> = rig.rotations(&muscles).into_iter().collect();
        for (i, a) in r.axes.iter().enumerate() {
            // The hips follow the body pose, which these cases hold fixed (#52).
            if bone(a) == HumanBone::Hips {
                continue;
            }
            let e = angle(got[&a.node], mirror(expected[i]));
            assert!(
                e < 0.1,
                "{file}: {:?} off by {e} degrees, muscles {:?}",
                bone(a),
                pose.muscles
            );
        }
        let back = rig.muscles(|n| got[&n]);
        for (m, (b, v)) in back.iter().zip(muscles).enumerate() {
            assert!(
                (b - v).abs() < 1e-3,
                "{file}: {} round trip {b} != {v}",
                MUSCLES[m].name
            );
        }
    }
}

#[test]
fn unity_t_pose() {
    check_recording("tpose.json");
}

#[test]
fn unity_rolled_bones() {
    check_recording("rolled.json");
}

#[test]
fn unity_twist_settings() {
    check_recording("twist.json");
}

const I: Quat = [0.0, 0.0, 0.0, 1.0];

fn about(axis: usize, degrees: f32) -> Quat {
    let (s, c) = (degrees.to_radians() * 0.5).sin_cos();
    let mut q = [0.0, 0.0, 0.0, c];
    q[axis] = s;
    q
}

/// Root, Hips, LeftUpperArm, LeftLowerArm, LeftHand at nodes 0 to 4; identity frames with default limits, signs 1,
/// arm twist share `twist`. The upper arm rests at `arm_rest`.
fn arm(twist: f32, arm_rest: Quat, clips: Vec<Clip>) -> Avatar {
    let bones = [
        HumanBone::Hips,
        HumanBone::LeftUpperArm,
        HumanBone::LeftLowerArm,
        HumanBone::LeftHand,
    ];
    let names = ["Root", "Hips", "UpperArm", "LowerArm", "Hand"];
    Avatar {
        hierarchy: civbb_format::hierarchy::Hierarchy {
            nodes: names
                .iter()
                .enumerate()
                .map(|(i, n)| Node {
                    name: (*n).into(),
                    parent: i.checked_sub(1).map(|p| p as u32),
                    rest: Transform {
                        rotation: if i == 2 { arm_rest } else { I },
                        ..Transform::IDENTITY
                    },
                    active: true,
                })
                .collect(),
        },
        humanoid: Some(Humanoid {
            bones: bones.iter().enumerate().map(|(i, b)| (*b, i as u32 + 1)).collect(),
            viewpoint: [0.0; 3],
            muscle_frames: bones
                .iter()
                .map(|&bone| {
                    let (min, max) = default_limits(bone);
                    MuscleFrame {
                        bone,
                        pre: I,
                        post: I,
                        sign: [1.0; 3],
                        min,
                        max,
                        twist: if bone.twist_child().is_some() { twist } else { 1.0 },
                    }
                })
                .collect(),
        }),
        clips,
        ..Avatar::default()
    }
}

fn rig(a: &Avatar) -> Rig {
    Rig::new(a.humanoid.as_ref().unwrap(), |n| {
        a.hierarchy.nodes[n as usize].rest.rotation
    })
    .unwrap()
}

fn muscles(values: &[(&str, f32)]) -> [f32; COUNT] {
    let mut m = [0.0; COUNT];
    for (name, v) in values {
        m[find(name).unwrap() as usize] = *v;
    }
    m
}

fn pose_of(rig: &Rig, values: &[(&str, f32)]) -> HashMap<NodeIndex, Quat> {
    rig.rotations(&muscles(values)).into_iter().collect()
}

fn same(a: Quat, b: Quat) {
    let sign = if a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>() < 0.0 {
        -1.0
    } else {
        1.0
    };
    assert!(
        a.iter().zip(b).all(|(x, y)| (x - sign * y).abs() < 1e-5),
        "{a:?} != {b:?}"
    );
}

#[test]
fn a_muscle_turns_its_axis_by_max_above_zero_and_min_below() {
    let r = rig(&arm(0.5, I, Vec::new()));
    // Left Arm Down-Up is the upper arm's Z, -60 to 100 degrees.
    same(pose_of(&r, &[("Left Arm Down-Up", 1.0)])[&2], about(2, 100.0));
    same(pose_of(&r, &[("Left Arm Down-Up", -0.5)])[&2], about(2, -30.0));
    same(pose_of(&r, &[])[&2], I);
    // Muscles beyond 1 extrapolate.
    same(pose_of(&r, &[("Left Arm Down-Up", 1.2)])[&2], about(2, 120.0));
}

#[test]
fn the_frame_sign_flips_an_axis() {
    let mut a = arm(0.5, I, Vec::new());
    a.humanoid.as_mut().unwrap().muscle_frames[1].sign = [1.0, 1.0, -1.0];
    same(pose_of(&rig(&a), &[("Left Arm Down-Up", 1.0)])[&2], about(2, -100.0));
}

#[test]
fn pre_and_post_carry_muscle_space_into_the_nodes() {
    let mut a = arm(0.5, I, Vec::new());
    let f = &mut a.humanoid.as_mut().unwrap().muscle_frames[1];
    f.pre = about(1, 90.0);
    f.post = about(0, 90.0);
    // pre * Rz(100) * post⁻¹ with h = sqrt(1/2), s = sin 50, c = cos 50:
    // (0, h, 0, h) * (0, 0, s, c) = h (s, c, s, c), then * (-h, 0, 0, h) = (s - c, c - s, c + s, c + s) / 2.
    let (s, c) = (50f32.to_radians().sin(), 50f32.to_radians().cos());
    let expected = [s - c, c - s, c + s, c + s].map(|x| x / 2.0);
    same(pose_of(&rig(&a), &[("Left Arm Down-Up", 1.0)])[&2], expected);
}

#[test]
fn swing_combines_y_and_z_by_half_angle_tangents() {
    let mut a = arm(0.5, I, Vec::new());
    let f = &mut a.humanoid.as_mut().unwrap().muscle_frames[3];
    f.max = [0.0, 90.0, 90.0];
    // Hand In-Out is Y, Down-Up is Z: tan(45) = 1 on both gives (0, 1, 1, 1) normalised, a 109.47 degree turn
    // about (0, 1, 1), not a 90 degree turn about each axis in turn.
    let q = pose_of(&rig(&a), &[("Left Hand In-Out", 1.0), ("Left Hand Down-Up", 1.0)])[&4];
    let third = 1.0 / 3f32.sqrt();
    same(q, [0.0, third, third, third]);
}

#[test]
fn twist_is_shared_with_the_next_bone_of_the_limb() {
    // Left Arm Twist In-Out is the upper arm's X, -90 to 90; the lower arm takes the share the upper arm passes on.
    let r = rig(&arm(0.5, I, Vec::new()));
    let p = pose_of(&r, &[("Left Arm Twist In-Out", 1.0)]);
    same(p[&2], about(0, 45.0));
    same(p[&3], about(0, 45.0));
    let r = rig(&arm(0.2, I, Vec::new()));
    let p = pose_of(
        &r,
        &[("Left Arm Twist In-Out", 1.0), ("Left Forearm Twist In-Out", -1.0)],
    );
    same(p[&2], about(0, 18.0));
    // 72 degrees passed on, then the forearm keeps 0.2 of its own -90: -18.
    same(p[&3], about(0, 54.0));
    // ... and passes -72 to the hand.
    same(p[&4], about(0, -72.0));
}

#[test]
fn rest_muscles_invert_the_rest_pose() {
    let r = rig(&arm(0.5, about(2, 50.0), Vec::new()));
    let m = find("Left Arm Down-Up").unwrap() as usize;
    assert!((r.rest[m] - 0.5).abs() < 1e-5, "{}", r.rest[m]);
    let r = rig(&arm(0.5, about(2, -30.0), Vec::new()));
    assert!((r.rest[m] + 0.5).abs() < 1e-5, "{}", r.rest[m]);
}

fn key(value: f32) -> Vec<Key> {
    vec![Key {
        time: 0.0,
        value,
        in_tangent: 0.0,
        out_tangent: 0.0,
    }]
}

fn muscle_clip(curves: &[(Property, f32)]) -> Clip {
    Clip {
        name: "c".into(),
        length: 1.0,
        looping: false,
        curves: curves
            .iter()
            .map(|(p, v)| Curve {
                binding: Binding {
                    path: String::new(),
                    property: p.clone(),
                },
                keys: key(*v),
            })
            .collect(),
    }
}

#[test]
fn muscle_curves_bind_to_slots_with_rest_defaults() {
    let clip = muscle_clip(&[
        (Property::Muscle("Left Arm Down-Up".into()), 1.0),
        (Property::Muscle("Left Arm Up".into()), 1.0),
        (Property::Muscle("Spine Front-Back".into()), 1.0),
        (Property::RootMotion(0), 1.0),
    ]);
    let a = arm(0.5, about(2, 50.0), vec![clip.clone()]);
    let b = bind(&a);
    let m = find("Left Arm Down-Up").unwrap();
    assert_eq!(b.bindings.targets, [Target::Muscle(m)]);
    assert!((b.bindings.defaults[0] - 0.5).abs() < 1e-5);
    let reasons: Vec<_> = b.unbound.iter().map(|u| (u.curve, u.reason)).collect();
    assert_eq!(
        reasons,
        [
            (1, "no such muscle"),
            (2, "no muscle frame for the muscle's bone"),
            (3, "body pose curves not supported yet (#52)"),
        ]
    );

    let mut a = arm(0.5, I, vec![clip]);
    a.humanoid.as_mut().unwrap().muscle_frames.clear();
    let b = bind(&a);
    assert!(b.bindings.is_empty() && b.rig.is_none());
    assert_eq!(b.unbound[0].reason, "no muscle frames (#53)");
}

fn state(clip: u32) -> State {
    State {
        name: format!("s{clip}"),
        motion: Some(Motion::Clip(clip)),
        speed: 1.0,
        speed_param: None,
        cycle_offset: 0.0,
        cycle_offset_param: None,
        time_param: None,
        write_defaults: true,
        transitions: Vec::new(),
        behaviours: Vec::new(),
    }
}

fn layer(clip: u32, mask: Option<AvatarMask>) -> Layer {
    Layer {
        name: format!("l{clip}"),
        group: 0,
        weight: 1.0,
        blending: LayerBlending::Override,
        mask,
        state_machine: StateMachine {
            states: vec![state(clip)],
            default_state: 0,
            entry_transitions: Vec::new(),
            any_state_transitions: Vec::new(),
        },
    }
}

#[test]
fn masks_gate_muscles_by_body_part_and_the_pose_follows_the_muscles() {
    let arm_and_hand = |v| {
        muscle_clip(&[
            (Property::Muscle("Left Arm Down-Up".into()), v),
            (Property::Muscle("Left Hand Down-Up".into()), v),
        ])
    };
    let mut a = arm(0.5, I, vec![arm_and_hand(1.0), arm_and_hand(-1.0)]);
    let only_fingers = AvatarMask {
        body_parts: vec![BodyPart::LeftFingers],
        transforms: None,
    };
    let left_arm = AvatarMask {
        body_parts: vec![BodyPart::LeftArm],
        transforms: Some(Vec::new()),
    };
    a.animator = Some(Animator {
        parameters: Vec::new(),
        groups: vec![LayerGroup {
            name: "All".into(),
            weight: 1.0,
        }],
        layers: vec![layer(0, None), layer(1, Some(only_fingers))],
    });
    let c = Controller::new(&a).unwrap();
    let mut i = Instance::new(&c, true);
    i.update(&c, 0.1);
    // The second layer's mask lacks the left arm, so the first layer's +1 stands.
    let p: HashMap<NodeIndex, Quat> = i.human_pose(&c).into_iter().collect();
    same(p[&2], about(2, 100.0));
    same(p[&4], about(2, 80.0));
    same(p[&1], I);

    // A mask with the left arm and an empty transform list still writes the arm's muscles.
    a.animator.as_mut().unwrap().layers[1].mask = Some(left_arm);
    let c = Controller::new(&a).unwrap();
    let mut i = Instance::new(&c, true);
    i.update(&c, 0.1);
    let p: HashMap<NodeIndex, Quat> = i.human_pose(&c).into_iter().collect();
    same(p[&2], about(2, -60.0));

    // Without muscle curves the bones keep their rotation slots.
    a.clips = vec![muscle_clip(&[])];
    a.animator.as_mut().unwrap().layers.truncate(1);
    let c = Controller::new(&a).unwrap();
    let mut i = Instance::new(&c, true);
    i.update(&c, 0.1);
    assert!(i.human_pose(&c).is_empty());
}
