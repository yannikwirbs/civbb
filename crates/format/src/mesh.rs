//! Meshes: vertex streams, skinning, submeshes and sparse blendshapes.

use serde::{Deserialize, Serialize};

use crate::hierarchy::NodeIndex;
use crate::math::{Mat4, Vec3};

/// Index into [`crate::Avatar::meshes`].
pub type MeshIndex = u32;

/// A triangle mesh attached to a node. All vertex streams have the same length, or are empty when optional.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    /// Name, used by animation bindings (blendshape and toggle paths).
    pub name: String,
    /// The node this mesh hangs from; it is hidden when that node is inactive.
    pub node: NodeIndex,
    /// Vertex positions.
    pub positions: Vec<Vec3>,
    /// Vertex normals.
    pub normals: Vec<Vec3>,
    /// Tangents with handedness in `w`; empty if absent.
    pub tangents: Vec<[f32; 4]>,
    /// Primary UVs, glTF convention (origin top-left, V down); empty if absent.
    pub uv0: Vec<[f32; 2]>,
    /// Secondary UVs (lightmaps, decals); empty if absent.
    pub uv1: Vec<[f32; 2]>,
    /// Skinning data; `None` for rigid meshes, which follow `node`.
    pub skin: Option<Skin>,
    /// Triangle list indices.
    pub indices: Vec<u32>,
    /// Index ranges, one per material.
    pub submeshes: Vec<Submesh>,
    /// Morph targets.
    pub blend_shapes: Vec<BlendShape>,
    /// Visibility in the owner's first-person view.
    pub first_person: FirstPerson,
    /// Lower levels of detail, each coarser than the one before; empty if none (ADR 0031).
    pub lods: Vec<Lod>,
}

/// A lower level of detail: simplified triangles over a prefix of the mesh's vertices, so skinning and shapes of
/// that level stop at [`Lod::vertices`] (ADR 0031).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lod {
    /// Vertices this level uses: the mesh's first `vertices`, at most the previous level's.
    pub vertices: u32,
    /// Triangle list indices, all below `vertices`.
    pub indices: Vec<u32>,
    /// Index ranges into `indices`, one per submesh of the mesh with the same material.
    pub submeshes: Vec<Submesh>,
    /// Largest deviation from the full mesh in the mesh's units, as meshoptimizer measures it (position and
    /// weighted attributes).
    pub error: f32,
}

impl Mesh {
    /// The mesh as level `n` draws it (0 is the full mesh, levels past the last give the last): vertex streams,
    /// skin and shape deltas cut to the level's vertices, its triangles, and no further levels.
    pub fn level(&self, n: usize) -> Mesh {
        let Some(lod) = n.checked_sub(1).and_then(|i| self.lods.get(i).or(self.lods.last())) else {
            return self.clone();
        };
        let v = lod.vertices as usize;
        let cut = |s: &[[f32; 2]]| s[..s.len().min(v)].to_vec();
        Mesh {
            name: self.name.clone(),
            node: self.node,
            positions: self.positions[..v].to_vec(),
            normals: self.normals[..v].to_vec(),
            tangents: self.tangents[..self.tangents.len().min(v)].to_vec(),
            uv0: cut(&self.uv0),
            uv1: cut(&self.uv1),
            skin: self.skin.as_ref().map(|s| Skin {
                joints: s.joints.clone(),
                inverse_bind: s.inverse_bind.clone(),
                vertex_joints: s.vertex_joints[..v].to_vec(),
                vertex_weights: s.vertex_weights[..v].to_vec(),
            }),
            indices: lod.indices.clone(),
            submeshes: lod.submeshes.clone(),
            blend_shapes: self
                .blend_shapes
                .iter()
                .map(|b| BlendShape {
                    name: b.name.clone(),
                    default_weight: b.default_weight,
                    deltas: b.deltas[..b.deltas.partition_point(|d| (d.vertex as usize) < v)].to_vec(),
                })
                .collect(),
            first_person: self.first_person,
            lods: Vec::new(),
        }
    }
}

/// First-person visibility of a mesh (VRM `firstPerson.meshAnnotations`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirstPerson {
    /// Visible, except the parts skinned to the head (legacy head shrinking).
    #[default]
    Auto,
    /// Visible in both views.
    Both,
    /// Only in the owner's first-person view.
    FirstPersonOnly,
    /// Hidden from the owner's first-person view.
    ThirdPersonOnly,
}

/// Linear blend skinning with up to four influences per vertex.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Skin {
    /// The nodes acting as joints; vertex joint indices index into this list.
    pub joints: Vec<NodeIndex>,
    /// Inverse bind matrix per joint.
    pub inverse_bind: Vec<Mat4>,
    /// Per-vertex joint indices into `joints`.
    pub vertex_joints: Vec<[u16; 4]>,
    /// Per-vertex weights, summing to 1.
    pub vertex_weights: Vec<[f32; 4]>,
}

/// A contiguous index range drawn with one material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submesh {
    /// First index in [`Mesh::indices`].
    pub first_index: u32,
    /// Number of indices, a multiple of 3.
    pub index_count: u32,
    /// Index into [`crate::Avatar::materials`].
    pub material: u32,
}

/// A morph target stored as sparse deltas: only vertices it moves are listed (design doc §6).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BlendShape {
    /// Name, as animations, visemes and face tracking refer to it.
    pub name: String,
    /// Weight at rest, 0 to 1 (Unity's 0 to 100 is divided by 100 on import).
    pub default_weight: f32,
    /// Moved vertices in ascending vertex order.
    pub deltas: Vec<ShapeDelta>,
}

/// One vertex's offset in a blendshape.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeDelta {
    /// Vertex index in the mesh.
    pub vertex: u32,
    /// Position offset at full weight.
    pub position: Vec3,
    /// Normal offset at full weight.
    pub normal: Vec3,
}
