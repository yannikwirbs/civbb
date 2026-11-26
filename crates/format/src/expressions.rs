//! Expression menu (design doc §7).

use serde::{Deserialize, Serialize};

use crate::state::EntryIndex;

/// The radial menu; the parameters it writes are state entries (ADR 0088).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Expressions {
    /// Root menu.
    pub menu: Menu,
}

/// A menu page.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Menu {
    /// Up to 8 controls in legacy content.
    pub controls: Vec<Control>,
}

/// One menu entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Control {
    /// Label.
    pub name: String,
    /// Icon texture index.
    pub icon: Option<u32>,
    /// Behaviour.
    pub kind: ControlKind,
    /// Entry set while active (buttons, toggles, submenus).
    pub parameter: Option<EntryIndex>,
    /// Value written to `parameter`.
    pub value: f32,
    /// Axis entries of puppets, in legacy order; `None` for an axis without one.
    pub sub_parameters: Vec<Option<EntryIndex>>,
}

/// Control kinds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ControlKind {
    /// Sets the parameter while held.
    Button,
    /// Sets the parameter until toggled off.
    Toggle,
    /// Opens a nested menu.
    SubMenu(Menu),
    /// Two-axis joystick.
    TwoAxisPuppet,
    /// Four-direction joystick.
    FourAxisPuppet,
    /// Radial dial.
    RadialPuppet,
}
