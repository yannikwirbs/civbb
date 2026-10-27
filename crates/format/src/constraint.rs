//! Transform constraints (position, rotation, scale, parent, aim, look-at), evaluated in list order.

use serde::{Deserialize, Serialize};

use crate::hierarchy::NodeIndex;
use crate::math::{Transform, Vec3};

/// A constraint. The compiler sorts an avatar's constraints topologically, so list order is evaluation order.
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
    /// Per-axis enable for position, rotation or scale (x, y, z).
    pub axes: [bool; 3],
    /// Transform of `target` when the constraint has zero weight.
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
    Parent,
    /// Points `aim_axis` at the weighted source position.
    Aim {
        /// Local axis that points at the target.
        aim_axis: Vec3,
        /// Local up axis.
        up_axis: Vec3,
        /// World up reference.
        world_up: WorldUp,
    },
    /// Points local Z at the weighted source position.
    LookAt {
        /// Roll in degrees around the look direction.
        roll: f32,
        /// Node whose up vector is used; `None` uses world up.
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
    /// Up axis of a node.
    ObjectUp(NodeIndex, Vec3),
    /// Rotation of a node applied to a vector.
    ObjectRotationUp(NodeIndex, Vec3),
    /// No up constraint.
    None,
}
