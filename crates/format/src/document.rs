//! The document root: parts shared by every kind of content, and the role saying which kind it is (ADR 0085).

use serde::{Deserialize, Serialize};

use crate::animator::{Animator, Clip};
use crate::avatar::Avatar;
use crate::constraint::Constraint;
use crate::dynamics::{Contacts, Dynamics};
use crate::hierarchy::Hierarchy;
use crate::material::{Material, Texture};
use crate::mesh::Mesh;
use crate::package::ContentKind;
use crate::state::StateTable;

/// A complete document. Cross references are indices into the vectors here; `validate` checks them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Display name.
    pub name: String,
    /// All transform nodes.
    pub hierarchy: Hierarchy,
    /// Meshes.
    pub meshes: Vec<Mesh>,
    /// Materials referenced by submeshes.
    pub materials: Vec<Material>,
    /// Textures referenced by materials; pixels live in blobs.
    pub textures: Vec<Texture>,
    /// Shared state: animator parameters, menu and network values (ADR 0088).
    pub state: StateTable,
    /// Animator controller; `None` plays the rest pose.
    pub animator: Option<Animator>,
    /// Clips referenced by the animator.
    pub clips: Vec<Clip>,
    /// Bone dynamics.
    pub dynamics: Dynamics,
    /// Contacts.
    pub contacts: Contacts,
    /// Constraints in evaluation order.
    pub constraints: Vec<Constraint>,
    /// What the document is; Phase 2 adds worlds.
    pub role: Role,
}

/// The kind of document and the parts only that kind has.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Role {
    /// Worn by a user.
    Avatar(Avatar),
}

impl Default for Role {
    fn default() -> Self {
        Role::Avatar(Avatar::default())
    }
}

impl Document {
    /// The avatar role, if this document is an avatar.
    pub fn avatar(&self) -> Option<&Avatar> {
        match &self.role {
            Role::Avatar(a) => Some(a),
        }
    }

    /// The avatar role, mutably.
    pub fn avatar_mut(&mut self) -> Option<&mut Avatar> {
        match &mut self.role {
            Role::Avatar(a) => Some(a),
        }
    }

    /// The package content kind of this document's role.
    pub fn kind(&self) -> ContentKind {
        match self.role {
            Role::Avatar(_) => ContentKind::Avatar,
        }
    }
}
