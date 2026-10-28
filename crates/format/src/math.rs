//! Plain math types used by the format: vectors, quaternions and transforms as arrays.

use serde::{Deserialize, Serialize};

/// `[x, y, z]`. Right-handed, Y up, metres, avatars facing +Z (glTF conventions, ADR 0006).
pub type Vec3 = [f32; 3];
/// `[x, y, z, w]` unit quaternion.
pub type Quat = [f32; 4];
/// Column-major 4x4 matrix.
pub type Mat4 = [f32; 16];

/// Translation, rotation and scale, applied scale first.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Translation in metres.
    pub translation: Vec3,
    /// Rotation as a unit quaternion.
    pub rotation: Quat,
    /// Non-uniform scale.
    pub scale: Vec3,
}

impl Transform {
    /// The identity transform.
    pub const IDENTITY: Self = Self {
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
    };
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}
