//! Unity dialect 1's animator and expression parameters lowered to state entries (ADR 0088, 0095).
//!
//! Names in the menu, contact receivers and PhysBone prefixes become entry references.

use civbb_format::dynamics::ChainOutputs;
use civbb_format::expressions::{Control, ControlKind, Menu};
use civbb_format::state::{Entry, EntryKind, StateTable, SyncMode};
use serde::Deserialize;

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
    let lowered = |e: &XExpressionParameter, kind| Entry {
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
    StateTable { entries }
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
