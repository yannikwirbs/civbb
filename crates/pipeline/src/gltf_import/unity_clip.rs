//! Unity dialect 1's clips and avatar masks lowered to typed targets (ADR 0089): `/`-separated node paths and Unity
//! properties resolve to nodes, meshes, blend shapes, state entries and constraints; curves that resolve to nothing
//! are reported and dropped.

use std::collections::{BTreeMap, HashMap, HashSet};

use civbb_format::animator::{Clip, Curve, HumanoidProperty, Key, MeshProperty, NodeProperty, RootSettings, Target};
use civbb_format::constraint::{Constraint, ConstraintField, ConstraintType};
use civbb_format::hierarchy::{Node, NodeIndex};
use civbb_format::mesh::{Mesh, MeshIndex};
use civbb_format::state::{Platform, StateTable};
use serde::Deserialize;
use serde_json::Value;

/// A clip as dialect 1 writes it (`X_animator.clips`).
#[derive(Deserialize)]
pub(super) struct XClip {
    name: String,
    length: f32,
    looping: bool,
    #[serde(default)]
    root: Option<RootSettings>,
    curves: Vec<XCurve>,
}

#[derive(Deserialize)]
struct XCurve {
    binding: XBinding,
    keys: Vec<Key>,
}

/// A node path (names below the root joined by `/`; empty for the root and for parameters) and a property.
#[derive(Deserialize)]
struct XBinding {
    path: String,
    property: XProperty,
}

/// Dialect 1's properties, in its units: Euler degrees applied Z, X, Y, blend shape weights 0 to 100.
#[derive(Deserialize)]
enum XProperty {
    Position(u8),
    Rotation(u8),
    EulerRotation(u8),
    Scale(u8),
    Muscle(String),
    RootMotion(u8),
    BlendShape(String),
    Active,
    RendererEnabled,
    Material(String),
    MaterialSlot(u8),
    Parameter(String),
    Constraint {
        kind: ConstraintType,
        field: ConstraintField,
    },
    #[allow(dead_code)]
    Component {
        component: String,
        property: String,
    },
}

/// What paths and names resolve against.
pub(super) struct Scope<'a> {
    pub nodes: &'a [Node],
    pub node_meshes: &'a HashMap<NodeIndex, usize>,
    pub meshes: &'a [Mesh],
    pub state: &'a StateTable,
    pub constraints: &'a [Constraint],
    /// Longest name a path can match, in bytes.
    pub string_len: usize,
}

/// Each node's path, `None` when it is longer than `max` bytes (no binding can name it).
fn paths(nodes: &[Node], max: usize) -> Vec<Option<String>> {
    let mut paths: Vec<Option<String>> = Vec::with_capacity(nodes.len());
    for n in nodes {
        paths.push(match n.parent {
            None => Some(String::new()),
            Some(0) => Some(n.name.clone()),
            Some(p) => paths[p as usize].as_ref().map(|p| format!("{p}/{}", n.name)),
        });
        if paths.last().expect("pushed").as_ref().is_some_and(|p| p.len() > max) {
            *paths.last_mut().expect("pushed") = None;
        }
    }
    paths
}

/// Clips with typed targets, and one warning per reason curves were dropped for.
pub(super) fn lower(clips: Vec<XClip>, s: &Scope) -> (Vec<Clip>, Vec<String>) {
    // Paths resolve to the first node with that path, as Unity's `Transform.Find` does.
    let mut by_path: HashMap<String, NodeIndex> = HashMap::new();
    for (i, p) in paths(s.nodes, s.string_len).into_iter().enumerate() {
        if let Some(p) = p {
            by_path.entry(p).or_insert(i as NodeIndex);
        }
    }
    let mut dropped: BTreeMap<&'static str, usize> = BTreeMap::new();
    let clips = clips
        .into_iter()
        .map(|c| Clip {
            name: c.name,
            length: c.length,
            looping: c.looping,
            root: c.root,
            curves: (c.curves.into_iter())
                .filter_map(|x| match target(x.binding, s, &by_path) {
                    Ok(target) => Some(Curve { target, keys: x.keys }),
                    Err(reason) => {
                        *dropped.entry(reason).or_default() += 1;
                        None
                    }
                })
                .collect(),
        })
        .collect();
    let warnings = (dropped.into_iter())
        .map(|(reason, n)| format!("X_animator: {n} curves dropped: {reason}"))
        .collect();
    (clips, warnings)
}

fn target(b: XBinding, s: &Scope, by_path: &HashMap<String, NodeIndex>) -> Result<Target, &'static str> {
    match b.property {
        XProperty::Parameter(name) => {
            // VRChat's built-ins are platform values under native names (ADR 0096), read-only to content.
            let platform = Platform::from_vrchat_name(&name).and_then(|p| s.state.find(p.name()));
            if platform.is_some() || Platform::from_name(&name).is_some() {
                return Err("writes a platform value");
            }
            return s.state.find(&name).map(Target::Entry).ok_or("no such parameter");
        }
        XProperty::Muscle(name) => return Ok(Target::Humanoid(HumanoidProperty::Muscle(name))),
        XProperty::RootMotion(i) => return Ok(Target::Humanoid(HumanoidProperty::Body(i))),
        XProperty::Component { .. } => return Err("unsupported component property"),
        _ => {}
    }
    let node = *by_path.get(&b.path).ok_or("no node at path")?;
    let on_node = |property| Ok(Target::Node { node, property });
    let mesh = s.node_meshes.get(&node).map(|&m| m as MeshIndex);
    let on_mesh = |property| {
        let mesh = mesh.ok_or("no mesh at path")?;
        Ok(Target::Mesh { mesh, property })
    };
    match b.property {
        XProperty::Position(i) => on_node(NodeProperty::Translation(i)),
        XProperty::Rotation(i) => on_node(NodeProperty::Rotation(i)),
        XProperty::EulerRotation(i) => on_node(NodeProperty::EulerRotation(i)),
        XProperty::Scale(i) => on_node(NodeProperty::Scale(i)),
        XProperty::Active => on_node(NodeProperty::Active),
        XProperty::RendererEnabled => on_mesh(MeshProperty::Visible),
        XProperty::BlendShape(name) => {
            let m = mesh.ok_or("no mesh at path")?;
            let shapes = &s.meshes[m as usize].blend_shapes;
            let i = shapes.iter().position(|x| x.name == name).ok_or("no such blendshape")?;
            on_mesh(MeshProperty::Weight(i as u32))
        }
        XProperty::Material(name) => on_mesh(MeshProperty::Material(name)),
        XProperty::MaterialSlot(i) => on_mesh(MeshProperty::MaterialSlot(i)),
        XProperty::Constraint { kind, field } => {
            let i = (s.constraints.iter())
                .position(|c| c.target == node && ConstraintType::of(&c.kind) == Some(kind))
                .ok_or("no such constraint on the node")?;
            Ok(Target::Constraint {
                constraint: i as u32,
                field,
            })
        }
        XProperty::Parameter(_) | XProperty::Muscle(_) | XProperty::RootMotion(_) | XProperty::Component { .. } => {
            unreachable!("handled above")
        }
    }
}

/// Rewrites dialect 1's avatar mask transform paths (`X_animator.animator.layers[].mask.transforms`) as the nodes
/// with those paths; paths no node has are dropped.
pub(super) fn mask_nodes(animator: &mut Value, nodes: &[Node], string_len: usize) {
    let paths = paths(nodes, string_len);
    let layers = animator.get_mut("layers").and_then(Value::as_array_mut);
    for layer in layers.into_iter().flatten() {
        let Some(t) = layer.pointer_mut("/mask/transforms").filter(|t| t.is_array()) else {
            continue;
        };
        let names: HashSet<&str> = t.as_array().expect("array").iter().filter_map(Value::as_str).collect();
        let nodes: Vec<NodeIndex> = (paths.iter().enumerate())
            .filter(|(_, p)| p.as_deref().is_some_and(|p| names.contains(&p)))
            .map(|(i, _)| i as NodeIndex)
            .collect();
        *t = Value::from(nodes);
    }
}
