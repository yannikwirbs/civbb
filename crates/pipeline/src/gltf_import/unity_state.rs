//! Unity dialect 1's animator and expression parameters lowered to state entries (ADR 0088, 0095), and its state
//! behaviours and Write Defaults to native actions and `unwritten` rules (ADR 0085).
//!
//! Names in the menu, contact receivers and PhysBone prefixes become entry references.

use civbb_format::animator::{Action, Animator, EntryOp, PosePart, PoseSource, Unwritten};
use civbb_format::dynamics::ChainOutputs;
use civbb_format::expressions::{Control, ControlKind, Menu};
use civbb_format::state::{Entry, EntryIndex, EntryKind, PLATFORM_NAMESPACE, Platform, Range, StateTable, SyncMode};
use serde::Deserialize;
use serde_json::Value;

/// An animator parameter as dialect 1 writes it (`X_animator.animator.parameters`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct XParameter {
    name: String,
    kind: EntryKind,
    default: f32,
}

/// The part of `X_animator.animator` this module reads.
#[derive(Deserialize)]
pub(super) struct XAnimatorParameters {
    pub parameters: Vec<XParameter>,
}

/// `X_expressions`.
#[derive(Deserialize)]
pub(super) struct XExpressions {
    parameters: Vec<XExpressionParameter>,
    pub menu: XMenu,
}

#[derive(Deserialize)]
struct XExpressionParameter {
    name: String,
    kind: XNetworkKind,
    default: f32,
    saved: bool,
    synced: bool,
}

#[derive(Clone, Copy, Deserialize)]
enum XNetworkKind {
    Bool,
    Int,
    Float,
}

#[derive(Deserialize)]
pub(super) struct XMenu {
    controls: Vec<XControl>,
}

#[derive(Deserialize)]
struct XControl {
    name: String,
    icon: Option<u32>,
    kind: XControlKind,
    parameter: Option<String>,
    value: f32,
    sub_parameters: Vec<String>,
}

#[derive(Deserialize)]
enum XControlKind {
    Button,
    Toggle,
    SubMenu(XMenu),
    TwoAxisPuppet,
    FourAxisPuppet,
    RadialPuppet,
}

/// The entries: the animator's parameters in their order (indices into them stay valid), then the expression
/// parameters the animator lacks (they sync but drive nothing). An expression parameter gives its entry `sync`,
/// `saved`, `external` and the start value (VRChat loads it, #85); the animator's type holds, and its default stays
/// the Write Defaults value of curves on the parameter (`rest`).
pub(super) fn lower(animator: Vec<XParameter>, expressions: Option<&XExpressions>) -> StateTable {
    let expressions = expressions.map_or(&[][..], |e| &e.parameters[..]);
    let find = |name: &str| expressions.iter().find(|e| e.name == name);
    // VRChat's expression types lower exactly (ADR 0098): Int to 0..255, Float to -1..1 in 8 bits.
    let lowered = |e: &XExpressionParameter, kind| Entry {
        range: match kind {
            EntryKind::Int => Some(Range::Int { min: 0, max: 255 }),
            EntryKind::Float => Some(Range::Float {
                min: -1.0,
                max: 1.0,
                bits: 8,
            }),
            EntryKind::Bool | EntryKind::Trigger => None,
        },
        sync: if e.synced { SyncMode::Owner } else { SyncMode::None },
        saved: e.saved,
        external: true,
        ..Entry::new(e.name.clone(), kind, e.default)
    };
    let mut entries: Vec<Entry> = animator
        .into_iter()
        .map(|p| match find(&p.name) {
            Some(e) => Entry {
                rest: (p.default != e.default).then_some(p.default),
                ..lowered(e, p.kind)
            },
            None => Entry::new(p.name, p.kind, p.default),
        })
        .collect();
    for e in expressions {
        if !entries.iter().any(|x| x.name == e.name) {
            let kind = match e.kind {
                XNetworkKind::Bool => EntryKind::Bool,
                XNetworkKind::Int => EntryKind::Int,
                XNetworkKind::Float => EntryKind::Float,
            };
            entries.push(lowered(e, kind));
        }
    }
    // VRChat's built-ins are platform values (ADR 0096): the platform sets them, so they are not synced, saved or
    // written from outside, and references by their VRChat names (menu, receivers) no longer find them.
    for e in &mut entries {
        if let Some(p) = Platform::from_vrchat_name(&e.name) {
            e.name = p.name().into();
            (e.sync, e.saved, e.external) = (SyncMode::None, false, false);
        }
    }
    StateTable { entries }
}

/// A state behaviour as dialect 1 writes it (`X_animator.animator.layers[].state_machine.states[].behaviours`).
#[derive(Deserialize)]
enum XBehaviour {
    ParameterDriver { local_only: bool, ops: Vec<EntryOp> },
    TrackingControl(Vec<(PosePart, PoseSource)>),
    LayerWeight { layer: u32, goal: f32, duration: f32 },
    GroupWeight { group: u32, goal: f32, duration: f32 },
    Locomotion { disabled: bool },
}

impl From<XBehaviour> for Action {
    fn from(b: XBehaviour) -> Self {
        match b {
            XBehaviour::ParameterDriver { local_only, ops } => Action::Entries {
                owner_only: local_only,
                ops,
            },
            XBehaviour::TrackingControl(parts) => Action::PoseSource(parts),
            XBehaviour::LayerWeight { layer, goal, duration } => Action::LayerWeight { layer, goal, duration },
            XBehaviour::GroupWeight { group, goal, duration } => Action::GroupWeight { group, goal, duration },
            XBehaviour::Locomotion { disabled } => Action::Locomotion { disabled },
        }
    }
}

/// Rewrites dialect 1's states (`X_animator.animator.layers[].state_machine.states`) in native shape: Write Defaults
/// on as `unwritten: Rest`, off as `Hold`; behaviours as actions.
pub(super) fn lower_states(animator: &mut Value) -> Result<(), String> {
    let err = |e: serde_json::Error| format!("X_animator: {e}");
    let layers = animator.get_mut("layers").and_then(Value::as_array_mut);
    let states = (layers.into_iter().flatten())
        .filter_map(|l| l.pointer_mut("/state_machine/states").and_then(Value::as_array_mut))
        .flatten();
    for s in states.filter_map(Value::as_object_mut) {
        if let Some(wd) = s.remove("write_defaults") {
            let wd = wd.as_bool().ok_or("X_animator: write_defaults is not a bool")?;
            let unwritten = if wd { Unwritten::Rest } else { Unwritten::Hold };
            s.insert("unwritten".into(), serde_json::to_value(unwritten).map_err(err)?);
        }
        if let Some(b) = s.remove("behaviours") {
            let b = Vec::<XBehaviour>::deserialize(&b).map_err(err)?;
            let actions: Vec<Action> = b.into_iter().map(Action::from).collect();
            s.insert("actions".into(), serde_json::to_value(actions).map_err(err)?);
        }
    }
    Ok(())
}

/// Removes parameter driver operations that write a platform value, returning how many: VRChat sets its built-ins
/// every frame over whatever a driver wrote.
pub(super) fn drop_platform_writes(animator: &mut Animator, state: &StateTable) -> usize {
    let platform = |e: EntryIndex| state.entries[e as usize].name.starts_with(PLATFORM_NAMESPACE);
    let mut dropped = 0;
    let states = animator.layers.iter_mut().flat_map(|l| &mut l.state_machine.states);
    for b in states.flat_map(|s| &mut s.actions) {
        if let Action::Entries { ops, .. } = b {
            let before = ops.len();
            ops.retain(|op| match *op {
                EntryOp::Set { param, .. } | EntryOp::Add { param, .. } | EntryOp::Random { param, .. } => {
                    !platform(param)
                }
                EntryOp::Copy { dst, .. } => !platform(dst),
            });
            dropped += before - ops.len();
        }
    }
    dropped
}

/// The suffixes VRChat appends to a PhysBone's parameter prefix, in [`ChainOutputs::entries`] order.
const CHAIN_SUFFIXES: [&str; 5] = ["_IsGrabbed", "_IsPosed", "_Angle", "_Stretch", "_Squish"];

/// A chain's outputs from its parameter prefix: `<prefix>_IsGrabbed` and so on, each `None` when no entry has the
/// name (VRChat writes only parameters the animator declares).
pub(super) fn chain_outputs(prefix: Option<&str>, state: &StateTable) -> ChainOutputs {
    let Some(prefix) = prefix.filter(|p| !p.is_empty()) else {
        return ChainOutputs::default();
    };
    let [is_grabbed, is_posed, angle, stretch, squish] = CHAIN_SUFFIXES.map(|s| state.find(&format!("{prefix}{s}")));
    ChainOutputs {
        is_grabbed,
        is_posed,
        angle,
        stretch,
        squish,
    }
}

/// The menu with its parameter names resolved; an empty or unknown name is `None`, which writes nothing in VRChat
/// either.
pub(super) fn menu(x: XMenu, state: &StateTable) -> Menu {
    let controls = x.controls.into_iter().map(|c| Control {
        name: c.name,
        icon: c.icon,
        kind: match c.kind {
            XControlKind::Button => ControlKind::Button,
            XControlKind::Toggle => ControlKind::Toggle,
            XControlKind::SubMenu(m) => ControlKind::SubMenu(menu(m, state)),
            XControlKind::TwoAxisPuppet => ControlKind::TwoAxisPuppet,
            XControlKind::FourAxisPuppet => ControlKind::FourAxisPuppet,
            XControlKind::RadialPuppet => ControlKind::RadialPuppet,
        },
        parameter: c.parameter.and_then(|p| state.find(&p)),
        value: c.value,
        sub_parameters: c.sub_parameters.iter().map(|p| state.find(p)).collect(),
    });
    Menu {
        controls: controls.collect(),
    }
}
