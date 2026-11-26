//! The avatar role: what a document needs to be worn (ADR 0085 point 5).

use serde::{Deserialize, Serialize};

use crate::expressions::Expressions;
use crate::face::Face;
use crate::hierarchy::Humanoid;
use crate::math::Vec3;
use crate::meta::Meta;

/// The parts only an avatar has. Node, mesh and entry references index the owning [`crate::Document`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Avatar {
    /// Humanoid mapping; `None` for non-humanoid avatars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub humanoid: Option<Humanoid>,
    /// Eye position in avatar space, used for first-person view and eye-height calibration.
    pub viewpoint: Vec3,
    /// Lip sync, blink and eye look; `None` for avatars without a face.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face: Option<Face>,
    /// Expression menu.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expressions: Option<Expressions>,
    /// Creator and licence metadata; `None` when the source has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}
