//! Creator and licence metadata (VRM `meta`): who made an avatar and what others may do with it (ADR 0012).

use serde::{Deserialize, Serialize};

/// Creator information and licence terms.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    /// Authors, in credit order.
    pub authors: Vec<String>,
    /// The creator's version string.
    pub version: String,
    /// Copyright notice.
    pub copyright: String,
    /// How to contact the creator.
    pub contact: String,
    /// Works this avatar is based on.
    pub references: Vec<String>,
    /// What others may do with it.
    pub licence: Licence,
}

/// Licence terms, modelled on VRM 1.0. Defaults are the most restrictive choice.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Licence {
    /// URL of the licence document.
    pub url: String,
    /// Who may wear the avatar.
    pub avatar_use: AvatarUse,
    /// Excessively violent use allowed.
    pub violent: bool,
    /// Excessively sexual use allowed.
    pub sexual: bool,
    /// Political or religious use allowed.
    pub political_or_religious: bool,
    /// Antisocial or hateful use allowed.
    pub antisocial_or_hate: bool,
    /// Commercial use allowed.
    pub commercial: Commercial,
    /// Credit to the authors required.
    pub credit_required: bool,
    /// Redistribution allowed.
    pub redistribution: bool,
    /// Modification allowed.
    pub modification: Modification,
    /// URL of further terms.
    pub other_terms_url: String,
}

/// Who may wear the avatar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AvatarUse {
    /// Only the authors.
    #[default]
    OnlyAuthor,
    /// People the authors licensed separately.
    OnlySeparatelyLicensed,
    /// Everyone.
    Everyone,
}

/// Commercial use allowed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Commercial {
    /// Personal, non-profit use only.
    #[default]
    PersonalNonProfit,
    /// Personal use, profit allowed.
    PersonalProfit,
    /// Corporate use too.
    Corporation,
}

/// Modification allowed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Modification {
    /// No modification.
    #[default]
    Prohibited,
    /// Modification, not redistributing the result.
    Allow,
    /// Modification and redistributing the result.
    AllowRedistribution,
}
