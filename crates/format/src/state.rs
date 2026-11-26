//! The document's state table (ADR 0088): typed entries every reader and writer references by index.

use serde::{Deserialize, Serialize};

/// Index into [`StateTable::entries`].
pub type EntryIndex = u32;

/// The shared state of a document. Animator conditions, blend inputs, drivers, menu controls and contact receivers
/// reference entries by index; names are looked up only at the edges (OSC, plugins, tools).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StateTable {
    /// Entries; names are unique.
    pub entries: Vec<Entry>,
}

impl StateTable {
    /// The entry called `name`.
    pub fn find(&self, name: &str) -> Option<EntryIndex> {
        self.entries
            .iter()
            .position(|e| e.name == name)
            .map(|i| i as EntryIndex)
    }

    /// Bits the synced entries cost in the network budget ([`Entry::sync_bits`]).
    pub fn synced_bits(&self) -> u32 {
        self.entries
            .iter()
            .filter(|e| e.sync == SyncMode::Owner)
            .map(Entry::sync_bits)
            .sum()
    }
}

/// One typed value of the state table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The creator's name; `/` separates namespaces.
    pub name: String,
    /// Type.
    pub kind: EntryKind,
    /// Value an instance starts at; bools and triggers use 0 or 1.
    pub default: f32,
    /// The values an int or float takes, which decide its sync encoding (ADR 0098); `None`: unbounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    /// Value Write Defaults restores where an animation curve writes this entry, when it is not `default` (lowered
    /// Unity content: the animator parameter's default, while `default` is the expression parameter's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<f32>,
    /// Who sends it over the network.
    pub sync: SyncMode,
    /// Persisted per user across sessions.
    pub saved: bool,
    /// Inputs from outside the content (menu, OSC, plugins) may write it.
    pub external: bool,
}

impl Entry {
    /// A local entry only the content writes.
    pub fn new(name: impl Into<String>, kind: EntryKind, default: f32) -> Self {
        Self {
            name: name.into(),
            kind,
            default,
            range: None,
            rest: None,
            sync: SyncMode::None,
            saved: false,
            external: false,
        }
    }

    /// The value Write Defaults restores where a curve writes this entry.
    pub fn rest(&self) -> f32 {
        self.rest.unwrap_or(self.default)
    }

    /// Bits a synced value of this entry costs on the wire (ADR 0098): bool 1, trigger 0 (never synced), ranged int
    /// ceil(log2(span + 1)), ranged float its `bits`, unranged int or float 32.
    pub fn sync_bits(&self) -> u32 {
        match (self.kind, self.range) {
            (EntryKind::Bool, _) => 1,
            (EntryKind::Trigger, _) => 0,
            (_, Some(Range::Int { min, max })) => u32::BITS - int_span(min, max).leading_zeros(),
            (_, Some(Range::Float { bits, .. })) => u32::from(bits),
            (EntryKind::Int | EntryKind::Float, None) => 32,
        }
    }

    /// `value` as its wire code in the low [`Entry::sync_bits`] bits: clamped to the range, ints rounded, ranged
    /// floats quantised to `2^bits - 2` equal steps so the midpoint is exact (ADR 0098).
    pub fn encode(&self, value: f32) -> u32 {
        match (self.kind, self.range) {
            (EntryKind::Bool | EntryKind::Trigger, _) => u32::from(value != 0.0),
            (_, Some(Range::Int { min, max })) => {
                let v = (value.round() as i64).clamp(i64::from(min), i64::from(max));
                (v - i64::from(min)) as u32
            }
            (_, Some(Range::Float { min, max, bits })) => {
                let t = ((value - min) / (max - min)).clamp(0.0, 1.0);
                (t * float_steps(bits) as f32).round() as u32
            }
            (EntryKind::Int, None) => value.round() as i32 as u32,
            (EntryKind::Float, None) => value.to_bits(),
        }
    }

    /// The value a wire code from [`Entry::encode`] stands for; codes past the range's end clamp to it.
    pub fn decode(&self, code: u32) -> f32 {
        match (self.kind, self.range) {
            (EntryKind::Bool | EntryKind::Trigger, _) => f32::from(u8::from(code & 1 == 1)),
            (_, Some(Range::Int { min, max })) => (i64::from(min) + i64::from(code)).min(i64::from(max)) as f32,
            (_, Some(Range::Float { min, max, bits })) => {
                let steps = float_steps(bits);
                min + (max - min) * (code.min(steps) as f32 / steps as f32)
            }
            (EntryKind::Int, None) => code as i32 as f32,
            (EntryKind::Float, None) => f32::from_bits(code),
        }
    }
}

/// The values a ranged int or float takes (ADR 0098).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Range {
    /// Integers `min ..= max`.
    Int {
        /// Least value.
        min: i32,
        /// Greatest value, at least `min`.
        max: i32,
    },
    /// `min ..= max` quantised to `bits` bits, 2 to 16.
    Float {
        /// Least value, finite.
        min: f32,
        /// Greatest value, finite and above `min`.
        max: f32,
        /// Wire bits.
        bits: u8,
    },
}

impl Range {
    /// Why this range cannot belong to an entry of type `kind`, if it cannot.
    pub fn check(self, kind: EntryKind) -> Result<(), &'static str> {
        match self {
            Range::Int { min, max } if kind == EntryKind::Int => (min <= max).then_some(()).ok_or("range min > max"),
            Range::Float { min, max, bits } if kind == EntryKind::Float => {
                let ordered = min.is_finite() && max.is_finite() && min < max;
                ordered
                    .then_some(())
                    .ok_or("float range must be finite with min < max")?;
                (2..=16)
                    .contains(&bits)
                    .then_some(())
                    .ok_or("float range bits must be 2 to 16")
            }
            _ => Err("range does not match the entry's type"),
        }
    }
}

/// `max - min` of an int range, as the largest code.
fn int_span(min: i32, max: i32) -> u32 {
    (i64::from(max) - i64::from(min)).clamp(0, i64::from(u32::MAX)) as u32
}

/// Steps of a float range of `bits`: the codes run `0 ..= 2^bits - 2`.
fn float_steps(bits: u8) -> u32 {
    (1u32 << bits.clamp(2, 16)) - 2
}

/// Entry types.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    Float,
    Int,
    Bool,
    /// A bool that a transition testing it resets.
    Trigger,
}

/// Values the platform provides every frame, as entries named `platform/<name>` (ADR 0088 point 3, ADR 0096):
/// read-only to content, never synced, saved or external. Values are in the units VRChat provides its built-ins in
/// (metres, metres per second, ratios); [`Platform::vrchat_name`] is the table the Unity importer and OSC (§17) map
/// VRChat's names with.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
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
    EyeHeight,
    EyeHeightPercent,
}

/// The namespace of [`Platform`] entries; content entries may not use it.
pub const PLATFORM_NAMESPACE: &str = "platform/";

impl Platform {
    /// Every platform value.
    pub const ALL: [Platform; 30] = {
        use Platform::*;
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
            EyeHeight,
            EyeHeightPercent,
        ]
    };

    /// The native entry name, `platform/` included.
    pub fn name(self) -> &'static str {
        use Platform::*;
        match self {
            IsLocal => "platform/is_local",
            PreviewMode => "platform/preview_mode",
            Viseme => "platform/viseme",
            Voice => "platform/voice",
            GestureLeft => "platform/gesture_left",
            GestureRight => "platform/gesture_right",
            GestureLeftWeight => "platform/gesture_left_weight",
            GestureRightWeight => "platform/gesture_right_weight",
            AngularY => "platform/angular_y",
            VelocityX => "platform/velocity_x",
            VelocityY => "platform/velocity_y",
            VelocityZ => "platform/velocity_z",
            VelocityMagnitude => "platform/velocity_magnitude",
            Upright => "platform/upright",
            Grounded => "platform/grounded",
            Seated => "platform/seated",
            Afk => "platform/afk",
            TrackingType => "platform/tracking_type",
            VrMode => "platform/vr_mode",
            MuteSelf => "platform/mute_self",
            InStation => "platform/in_station",
            Earmuffs => "platform/earmuffs",
            IsOnFriendsList => "platform/is_on_friends_list",
            AvatarVersion => "platform/avatar_version",
            IsAnimatorEnabled => "platform/is_animator_enabled",
            ScaleModified => "platform/scale_modified",
            ScaleFactor => "platform/scale_factor",
            ScaleFactorInverse => "platform/scale_factor_inverse",
            EyeHeight => "platform/eye_height",
            EyeHeightPercent => "platform/eye_height_percent",
        }
    }

    /// VRChat's name for the built-in parameter.
    pub fn vrchat_name(self) -> &'static str {
        use Platform::*;
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
            EyeHeight => "EyeHeightAsMeters",
            EyeHeightPercent => "EyeHeightAsPercent",
        }
    }

    /// The platform value called `name` natively.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    /// The platform value VRChat calls `name`.
    pub fn from_vrchat_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.vrchat_name() == name)
    }

    /// The type the platform provides it as; an entry may declare another, which the value is converted to.
    pub fn kind(self) -> EntryKind {
        use Platform::*;
        match self {
            IsLocal | Grounded | Seated | Afk | MuteSelf | InStation | Earmuffs | IsOnFriendsList
            | IsAnimatorEnabled | ScaleModified => EntryKind::Bool,
            PreviewMode | Viseme | GestureLeft | GestureRight | TrackingType | VrMode | AvatarVersion => EntryKind::Int,
            _ => EntryKind::Float,
        }
    }
}

/// Network sync of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncMode {
    /// Local only.
    None,
    /// The owning client sends it.
    Owner,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::sample_avatar;
    use crate::{Caps, validate};

    fn ranged(kind: EntryKind, range: Option<Range>) -> Entry {
        Entry {
            range,
            sync: SyncMode::Owner,
            ..Entry::new("x", kind, 0.0)
        }
    }

    fn float(min: f32, max: f32, bits: u8) -> Entry {
        ranged(EntryKind::Float, Some(Range::Float { min, max, bits }))
    }

    /// Encodes `value`, checks the code fits the entry's bits, and decodes it.
    fn round_trip(e: &Entry, value: f32) -> f32 {
        let code = e.encode(value);
        let bits = e.sync_bits();
        assert!(
            bits == 32 || code >> bits == 0,
            "{value} -> {code} needs more than {bits} bits"
        );
        e.decode(code)
    }

    #[test]
    fn sync_bits_follow_type_and_range() {
        let int = |min, max| ranged(EntryKind::Int, Some(Range::Int { min, max }));
        let bits = |e: Entry| e.sync_bits();
        assert_eq!(bits(ranged(EntryKind::Bool, None)), 1);
        assert_eq!(bits(ranged(EntryKind::Trigger, None)), 0);
        assert_eq!(bits(int(0, 255)), 8);
        assert_eq!(bits(int(0, 256)), 9);
        assert_eq!(bits(int(5, 5)), 0);
        assert_eq!(bits(int(-1, 1)), 2);
        assert_eq!(bits(int(i32::MIN, i32::MAX)), 32);
        assert_eq!(bits(float(-1.0, 1.0, 8)), 8);
        assert_eq!(bits(float(0.0, 1.0, 12)), 12);
        assert_eq!(bits(ranged(EntryKind::Int, None)), 32);
        assert_eq!(bits(ranged(EntryKind::Float, None)), 32);
    }

    #[test]
    fn values_round_trip_at_the_range_edges() {
        let bool = ranged(EntryKind::Bool, None);
        assert_eq!((round_trip(&bool, 0.0), round_trip(&bool, 1.0)), (0.0, 1.0));

        let int = ranged(EntryKind::Int, Some(Range::Int { min: -3, max: 252 }));
        for (v, back) in [
            (-3.0, -3.0),
            (252.0, 252.0),
            (0.0, 0.0),
            (-9.0, -3.0),
            (300.0, 252.0),
            (1.6, 2.0),
        ] {
            assert_eq!(round_trip(&int, v), back, "int {v}");
        }
        let full = ranged(
            EntryKind::Int,
            Some(Range::Int {
                min: i32::MIN,
                max: i32::MAX,
            }),
        );
        assert_eq!(full.encode(i32::MIN as f32), 0);
        assert_eq!(full.encode(i32::MAX as f32), u32::MAX);

        // -1..1 in 8 bits: steps of 1/127, so -1, 0 and 1 are exact.
        let f = float(-1.0, 1.0, 8);
        for (v, back) in [(-1.0, -1.0), (0.0, 0.0), (1.0, 1.0), (-5.0, -1.0), (5.0, 1.0)] {
            assert_eq!(round_trip(&f, v), back, "float {v}");
        }
        assert_eq!(f.encode(1.0), 254);
        assert!((round_trip(&f, 0.5) - 0.5).abs() <= 0.5 / 127.0);
        assert_eq!(f.decode(255), 1.0, "the unused code clamps");
        let f = float(2.0, 10.0, 16);
        assert_eq!(
            (round_trip(&f, 2.0), round_trip(&f, 6.0), round_trip(&f, 10.0)),
            (2.0, 6.0, 10.0)
        );

        let int = ranged(EntryKind::Int, None);
        assert_eq!((round_trip(&int, -7.0), round_trip(&int, 1e6)), (-7.0, 1e6));
        let float = ranged(EntryKind::Float, None);
        assert_eq!(round_trip(&float, -0.123), -0.123);
    }

    #[test]
    fn validation_checks_ranges_triggers_and_the_budget() {
        let check = |entry: Entry| {
            let mut a = sample_avatar();
            a.state.entries.push(Entry {
                name: "new".into(),
                ..entry
            });
            validate(&a, &Caps::default(), |_| true)
        };
        check(float(-1.0, 1.0, 8)).unwrap();
        check(ranged(EntryKind::Int, Some(Range::Int { min: 0, max: 0 }))).unwrap();
        check(float(1.0, 1.0, 8)).unwrap_err();
        check(float(0.0, f32::INFINITY, 8)).unwrap_err();
        check(float(0.0, 1.0, 1)).unwrap_err();
        check(float(0.0, 1.0, 17)).unwrap_err();
        check(ranged(EntryKind::Int, Some(Range::Int { min: 1, max: 0 }))).unwrap_err();
        check(ranged(EntryKind::Bool, Some(Range::Int { min: 0, max: 1 }))).unwrap_err();
        check(ranged(EntryKind::Float, Some(Range::Int { min: 0, max: 1 }))).unwrap_err();
        check(ranged(EntryKind::Trigger, None)).unwrap_err();

        // A native document over the 1,024-bit budget: unranged values cost 32 bits each.
        let mut a = sample_avatar();
        let used = a.state.synced_bits();
        let unranged = (1024 - used) / 32;
        for i in 0..unranged {
            a.state.entries.push(Entry {
                name: format!("n{i}"),
                ..ranged(EntryKind::Float, None)
            });
        }
        let pad = 1024 - a.state.synced_bits();
        a.state.entries.extend((0..pad).map(|i| Entry {
            name: format!("b{i}"),
            ..ranged(EntryKind::Bool, None)
        }));
        assert_eq!(a.state.synced_bits(), 1024);
        validate(&a, &Caps::default(), |_| true).unwrap();
        a.state.entries.push(Entry {
            name: "over".into(),
            ..ranged(EntryKind::Bool, None)
        });
        let errors = validate(&a, &Caps::default(), |_| true).unwrap_err();
        assert!(
            errors.iter().any(|e| e.problem.contains("synced bits over cap")),
            "{errors:?}"
        );
    }
}
