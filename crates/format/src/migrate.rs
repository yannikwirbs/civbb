//! Per-extension versions of the `CIVBB_*` source extensions and the JSON migrations between them (ADR 0087).
//!
//! An extension's newest version is 1 plus its number of [`Step`]s in [`STEPS`]. A breaking change to an
//! extension adds one step from the old newest version and a frozen fixture `tests/fixtures/<extension>/v<N>.json`.

use serde_json::{Map, Value};

/// One breaking change of one extension: rewrites version `from`'s JSON (without `version`) into `from + 1`'s.
pub struct Step {
    /// The extension's name, e.g. `CIVBB_state`.
    pub extension: &'static str,
    /// The version this step reads.
    pub from: u64,
    /// The rewrite; an error names what the old JSON lacks.
    pub run: fn(&mut Map<String, Value>) -> Result<(), String>,
}

/// Every migration of every extension. Empty: every extension is at version 1.
pub const STEPS: &[Step] = &[];

/// Why an extension object cannot be brought to the newest version.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum MigrateError {
    /// The extension is not a JSON object.
    #[error("{0}: not an object")]
    NotObject(String),
    /// The object has no `version`.
    #[error("{0}: no version")]
    NoVersion(String),
    /// The version is not an integer from 1 to the newest.
    #[error("{extension}: version {found}, this importer reads version 1 to {newest}")]
    Unsupported {
        /// The extension's name.
        extension: String,
        /// The `version` as written.
        found: String,
        /// The newest version read.
        newest: u64,
    },
    /// [`STEPS`] has a gap.
    #[error("{extension}: no migration from version {from}")]
    MissingStep {
        /// The extension's name.
        extension: String,
        /// The version without a step.
        from: u64,
    },
    /// A step failed.
    #[error("{extension}: migrating version {from}: {reason}")]
    Step {
        /// The extension's name.
        extension: String,
        /// The version the step reads.
        from: u64,
        /// The step's error.
        reason: String,
    },
}

/// The newest version of `extension` this crate reads.
pub fn newest(extension: &str) -> u64 {
    newest_in(STEPS, extension)
}

/// The newest version of `extension` given `steps`.
pub fn newest_in(steps: &[Step], extension: &str) -> u64 {
    1 + steps.iter().filter(|s| s.extension == extension).count() as u64
}

/// Checks `value`'s `version` and migrates it to the newest version; the result has no `version`.
pub fn migrate(extension: &str, value: &Value) -> Result<Map<String, Value>, MigrateError> {
    migrate_with(STEPS, extension, value)
}

/// [`migrate`] with the given steps (tests use their own).
pub fn migrate_with(steps: &[Step], extension: &str, value: &Value) -> Result<Map<String, Value>, MigrateError> {
    let mut o = value
        .as_object()
        .ok_or_else(|| MigrateError::NotObject(extension.into()))?
        .clone();
    let newest = newest_in(steps, extension);
    let found = o
        .remove("version")
        .ok_or_else(|| MigrateError::NoVersion(extension.into()))?;
    let version = found
        .as_u64()
        .filter(|v| (1..=newest).contains(v))
        .ok_or_else(|| MigrateError::Unsupported {
            extension: extension.into(),
            found: found.to_string(),
            newest,
        })?;
    for from in version..newest {
        let step = steps
            .iter()
            .find(|s| s.extension == extension && s.from == from)
            .ok_or_else(|| MigrateError::MissingStep {
                extension: extension.into(),
                from,
            })?;
        (step.run)(&mut o).map_err(|reason| MigrateError::Step {
            extension: extension.into(),
            from,
            reason,
        })?;
    }
    Ok(o)
}
