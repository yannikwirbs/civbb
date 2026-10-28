//! Node hierarchy (bones and other transforms) and the humanoid bone mapping.

use serde::{Deserialize, Serialize};

use crate::math::{Quat, Transform, Vec3};

/// Index into [`Hierarchy::nodes`].
pub type NodeIndex = u32;

/// A transform node: a bone, a toggleable object, or both.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Name, unique among siblings. Animation bindings address nodes by their path of names.
    pub name: String,
    /// Parent node; `None` only for the root. A parent always has a lower index than its children.
    pub parent: Option<NodeIndex>,
    /// Local rest transform relative to the parent.
    pub rest: Transform,
    /// Whether the node starts active. Inactive nodes hide everything below them.
    pub active: bool,
}

/// All nodes of an avatar, parents before children; node 0 is the root.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hierarchy {
    /// Nodes in parent-first order.
    pub nodes: Vec<Node>,
}

/// Humanoid bones, matching the Unity/VRM humanoid set so legacy animations retarget unchanged.
#[allow(missing_docs)] // The names are the documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum HumanBone {
    Hips,
    Spine,
    Chest,
    UpperChest,
    Neck,
    Head,
    Jaw,
    LeftEye,
    RightEye,
    LeftShoulder,
    LeftUpperArm,
    LeftLowerArm,
    LeftHand,
    RightShoulder,
    RightUpperArm,
    RightLowerArm,
    RightHand,
    LeftUpperLeg,
    LeftLowerLeg,
    LeftFoot,
    LeftToes,
    RightUpperLeg,
    RightLowerLeg,
    RightFoot,
    RightToes,
    LeftThumbProximal,
    LeftThumbIntermediate,
    LeftThumbDistal,
    LeftIndexProximal,
    LeftIndexIntermediate,
    LeftIndexDistal,
    LeftMiddleProximal,
    LeftMiddleIntermediate,
    LeftMiddleDistal,
    LeftRingProximal,
    LeftRingIntermediate,
    LeftRingDistal,
    LeftLittleProximal,
    LeftLittleIntermediate,
    LeftLittleDistal,
    RightThumbProximal,
    RightThumbIntermediate,
    RightThumbDistal,
    RightIndexProximal,
    RightIndexIntermediate,
    RightIndexDistal,
    RightMiddleProximal,
    RightMiddleIntermediate,
    RightMiddleDistal,
    RightRingProximal,
    RightRingIntermediate,
    RightRingDistal,
    RightLittleProximal,
    RightLittleIntermediate,
    RightLittleDistal,
}

/// Maps humanoid bones to nodes and holds the avatar's viewpoint.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Humanoid {
    /// Each mapped humanoid bone and its node. A bone appears at most once.
    pub bones: Vec<(HumanBone, NodeIndex)>,
    /// Eye position in avatar space, used for first-person view and eye-height calibration.
    pub viewpoint: Vec3,
    /// Muscle frames of mapped bones, at most one each; empty when the source has none (glTF and VRM, #53).
    pub muscle_frames: Vec<MuscleFrame>,
}

/// How muscle values turn one humanoid bone (Unity's per-bone axes, ADR 0014). With angles `a` in degrees from the
/// bone's muscles, its local rotation is `pre * zyroll(a) * post⁻¹`, where `zyroll` twists about X, then swings
/// about Y and Z.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MuscleFrame {
    /// The bone.
    pub bone: HumanBone,
    /// Muscle space to the parent node's space.
    pub pre: Quat,
    /// Muscle space to the bone node's space.
    pub post: Quat,
    /// Per axis (X twist, Y, Z), the sign applied to muscle angles: 1 or -1.
    pub sign: Vec3,
    /// Per axis, the angle at muscle -1 in degrees (zero or negative).
    pub min: Vec3,
    /// Per axis, the angle at muscle +1 in degrees (zero or positive).
    pub max: Vec3,
    /// Share of the X twist the bone keeps; the rest turns the next bone of the limb about the same axis. Unity's
    /// arm and leg twist settings (default 0.5) on upper and lower arms and legs, 1 on every other bone.
    pub twist: f32,
}

impl Humanoid {
    /// The node mapped to `bone`, if any.
    pub fn node(&self, bone: HumanBone) -> Option<NodeIndex> {
        self.bones.iter().find(|(b, _)| *b == bone).map(|(_, n)| *n)
    }
}

impl HumanBone {
    /// The bone that takes the rest of this bone's twist ([`MuscleFrame::twist`]), for the four limb bones that
    /// split their twist.
    pub fn twist_child(self) -> Option<HumanBone> {
        use HumanBone as H;
        Some(match self {
            H::LeftUpperArm => H::LeftLowerArm,
            H::LeftLowerArm => H::LeftHand,
            H::RightUpperArm => H::RightLowerArm,
            H::RightLowerArm => H::RightHand,
            H::LeftUpperLeg => H::LeftLowerLeg,
            H::LeftLowerLeg => H::LeftFoot,
            H::RightUpperLeg => H::RightLowerLeg,
            H::RightLowerLeg => H::RightFoot,
            _ => return None,
        })
    }
}
