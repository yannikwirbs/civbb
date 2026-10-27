//! The avatar root type tying every part of an avatar together.

use serde::{Deserialize, Serialize};

use crate::animator::{Animator, Clip};
use crate::constraint::Constraint;
use crate::dynamics::{Contacts, Dynamics};
use crate::expressions::Expressions;
use crate::hierarchy::{Hierarchy, Humanoid};
use crate::material::{Material, Texture};
use crate::mesh::Mesh;

/// A complete avatar. Cross references are indices into the vectors here; `validate` checks them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Avatar {
    /// Display name.
    pub name: String,
    /// All transform nodes.
    pub hierarchy: Hierarchy,
    /// Humanoid mapping; `None` for non-humanoid avatars.
    pub humanoid: Option<Humanoid>,
    /// Meshes.
    pub meshes: Vec<Mesh>,
    /// Materials referenced by submeshes.
    pub materials: Vec<Material>,
    /// Textures referenced by materials; pixels live in blobs.
    pub textures: Vec<Texture>,
    /// Animator controller; `None` plays the rest pose.
    pub animator: Option<Animator>,
    /// Clips referenced by the animator.
    pub clips: Vec<Clip>,
    /// Expression menu and synced parameters.
    pub expressions: Option<Expressions>,
    /// Bone dynamics.
    pub dynamics: Dynamics,
    /// Contacts.
    pub contacts: Contacts,
    /// Constraints in evaluation order.
    pub constraints: Vec<Constraint>,
}
