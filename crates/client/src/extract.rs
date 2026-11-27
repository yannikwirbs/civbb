//! The render snapshot: what the renderer reads of a frame, copied out of the islands by the `Extract` step so a
//! render thread can draw frame N-1 while the schedule simulates frame N (design doc §5 step 9, ADR 0103).

use std::sync::Arc;

use bevy_ecs::prelude::*;
use civbb_format::Document;
use civbb_format::material::Material;
use civbb_format::math::{Mat4, Vec3};

use crate::island::Island;
use crate::view::skin_matrices;

/// Where the frame is seen from.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Camera {
    /// The camera position.
    pub eye: Vec3,
    /// The point looked at.
    pub target: Vec3,
    /// Vertical field of view in radians.
    pub fov_y: f32,
    /// The background, linear.
    pub clear: [f32; 4],
}

/// The avatar every island shows, for the extract system (one avatar per runtime until packages are shared, #134).
#[derive(Resource)]
pub struct Avatar(pub Arc<Document>);

/// One island as the renderer reads it.
#[derive(Default)]
pub struct IslandSnapshot {
    /// Where the avatar's root parent is in the world.
    pub origin: Vec3,
    /// Per mesh, its skinning matrices; `None` for a mesh whose node is inactive.
    pub joints: Vec<Option<Vec<Mat4>>>,
    /// Per mesh, its blendshape weights.
    pub weights: Vec<Vec<f32>>,
    /// The island's animated materials, in [`crate::Model::animated_materials`] order.
    pub materials: Vec<Material>,
    /// Per mesh, the material of each submesh when the animator swaps any; empty otherwise.
    pub slots: Vec<Vec<u32>>,
}

/// A frame as the renderer reads it. Reused across frames: extracting overwrites everything without freeing.
#[derive(Resource, Default)]
pub struct Snapshot {
    /// The camera.
    pub camera: Camera,
    /// Every island, in spawn order.
    pub islands: Vec<IslandSnapshot>,
}

impl Snapshot {
    /// Copies `islands`, each showing `avatar`, and `camera` into the snapshot.
    pub fn extract<'a>(
        &mut self,
        avatar: &Document,
        islands: impl ExactSizeIterator<Item = &'a Island>,
        camera: Camera,
    ) {
        self.camera = camera;
        self.islands.resize_with(islands.len(), IslandSnapshot::default);
        for (island, s) in islands.zip(&mut self.islands) {
            s.origin = island.origin.pos;
            island.shape_weights(&mut s.weights);
            island.materials(&mut s.materials);
            s.joints.resize(avatar.meshes.len(), None);
            s.slots.resize_with(avatar.meshes.len(), Vec::new);
            let (world, active) = (island.world(), island.active());
            for (m, mesh) in avatar.meshes.iter().enumerate() {
                if active[mesh.node as usize] {
                    skin_matrices(mesh, world, s.joints[m].get_or_insert_with(Vec::new));
                } else {
                    s.joints[m] = None;
                }
                island.submesh_materials(avatar, m as u32, &mut s.slots[m]);
            }
        }
    }
}

/// The extract system of [`civbb_core::schedule::Step::Extract`]: every island into the [`Snapshot`] resource.
pub(crate) fn extract_system(
    q: Query<&Island>,
    avatar: Res<Avatar>,
    camera: Res<Camera>,
    mut snapshot: ResMut<Snapshot>,
) {
    snapshot.extract(&avatar.0, q.iter(), *camera);
}
