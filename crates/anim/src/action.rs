//! Platform values at rest and state actions: entry operations, pose sources, layer weights and locomotion.

use civbb_format::animator::{EntryOp, PosePart, PoseSource};
use civbb_format::state::{Entry, EntryKind, Platform};

/// A platform value's value for an avatar standing still on the ground at its own scale, which an instance starts
/// at (#86); `None` leaves the entry's default until the engine sets it. A creator's default of 0 for `Grounded`
/// would start the avatar falling (VRChat's locomotion layer leaves Standing for SmallHop).
pub fn at_rest(p: Platform) -> Option<f32> {
    use Platform::*;
    match p {
        Grounded | Upright | IsAnimatorEnabled | ScaleFactor | ScaleFactorInverse => Some(1.0),
        AvatarVersion => Some(3.0),
        _ => None,
    }
}

/// `v` as a parameter of type `kind` holds it: ints round, bools and triggers are 1 when not 0 (ADR 0011).
pub fn coerce(kind: EntryKind, v: f32) -> f32 {
    match kind {
        EntryKind::Float => v,
        EntryKind::Int => v.round(),
        EntryKind::Bool | EntryKind::Trigger => f32::from(u8::from(v != 0.0)),
    }
}

/// Small deterministic generator for random driver operations (xorshift64*).
#[derive(Clone, Debug)]
pub struct Rng(pub u64);

impl Rng {
    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// Applies entry operations in order.
pub fn drive(ops: &[EntryOp], decls: &[Entry], params: &mut [f32], rng: &mut Rng) {
    for op in ops {
        let (dst, v) = match *op {
            EntryOp::Set { param, value } => (param, value),
            EntryOp::Add { param, value } => (param, params[param as usize] + value),
            EntryOp::Random {
                param,
                min,
                max,
                chance,
            } => {
                let r = rng.unit();
                let v = match decls[param as usize].kind {
                    EntryKind::Float => min + r * (max - min),
                    // Inclusive integer range.
                    EntryKind::Int => (min + r * (max - min + 1.0)).floor().min(max),
                    EntryKind::Bool | EntryKind::Trigger => f32::from(u8::from(r < chance)),
                };
                (param, v)
            }
            EntryOp::Copy { src, dst, remap } => {
                let v = params[src as usize];
                let v = match remap {
                    Some(([a, b], [c, d])) if b != a => c + (v - a) / (b - a) * (d - c),
                    Some((_, [c, _])) => c,
                    None => v,
                };
                (dst, v)
            }
        };
        params[dst as usize] = coerce(decls[dst as usize].kind, v);
    }
}

/// Where each body part takes its pose from, in [`PosePart`] order.
pub type PoseSources = [PoseSource; 10];

/// The index of a part in [`PoseSources`].
pub fn part_index(p: PosePart) -> usize {
    p as usize
}
