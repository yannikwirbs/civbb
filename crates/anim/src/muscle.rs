//! Humanoid muscles: Unity's muscle table, and muscle values to human bones' local rotations (ADR 0014).

use civbb_format::hierarchy::{HumanBone, Humanoid, MuscleFrame, NodeIndex};
use civbb_format::math::Quat;

use crate::clip::normalize;
use crate::mix::{conj, mul};

/// One Unity muscle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Muscle {
    /// Clip property name, e.g. `Left Arm Down-Up` or `LeftHand.Index.1 Stretched`.
    pub name: &'static str,
    /// The bone it turns.
    pub bone: HumanBone,
    /// Axis of the bone's muscle frame: 0 twist (X), 1 Y, 2 Z.
    pub axis: u8,
    /// Unity's default angle at -1, degrees.
    pub min: f32,
    /// Unity's default angle at +1, degrees.
    pub max: f32,
}

/// Number of muscles.
pub const COUNT: usize = 95;

const fn m(name: &'static str, bone: HumanBone, axis: u8, min: f32, max: f32) -> Muscle {
    Muscle {
        name,
        bone,
        axis,
        min,
        max,
    }
}

use HumanBone as H;

/// Unity's muscles in Unity's order (`HumanTrait.MuscleName`), as recorded in `tests/unity/muscles.json`.
pub const MUSCLES: [Muscle; COUNT] = [
    m("Spine Front-Back", H::Spine, 2, -40.0, 40.0),
    m("Spine Left-Right", H::Spine, 1, -40.0, 40.0),
    m("Spine Twist Left-Right", H::Spine, 0, -40.0, 40.0),
    m("Chest Front-Back", H::Chest, 2, -40.0, 40.0),
    m("Chest Left-Right", H::Chest, 1, -40.0, 40.0),
    m("Chest Twist Left-Right", H::Chest, 0, -40.0, 40.0),
    m("UpperChest Front-Back", H::UpperChest, 2, -20.0, 20.0),
    m("UpperChest Left-Right", H::UpperChest, 1, -20.0, 20.0),
    m("UpperChest Twist Left-Right", H::UpperChest, 0, -20.0, 20.0),
    m("Neck Nod Down-Up", H::Neck, 2, -40.0, 40.0),
    m("Neck Tilt Left-Right", H::Neck, 1, -40.0, 40.0),
    m("Neck Turn Left-Right", H::Neck, 0, -40.0, 40.0),
    m("Head Nod Down-Up", H::Head, 2, -40.0, 40.0),
    m("Head Tilt Left-Right", H::Head, 1, -40.0, 40.0),
    m("Head Turn Left-Right", H::Head, 0, -40.0, 40.0),
    m("Left Eye Down-Up", H::LeftEye, 2, -10.0, 15.0),
    m("Left Eye In-Out", H::LeftEye, 1, -20.0, 20.0),
    m("Right Eye Down-Up", H::RightEye, 2, -10.0, 15.0),
    m("Right Eye In-Out", H::RightEye, 1, -20.0, 20.0),
    m("Jaw Close", H::Jaw, 2, -10.0, 10.0),
    m("Jaw Left-Right", H::Jaw, 1, -10.0, 10.0),
    m("Left Upper Leg Front-Back", H::LeftUpperLeg, 2, -90.0, 50.0),
    m("Left Upper Leg In-Out", H::LeftUpperLeg, 1, -60.0, 60.0),
    m("Left Upper Leg Twist In-Out", H::LeftUpperLeg, 0, -60.0, 60.0),
    m("Left Lower Leg Stretch", H::LeftLowerLeg, 2, -80.0, 80.0),
    m("Left Lower Leg Twist In-Out", H::LeftLowerLeg, 0, -90.0, 90.0),
    m("Left Foot Up-Down", H::LeftFoot, 2, -50.0, 50.0),
    m("Left Foot Twist In-Out", H::LeftFoot, 1, -30.0, 30.0),
    m("Left Toes Up-Down", H::LeftToes, 2, -50.0, 50.0),
    m("Right Upper Leg Front-Back", H::RightUpperLeg, 2, -90.0, 50.0),
    m("Right Upper Leg In-Out", H::RightUpperLeg, 1, -60.0, 60.0),
    m("Right Upper Leg Twist In-Out", H::RightUpperLeg, 0, -60.0, 60.0),
    m("Right Lower Leg Stretch", H::RightLowerLeg, 2, -80.0, 80.0),
    m("Right Lower Leg Twist In-Out", H::RightLowerLeg, 0, -90.0, 90.0),
    m("Right Foot Up-Down", H::RightFoot, 2, -50.0, 50.0),
    m("Right Foot Twist In-Out", H::RightFoot, 1, -30.0, 30.0),
    m("Right Toes Up-Down", H::RightToes, 2, -50.0, 50.0),
    m("Left Shoulder Down-Up", H::LeftShoulder, 2, -15.0, 30.0),
    m("Left Shoulder Front-Back", H::LeftShoulder, 1, -15.0, 15.0),
    m("Left Arm Down-Up", H::LeftUpperArm, 2, -60.0, 100.0),
    m("Left Arm Front-Back", H::LeftUpperArm, 1, -100.0, 100.0),
    m("Left Arm Twist In-Out", H::LeftUpperArm, 0, -90.0, 90.0),
    m("Left Forearm Stretch", H::LeftLowerArm, 2, -80.0, 80.0),
    m("Left Forearm Twist In-Out", H::LeftLowerArm, 0, -90.0, 90.0),
    m("Left Hand Down-Up", H::LeftHand, 2, -80.0, 80.0),
    m("Left Hand In-Out", H::LeftHand, 1, -40.0, 40.0),
    m("Right Shoulder Down-Up", H::RightShoulder, 2, -15.0, 30.0),
    m("Right Shoulder Front-Back", H::RightShoulder, 1, -15.0, 15.0),
    m("Right Arm Down-Up", H::RightUpperArm, 2, -60.0, 100.0),
    m("Right Arm Front-Back", H::RightUpperArm, 1, -100.0, 100.0),
    m("Right Arm Twist In-Out", H::RightUpperArm, 0, -90.0, 90.0),
    m("Right Forearm Stretch", H::RightLowerArm, 2, -80.0, 80.0),
    m("Right Forearm Twist In-Out", H::RightLowerArm, 0, -90.0, 90.0),
    m("Right Hand Down-Up", H::RightHand, 2, -80.0, 80.0),
    m("Right Hand In-Out", H::RightHand, 1, -40.0, 40.0),
    m("LeftHand.Thumb.1 Stretched", H::LeftThumbProximal, 2, -20.0, 20.0),
    m("LeftHand.Thumb.Spread", H::LeftThumbProximal, 1, -25.0, 25.0),
    m("LeftHand.Thumb.2 Stretched", H::LeftThumbIntermediate, 2, -40.0, 35.0),
    m("LeftHand.Thumb.3 Stretched", H::LeftThumbDistal, 2, -40.0, 35.0),
    m("LeftHand.Index.1 Stretched", H::LeftIndexProximal, 2, -50.0, 50.0),
    m("LeftHand.Index.Spread", H::LeftIndexProximal, 1, -20.0, 20.0),
    m("LeftHand.Index.2 Stretched", H::LeftIndexIntermediate, 2, -45.0, 45.0),
    m("LeftHand.Index.3 Stretched", H::LeftIndexDistal, 2, -45.0, 45.0),
    m("LeftHand.Middle.1 Stretched", H::LeftMiddleProximal, 2, -50.0, 50.0),
    m("LeftHand.Middle.Spread", H::LeftMiddleProximal, 1, -7.5, 7.5),
    m("LeftHand.Middle.2 Stretched", H::LeftMiddleIntermediate, 2, -45.0, 45.0),
    m("LeftHand.Middle.3 Stretched", H::LeftMiddleDistal, 2, -45.0, 45.0),
    m("LeftHand.Ring.1 Stretched", H::LeftRingProximal, 2, -50.0, 50.0),
    m("LeftHand.Ring.Spread", H::LeftRingProximal, 1, -7.5, 7.5),
    m("LeftHand.Ring.2 Stretched", H::LeftRingIntermediate, 2, -45.0, 45.0),
    m("LeftHand.Ring.3 Stretched", H::LeftRingDistal, 2, -45.0, 45.0),
    m("LeftHand.Little.1 Stretched", H::LeftLittleProximal, 2, -50.0, 50.0),
    m("LeftHand.Little.Spread", H::LeftLittleProximal, 1, -20.0, 20.0),
    m("LeftHand.Little.2 Stretched", H::LeftLittleIntermediate, 2, -45.0, 45.0),
    m("LeftHand.Little.3 Stretched", H::LeftLittleDistal, 2, -45.0, 45.0),
    m("RightHand.Thumb.1 Stretched", H::RightThumbProximal, 2, -20.0, 20.0),
    m("RightHand.Thumb.Spread", H::RightThumbProximal, 1, -25.0, 25.0),
    m("RightHand.Thumb.2 Stretched", H::RightThumbIntermediate, 2, -40.0, 35.0),
    m("RightHand.Thumb.3 Stretched", H::RightThumbDistal, 2, -40.0, 35.0),
    m("RightHand.Index.1 Stretched", H::RightIndexProximal, 2, -50.0, 50.0),
    m("RightHand.Index.Spread", H::RightIndexProximal, 1, -20.0, 20.0),
    m("RightHand.Index.2 Stretched", H::RightIndexIntermediate, 2, -45.0, 45.0),
    m("RightHand.Index.3 Stretched", H::RightIndexDistal, 2, -45.0, 45.0),
    m("RightHand.Middle.1 Stretched", H::RightMiddleProximal, 2, -50.0, 50.0),
    m("RightHand.Middle.Spread", H::RightMiddleProximal, 1, -7.5, 7.5),
    m(
        "RightHand.Middle.2 Stretched",
        H::RightMiddleIntermediate,
        2,
        -45.0,
        45.0,
    ),
    m("RightHand.Middle.3 Stretched", H::RightMiddleDistal, 2, -45.0, 45.0),
    m("RightHand.Ring.1 Stretched", H::RightRingProximal, 2, -50.0, 50.0),
    m("RightHand.Ring.Spread", H::RightRingProximal, 1, -7.5, 7.5),
    m("RightHand.Ring.2 Stretched", H::RightRingIntermediate, 2, -45.0, 45.0),
    m("RightHand.Ring.3 Stretched", H::RightRingDistal, 2, -45.0, 45.0),
    m("RightHand.Little.1 Stretched", H::RightLittleProximal, 2, -50.0, 50.0),
    m("RightHand.Little.Spread", H::RightLittleProximal, 1, -20.0, 20.0),
    m(
        "RightHand.Little.2 Stretched",
        H::RightLittleIntermediate,
        2,
        -45.0,
        45.0,
    ),
    m("RightHand.Little.3 Stretched", H::RightLittleDistal, 2, -45.0, 45.0),
];

/// The muscle a clip property names.
pub fn find(name: &str) -> Option<u8> {
    MUSCLES.iter().position(|m| m.name == name).map(|i| i as u8)
}

/// An avatar's muscle frames ready for evaluation, parents first, with its rest pose in muscle values.
#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    joints: Vec<Joint>,
    /// Muscle values of the rest pose; muscles of bones without a frame are 0.
    pub rest: [f32; COUNT],
}

#[derive(Clone, Debug, PartialEq)]
struct Joint {
    node: NodeIndex,
    frame: MuscleFrame,
    /// The muscle on each axis.
    muscles: [Option<u8>; 3],
    /// The joint whose remaining twist turns this one.
    twist_from: Option<usize>,
}

impl Rig {
    /// The rig of a validated humanoid, with `rest(node)` giving a node's rest rotation; `None` without frames.
    pub fn new(h: &Humanoid, rest: impl Fn(NodeIndex) -> Quat) -> Option<Self> {
        if h.muscle_frames.is_empty() {
            return None;
        }
        let mut frames: Vec<(NodeIndex, &MuscleFrame)> = h
            .muscle_frames
            .iter()
            .map(|f| (h.node(f.bone).expect("validated"), f))
            .collect();
        // Parents have lower node indices than their children.
        frames.sort_by_key(|(n, _)| *n);
        let mut joints: Vec<Joint> = frames
            .iter()
            .map(|&(node, f)| Joint {
                node,
                frame: f.clone(),
                muscles: [0, 1, 2].map(|axis| {
                    MUSCLES
                        .iter()
                        .position(|m| m.bone == f.bone && m.axis == axis)
                        .map(|i| i as u8)
                }),
                twist_from: None,
            })
            .collect();
        for j in 0..joints.len() {
            let bone = joints[j].frame.bone;
            joints[j].twist_from = joints.iter().position(|p| p.frame.bone.twist_child() == Some(bone));
        }
        let mut rig = Self {
            joints,
            rest: [0.0; COUNT],
        };
        rig.rest = rig.muscles(rest);
        Some(rig)
    }

    /// Whether `bone` has a muscle frame.
    pub fn has(&self, bone: HumanBone) -> bool {
        self.joints.iter().any(|j| j.frame.bone == bone)
    }

    /// Each framed bone's local rotation for these muscle values, parents first.
    pub fn rotations(&self, muscles: &[f32; COUNT]) -> Vec<(NodeIndex, Quat)> {
        let mut twist = vec![0.0; self.joints.len()];
        let mut out = Vec::with_capacity(self.joints.len());
        for (j, joint) in self.joints.iter().enumerate() {
            let f = &joint.frame;
            let mut a = [0.0; 3];
            for (k, m) in joint.muscles.iter().enumerate() {
                if let Some(m) = m {
                    let v = muscles[*m as usize];
                    a[k] = f.sign[k] * v * if v < 0.0 { -f.min[k] } else { f.max[k] };
                }
            }
            twist[j] = a[0];
            a[0] *= f.twist;
            let mut q = mul(mul(f.pre, zyroll(a)), conj(f.post));
            if let Some(p) = joint.twist_from {
                q = mul(self.passed_twist(p, twist[p]), q);
            }
            out.push((joint.node, q));
        }
        out
    }

    /// The muscle values that give the framed bones these local rotations: the inverse of [`Rig::rotations`].
    pub fn muscles(&self, local: impl Fn(NodeIndex) -> Quat) -> [f32; COUNT] {
        let mut twist = vec![0.0; self.joints.len()];
        let mut out = [0.0; COUNT];
        for (j, joint) in self.joints.iter().enumerate() {
            let f = &joint.frame;
            let mut q = local(joint.node);
            if let Some(p) = joint.twist_from {
                q = mul(conj(self.passed_twist(p, twist[p])), q);
            }
            let [x, y, z, w] = mul(mul(conj(f.pre), q), f.post);
            let (x, y, z, w) = if w < 0.0 { (-x, -y, -z, -w) } else { (x, y, z, w) };
            // Half-angle tangents of zyroll, solved for its three angles.
            let d = w * w + x * x;
            let mut a = [x.atan2(w), (y * w - x * z).atan2(d), (z * w + x * y).atan2(d)].map(|h| 2.0 * h.to_degrees());
            if f.twist > 0.0 {
                a[0] /= f.twist;
            }
            twist[j] = a[0];
            for (k, m) in joint.muscles.iter().enumerate() {
                if let Some(m) = m {
                    let s = f.sign[k] * a[k];
                    let range = if s < 0.0 { -f.min[k] } else { f.max[k] };
                    out[*m as usize] = if range > 0.0 { s / range } else { 0.0 };
                }
            }
        }
        out
    }

    /// The rotation, in joint `p`'s parent space, that turns its twist child by the twist share `p` passes on.
    fn passed_twist(&self, p: usize, total: f32) -> Quat {
        let f = &self.joints[p].frame;
        let rest = (1.0 - f.twist) * total;
        mul(mul(f.post, zyroll([rest, 0.0, 0.0])), conj(f.post))
    }
}

/// Twist about X by `a[0]`, then swing about Y and Z, from half-angle tangents as Unity's muscle space does:
/// `normalize(tx, ty + tx tz, tz - tx ty, 1)` is the swing `(0, ty, tz, 1)` times the twist `(tx, 0, 0, 1)`.
fn zyroll(a: [f32; 3]) -> Quat {
    let [tx, ty, tz] = a.map(|d| (d.to_radians() * 0.5).tan());
    normalize([tx, ty + tx * tz, tz - tx * ty, 1.0])
}
