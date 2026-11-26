//! Compile-time binding: gives curve targets slots in one flat buffer with per-slot defaults (ADRs 0007, 0089).

use std::collections::{HashMap, HashSet};

use civbb_format::Document;
use civbb_format::animator::{
    Animator, ClipIndex, Curve, EulerOrder, HumanoidProperty, Key, MeshProperty, Motion, NodeProperty,
    Target as CurveTarget,
};
use civbb_format::constraint::ConstraintField;
use civbb_format::hierarchy::NodeIndex;
use civbb_format::material::MaterialField;
use civbb_format::mesh::MeshIndex;
use civbb_format::state::{EntryIndex, EntryKind};

use civbb_core::math::quat_to_euler_zxy;

use crate::body::{BodyRig, RootMotion};
use crate::clip::{BodyCurves, BoundClip, BoundCurve, EulerCurves};
use crate::material;
use crate::muscle::{self, MUSCLES, Rig};

/// Index into the slot buffer.
pub type Slot = u32;

/// What a slot drives. Values are in curve units: metres, quaternion components, scale factors, blendshape
/// weights 0 to 1, 0/1 flags, parameter values.
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
    Parameter(EntryIndex),
    /// Humanoid muscle, by index into [`MUSCLES`].
    Muscle(u8),
    /// Component of a material field on every material of a renderer that has the field (ADRs 0025, 0099); sRGB-encoded
    /// when the flag is set.
    Material(MeshIndex, MaterialField, u8, bool),
    /// The material a submesh draws with, as an index into [`Document::materials`] (values round to the nearest).
    MaterialSlot(MeshIndex, u8),
    /// A field of a constraint, by index into [`Document::constraints`] (ADR 0040).
    Constraint(u32, ConstraintField),
    /// Humanoid body pose: position (0..3, `RootT`) or rotation (3..7, `RootQ`) component (ADR 0063).
    Body(u8),
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
    /// One per [`Document::clips`] entry, same order.
    pub clips: Vec<BoundClip>,
    /// Curves left out.
    pub unbound: Vec<Unbound>,
    /// The humanoid's muscle frames; `None` without them, and then muscle curves are unbound.
    pub rig: Option<Rig>,
    /// Places the hips from the body pose; `None` without hips, upper arms and upper legs, and then body pose
    /// curves are unbound.
    pub body: Option<BodyRig>,
}

/// Binds every clip of a validated avatar.
pub fn bind(avatar: &Document) -> Bound {
    let mut b = Binder::new(avatar);
    let clips = (0..avatar.clips.len()).map(|ci| b.clip(ci)).collect();
    // A mirrored motion writes each muscle's counterpart (ADR 0060): give those slots too.
    if avatar.animator.as_ref().is_some_and(mirrors)
        && let Some(rig) = &b.rig
    {
        let counterparts: Vec<u8> = (b.targets.iter())
            .filter_map(|t| match *t {
                Target::Muscle(m) => Some(muscle::mirror(m).0),
                _ => None,
            })
            .filter(|&m| rig.has(MUSCLES[m as usize].bone))
            .collect();
        for m in counterparts {
            b.slot(Target::Muscle(m));
        }
    }
    Bound {
        bindings: Bindings {
            targets: b.targets,
            defaults: b.defaults,
        },
        clips,
        unbound: b.unbound,
        rig: b.rig,
        body: b.body,
    }
}

/// Whether any state or blend tree child of `animator` mirrors.
fn mirrors(animator: &Animator) -> bool {
    fn motion(m: &Motion) -> bool {
        match m {
            Motion::Clip { clip: _ } => false,
            Motion::BlendTree(t) => t.children.iter().any(|c| c.mirror || motion(&c.motion)),
        }
    }
    (animator.layers.iter().flat_map(|l| &l.state_machine.states))
        .any(|s| s.mirror || s.mirror_param.is_some() || s.motion.as_ref().is_some_and(motion))
}

struct Binder<'a> {
    avatar: &'a Document,
    rig: Option<Rig>,
    body: Option<BodyRig>,
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
    fn new(avatar: &'a Document) -> Self {
        let rest = |n: NodeIndex| avatar.hierarchy.nodes[n as usize].rest.rotation;
        let humanoid = avatar.avatar().and_then(|r| r.humanoid.as_ref());
        let rig = humanoid.and_then(|h| Rig::new(h, rest));
        let body = humanoid.and_then(|h| BodyRig::new(&avatar.hierarchy, h));
        Self {
            avatar,
            rig,
            body,
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
        let mut body: [Option<&[Key]>; 7] = [None; 7];
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
                Ok(Resolved::Target(Target::Body(i))) => {
                    let slot = &mut body[i as usize];
                    if slot.is_some() {
                        skipped.push((ki, "duplicate binding"));
                    } else {
                        *slot = Some(&curve.keys);
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
        let constant = |value: f32| {
            vec![Key {
                time: 0.0,
                value,
                in_tangent: 0.0,
                out_tangent: 0.0,
            }]
        };
        for (node, r) in rotations {
            let first = self.rotation_slots(node);
            let rest = avatar.hierarchy.nodes[node as usize].rest.rotation;
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
                let rest = quat_to_euler_zxy(rest);
                out.eulers.push(EulerCurves {
                    slot: first,
                    axes: [0, 1, 2].map(|i| r.euler[i].map_or_else(|| constant(rest[i]), <[Key]>::to_vec)),
                });
            }
        }
        if body.iter().any(Option::is_some) {
            let slot = self.slot(Target::Body(0));
            for i in 1..7 {
                self.slot(Target::Body(i));
            }
            let axes: [Vec<Key>; 7] = std::array::from_fn(|i| {
                body[i].map_or_else(|| constant(if i == 6 { 1.0 } else { 0.0 }), <[Key]>::to_vec)
            });
            let mut b = BodyCurves { slot, axes, root: None };
            b.root = clip
                .root
                .map(|r| RootMotion::new(r, b.eval(0.0), b.eval(clip.length), clip.looping));
            out.body = Some(b);
        }
        out.slots = out.curves.iter().map(|c| c.slot).collect();
        out.slots.extend(out.eulers.iter().flat_map(|e| e.slot..e.slot + 4));
        out.slots.extend(out.body.iter().flat_map(|b| b.slot..b.slot + 7));
        out.slots.sort_unstable();
        out
    }

    fn resolve(&self, curve: &Curve) -> Result<Resolved, &'static str> {
        let t = match &curve.target {
            CurveTarget::Entry { entry: p } => match self.avatar.state.entries[*p as usize].kind {
                EntryKind::Float => Target::Parameter(*p),
                _ => return Err("only float parameters can be animated"),
            },
            &CurveTarget::Humanoid {
                property: HumanoidProperty::Muscle { bone, axis },
            } => {
                let m = muscle::index(bone, axis).ok_or("no such muscle")?;
                let rig = self.rig.as_ref().ok_or("no muscle frames (#53)")?;
                if !rig.has(MUSCLES[m as usize].bone) {
                    return Err("no muscle frame for the muscle's bone");
                }
                Target::Muscle(m)
            }
            CurveTarget::Humanoid {
                property: HumanoidProperty::Body { component: i },
            } => {
                self.body
                    .as_ref()
                    .ok_or("no humanoid with hips, upper arms and upper legs")?;
                Target::Body(*i)
            }
            &CurveTarget::Node { node, property } => match property {
                NodeProperty::Translation { component: i } => Target::Position(node, i),
                NodeProperty::Scale { component: i } => Target::Scale(node, i),
                NodeProperty::Rotation { component: i } => return Ok(Resolved::Rotation(node, false, i as usize)),
                NodeProperty::EulerRotation {
                    axis,
                    order: EulerOrder::Zxy,
                } => {
                    return Ok(Resolved::Rotation(node, true, axis as usize));
                }
                NodeProperty::Active => Target::Active(node),
            },
            CurveTarget::Mesh { mesh, property } => {
                let mesh = *mesh;
                match property {
                    MeshProperty::Visible => Target::RendererEnabled(mesh),
                    MeshProperty::Weight { shape: s } => Target::BlendShape(mesh, *s),
                    &MeshProperty::Material { field, component, srgb } => {
                        renderer_default(self.avatar, mesh, field)
                            .ok_or("no material on the renderer has the field")?;
                        Target::Material(mesh, field, component, srgb)
                    }
                    MeshProperty::MaterialSlot { submesh: s } => {
                        if *s as usize >= self.avatar.meshes[mesh as usize].submeshes.len() {
                            return Err("no such material slot");
                        }
                        let n = self.avatar.materials.len() as f32;
                        if !(curve.keys.iter()).all(|k| k.value >= 0.0 && k.value < n && k.value.fract() == 0.0) {
                            return Err("material slot key is not a material index");
                        }
                        Target::MaterialSlot(mesh, *s)
                    }
                }
            }
            &CurveTarget::Constraint { constraint, field } => {
                let c = &self.avatar.constraints[constraint as usize];
                if let ConstraintField::SourceWeight { source: s } = field
                    && s as usize >= c.sources.len()
                {
                    return Err("no such constraint source");
                }
                Target::Constraint(constraint, field)
            }
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
            Target::BlendShape(m, i) => a.meshes[m as usize].blend_shapes[i as usize].default_weight,
            Target::Active(n) => f32::from(u8::from(a.hierarchy.nodes[n as usize].active)),
            Target::RendererEnabled(_) => 1.0,
            Target::Parameter(p) => a.state.entries[p as usize].rest(),
            // Unity's default human pose is every muscle at 0, not the rest pose (#86, recorded at rest).
            Target::Muscle(_) => 0.0,
            // The rest pose's body: hips that are not animated stay where they rest (ADR 0063).
            Target::Body(i) => self.body.as_ref().expect("bound").rest[i as usize],
            Target::Material(m, f, c, srgb) => {
                let v = renderer_default(a, m, f).expect("bound")[c as usize];
                if srgb && c < 3 { material::to_srgb(v) } else { v }
            }
            Target::MaterialSlot(m, s) => a.meshes[m as usize].submeshes[s as usize].material as f32,
            Target::Constraint(i, field) => {
                let c = &a.constraints[i as usize];
                match field {
                    ConstraintField::Weight => c.weight,
                    ConstraintField::Active => f32::from(u8::from(c.active)),
                    ConstraintField::SourceWeight { source: s } => c.sources[s as usize].weight,
                    ConstraintField::Freeze => f32::from(u8::from(c.options.freeze)),
                }
            }
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

/// A renderer's default for a material field: its first material's value, or 0 when that material lacks the field,
/// as Unity reads it from the renderer's material 0 (ADRs 0067, 0099). `None` when no material of the renderer has it.
fn renderer_default(avatar: &Document, mesh: MeshIndex, field: MaterialField) -> Option<[f32; 4]> {
    let get = |s: &civbb_format::mesh::Submesh| material::get(&avatar.materials[s.material as usize], field);
    let submeshes = &avatar.meshes[mesh as usize].submeshes;
    submeshes.iter().find_map(get)?;
    Some(submeshes.first().and_then(get).unwrap_or([0.0; 4]))
}

enum Resolved {
    Target(Target),
    /// Node, Euler (else quaternion), component.
    Rotation(NodeIndex, bool, usize),
}
