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

/// Network sync of an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncMode {
    /// Local only.
    None,
    /// The owning client sends it.
    Owner,
}
