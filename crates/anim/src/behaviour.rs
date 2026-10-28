//! Legacy built-in parameters and state behaviours: parameter drivers, tracking, layer and locomotion control.

use civbb_format::animator::{DriverOp, ParamKind, Parameter, TrackingMode, TrackingPart};

/// Built-in parameters the engine sets each frame, with their legacy names and types (design doc §7).
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Builtin {
    IsLocal,
    PreviewMode,
    Viseme,
    Voice,
    GestureLeft,
    GestureRight,
    GestureLeftWeight,
    GestureRightWeight,
    AngularY,
    VelocityX,
    VelocityY,
    VelocityZ,
    VelocityMagnitude,
    Upright,
    Grounded,
    Seated,
    Afk,
    TrackingType,
    VrMode,
    MuteSelf,
    InStation,
    Earmuffs,
    IsOnFriendsList,
    AvatarVersion,
    IsAnimatorEnabled,
    ScaleModified,
    ScaleFactor,
    ScaleFactorInverse,
    EyeHeightAsMeters,
    EyeHeightAsPercent,
}

impl Builtin {
    /// Every built-in.
    pub const ALL: [Builtin; 30] = {
        use Builtin::*;
        [
            IsLocal,
            PreviewMode,
            Viseme,
            Voice,
            GestureLeft,
            GestureRight,
            GestureLeftWeight,
            GestureRightWeight,
            AngularY,
            VelocityX,
            VelocityY,
            VelocityZ,
            VelocityMagnitude,
            Upright,
            Grounded,
            Seated,
            Afk,
            TrackingType,
            VrMode,
            MuteSelf,
            InStation,
            Earmuffs,
            IsOnFriendsList,
            AvatarVersion,
            IsAnimatorEnabled,
            ScaleModified,
            ScaleFactor,
            ScaleFactorInverse,
            EyeHeightAsMeters,
            EyeHeightAsPercent,
        ]
    };

    /// The legacy parameter name animators use.
    pub fn name(self) -> &'static str {
        use Builtin::*;
        match self {
            IsLocal => "IsLocal",
            PreviewMode => "PreviewMode",
            Viseme => "Viseme",
            Voice => "Voice",
            GestureLeft => "GestureLeft",
            GestureRight => "GestureRight",
            GestureLeftWeight => "GestureLeftWeight",
            GestureRightWeight => "GestureRightWeight",
            AngularY => "AngularY",
            VelocityX => "VelocityX",
            VelocityY => "VelocityY",
            VelocityZ => "VelocityZ",
            VelocityMagnitude => "VelocityMagnitude",
            Upright => "Upright",
            Grounded => "Grounded",
            Seated => "Seated",
            Afk => "AFK",
            TrackingType => "TrackingType",
            VrMode => "VRMode",
            MuteSelf => "MuteSelf",
            InStation => "InStation",
            Earmuffs => "Earmuffs",
            IsOnFriendsList => "IsOnFriendsList",
            AvatarVersion => "AvatarVersion",
            IsAnimatorEnabled => "IsAnimatorEnabled",
            ScaleModified => "ScaleModified",
            ScaleFactor => "ScaleFactor",
            ScaleFactorInverse => "ScaleFactorInverse",
            EyeHeightAsMeters => "EyeHeightAsMeters",
            EyeHeightAsPercent => "EyeHeightAsPercent",
        }
    }

    /// The type the engine provides it as.
    pub fn kind(self) -> ParamKind {
        use Builtin::*;
        match self {
            IsLocal | Grounded | Seated | Afk | MuteSelf | InStation | Earmuffs | IsOnFriendsList
            | IsAnimatorEnabled | ScaleModified => ParamKind::Bool,
            PreviewMode | Viseme | GestureLeft | GestureRight | TrackingType | VrMode | AvatarVersion => ParamKind::Int,
            _ => ParamKind::Float,
        }
    }
}

/// `v` as a parameter of type `kind` holds it: ints round, bools and triggers are 1 when not 0 (ADR 0011).
pub fn coerce(kind: ParamKind, v: f32) -> f32 {
    match kind {
        ParamKind::Float => v,
        ParamKind::Int => v.round(),
        ParamKind::Bool | ParamKind::Trigger => f32::from(u8::from(v != 0.0)),
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

/// Applies parameter driver operations in order.
pub fn drive(ops: &[DriverOp], decls: &[Parameter], params: &mut [f32], rng: &mut Rng) {
    for op in ops {
        let (dst, v) = match *op {
            DriverOp::Set { param, value } => (param, value),
            DriverOp::Add { param, value } => (param, params[param as usize] + value),
            DriverOp::Random {
                param,
                min,
                max,
                chance,
            } => {
                let r = rng.unit();
                let v = match decls[param as usize].kind {
                    ParamKind::Float => min + r * (max - min),
                    // Inclusive integer range.
                    ParamKind::Int => (min + r * (max - min + 1.0)).floor().min(max),
                    ParamKind::Bool | ParamKind::Trigger => f32::from(u8::from(r < chance)),
                };
                (param, v)
            }
            DriverOp::Copy { src, dst, remap } => {
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

/// Who drives each body part (tracking control), in [`TrackingPart`] order.
pub type Tracking = [TrackingMode; 10];

/// The index of a part in [`Tracking`].
pub fn part_index(p: TrackingPart) -> usize {
    p as usize
}
