//! A clip bound to slots, and sampling it into the slot buffer.

use civbb_format::animator::Key;

use civbb_core::math::{euler_to_quat, qnormalize};

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
            let n = qnormalize([out[q], out[q + 1], out[q + 2], out[q + 3]]);
            out[q..q + 4].copy_from_slice(&n);
        }
        for e in &self.eulers {
            let deg = [0, 1, 2].map(|i| curve::eval(&e.axes[i], time));
            let s = e.slot as usize;
            out[s..s + 4].copy_from_slice(&euler_to_quat(deg));
        }
    }
}
