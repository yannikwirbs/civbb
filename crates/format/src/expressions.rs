//! Expression menu and synced expression parameters (design doc §7).

use serde::{Deserialize, Serialize};

/// Synced parameters and the radial menu that drives them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Expressions {
    /// Parameters exposed to the menu, OSC and the network.
    pub parameters: Vec<ExpressionParam>,
    /// Root menu.
    pub menu: Menu,
}

/// A parameter declared for expressions. It names an animator parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExpressionParam {
    /// Animator parameter name.
    pub name: String,
    /// Network type; decides its synced bit cost (bool 1, int 8, float 8).
    pub kind: ExpressionParamKind,
    /// Initial value.
    pub default: f32,
    /// Persisted across sessions.
    pub saved: bool,
    /// Sent to other players.
    pub synced: bool,
}

/// Network types of expression parameters.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExpressionParamKind {
    Bool,
    Int,
    Float,
}

impl ExpressionParamKind {
    /// Bits this parameter costs in the synced budget.
    pub fn bits(self) -> u32 {
        match self {
            Self::Bool => 1,
            Self::Int | Self::Float => 8,
        }
    }
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
    /// Parameter set while active (buttons, toggles, submenus).
    pub parameter: Option<String>,
    /// Value written to `parameter`.
    pub value: f32,
    /// Axis parameters of puppets, in legacy order.
    pub sub_parameters: Vec<String>,
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
