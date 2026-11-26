//! Bone dynamics (PhysBone-compatible chains and colliders) and contact senders and receivers.

use serde::{Deserialize, Serialize};

use crate::hierarchy::NodeIndex;
use crate::math::{Quat, Vec3};
use crate::state::EntryIndex;

/// All dynamics of an avatar.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dynamics {
    /// Simulated bone chains.
    pub chains: Vec<Chain>,
    /// Colliders chains can reference.
    pub colliders: Vec<Collider>,
}

/// A value scaled along the chain by an optional curve over normalised depth (0 at root, 1 at tip).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct Chain {
    /// Root transform; every descendant not in `ignore` is simulated.
    pub root: NodeIndex,
    /// Excluded subtrees.
    pub ignore: Vec<NodeIndex>,
    /// Virtual tip bone offset added to each leaf, in leaf space.
    pub endpoint: Vec3,
    /// How the root behaves when it has several children.
    pub multi_child: MultiChild,
    /// Solver version: 1.1 bends the rest direction towards gravity, 1.0 pulls with a force.
    pub version: Version,
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
    /// State entries the chain writes every frame.
    pub outputs: ChainOutputs,
}

/// The state entries a chain writes after each frame (ADR 0088); `None` writes nothing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainOutputs {
    /// 1 while any bone is grabbed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_grabbed: Option<EntryIndex>,
    /// 1 while the chain is posed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_posed: Option<EntryIndex>,
    /// The tip bone's angle from its rest direction, 0 to 1 for 0 to 180 degrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub angle: Option<EntryIndex>,
    /// How close the most stretched bone is to its max stretch, 0 to 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stretch: Option<EntryIndex>,
    /// How close the most squished bone is to its max squish, 0 to 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub squish: Option<EntryIndex>,
}

impl ChainOutputs {
    /// The entries in field order: grabbed, posed, angle, stretch, squish.
    pub fn entries(&self) -> [Option<EntryIndex>; 5] {
        [self.is_grabbed, self.is_posed, self.angle, self.stretch, self.squish]
    }
}

/// Root behaviour with several children.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiChild {
    Ignore,
    First,
    Average,
}

/// PhysBone versions, `1.0` and `1.1` as the VRChat SDK numbers them.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Version {
    #[serde(rename = "1.0")]
    V1_0,
    #[serde(rename = "1.1")]
    V1_1,
}

/// Integration models.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Integration {
    Simplified,
    Advanced,
}

/// What `immobile` measures motion against.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImmobileType {
    AllMotion,
    World,
}

/// Rotation limits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct Contacts {
    /// Senders.
    pub senders: Vec<ContactSender>,
    /// Receivers.
    pub receivers: Vec<ContactReceiver>,
}

/// Contact shapes (ADR 0017, 0047).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContactShape {
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
    /// Box centred on the volume's position.
    Box {
        /// Edge lengths along local X, Y and Z.
        size: Vec3,
    },
}

/// A shape on a node, shared by senders and receivers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactVolume {
    /// Node it is placed on (VRChat's *Root Transform*).
    pub node: NodeIndex,
    /// Node whose activity enables the contact (the component's GameObject) when it is not `node` (ADR 0047).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<NodeIndex>,
    /// Sphere, capsule or box.
    pub shape: ContactShape,
    /// Offset from the node.
    pub position: Vec3,
    /// Rotation relative to the node.
    pub rotation: Quat,
}

/// Emits tags other avatars' receivers can detect.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactSender {
    /// Volume.
    pub volume: ContactVolume,
    /// Collision tags.
    pub tags: Vec<String>,
    /// Exists only on the avatar owner's client.
    pub local_only: bool,
}

/// Detects senders with matching tags and writes an animator parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactReceiver {
    /// Volume.
    pub volume: ContactVolume,
    /// Accepted tags; any match counts.
    pub tags: Vec<String>,
    /// Entry written; `None` writes nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<EntryIndex>,
    /// How the parameter is driven.
    pub kind: ReceiverKind,
    /// Senders on the same avatar count.
    pub allow_self: bool,
    /// Senders on other avatars count.
    pub allow_others: bool,
    /// Exists only on the avatar owner's client.
    pub local_only: bool,
    /// Minimum sender speed relative to the receiver for `OnEnter`, metres per second.
    pub min_velocity: f32,
    /// Box receivers: proximity towards the +Z face instead of the centre (*Use Face Proximity*, ADR 0047).
    #[serde(default)]
    pub face: bool,
}

/// Receiver types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiverKind {
    /// 1 while any sender overlaps.
    Constant,
    /// 1 for one frame when a sender enters.
    OnEnter,
    /// 0 to 1 by how deep the nearest sender is.
    Proximity,
}
