//! One remap of curve targets and avatar mask nodes for every stage that renumbers nodes, meshes, blend shapes or
//! materials (ADR 0089 point 4).

use civbb_format::Document;
use civbb_format::animator::{Curve, MeshProperty, Target};
use civbb_format::hierarchy::NodeIndex;
use civbb_format::mesh::MeshIndex;

/// Old index to new per element kind, `None` for an element a stage removed. An empty map leaves that kind as is.
#[derive(Debug, Default)]
pub(crate) struct Renumber {
    /// Nodes.
    pub nodes: Vec<Option<NodeIndex>>,
    /// Meshes.
    pub meshes: Vec<Option<MeshIndex>>,
    /// Per old mesh, its blend shapes' indices in the mesh it maps to.
    pub shapes: Vec<Vec<Option<u32>>>,
    /// Materials (the key values of material slot curves).
    pub materials: Vec<Option<u32>>,
}

impl Renumber {
    /// Remaps every curve target and avatar mask node; a curve or mask node whose element was removed goes with it.
    pub(crate) fn apply(&self, a: &mut Document) {
        for clip in &mut a.clips {
            clip.curves.retain_mut(|c| self.curve(c));
        }
        let masks = a.animator.iter_mut().flat_map(|an| &mut an.layers);
        for nodes in masks.filter_map(|l| l.mask.as_mut()?.transforms.as_mut()) {
            nodes.retain_mut(|n| remap(&self.nodes, n));
        }
    }

    /// Whether the curve's element is kept.
    fn curve(&self, c: &mut Curve) -> bool {
        match &mut c.target {
            Target::Node { node, .. } => remap(&self.nodes, node),
            Target::Mesh { mesh, property } => {
                match property {
                    MeshProperty::Weight { shape: s } if !self.shapes.is_empty() => {
                        match self.shapes[*mesh as usize][*s as usize] {
                            Some(n) => *s = n,
                            None => return false,
                        }
                    }
                    MeshProperty::MaterialSlot { submesh: _ } if !self.materials.is_empty() => {
                        // Keys that are not material indices bind to nothing (`civbb_anim::bind`); left as they are.
                        for k in &mut c.keys {
                            if k.value >= 0.0
                                && k.value.fract() == 0.0
                                && let Some(&Some(m)) = self.materials.get(k.value as usize)
                            {
                                k.value = m as f32;
                            }
                        }
                    }
                    _ => {}
                }
                remap(&self.meshes, mesh)
            }
            Target::Humanoid { property: _ } | Target::Entry { entry: _ } | Target::Constraint { .. } => true,
        }
    }
}

/// Maps `i` through `map` (empty: unchanged); false when its element was removed.
fn remap(map: &[Option<u32>], i: &mut u32) -> bool {
    if map.is_empty() {
        return true;
    }
    match map[*i as usize] {
        Some(n) => {
            *i = n;
            true
        }
        None => false,
    }
}
