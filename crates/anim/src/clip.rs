//! A clip bound to slots, and sampling it into the slot buffer.

use civbb_format::animator::Key;
use civbb_format::math::Quat;

use crate::bind::Slot;
use crate::curve;

/// A clip whose curves address slots instead of paths.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoundClip {
    /// Length in seconds.
    pub length: f32,
    /// Loop time.
    pub looping: bool,
    /// One curve per animated scalar slot, quaternion components included.
    pub curves: Vec<BoundCurve>,
    /// First slots of quaternions written by `curves`; normalised after sampling.
    pub quats: Vec<Slot>,
    /// Rotations animated as Euler angles; each writes the 4 quaternion slots starting at its `slot`.
    pub eulers: Vec<EulerCurves>,
    /// Every slot the clip writes, ascending. Write Defaults reverts animated slots outside this set.
    pub slots: Vec<Slot>,
}

/// A curve on one slot.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundCurve {
    /// Written slot.
    pub slot: Slot,
    /// Keys in ascending time.
    pub keys: Vec<Key>,
}

/// Euler angle curves of one node, in degrees about our axes, applied Z, then X, then Y (ADR 0007).
#[derive(Clone, Debug, PartialEq)]
pub struct EulerCurves {
    /// First of the node's 4 quaternion slots.
    pub slot: Slot,
    /// X, Y and Z angle curves; components the clip lacks hold the rest pose's angle.
    pub axes: [Vec<Key>; 3],
}

impl BoundClip {
    /// Writes the clip's value at `time` seconds into every slot it animates; other slots are untouched.
    /// `out` must be at least [`crate::Bindings::len`] long.
    pub fn sample(&self, time: f32, out: &mut [f32]) {
        for c in &self.curves {
            out[c.slot as usize] = curve::eval(&c.keys, time);
        }
        for &q in &self.quats {
            let q = q as usize;
            let n = normalize([out[q], out[q + 1], out[q + 2], out[q + 3]]);
            out[q..q + 4].copy_from_slice(&n);
        }
        for e in &self.eulers {
            let deg = [0, 1, 2].map(|i| curve::eval(&e.axes[i], time));
            let s = e.slot as usize;
            out[s..s + 4].copy_from_slice(&euler_to_quat(deg));
        }
    }
}

/// Unit quaternion; a zero or non-finite one becomes the identity, as Unity's `Quaternion.Normalize` does.
pub fn normalize(q: Quat) -> Quat {
    let len = q.iter().map(|c| c * c).sum::<f32>().sqrt();
    if len > f32::EPSILON && len.is_finite() {
        q.map(|c| c / len)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// `[x, y, z]` degrees to a quaternion rotating about Z, then X, then Y: `q = qy * qx * qz`.
pub fn euler_to_quat(deg: [f32; 3]) -> Quat {
    let [(sx, cx), (sy, cy), (sz, cz)] = deg.map(|d| (d.to_radians() * 0.5).sin_cos());
    // qy * qx, then * qz.
    let (x, y, z, w) = (cy * sx, sy * cx, -sy * sx, cy * cx);
    [x * cz + y * sz, y * cz - x * sz, w * sz + z * cz, w * cz - z * sz]
}

/// The inverse of [`euler_to_quat`] for a unit quaternion: `[x, y, z]` degrees, x in -90..=90.
pub fn quat_to_euler(q: Quat) -> [f32; 3] {
    let [x, y, z, w] = q;
    // Rotation matrix entries of R = Ry * Rx * Rz.
    let r12 = 2.0 * (y * z - w * x);
    let r02 = 2.0 * (x * z + w * y);
    let r22 = 1.0 - 2.0 * (x * x + y * y);
    let r10 = 2.0 * (x * y + w * z);
    let r11 = 1.0 - 2.0 * (x * x + z * z);
    let ax = (-r12).clamp(-1.0, 1.0).asin();
    let (ay, az) = if r12.abs() < 0.999_999 {
        (r02.atan2(r22), r10.atan2(r11))
    } else {
        // Gimbal lock: Y and Z rotate about the same axis; put it all on Y.
        let r00 = 1.0 - 2.0 * (y * y + z * z);
        let r20 = 2.0 * (x * z - w * y);
        ((-r20).atan2(r00), 0.0)
    };
    [ax, ay, az].map(f32::to_degrees)
}
