//! Node hierarchy (bones and other transforms) and the humanoid bone mapping.

use serde::{Deserialize, Serialize};

use crate::math::{Quat, Transform, Vec3};

/// Index into [`Hierarchy::nodes`].
pub type NodeIndex = u32;

/// A transform node: a bone, a toggleable object, or both.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    /// Name, unique among siblings. Animation bindings address nodes by their path of names.
    pub name: String,
    /// Parent node; `None` only for the root. A parent always has a lower index than its children.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<NodeIndex>,
    /// Local rest transform relative to the parent.
    pub rest: Transform,
    /// Whether the node starts active. Inactive nodes hide everything below them.
    pub active: bool,
}

/// All nodes of an avatar, parents before children; node 0 is the root.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hierarchy {
    /// Nodes in parent-first order.
    pub nodes: Vec<Node>,
}

/// Humanoid bones: VRM 1.0's humanoid bone set and names (ADR 0085 point 6), serialised as VRM writes them
/// (`leftThumbMetacarpal`). Importers map other names (Unity's `HumanBodyBones`) by table.
#[allow(missing_docs)] // The names are the documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    LeftThumbMetacarpal,
    LeftThumbProximal,
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
    RightThumbMetacarpal,
    RightThumbProximal,
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

/// Maps humanoid bones to nodes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Humanoid {
    /// Each mapped humanoid bone and its node. A bone appears at most once.
    pub bones: Vec<BoneNode>,
    /// Muscle frames of mapped bones, at most one each; empty when the source has none (glTF and VRM, #53).
    pub muscle_frames: Vec<MuscleFrame>,
    /// Metres per body position unit (Unity's `Animator.humanScale`, the T-pose's centre-of-mass height; ADR 0063);
    /// 0 when the source has none.
    pub human_scale: f32,
    /// Unity's centre of mass of the rest pose in avatar space (`bodyPosition * humanScale`), which anchors the
    /// runtime's mass model (ADR 0063); `None` when the source has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center_of_mass: Option<Vec3>,
}

/// One humanoid bone and the node it maps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoneNode {
    /// The bone.
    pub bone: HumanBone,
    /// Its node.
    pub node: NodeIndex,
}

/// How muscle values turn one humanoid bone (Unity's per-bone axes, ADR 0014). With angles `a` in degrees from the
/// bone's muscles, its local rotation is `pre * zyroll(a) * post⁻¹`, where `zyroll` twists about X, then swings
/// about Y and Z.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
        self.bones.iter().find(|b| b.bone == bone).map(|b| b.node)
    }
}

impl HumanBone {
    /// Every bone, in declaration order.
    pub const ALL: [HumanBone; 55] = {
        use HumanBone as H;
        [
            H::Hips,
            H::Spine,
            H::Chest,
            H::UpperChest,
            H::Neck,
            H::Head,
            H::Jaw,
            H::LeftEye,
            H::RightEye,
            H::LeftShoulder,
            H::LeftUpperArm,
            H::LeftLowerArm,
            H::LeftHand,
            H::RightShoulder,
            H::RightUpperArm,
            H::RightLowerArm,
            H::RightHand,
            H::LeftUpperLeg,
            H::LeftLowerLeg,
            H::LeftFoot,
            H::LeftToes,
            H::RightUpperLeg,
            H::RightLowerLeg,
            H::RightFoot,
            H::RightToes,
            H::LeftThumbMetacarpal,
            H::LeftThumbProximal,
            H::LeftThumbDistal,
            H::LeftIndexProximal,
            H::LeftIndexIntermediate,
            H::LeftIndexDistal,
            H::LeftMiddleProximal,
            H::LeftMiddleIntermediate,
            H::LeftMiddleDistal,
            H::LeftRingProximal,
            H::LeftRingIntermediate,
            H::LeftRingDistal,
            H::LeftLittleProximal,
            H::LeftLittleIntermediate,
            H::LeftLittleDistal,
            H::RightThumbMetacarpal,
            H::RightThumbProximal,
            H::RightThumbDistal,
            H::RightIndexProximal,
            H::RightIndexIntermediate,
            H::RightIndexDistal,
            H::RightMiddleProximal,
            H::RightMiddleIntermediate,
            H::RightMiddleDistal,
            H::RightRingProximal,
            H::RightRingIntermediate,
            H::RightRingDistal,
            H::RightLittleProximal,
            H::RightLittleIntermediate,
            H::RightLittleDistal,
        ]
    };

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
