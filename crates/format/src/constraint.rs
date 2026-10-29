//! Transform constraints (position, rotation, scale, parent, aim, look-at) with Unity's semantics (ADR 0018).

use serde::{Deserialize, Serialize};

use crate::hierarchy::NodeIndex;
use crate::math::{Transform, Vec3};

/// A constraint. The runtime orders an avatar's constraints by dependency when it loads them; list order breaks ties.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    /// Constraint type.
    pub kind: ConstraintKind,
    /// The constrained node.
    pub target: NodeIndex,
    /// Sources with their weights.
    pub sources: Vec<ConstraintSource>,
    /// Overall weight, 0 to 1.
    pub weight: f32,
    /// Enabled at start.
    pub active: bool,
    /// Constrained axes (x, y, z) of position, rotation (Euler) or scale; a parent constraint's position axes. Other
    /// axes keep the animated value.
    pub axes: [bool; 3],
    /// Local at-rest values `weight` blends from (the parts the kind constrains).
    pub rest: Transform,
}

/// One constraint source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConstraintSource {
    /// Source node.
    pub node: NodeIndex,
    /// Weight.
    pub weight: f32,
    /// Offset applied relative to this source (parent constraints).
    pub offset: Transform,
}

/// Constraint types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConstraintKind {
    /// Copies the weighted source position, plus `offset`.
    Position {
        /// Position offset.
        offset: Vec3,
    },
    /// Copies the weighted source rotation, plus `offset` (Euler degrees).
    Rotation {
        /// Rotation offset.
        offset: Vec3,
    },
    /// Copies the weighted source scale, times `offset`.
    Scale {
        /// Scale factor.
        offset: Vec3,
    },
    /// Follows sources as if parented, with per-source offsets.
    Parent {
        /// Constrained rotation axes (Euler x, y, z).
        rotation_axes: [bool; 3],
    },
    /// Points `aim_axis` at the weighted source position.
    Aim {
        /// Rotation offset (Euler degrees), applied in local space.
        offset: Vec3,
        /// Local axis that points at the target.
        aim_axis: Vec3,
        /// Local up axis.
        up_axis: Vec3,
        /// World up reference.
        world_up: WorldUp,
    },
    /// Points local Z at the weighted source position.
    LookAt {
        /// Rotation offset (Euler degrees), applied in local space.
        offset: Vec3,
        /// Roll in degrees around the look direction.
        roll: f32,
        /// Node the up vector points towards; `None` uses scene up.
        world_up_node: Option<NodeIndex>,
    },
}

/// World-up references for aim constraints.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum WorldUp {
    /// Scene up.
    SceneUp,
    /// A fixed vector.
    Vector(Vec3),
    /// Towards a node.
    ObjectUp(NodeIndex),
    /// Rotation of a node applied to a vector.
    ObjectRotationUp(NodeIndex, Vec3),
    /// No up constraint.
    None,
}
