//! Face setup: blendshapes driven by lip sync, automatic blinking and eye look (design doc §7, `X_avatar`).

use serde::{Deserialize, Serialize};

use crate::hierarchy::NodeIndex;
use crate::mesh::MeshIndex;

/// Which blendshapes lip sync and blinking drive.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Face {
    /// Binds per viseme. A viseme appears at most once; missing visemes stay at rest.
    pub visemes: Vec<(Viseme, Vec<ShapeBind>)>,
    /// Binds that close both eyes, for automatic blinking; empty disables it.
    pub blink: Vec<ShapeBind>,
    /// How the eyes follow a look target; `None` leaves them still.
    pub eyes: Option<EyeLook>,
}

/// Eye look (VRM `lookAt`, legacy eye-look settings). Each range map turns a look angle in one direction into
/// a bone rotation or a blendshape weight.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EyeLook {
    /// What moves.
    pub driver: EyeDriver,
    /// Each eye turning towards the nose.
    pub horizontal_inner: RangeMap,
    /// Each eye turning away from the nose.
    pub horizontal_outer: RangeMap,
    /// Looking down.
    pub vertical_down: RangeMap,
    /// Looking up.
    pub vertical_up: RangeMap,
}

/// What eye look moves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum EyeDriver {
    /// Eye bones rotate; range map outputs are degrees.
    Bones {
        /// Left eye node.
        left: Option<NodeIndex>,
        /// Right eye node.
        right: Option<NodeIndex>,
    },
    /// Blendshapes; range map outputs are weights, 0 to 1.
    Shapes {
        /// Binds for looking up.
        up: Vec<ShapeBind>,
        /// Binds for looking down.
        down: Vec<ShapeBind>,
        /// Binds for looking left.
        left: Vec<ShapeBind>,
        /// Binds for looking right.
        right: Vec<ShapeBind>,
    },
}

/// Linear from 0 to `output_scale` as the look angle goes from 0 to `input_max` degrees, held beyond.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RangeMap {
    /// Angle in degrees at which the output is full.
    pub input_max: f32,
    /// Output at full: degrees for bones, weight for shapes.
    pub output_scale: f32,
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
