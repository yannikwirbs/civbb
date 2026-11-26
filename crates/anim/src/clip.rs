//! A clip bound to slots, and sampling it into the slot buffer.

use civbb_format::animator::Key;

use civbb_core::math::{euler_zxy_to_quat, qnormalize};
use civbb_format::math::{Quat, Vec3};

use crate::bind::Slot;
use crate::body::RootMotion;
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
    /// Body pose curves; they write the 7 body slots starting at `slot` (ADR 0063).
    pub body: Option<BodyCurves>,
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

/// A humanoid clip's body pose curves (`RootT` then `RootQ`) and how its root-motion settings apply them.
#[derive(Clone, Debug, PartialEq)]
pub struct BodyCurves {
    /// First of the 7 body slots.
    pub slot: Slot,
    /// One curve per component; components the clip lacks hold 0 (and 1 for the rotation's w).
    pub axes: [Vec<Key>; 7],
    /// The settings, or `None` to take the curves as keyed.
    pub root: Option<RootMotion>,
}

impl BodyCurves {
    /// The raw body pose at `time` seconds: position and normalised rotation.
    pub fn eval(&self, time: f32) -> (Vec3, Quat) {
        let v: [f32; 7] = std::array::from_fn(|i| curve::eval(&self.axes[i], time));
        ([v[0], v[1], v[2]], qnormalize([v[3], v[4], v[5], v[6]]))
    }
}

/// Euler angle curves of one node, in radians about its axes, applied Z, then X, then Y (ADR 0007).
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
        if let Some(b) = &self.body {
            let mut pose = b.eval(time);
            if let Some(root) = &b.root {
                let w = if self.length > 0.0 {
                    (time / self.length).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                pose = root.apply(pose, w);
            }
            let s = b.slot as usize;
            out[s..s + 3].copy_from_slice(&pose.0);
            out[s + 3..s + 7].copy_from_slice(&pose.1);
        }
        for e in &self.eulers {
            let rad = [0, 1, 2].map(|i| curve::eval(&e.axes[i], time));
            let s = e.slot as usize;
            out[s..s + 4].copy_from_slice(&euler_zxy_to_quat(rad));
        }
    }
}
