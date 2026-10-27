//! Node hierarchy (bones and other transforms) and the humanoid bone mapping.

use serde::{Deserialize, Serialize};

use crate::math::{Transform, Vec3};

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
}

impl Humanoid {
    /// The node mapped to `bone`, if any.
    pub fn node(&self, bone: HumanBone) -> Option<NodeIndex> {
        self.bones.iter().find(|(b, _)| *b == bone).map(|(_, n)| *n)
    }
}
