//! The source package's `X_*` extensions (ADR 0041): humanoid map and viewpoint, animator and clips, constraints,
//! dynamics, contacts and expressions, read as the format model's JSON with glTF node indices.
//!
//! A malformed extension or a node index outside the scene fails the import; `validate` then checks the rest.

use civbb_format::animator::{Animator, Clip, Motion};
use civbb_format::constraint::{Constraint, ConstraintKind, WorldUp};
use civbb_format::dynamics::{Contacts, Dynamics};
use civbb_format::expressions::Expressions;
use civbb_format::hierarchy::{HumanBone, Humanoid, NodeIndex};
use civbb_format::math::Vec3;
use serde::Deserialize;
use serde_json::Value;

use super::Importer;

/// The avatar parts the `X_*` extensions add to a plain glTF import.
#[derive(Default)]
pub(super) struct Source {
    pub humanoid: Option<Humanoid>,
    pub animator: Option<Animator>,
    pub clips: Vec<Clip>,
    pub constraints: Vec<Constraint>,
    pub dynamics: Dynamics,
    pub contacts: Contacts,
    pub expressions: Option<Expressions>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct XAvatar {
    #[serde(default)]
    humanoid: serde_json::Map<String, Value>,
    #[serde(default)]
    viewpoint: Option<Vec3>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct XAnimator {
    animator: Animator,
    #[serde(default)]
    clips: Vec<Clip>,
}

/// Extensions this module reads.
const NAMES: [&str; 6] = [
    "X_avatar",
    "X_animator",
    "X_constraints",
    "X_dynamics",
    "X_contacts",
    "X_expressions",
];

fn parse<'a, T: Deserialize<'a>>(name: &str, v: &'a Value) -> Result<T, String> {
    T::deserialize(v).map_err(|e| format!("{name}: {e}"))
}

impl Importer<'_> {
    /// Reads the `X_*` extensions if any is present. `clip_base` is the number of clips before the extension's.
    pub(super) fn source(&mut self, clip_base: usize) -> Result<Option<Source>, String> {
        let doc = self.doc;
        if NAMES.iter().all(|n| doc.extension_value(n).is_none()) {
            return Ok(None);
        }
        let mut s = Source::default();
        if let Some(v) = doc.extension_value("X_avatar") {
            s.humanoid = Some(self.x_avatar(parse("X_avatar", v)?)?);
        }
        if let Some(v) = doc.extension_value("X_animator") {
            let x: XAnimator = parse("X_animator", v)?;
            let mut animator = x.animator;
            let base = u32::try_from(clip_base).map_err(|_| "X_animator: too many clips")?;
            for layer in &mut animator.layers {
                for state in &mut layer.state_machine.states {
                    if let Some(m) = &mut state.motion {
                        shift(m, base)?;
                    }
                }
            }
            s.animator = Some(animator);
            s.clips = x.clips;
        }
        if let Some(v) = doc.extension_value("X_constraints") {
            s.constraints = parse("X_constraints", v)?;
            for c in &mut s.constraints {
                self.constraint_nodes(c)?;
            }
        }
        if let Some(v) = doc.extension_value("X_dynamics") {
            s.dynamics = parse("X_dynamics", v)?;
            for c in &mut s.dynamics.chains {
                c.root = self.x_node("X_dynamics", c.root)?;
                for n in &mut c.ignore {
                    *n = self.x_node("X_dynamics", *n)?;
                }
            }
            for c in &mut s.dynamics.colliders {
                c.node = self.x_node("X_dynamics", c.node)?;
            }
        }
        if let Some(v) = doc.extension_value("X_contacts") {
            s.contacts = parse("X_contacts", v)?;
            let volumes = s.contacts.senders.iter_mut().map(|c| &mut c.volume);
            for v in volumes.chain(s.contacts.receivers.iter_mut().map(|c| &mut c.volume)) {
                v.node = self.x_node("X_contacts", v.node)?;
            }
        }
        if let Some(v) = doc.extension_value("X_expressions") {
            s.expressions = Some(parse("X_expressions", v)?);
        }
        Ok(Some(s))
    }

    /// A glTF node index from an extension as a hierarchy index.
    fn x_node(&self, ext: &str, i: NodeIndex) -> Result<NodeIndex, String> {
        self.map
            .get(i as usize)
            .copied()
            .flatten()
            .ok_or_else(|| format!("{ext}: node {i} is not in the scene"))
    }

    fn x_avatar(&mut self, x: XAvatar) -> Result<Humanoid, String> {
        let mut h = Humanoid::default();
        for (name, node) in &x.humanoid {
            let bone: HumanBone = parse("X_avatar", &Value::String(name.clone()))?;
            let i = node
                .as_u64()
                .and_then(|i| u32::try_from(i).ok())
                .ok_or_else(|| format!("X_avatar: bone {name} has no node index"))?;
            if h.node(bone).is_some() {
                return Err(format!("X_avatar: bone {name} mapped twice"));
            }
            h.bones.push((bone, self.x_node("X_avatar", i)?));
        }
        h.bones.sort_unstable_by_key(|&(b, _)| b);
        h.viewpoint = match (x.viewpoint, h.node(HumanBone::Head)) {
            (Some(v), _) => v,
            (None, Some(head)) => {
                let t = self.world()[head as usize].0;
                [t[0], t[1] + 0.06, t[2]]
            }
            (None, None) => {
                self.warnings
                    .push("X_avatar has no head bone or viewpoint; viewpoint at origin".into());
                [0.0; 3]
            }
        };
        Ok(h)
    }

    fn constraint_nodes(&self, c: &mut Constraint) -> Result<(), String> {
        let ext = "X_constraints";
        c.target = self.x_node(ext, c.target)?;
        for s in &mut c.sources {
            s.node = self.x_node(ext, s.node)?;
        }
        match &mut c.kind {
            ConstraintKind::Aim {
                world_up: WorldUp::ObjectUp(n) | WorldUp::ObjectRotationUp(n, _),
                ..
            }
            | ConstraintKind::LookAt {
                world_up_node: Some(n), ..
            } => *n = self.x_node(ext, *n)?,
            _ => {}
        }
        Ok(())
    }
}

/// Moves a motion's clip indices past the clips imported before the extension's.
fn shift(m: &mut Motion, base: u32) -> Result<(), String> {
    match m {
        Motion::Clip(i) => *i = i.checked_add(base).ok_or("X_animator: clip index overflows")?,
        Motion::BlendTree(t) => {
            for c in &mut t.children {
                shift(&mut c.motion, base)?;
            }
        }
    }
    Ok(())
}
