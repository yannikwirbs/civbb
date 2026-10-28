//! Face setup: blendshapes driven by lip sync and automatic blinking (design doc §7, `X_avatar`).

use serde::{Deserialize, Serialize};

use crate::mesh::MeshIndex;

/// Which blendshapes lip sync and blinking drive.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Face {
    /// Binds per viseme. A viseme appears at most once; missing visemes stay at rest.
    pub visemes: Vec<(Viseme, Vec<ShapeBind>)>,
    /// Binds that close both eyes, for automatic blinking; empty disables it.
    pub blink: Vec<ShapeBind>,
}

/// One blendshape at a weight.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeBind {
    /// Index into [`crate::Avatar::meshes`].
    pub mesh: MeshIndex,
    /// Index into that mesh's blend shapes.
    pub shape: u32,
    /// Weight at full activation, 0 to 1.
    pub weight: f32,
}

/// The 15 standard visemes, in legacy order.
#[allow(missing_docs)] // The names are the documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Viseme {
    Sil,
    Pp,
    Ff,
    Th,
    Dd,
    Kk,
    Ch,
    Ss,
    Nn,
    Rr,
    Aa,
    E,
    I,
    O,
    U,
}

impl Viseme {
    /// All visemes in legacy order.
    pub const ALL: [Viseme; 15] = [
        Self::Sil,
        Self::Pp,
        Self::Ff,
        Self::Th,
        Self::Dd,
        Self::Kk,
        Self::Ch,
        Self::Ss,
        Self::Nn,
        Self::Rr,
        Self::Aa,
        Self::E,
        Self::I,
        Self::O,
        Self::U,
    ];
}
