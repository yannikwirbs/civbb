//! Unity's muscle frames derived from a humanoid's rest pose (#53, ADR 0064), for sources that carry none (glTF,
//! VRM). As recorded from `AvatarBuilder` on reshaped skeletons (`tests/unity/muscle_frames.json`): the post frame's X
//! runs along the bone and its roll follows a fixed avatar axis, and the pre frame is the post frame turned by a
//! fixed avatar-space rotation per bone.

use civbb_core::math::{Pose, conj, cross, dot, mul, normalize, qnormalize, scale, sub, world_poses};
use civbb_format::hierarchy::{Hierarchy, HumanBone, Humanoid, MuscleFrame};
use civbb_format::math::{Quat, Transform, Vec3};

use crate::body::basis;
use crate::muscle::MUSCLES;

use HumanBone as H;

/// Per bone, in our axes: the avatar-space rotation from its post frame to its pre frame (Unity's muscle-zero frame),
/// and Unity's limit signs. Read from the T-pose recording.
const TABLE: [(HumanBone, Quat, Vec3); 55] = [
    (H::Hips, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (H::Spine, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (H::Chest, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (H::UpperChest, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (H::Neck, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (H::Head, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (H::Jaw, [0.089638, 0.0, 0.0, 0.995974], [1.0, -1.0, 1.0]),
    (H::LeftEye, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, 1.0]),
    (H::RightEye, [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, 1.0]),
    (H::LeftShoulder, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, 1.0]),
    (H::LeftUpperArm, [0.0, -0.24421, -0.331689, 0.911232], [1.0, -1.0, 1.0]),
    (H::LeftLowerArm, [0.0, -0.642743, 0.0, 0.766082], [1.0, -1.0, 1.0]),
    (H::LeftHand, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, 1.0]),
    (H::RightShoulder, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (H::RightUpperArm, [0.0, 0.24421, 0.331689, 0.911232], [-1.0, -1.0, -1.0]),
    (H::RightLowerArm, [0.0, 0.642743, 0.0, 0.766082], [-1.0, -1.0, -1.0]),
    (H::RightHand, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (H::LeftUpperLeg, [-0.258865, 0.0, 0.0, 0.965914], [1.0, -1.0, -1.0]),
    (H::LeftLowerLeg, [0.642743, 0.0, 0.0, 0.766082], [1.0, -1.0, 1.0]),
    (H::LeftFoot, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (H::LeftToes, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (H::RightUpperLeg, [-0.258865, 0.0, 0.0, 0.965914], [-1.0, 1.0, -1.0]),
    (H::RightLowerLeg, [0.642743, 0.0, 0.0, 0.766082], [-1.0, -1.0, 1.0]),
    (H::RightFoot, [0.0, 0.0, 0.0, 1.0], [1.0, 1.0, -1.0]),
    (H::RightToes, [0.0, 0.0, 0.0, 1.0], [1.0, -1.0, -1.0]),
    (
        H::LeftThumbProximal,
        [0.0, -0.123091, -0.123092, 0.984732],
        [-1.0, 1.0, -1.0],
    ),
    (
        H::LeftThumbIntermediate,
        [0.0, 0.196116, 0.0, 0.980581],
        [-1.0, -1.0, -1.0],
    ),
    (H::LeftThumbDistal, [0.0, 0.196116, 0.0, 0.980581], [-1.0, -1.0, -1.0]),
    (
        H::LeftIndexProximal,
        [0.0, -0.076402, -0.286508, 0.955027],
        [-1.0, 1.0, 1.0],
    ),
    (
        H::LeftIndexIntermediate,
        [0.0, 0.0, -0.313377, 0.949629],
        [-1.0, -1.0, 1.0],
    ),
    (H::LeftIndexDistal, [0.0, 0.0, -0.313377, 0.949629], [-1.0, -1.0, 1.0]),
    (
        H::LeftMiddleProximal,
        [0.0, -0.038285, -0.287137, 0.957124],
        [-1.0, 1.0, 1.0],
    ),
    (
        H::LeftMiddleIntermediate,
        [0.0, 0.0, -0.313377, 0.949629],
        [-1.0, -1.0, 1.0],
    ),
    (H::LeftMiddleDistal, [0.0, 0.0, -0.313377, 0.949629], [-1.0, -1.0, 1.0]),
    (
        H::LeftRingProximal,
        [0.0, 0.038285, -0.287137, 0.957124],
        [-1.0, -1.0, 1.0],
    ),
    (
        H::LeftRingIntermediate,
        [0.0, 0.0, -0.313377, 0.949629],
        [-1.0, -1.0, 1.0],
    ),
    (H::LeftRingDistal, [0.0, 0.0, -0.313377, 0.949629], [-1.0, -1.0, 1.0]),
    (
        H::LeftLittleProximal,
        [0.0, 0.076402, -0.286508, 0.955027],
        [-1.0, -1.0, 1.0],
    ),
    (
        H::LeftLittleIntermediate,
        [0.0, 0.0, -0.313377, 0.949629],
        [-1.0, -1.0, 1.0],
    ),
    (H::LeftLittleDistal, [0.0, 0.0, -0.313377, 0.949629], [-1.0, -1.0, 1.0]),
    (
        H::RightThumbProximal,
        [0.0, 0.123091, 0.123091, 0.984732],
        [1.0, 1.0, 1.0],
    ),
    (
        H::RightThumbIntermediate,
        [0.0, -0.196116, 0.0, 0.980581],
        [1.0, -1.0, 1.0],
    ),
    (H::RightThumbDistal, [0.0, -0.196116, 0.0, 0.980581], [1.0, -1.0, 1.0]),
    (
        H::RightIndexProximal,
        [0.0, 0.076402, 0.286508, 0.955027],
        [1.0, 1.0, -1.0],
    ),
    (
        H::RightIndexIntermediate,
        [0.0, 0.0, 0.313377, 0.949629],
        [1.0, -1.0, -1.0],
    ),
    (H::RightIndexDistal, [0.0, 0.0, 0.313377, 0.949629], [1.0, -1.0, -1.0]),
    (
        H::RightMiddleProximal,
        [0.0, 0.038285, 0.287137, 0.957124],
        [1.0, 1.0, -1.0],
    ),
    (
        H::RightMiddleIntermediate,
        [0.0, 0.0, 0.313377, 0.949629],
        [1.0, -1.0, -1.0],
    ),
    (H::RightMiddleDistal, [0.0, 0.0, 0.313377, 0.949629], [1.0, -1.0, -1.0]),
    (
        H::RightRingProximal,
        [0.0, -0.038285, 0.287137, 0.957124],
        [1.0, -1.0, -1.0],
    ),
    (
        H::RightRingIntermediate,
        [0.0, 0.0, 0.313377, 0.949629],
        [1.0, -1.0, -1.0],
    ),
    (H::RightRingDistal, [0.0, 0.0, 0.313377, 0.949629], [1.0, -1.0, -1.0]),
    (
        H::RightLittleProximal,
        [0.0, -0.076402, 0.286508, 0.955027],
        [1.0, -1.0, -1.0],
    ),
    (
        H::RightLittleIntermediate,
        [0.0, 0.0, 0.313377, 0.949629],
        [1.0, -1.0, -1.0],
    ),
    (H::RightLittleDistal, [0.0, 0.0, 0.313377, 0.949629], [1.0, -1.0, -1.0]),
];

/// Where a bone's post X points, in our axes (X is Unity's bone direction mirrored, so it points back along the bone).
enum Along {
    /// Back from the first of these bones that is mapped.
    Child(&'static [HumanBone]),
    /// As `Child`, or without one, as `Parent` (finger intermediates without a distal joint).
    ChildOr(&'static [HumanBone], &'static [HumanBone]),
    /// Along the line from the first of these bones that is mapped through the bone.
    Parent(&'static [HumanBone]),
    /// A fixed avatar axis.
    Axis(Vec3),
}

fn along(b: HumanBone) -> Along {
    use Along::*;
    match b {
        H::Hips => Child(&[H::Spine]),
        H::Spine => Child(&[H::Chest, H::UpperChest, H::Neck, H::Head]),
        H::Chest => Child(&[H::UpperChest, H::Neck, H::Head]),
        H::UpperChest => Child(&[H::Neck, H::Head]),
        H::Neck => Child(&[H::Head]),
        H::Head => Axis([0.0, -1.0, 0.0]),
        H::LeftFoot | H::RightFoot => Axis([0.0, 1.0, 0.0]),
        H::Jaw | H::LeftEye | H::RightEye | H::LeftToes | H::RightToes => Axis([0.0, 0.0, -1.0]),
        H::LeftUpperLeg => Child(&[H::LeftLowerLeg]),
        H::LeftLowerLeg => Child(&[H::LeftFoot]),
        H::RightUpperLeg => Child(&[H::RightLowerLeg]),
        H::RightLowerLeg => Child(&[H::RightFoot]),
        H::LeftShoulder => Child(&[H::LeftUpperArm]),
        H::LeftUpperArm => Child(&[H::LeftLowerArm]),
        H::LeftLowerArm => Child(&[H::LeftHand]),
        H::LeftHand => Parent(&[H::LeftLowerArm]),
        H::RightShoulder => Child(&[H::RightUpperArm]),
        H::RightUpperArm => Child(&[H::RightLowerArm]),
        H::RightLowerArm => Child(&[H::RightHand]),
        H::RightHand => Parent(&[H::RightLowerArm]),
        _ => finger(b),
    }
}

/// Finger bones: proximal and intermediate point to the next joint, distal ones continue the intermediate.
fn finger(b: HumanBone) -> Along {
    const FINGERS: [[HumanBone; 3]; 10] = [
        [H::LeftThumbProximal, H::LeftThumbIntermediate, H::LeftThumbDistal],
        [H::LeftIndexProximal, H::LeftIndexIntermediate, H::LeftIndexDistal],
        [H::LeftMiddleProximal, H::LeftMiddleIntermediate, H::LeftMiddleDistal],
        [H::LeftRingProximal, H::LeftRingIntermediate, H::LeftRingDistal],
        [H::LeftLittleProximal, H::LeftLittleIntermediate, H::LeftLittleDistal],
        [H::RightThumbProximal, H::RightThumbIntermediate, H::RightThumbDistal],
        [H::RightIndexProximal, H::RightIndexIntermediate, H::RightIndexDistal],
        [H::RightMiddleProximal, H::RightMiddleIntermediate, H::RightMiddleDistal],
        [H::RightRingProximal, H::RightRingIntermediate, H::RightRingDistal],
        [H::RightLittleProximal, H::RightLittleIntermediate, H::RightLittleDistal],
    ];
    for f in &FINGERS {
        match f.iter().position(|&x| x == b) {
            Some(0) => return Along::Child(&f[1..]),
            Some(1) => return Along::ChildOr(&f[2..], &f[..1]),
            // The intermediate joint.
            Some(_) => return Along::Parent(std::slice::from_ref(&f[1])),
            None => {}
        }
    }
    unreachable!("every human bone is in along()")
}

/// The avatar axis a bone's post frame rolls to, in our axes, and whether it sets Z (else Y) across X.
fn roll(b: HumanBone) -> (bool, Vec3) {
    let name = format!("{b:?}");
    let left = name.starts_with("Left");
    match b {
        H::Hips | H::Spine | H::Chest | H::UpperChest | H::Neck | H::Head => (true, [1.0, 0.0, 0.0]),
        H::LeftShoulder | H::RightShoulder | H::LeftHand | H::RightHand => {
            (false, [0.0, if left { -1.0 } else { 1.0 }, 0.0])
        }
        H::LeftUpperArm | H::RightUpperArm => (true, [0.0, 0.0, 1.0]),
        H::LeftLowerArm | H::RightLowerArm => (true, [0.0, 1.0, 0.0]),
        _ if name.contains("Thumb") => (true, [0.0, 1.0, 0.0]),
        _ if name.contains("Proximal") || name.contains("Intermediate") || name.contains("Distal") => {
            (true, [0.0, 0.0, 1.0])
        }
        // Legs, feet, toes, eyes and jaw.
        _ => (true, [-1.0, 0.0, 0.0]),
    }
}

/// Muscle frames for every mapped bone of `humanoid` from the rest pose, with Unity's default limits and twist
/// shares. A bone whose direction cannot be found (its next bone unmapped or on top of it) gets none.
pub fn derive(hierarchy: &Hierarchy, humanoid: &Humanoid) -> Vec<MuscleFrame> {
    let parents: Vec<_> = hierarchy.nodes.iter().map(|n| n.parent).collect();
    let locals: Vec<Transform> = hierarchy.nodes.iter().map(|n| n.rest).collect();
    let world = world_poses(&parents, Pose::IDENTITY, &locals);
    let pos = |b: HumanBone| humanoid.node(b).map(|n| world[n as usize].pos);
    let mut out = Vec::new();
    for &(bone, g, sign) in &TABLE {
        let Some(node) = humanoid.node(bone) else { continue };
        let at = world[node as usize].pos;
        let x = match along(bone) {
            Along::Child(next) => next.iter().find_map(|&c| pos(c)).map(|c| sub(at, c)),
            Along::ChildOr(next, prev) => (next.iter().find_map(|&c| pos(c)).map(|c| sub(at, c)))
                .or_else(|| prev.iter().find_map(|&p| pos(p)).map(|p| sub(p, at))),
            Along::Parent(prev) => prev.iter().find_map(|&p| pos(p)).map(|p| sub(p, at)),
            Along::Axis(a) => Some(a),
        };
        let Some(x) = x.filter(|x| dot(*x, *x) > 1e-12).map(normalize) else {
            continue;
        };
        let (z_set, r) = roll(bone);
        let across = sub(r, scale(x, dot(r, x)));
        if dot(across, across) < 1e-6 {
            continue;
        }
        let across = normalize(across);
        let frame = if z_set {
            basis(x, cross(across, x), across)
        } else {
            basis(x, across, cross(x, across))
        };
        // The hips' muscle space is in avatar space, every other bone's in its parent node's.
        let parent = match (bone, parents[node as usize]) {
            (H::Hips, _) | (_, None) => Pose::IDENTITY,
            (_, Some(p)) => world[p as usize],
        };
        let (mut min, mut max) = ([0.0; 3], [0.0; 3]);
        for m in MUSCLES.iter().filter(|m| m.bone == bone) {
            min[m.axis as usize] = m.min;
            max[m.axis as usize] = m.max;
        }
        out.push(MuscleFrame {
            bone,
            pre: qnormalize(mul(conj(parent.rot), mul(qnormalize(g), frame))),
            post: qnormalize(mul(conj(world[node as usize].rot), frame)),
            sign,
            min,
            max,
            twist: if bone.twist_child().is_some() { 0.5 } else { 1.0 },
        });
    }
    out
}
