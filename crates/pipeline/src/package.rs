//! Turns imports into runtime packages and summarises avatars for `civbb inspect`.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use civbb_format::material::MaterialModel;
use civbb_format::package::{ContentKind, Header, Platform};
use civbb_format::{Avatar, BlobId, FORMAT_VERSION, Package, cbor};

use serde::Serialize;

use crate::Imported;

/// Compiler name and version recorded in every manifest, so old content can be found and recompiled.
pub const COMPILER: &str = concat!("civbb-pipeline ", env!("CARGO_PKG_VERSION"));

/// Packages an import for `platform`: its texture blobs plus the avatar as root. No compile or measure stages yet.
pub fn package(imported: Imported, platform: Platform) -> Package {
    Package {
        header: Header {
            format_version: FORMAT_VERSION,
            compiler: COMPILER.into(),
            kind: ContentKind::Avatar,
            platform,
            root: BlobId([0; 32]),
            cost: None,
        },
        blobs: imported.blobs,
    }
    .with_avatar(&imported.avatar)
}

/// Counts that describe an avatar's size (the report's keys, ADR 0032).
#[allow(missing_docs)] // Each field counts what its name says.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub nodes: usize,
    pub bones: usize,
    pub human_bones: usize,
    pub visemes: usize,
    pub meshes: usize,
    pub skinned_meshes: usize,
    /// Submeshes, one draw each when every mesh shows.
    pub draws: usize,
    pub vertices: usize,
    pub triangles: usize,
    /// Per LOD level, full detail first, over the deepest chain; a mesh with fewer levels counts its last.
    pub lod: Vec<Level>,
    pub blend_shapes: usize,
    pub shape_deltas: usize,
    pub materials: usize,
    pub toon_materials: usize,
    pub textures: usize,
    /// Encoded bytes of the distinct texture blobs.
    pub texture_bytes: usize,
    pub clips: usize,
    pub curves: usize,
    pub keys: usize,
    pub parameters: usize,
    pub layers: usize,
    pub states: usize,
    pub transitions: usize,
    /// CBOR size of the animator, a rough measure of its complexity.
    pub animator_bytes: usize,
    pub expression_params: usize,
    pub synced_bits: u32,
    pub chains: usize,
    pub colliders: usize,
    pub contacts: usize,
    pub constraints: usize,
}

/// Vertices and triangles one LOD level draws.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Level {
    pub vertices: usize,
    pub triangles: usize,
}

/// Counts everything in `a`, with texture bytes from `blobs`. Bones are the distinct nodes any skin uses as a joint.
pub fn stats(a: &Avatar, blobs: &BTreeMap<BlobId, Vec<u8>>) -> Stats {
    let bones: HashSet<u32> = a
        .meshes
        .iter()
        .filter_map(|m| m.skin.as_ref())
        .flat_map(|s| s.joints.iter().copied())
        .collect();
    let curves = a.clips.iter().flat_map(|c| &c.curves);
    let mut s = Stats {
        nodes: a.hierarchy.nodes.len(),
        bones: bones.len(),
        human_bones: a.humanoid.as_ref().map_or(0, |h| h.bones.len()),
        visemes: a.face.as_ref().map_or(0, |f| f.visemes.len()),
        meshes: a.meshes.len(),
        skinned_meshes: a.meshes.iter().filter(|m| m.skin.is_some()).count(),
        draws: a.meshes.iter().map(|m| m.submeshes.len()).sum(),
        vertices: a.meshes.iter().map(|m| m.positions.len()).sum(),
        triangles: a.meshes.iter().map(|m| m.indices.len() / 3).sum(),
        blend_shapes: a.meshes.iter().map(|m| m.blend_shapes.len()).sum(),
        shape_deltas: a
            .meshes
            .iter()
            .flat_map(|m| &m.blend_shapes)
            .map(|b| b.deltas.len())
            .sum(),
        materials: a.materials.len(),
        toon_materials: a
            .materials
            .iter()
            .filter(|m| matches!(m.model, MaterialModel::Toon(_)))
            .count(),
        textures: a.textures.len(),
        texture_bytes: a
            .textures
            .iter()
            .map(|t| t.blob)
            .collect::<HashSet<_>>()
            .iter()
            .filter_map(|id| blobs.get(id))
            .map(Vec::len)
            .sum(),
        clips: a.clips.len(),
        curves: curves.clone().count(),
        keys: curves.map(|c| c.keys.len()).sum(),
        chains: a.dynamics.chains.len(),
        colliders: a.dynamics.colliders.len(),
        contacts: a.contacts.senders.len() + a.contacts.receivers.len(),
        constraints: a.constraints.len(),
        ..Stats::default()
    };
    let levels = a.meshes.iter().map(|m| m.lods.len()).max().unwrap_or(0) + 1;
    s.lod = (0..levels)
        .map(|n| {
            a.meshes.iter().fold(Level::default(), |l, m| {
                let (vertices, triangles) = match n.checked_sub(1).and_then(|i| m.lods.get(i).or(m.lods.last())) {
                    Some(lod) => (lod.vertices as usize, lod.indices.len() / 3),
                    None => (m.positions.len(), m.indices.len() / 3),
                };
                Level {
                    vertices: l.vertices + vertices,
                    triangles: l.triangles + triangles,
                }
            })
        })
        .collect();
    if let Some(anim) = &a.animator {
        let machines = anim.layers.iter().map(|l| &l.state_machine);
        s.parameters = anim.parameters.len();
        s.layers = anim.layers.len();
        s.states = machines.clone().map(|m| m.states.len()).sum();
        s.transitions = machines
            .map(|m| {
                let per_state: usize = m.states.iter().map(|st| st.transitions.len()).sum();
                per_state + m.entry_transitions.len() + m.any_state_transitions.len()
            })
            .sum();
        s.animator_bytes = cbor::to_vec(anim).len();
    }
    if let Some(e) = &a.expressions {
        s.expression_params = e.parameters.len();
        s.synced_bits = e.parameters.iter().filter(|p| p.synced).map(|p| p.kind.bits()).sum();
    }
    s
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "skeleton    {} nodes, {} bones, {} humanoid bones, {} visemes",
            self.nodes, self.bones, self.human_bones, self.visemes
        )?;
        writeln!(
            f,
            "meshes      {} meshes ({} skinned), {} draws, {} vertices, {} triangles, LOD triangles {:?}",
            self.meshes,
            self.skinned_meshes,
            self.draws,
            self.vertices,
            self.triangles,
            self.lod.iter().map(|l| l.triangles).collect::<Vec<_>>()
        )?;
        writeln!(
            f,
            "shapes      {} blend shapes, {} deltas",
            self.blend_shapes, self.shape_deltas
        )?;
        writeln!(
            f,
            "materials   {} materials ({} toon), {} textures, {} texture bytes",
            self.materials, self.toon_materials, self.textures, self.texture_bytes
        )?;
        writeln!(
            f,
            "animation   {} clips, {} curves, {} keys",
            self.clips, self.curves, self.keys
        )?;
        writeln!(
            f,
            "animator    {} parameters, {} layers, {} states, {} transitions, {} bytes",
            self.parameters, self.layers, self.states, self.transitions, self.animator_bytes
        )?;
        writeln!(
            f,
            "expressions {} parameters, {} synced bits",
            self.expression_params, self.synced_bits
        )?;
        write!(
            f,
            "dynamics    {} chains, {} colliders, {} contacts, {} constraints",
            self.chains, self.colliders, self.contacts, self.constraints
        )
    }
}
