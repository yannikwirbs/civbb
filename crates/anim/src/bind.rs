//! Compile-time binding: resolves curve paths to slots in one flat buffer with per-slot defaults (ADR 0007).

use std::collections::{HashMap, HashSet};

use civbb_format::Avatar;
use civbb_format::animator::{ClipIndex, Curve, Key, ParamIndex, ParamKind, Property};
use civbb_format::hierarchy::NodeIndex;
use civbb_format::mesh::MeshIndex;

use crate::clip::{BoundClip, BoundCurve, EulerCurves, quat_to_euler};

/// Index into the slot buffer.
pub type Slot = u32;

/// What a slot drives. Values are in curve units: metres, quaternion components, scale factors, blendshape
/// weights 0 to 100, 0/1 flags, parameter values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// Local position component.
    Position(NodeIndex, u8),
    /// Local rotation quaternion component; a node's 4 components have consecutive slots, x first.
    Rotation(NodeIndex, u8),
    /// Local scale component.
    Scale(NodeIndex, u8),
    /// Blendshape weight.
    BlendShape(MeshIndex, u32),
    /// Node active flag.
    Active(NodeIndex),
    /// Mesh renderer enabled flag.
    RendererEnabled(MeshIndex),
    /// Float animator parameter.
    Parameter(ParamIndex),
}

/// Every slot of an avatar: what it drives and its default (the Write Defaults value).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bindings {
    /// Slot targets.
    pub targets: Vec<Target>,
    /// Slot defaults, from the rest pose, mesh and animator.
    pub defaults: Vec<f32>,
}

impl Bindings {
    /// Number of slots.
    pub fn len(&self) -> usize {
        self.targets.len()
    }

    /// No slots.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
}

/// A curve that binds to nothing. Reported, not fatal: Unity ignores such curves too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unbound {
    /// Clip.
    pub clip: ClipIndex,
    /// Curve index within the clip.
    pub curve: u32,
    /// Why.
    pub reason: &'static str,
}

/// An avatar's clips bound to slots.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bound {
    /// The slots.
    pub bindings: Bindings,
    /// One per [`Avatar::clips`] entry, same order.
    pub clips: Vec<BoundClip>,
    /// Curves left out.
    pub unbound: Vec<Unbound>,
}

/// Binds every clip of a validated avatar.
pub fn bind(avatar: &Avatar) -> Bound {
    let mut b = Binder::new(avatar);
    let clips = (0..avatar.clips.len()).map(|ci| b.clip(ci)).collect();
    Bound {
        bindings: Bindings {
            targets: b.targets,
            defaults: b.defaults,
        },
        clips,
        unbound: b.unbound,
    }
}

struct Binder<'a> {
    avatar: &'a Avatar,
    nodes: HashMap<String, NodeIndex>,
    meshes: HashMap<NodeIndex, Vec<MeshIndex>>,
    params: HashMap<&'a str, ParamIndex>,
    slots: HashMap<Target, Slot>,
    targets: Vec<Target>,
    defaults: Vec<f32>,
    unbound: Vec<Unbound>,
}

/// A clip's rotation curves of one node, gathered before they become quaternion or Euler slots.
#[derive(Default)]
struct NodeRotation<'a> {
    quat: [Option<&'a [Key]>; 4],
    euler: [Option<&'a [Key]>; 3],
    euler_curves: Vec<u32>,
}

impl<'a> Binder<'a> {
    fn new(avatar: &'a Avatar) -> Self {
        let mut paths: Vec<String> = Vec::with_capacity(avatar.hierarchy.nodes.len());
        let mut nodes = HashMap::new();
        for (i, n) in avatar.hierarchy.nodes.iter().enumerate() {
            let path = match n.parent {
                None => String::new(),
                Some(0) => n.name.clone(),
                Some(p) => format!("{}/{}", paths[p as usize], n.name),
            };
            // Sibling names should be unique; if not, the first node wins, as Unity's `Transform.Find` does.
            nodes.entry(path.clone()).or_insert(i as NodeIndex);
            paths.push(path);
        }
        let mut meshes: HashMap<NodeIndex, Vec<MeshIndex>> = HashMap::new();
        for (i, m) in avatar.meshes.iter().enumerate() {
            meshes.entry(m.node).or_default().push(i as MeshIndex);
        }
        let mut params = HashMap::new();
        for (i, p) in avatar.animator.iter().flat_map(|a| a.parameters.iter()).enumerate() {
            params.entry(p.name.as_str()).or_insert(i as ParamIndex);
        }
        Self {
            avatar,
            nodes,
            meshes,
            params,
            slots: HashMap::new(),
            targets: Vec::new(),
            defaults: Vec::new(),
            unbound: Vec::new(),
        }
    }

    fn clip(&mut self, ci: usize) -> BoundClip {
        let avatar = self.avatar;
        let clip = &avatar.clips[ci];
        let mut out = BoundClip {
            length: clip.length,
            looping: clip.looping,
            ..BoundClip::default()
        };
        let mut rotations: Vec<(NodeIndex, NodeRotation)> = Vec::new();
        let mut seen = HashSet::new();
        let mut skipped: Vec<(u32, &'static str)> = Vec::new();
        for (ki, curve) in clip.curves.iter().enumerate() {
            let ki = ki as u32;
            if curve.keys.is_empty() {
                skipped.push((ki, "no keys"));
                continue;
            }
            let target = match self.resolve(curve) {
                Ok(Resolved::Rotation(node, euler, axis)) => {
                    let i = match rotations.iter().position(|(n, _)| *n == node) {
                        Some(i) => i,
                        None => {
                            rotations.push((node, NodeRotation::default()));
                            rotations.len() - 1
                        }
                    };
                    let r = &mut rotations[i].1;
                    let slot = if euler { &mut r.euler[axis] } else { &mut r.quat[axis] };
                    if slot.is_some() {
                        skipped.push((ki, "duplicate binding"));
                    } else {
                        *slot = Some(&curve.keys);
                        if euler {
                            r.euler_curves.push(ki);
                        }
                    }
                    continue;
                }
                Ok(Resolved::Target(t)) => t,
                Err(reason) => {
                    skipped.push((ki, reason));
                    continue;
                }
            };
            if !seen.insert(target) {
                skipped.push((ki, "duplicate binding"));
                continue;
            }
            let slot = self.slot(target);
            out.curves.push(BoundCurve {
                slot,
                keys: curve.keys.clone(),
            });
        }
        for (ki, reason) in skipped {
            self.unbound.push(Unbound {
                clip: ci as ClipIndex,
                curve: ki,
                reason,
            });
        }
        for (node, r) in rotations {
            let first = self.rotation_slots(node);
            let rest = avatar.hierarchy.nodes[node as usize].rest.rotation;
            let constant = |value: f32| {
                vec![Key {
                    time: 0.0,
                    value,
                    in_tangent: 0.0,
                    out_tangent: 0.0,
                }]
            };
            if r.quat.iter().any(Option::is_some) {
                // Quaternion curves win over Euler ones on the same node; missing components hold the rest value.
                for &ki in &r.euler_curves {
                    self.unbound.push(Unbound {
                        clip: ci as ClipIndex,
                        curve: ki,
                        reason: "Euler curve on a node with quaternion curves",
                    });
                }
                for (i, keys) in r.quat.iter().enumerate() {
                    out.curves.push(BoundCurve {
                        slot: first + i as Slot,
                        keys: keys.map_or_else(|| constant(rest[i]), <[Key]>::to_vec),
                    });
                }
                out.quats.push(first);
            } else {
                let rest = quat_to_euler(rest);
                out.eulers.push(EulerCurves {
                    slot: first,
                    axes: [0, 1, 2].map(|i| r.euler[i].map_or_else(|| constant(rest[i]), <[Key]>::to_vec)),
                });
            }
        }
        out.slots = out.curves.iter().map(|c| c.slot).collect();
        out.slots.extend(out.eulers.iter().flat_map(|e| e.slot..e.slot + 4));
        out.slots.sort_unstable();
        out
    }

    fn resolve(&self, curve: &Curve) -> Result<Resolved, &'static str> {
        let b = &curve.binding;
        if let Property::Parameter(name) = &b.property {
            let &p = self.params.get(name.as_str()).ok_or("no such parameter")?;
            let kind = self.avatar.animator.as_ref().expect("params exist").parameters[p as usize].kind;
            return match kind {
                ParamKind::Float => Ok(Resolved::Target(Target::Parameter(p))),
                _ => Err("only float parameters can be animated"),
            };
        }
        let node = *self.nodes.get(&b.path).ok_or("no node at path")?;
        let on_node = self.meshes.get(&node).map_or(&[][..], Vec::as_slice);
        let t = match &b.property {
            Property::Position(i) => Target::Position(node, *i),
            Property::Scale(i) => Target::Scale(node, *i),
            Property::Rotation(i) => return Ok(Resolved::Rotation(node, false, *i as usize)),
            Property::EulerRotation(i) => return Ok(Resolved::Rotation(node, true, *i as usize)),
            Property::Active => Target::Active(node),
            Property::RendererEnabled => Target::RendererEnabled(*on_node.first().ok_or("no mesh at path")?),
            Property::BlendShape(name) => {
                if on_node.is_empty() {
                    return Err("no mesh at path");
                }
                on_node
                    .iter()
                    .find_map(|&m| {
                        let shapes = &self.avatar.meshes[m as usize].blend_shapes;
                        let s = shapes.iter().position(|s| s.name == *name)?;
                        Some(Target::BlendShape(m, s as u32))
                    })
                    .ok_or("no such blendshape")?
            }
            Property::Muscle(_) | Property::RootMotion(_) => return Err("humanoid curves not supported yet (#48)"),
            Property::Material(_) => return Err("material curves not supported yet (#49)"),
            Property::Component { .. } => return Err("unsupported component property"),
            Property::Parameter(_) => unreachable!("handled above"),
        };
        Ok(Resolved::Target(t))
    }

    /// The slot of `t`, allocated with its default on first use.
    fn slot(&mut self, t: Target) -> Slot {
        if let Some(&s) = self.slots.get(&t) {
            return s;
        }
        let s = self.targets.len() as Slot;
        let a = self.avatar;
        let default = match t {
            Target::Position(n, i) => a.hierarchy.nodes[n as usize].rest.translation[i as usize],
            Target::Rotation(n, i) => a.hierarchy.nodes[n as usize].rest.rotation[i as usize],
            Target::Scale(n, i) => a.hierarchy.nodes[n as usize].rest.scale[i as usize],
            Target::BlendShape(m, i) => a.meshes[m as usize].blend_shapes[i as usize].default_weight * 100.0,
            Target::Active(n) => f32::from(u8::from(a.hierarchy.nodes[n as usize].active)),
            Target::RendererEnabled(_) => 1.0,
            Target::Parameter(p) => a.animator.as_ref().expect("bound").parameters[p as usize].default,
        };
        self.slots.insert(t, s);
        self.targets.push(t);
        self.defaults.push(default);
        s
    }

    /// The first of a node's 4 consecutive quaternion slots.
    fn rotation_slots(&mut self, node: NodeIndex) -> Slot {
        let first = self.slot(Target::Rotation(node, 0));
        for i in 1..4 {
            self.slot(Target::Rotation(node, i));
        }
        first
    }
}

enum Resolved {
    Target(Target),
    /// Node, Euler (else quaternion), component.
    Rotation(NodeIndex, bool, usize),
}
