//! The render snapshot: what the renderer reads of a frame, copied out of the islands by the `Extract` step so a
//! render thread can draw frame N-1 while the schedule simulates frame N (design doc §5 step 9, ADR 0103).

use std::sync::Arc;

use bevy_ecs::prelude::*;
use civbb_core::math::Pose;
use civbb_core::{Counters, System, jobs};
use civbb_format::material::Material;
use civbb_format::math::{Mat4, Vec3};
use civbb_format::mesh::Mesh;

use crate::frame::{Shows, Spawned, Workers};
use crate::island::Island;
use crate::menu::Overlay;
use crate::package::Handle;
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
    /// Where the headset's play space sits in the world, placed by the VR player (ADR 0125); `None` puts it below
    /// the camera ([`crate::xr::stage`]).
    pub stage: Option<Pose>,
}

/// One island as the renderer reads it.
pub struct IslandSnapshot {
    /// The island's key, stable while it lives: the render thread keys its GPU instance by it (ADR 0104).
    pub key: u64,
    /// The package it shows.
    pub package: Handle,
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
    /// Screen-space UI over the frame (ADR 0117).
    pub overlay: Overlay,
}

/// One island to copy: its key, the package it shows, the island.
type Source<'a> = (u64, &'a Handle, &'a Island);

impl Snapshot {
    /// Copies `islands`, each with its key and the package it shows, and `camera` into the snapshot.
    pub fn extract<'a>(&mut self, islands: impl ExactSizeIterator<Item = Source<'a>>, camera: Camera) {
        let islands: Vec<_> = islands.collect();
        for (s, (key, package, island)) in self.entries(&islands, camera) {
            s.fill(key, package, island);
        }
    }

    /// [`Snapshot::extract`] with one job per island on `pool`; returns the jobs' counters, [`System::Extract`] once
    /// per island.
    pub fn extract_on<'a>(
        &mut self,
        pool: &jobs::Pool,
        islands: impl ExactSizeIterator<Item = Source<'a>>,
        camera: Camera,
    ) -> Counters {
        let islands: Vec<_> = islands.collect();
        let mut pairs: Vec<_> = self.entries(&islands, camera).collect();
        jobs::islands(pool, &mut pairs, |(s, (key, package, island)), c| {
            c.time(System::Extract, || s.fill(*key, package, island));
        })
    }

    /// Sets the camera and one entry per island, keeping existing entries' buffers; pairs each with its island.
    fn entries<'s, 'a>(
        &'s mut self,
        islands: &[Source<'a>],
        camera: Camera,
    ) -> impl Iterator<Item = (&'s mut IslandSnapshot, Source<'a>)> {
        self.camera = camera;
        self.islands.truncate(islands.len());
        for &(key, package, _) in &islands[self.islands.len()..] {
            self.islands.push(IslandSnapshot {
                key,
                package: Arc::clone(package),
                origin: [0.0; 3],
                joints: Vec::new(),
                weights: Vec::new(),
                materials: Vec::new(),
                slots: Vec::new(),
            });
        }
        self.islands.iter_mut().zip(islands.iter().copied())
    }
}

impl IslandSnapshot {
    /// Overwrites the entry with `island`, which has key `key` and shows `package`.
    fn fill(&mut self, key: u64, package: &Handle, island: &Island) {
        self.key = key;
        if !Arc::ptr_eq(&self.package, package) {
            self.package = Arc::clone(package);
        }
        let avatar = &package.avatar;
        self.origin = island.origin.pos;
        island.shape_weights(&mut self.weights);
        island.materials(&mut self.materials);
        self.joints.resize(avatar.meshes.len(), None);
        self.slots.resize_with(avatar.meshes.len(), Vec::new);
        let (world, active) = (island.world(), island.active());
        let hidden = island.hidden();
        for (m, mesh) in avatar.meshes.iter().enumerate() {
            if active[mesh.node as usize] {
                let joints = self.joints[m].get_or_insert_with(Vec::new);
                skin_matrices(mesh, world, joints);
                if let Some((under, head)) = hidden {
                    shrink(mesh, under, head.pos, joints);
                }
            } else {
                self.joints[m] = None;
            }
            island.submesh_materials(avatar, m as u32, &mut self.slots[m]);
        }
    }
}

/// Replaces the skinning matrices of `mesh` that the nodes marked in `under` place with one that puts every vertex at
/// `at`: the head hidden from its own first-person view (ADR 0106).
fn shrink(mesh: &Mesh, under: &[bool], at: Vec3, joints: &mut [Mat4]) {
    let mut collapsed = [0.0; 16];
    (collapsed[12], collapsed[13], collapsed[14], collapsed[15]) = (at[0], at[1], at[2], 1.0);
    let nodes = match &mesh.skin {
        Some(skin) if !skin.joints.is_empty() => &skin.joints[..],
        _ => std::slice::from_ref(&mesh.node),
    };
    for (m, &n) in joints.iter_mut().zip(nodes) {
        if under[n as usize] {
            *m = collapsed;
        }
    }
}

/// The extract system of [`civbb_core::schedule::Step::Extract`]: every island showing a package into the
/// [`Snapshot`] resource, in spawn order, one job per island on the [`Workers`] pool (#137).
pub(crate) fn extract_system(
    q: Query<(Entity, &Spawned, &Shows, &Island)>,
    camera: Res<Camera>,
    workers: Res<Workers>,
    overlay: Option<Res<Overlay>>,
    mut snapshot: ResMut<Snapshot>,
    mut counters: ResMut<Counters>,
) {
    let s = &mut snapshot.overlay;
    match overlay {
        // Field by field: the vertex list keeps its allocation.
        Some(o) => {
            s.list.vertices.clone_from(&o.list.vertices);
            (s.atlas, s.icons) = (o.atlas.clone(), o.icons);
        }
        None => *s = Overlay::default(),
    }
    let mut all: Vec<_> = q.iter().collect();
    all.sort_unstable_by_key(|(_, spawned, ..)| **spawned);
    let islands = all
        .into_iter()
        .map(|(e, _, shows, island)| (e.to_bits(), &shows.0, island));
    counters.merge(&snapshot.extract_on(&workers.0, islands, *camera));
}
