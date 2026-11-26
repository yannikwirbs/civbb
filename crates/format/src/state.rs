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

    /// Bits the synced entries cost in the network budget (bool and trigger 1, int and float 8; #118 replaces it).
    pub fn synced_bits(&self) -> u32 {
        let bits = |e: &Entry| match e.kind {
            EntryKind::Bool | EntryKind::Trigger => 1,
            EntryKind::Int | EntryKind::Float => 8,
        };
        self.entries
            .iter()
            .filter(|e| e.sync == SyncMode::Owner)
            .map(bits)
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
