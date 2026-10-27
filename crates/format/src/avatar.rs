//! The avatar root type tying hierarchy, meshes, materials and textures together.

use serde::{Deserialize, Serialize};

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
}
