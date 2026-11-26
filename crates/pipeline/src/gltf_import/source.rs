//! The source package's `X_*` extensions (ADR 0041): humanoid map, viewpoint and face, animator and clips,
//! constraints, dynamics, contacts and expressions, read as the format model's JSON with glTF node indices, the
//! parameters lowered to the state table by `unity_state`.
//!
//! A malformed extension or a node index outside the scene fails the import; `validate` then checks the rest.

use civbb_format::animator::{Animator, Clip, Motion};
use civbb_format::constraint::{Constraint, ConstraintKind, WorldUp};
use civbb_format::dynamics::{Contacts, Dynamics};
use civbb_format::expressions::Expressions;
use civbb_format::face::{EyeDriver, Face, ShapeBind};
use civbb_format::hierarchy::{HumanBone, Humanoid, MuscleFrame, NodeIndex};
use civbb_format::math::Vec3;
use civbb_format::state::StateTable;
use serde::Deserialize;
use serde_json::Value;

use super::Importer;
use super::unity_state::{self, XAnimatorParameters, XExpressions};

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
    pub face: Option<Face>,
    pub state: StateTable,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct XAvatar {
    #[serde(default)]
    humanoid: serde_json::Map<String, Value>,
    #[serde(default)]
    viewpoint: Option<Vec3>,
    /// Format `MuscleFrame`s in glTF space (ADR 0042).
    #[serde(default)]
    muscle_frames: Vec<MuscleFrame>,
    /// The format `Face` with each bind's `mesh` the glTF node carrying the mesh and eye bones as glTF nodes
    /// (ADR 0042).
    #[serde(default)]
    face: Option<Face>,
    /// Unity's `Animator.humanScale` (ADR 0063).
    #[serde(default)]
    human_scale: f32,
    /// Unity's rest centre of mass (ADR 0063).
    #[serde(default)]
    center_of_mass: Option<[f32; 3]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct XAnimator {
    animator: Animator,
    #[serde(default)]
    clips: Vec<Clip>,
}

/// Newest Unity dialect this module reads (ADRs 0085, 0087). Dialect 1 is the `X_*` shapes of format v25, frozen by
/// `tests/unity_dialect.rs`.
const UNITY_DIALECT: u64 = 1;

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
        self.unity_dialect()?;
        let mut s = Source::default();
        if let Some(v) = doc.extension_value("X_avatar") {
            let mut x: XAvatar = parse("X_avatar", v)?;
            if let Some(mut face) = x.face.take() {
                self.face_meshes(&mut face)?;
                s.face = Some(face);
            }
            s.humanoid = Some(self.x_avatar(x)?);
        }
        let mut params = Vec::new();
        if let Some(v) = doc.extension_value("X_animator") {
            let x: XAnimator = parse("X_animator", v)?;
            params = parse::<XAnimatorParameters>("X_animator", &v["animator"])?.parameters;
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
        let expressions: Option<XExpressions> = (doc.extension_value("X_expressions"))
            .map(|v| parse("X_expressions", v))
            .transpose()?;
        s.state = unity_state::lower(params, expressions.as_ref());
        if let Some(a) = &mut s.animator {
            let dropped = unity_state::drop_platform_writes(a, &s.state);
            if dropped > 0 {
                self.warnings.push(format!(
                    "X_animator: {dropped} driver operations write VRChat built-ins; dropped"
                ));
            }
        }
        if let Some(v) = doc.extension_value("X_dynamics") {
            let mut v = v.clone();
            let chains = v.get_mut("chains").and_then(Value::as_array_mut);
            for c in chains.into_iter().flatten() {
                let c = c.as_object_mut().ok_or("X_dynamics: a chain is not an object")?;
                let prefix = c.remove("parameter").ok_or("X_dynamics: a chain has no parameter")?;
                let outputs = unity_state::chain_outputs(prefix.as_str(), &s.state);
                c.insert(
                    "outputs".into(),
                    serde_json::to_value(outputs).map_err(|e| e.to_string())?,
                );
            }
            s.dynamics = parse("X_dynamics", &v)?;
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
            let mut v = v.clone();
            let receivers = v.get_mut("receivers").and_then(Value::as_array_mut);
            for r in receivers.into_iter().flatten() {
                let p = r
                    .get_mut("parameter")
                    .ok_or("X_contacts: a receiver has no parameter")?;
                let name = p.as_str().ok_or("X_contacts: a receiver's parameter is not a string")?;
                *p = Value::from(s.state.find(name));
            }
            s.contacts = parse("X_contacts", &v)?;
            let volumes = s.contacts.senders.iter_mut().map(|c| &mut c.volume);
            for v in volumes.chain(s.contacts.receivers.iter_mut().map(|c| &mut c.volume)) {
                v.node = self.x_node("X_contacts", v.node)?;
            }
        }
        s.expressions = expressions.map(|x| Expressions {
            menu: unity_state::menu(x.menu, &s.state),
        });
        Ok(Some(s))
    }

    /// The Unity dialect version in `asset.extras.civbb_unity_dialect` (absent means 1); newer ones are refused.
    fn unity_dialect(&self) -> Result<u64, String> {
        let Some(extras) = &self.doc.as_json().asset.extras else {
            return Ok(1);
        };
        let extras: Value = serde_json::from_str(extras.get()).map_err(|e| format!("asset.extras: {e}"))?;
        match extras.get("civbb_unity_dialect") {
            None => Ok(1),
            Some(v) => match v.as_u64() {
                Some(n @ 1..=UNITY_DIALECT) => Ok(n),
                _ => Err(format!(
                    "asset.extras.civbb_unity_dialect {v}: this importer reads Unity dialect 1 to {UNITY_DIALECT}"
                )),
            },
        }
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
        h.muscle_frames = x.muscle_frames;
        h.human_scale = x.human_scale;
        h.center_of_mass = x.center_of_mass;
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

    /// Maps a face's glTF node indices: binds' mesh nodes to format meshes, eye bones to hierarchy nodes.
    fn face_meshes(&self, face: &mut Face) -> Result<(), String> {
        let ext = "X_avatar";
        let mesh = |n: u32| -> Result<u32, String> {
            let at = self.x_node(ext, n)?;
            self.node_meshes
                .get(&at)
                .map(|&m| m as u32)
                .ok_or_else(|| format!("{ext}: node {n} has no mesh"))
        };
        let mut eye_binds: Vec<&mut ShapeBind> = Vec::new();
        match face.eyes.as_mut().map(|e| &mut e.driver) {
            Some(EyeDriver::Bones { left, right }) => {
                for n in [left, right].into_iter().flatten() {
                    *n = self.x_node(ext, *n)?;
                }
            }
            Some(EyeDriver::Shapes { up, down, left, right }) => {
                eye_binds.extend([up, down, left, right].into_iter().flatten());
            }
            None => {}
        }
        let binds = face.visemes.iter_mut().flat_map(|(_, b)| b.iter_mut());
        for b in binds.chain(face.blink.iter_mut()).chain(eye_binds) {
            b.mesh = mesh(b.mesh)?;
        }
        Ok(())
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
