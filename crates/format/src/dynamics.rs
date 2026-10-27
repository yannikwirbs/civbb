//! Bone dynamics (PhysBone-compatible chains and colliders) and contact senders and receivers.

use serde::{Deserialize, Serialize};

use crate::hierarchy::NodeIndex;
use crate::math::{Quat, Vec3};

/// All dynamics of an avatar.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Dynamics {
    /// Simulated bone chains.
    pub chains: Vec<Chain>,
    /// Colliders chains can reference.
    pub colliders: Vec<Collider>,
}

/// A value scaled along the chain by an optional curve over normalised depth (0 at root, 1 at tip).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graded {
    /// Base value.
    pub value: f32,
    /// `(depth, factor)` points, linearly interpolated; empty means a constant factor of 1.
    pub curve: Vec<[f32; 2]>,
}

impl Graded {
    /// A constant value.
    pub fn constant(value: f32) -> Self {
        Self {
            value,
            curve: Vec::new(),
        }
    }
}

/// One simulated chain, mirroring the documented PhysBone parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chain {
    /// Root transform; every descendant not in `ignore` is simulated.
    pub root: NodeIndex,
    /// Excluded subtrees.
    pub ignore: Vec<NodeIndex>,
    /// Virtual tip bone offset added to each leaf, in leaf space.
    pub endpoint: Vec3,
    /// How the root behaves when it has several children.
    pub multi_child: MultiChild,
    /// Integration model.
    pub integration: Integration,
    /// Pull back to rest pose, 0 to 1.
    pub pull: Graded,
    /// Spring (simplified) or momentum (advanced), 0 to 1.
    pub spring: Graded,
    /// Stiffness (advanced only), 0 to 1.
    pub stiffness: Graded,
    /// Gravity, -1 to 1.
    pub gravity: Graded,
    /// How much gravity is reduced at rest pose, 0 to 1.
    pub gravity_falloff: Graded,
    /// Resistance to the avatar's own motion, 0 to 1.
    pub immobile: Graded,
    /// What `immobile` resists.
    pub immobile_type: ImmobileType,
    /// Rotation limit.
    pub limit: Limit,
    /// Collision radius.
    pub radius: Graded,
    /// Indices into [`Dynamics::colliders`].
    pub colliders: Vec<u32>,
    /// Allowed stretch as a fraction of bone length.
    pub max_stretch: Graded,
    /// Allowed squish as a fraction of bone length.
    pub max_squish: Graded,
    /// Grabbing allowed.
    pub allow_grab: bool,
    /// Posing (leaving grabbed bones in place) allowed.
    pub allow_posing: bool,
    /// Animator parameter prefix receiving `_IsGrabbed`, `_Angle`, `_Stretch` etc.
    pub parameter: Option<String>,
}

/// Root behaviour with several children.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultiChild {
    Ignore,
    First,
    Average,
}

/// Integration models.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Integration {
    Simplified,
    Advanced,
}

/// What `immobile` measures motion against.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImmobileType {
    AllMotion,
    World,
}

/// Rotation limits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Limit {
    /// Unlimited.
    None,
    /// Cone around the rest direction.
    Angle {
        /// Max angle in degrees.
        max_angle: Graded,
        /// Limit frame rotation, Euler degrees.
        rotation: Vec3,
    },
    /// Plane of rotation.
    Hinge {
        /// Max angle in degrees.
        max_angle: Graded,
        /// Limit frame rotation, Euler degrees.
        rotation: Vec3,
    },
    /// Elliptical cone.
    Polar {
        /// Max pitch in degrees.
        max_pitch: Graded,
        /// Max yaw in degrees.
        max_yaw: Graded,
        /// Limit frame rotation, Euler degrees.
        rotation: Vec3,
    },
}

/// A collider attached to a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Collider {
    /// Parent node.
    pub node: NodeIndex,
    /// Shape.
    pub shape: Shape,
    /// Offset from the node.
    pub position: Vec3,
    /// Rotation relative to the node.
    pub rotation: Quat,
    /// Keep bones inside the shape instead of outside.
    pub inside: bool,
}

/// Collision and contact shapes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Shape {
    /// Sphere.
    Sphere {
        /// Radius.
        radius: f32,
    },
    /// Capsule along local Y.
    Capsule {
        /// Radius.
        radius: f32,
        /// Total height including caps.
        height: f32,
    },
    /// Infinite plane with normal along local Y.
    Plane,
}

/// Contact senders and receivers (design doc §7).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Contacts {
    /// Senders.
    pub senders: Vec<ContactSender>,
    /// Receivers.
    pub receivers: Vec<ContactReceiver>,
}

/// A shape on a node, shared by senders and receivers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContactVolume {
    /// Parent node.
    pub node: NodeIndex,
    /// Sphere or capsule.
    pub shape: Shape,
    /// Offset from the node.
    pub position: Vec3,
    /// Rotation relative to the node.
    pub rotation: Quat,
}

/// Emits tags other avatars' receivers can detect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContactSender {
    /// Volume.
    pub volume: ContactVolume,
    /// Collision tags.
    pub tags: Vec<String>,
}

/// Detects senders with matching tags and writes an animator parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContactReceiver {
    /// Volume.
    pub volume: ContactVolume,
    /// Accepted tags; any match counts.
    pub tags: Vec<String>,
    /// Animator parameter written.
    pub parameter: String,
    /// How the parameter is driven.
    pub kind: ReceiverKind,
    /// Senders on the same avatar count.
    pub allow_self: bool,
    /// Senders on other avatars count.
    pub allow_others: bool,
    /// Evaluate only for the avatar's owner.
    pub local_only: bool,
    /// Minimum sender speed for `OnEnter`.
    pub min_velocity: f32,
}

/// Receiver types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiverKind {
    /// 1 while any sender overlaps.
    Constant,
    /// 1 for one frame when a sender enters.
    OnEnter,
    /// 0 to 1 by how deep the nearest sender is.
    Proximity,
}
