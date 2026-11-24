//! VRM `meta` (0.x and 1.0) to the format's creator, licence and permission fields.

use civbb_format::meta::{AvatarUse, Commercial, Licence, Meta, Modification};
use serde_json::Value;

/// Creator and licence metadata; unknown or missing licence fields take the most restrictive value (ADR 0012).
pub(super) fn meta(m: &Value, v1: bool) -> Option<Meta> {
    m.as_object()?;
    let text = |k: &str| m[k].as_str().unwrap_or_default().to_owned();
    let list = |v: &Value| -> Vec<String> {
        match v {
            Value::Array(a) => a.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
            Value::String(s) if !s.is_empty() => vec![s.clone()],
            _ => Vec::new(),
        }
    };
    let allowed = |k: &str| m[k].as_bool().unwrap_or(false);
    if v1 {
        let avatar_use = match m["avatarPermission"].as_str() {
            Some("everyone") => AvatarUse::Everyone,
            Some("onlySeparatelyLicensedPerson") => AvatarUse::OnlySeparatelyLicensed,
            _ => AvatarUse::OnlyAuthor,
        };
        let commercial = match m["commercialUsage"].as_str() {
            Some("corporation") => Commercial::Corporation,
            Some("personalProfit") => Commercial::PersonalProfit,
            _ => Commercial::PersonalNonProfit,
        };
        let modification = match m["modification"].as_str() {
            Some("allowModificationRedistribution") => Modification::AllowRedistribution,
            Some("allowModification") => Modification::Allow,
            _ => Modification::Prohibited,
        };
        return Some(Meta {
            authors: list(&m["authors"]),
            version: text("version"),
            copyright: text("copyrightInformation"),
            contact: text("contactInformation"),
            references: list(&m["references"]),
            licence: Licence {
                url: text("licenseUrl"),
                avatar_use,
                violent: allowed("allowExcessivelyViolentUsage"),
                sexual: allowed("allowExcessivelySexualUsage"),
                political_or_religious: allowed("allowPoliticalOrReligiousUsage"),
                antisocial_or_hate: allowed("allowAntisocialOrHateUsage"),
                commercial,
                credit_required: m["creditNotation"].as_str() != Some("unnecessary"),
                redistribution: allowed("allowRedistribution"),
                modification,
                other_terms_url: text("otherLicenseUrl"),
            },
        });
    }
    let allow = |k: &str| m[k].as_str() == Some("Allow");
    let avatar_use = match m["allowedUserName"].as_str() {
        Some("Everyone") => AvatarUse::Everyone,
        Some("ExplicitlyLicensedPerson") => AvatarUse::OnlySeparatelyLicensed,
        _ => AvatarUse::OnlyAuthor,
    };
    // VRM 0.x names a licence; its terms become our fields, its deed our URL.
    let licence = m["licenseName"].as_str().unwrap_or_default();
    let (redistribution, modification, credit, non_commercial, deed) = match licence {
        "CC0" => (
            true,
            Modification::AllowRedistribution,
            false,
            false,
            "publicdomain/zero/1.0/",
        ),
        "CC_BY" => (true, Modification::AllowRedistribution, true, false, "licenses/by/"),
        "CC_BY_SA" => (true, Modification::AllowRedistribution, true, false, "licenses/by-sa/"),
        "CC_BY_NC" => (true, Modification::AllowRedistribution, true, true, "licenses/by-nc/"),
        "CC_BY_NC_SA" => (
            true,
            Modification::AllowRedistribution,
            true,
            true,
            "licenses/by-nc-sa/",
        ),
        "CC_BY_ND" => (true, Modification::Allow, true, false, "licenses/by-nd/"),
        "CC_BY_NC_ND" => (true, Modification::Allow, true, true, "licenses/by-nc-nd/"),
        _ => (false, Modification::Prohibited, true, false, ""),
    };
    let url = if deed.is_empty() {
        text("otherLicenseUrl")
    } else {
        format!("https://creativecommons.org/{deed}")
    };
    let commercial = if allow("commercialUssageName") && !non_commercial {
        Commercial::Corporation
    } else {
        Commercial::PersonalNonProfit
    };
    Some(Meta {
        authors: list(&m["author"]),
        version: text("version"),
        copyright: String::new(),
        contact: text("contactInformation"),
        references: list(&m["reference"]),
        licence: Licence {
            url,
            avatar_use,
            violent: allow("violentUssageName"),
            sexual: allow("sexualUssageName"),
            political_or_religious: false,
            antisocial_or_hate: false,
            commercial,
            credit_required: credit,
            redistribution,
            modification,
            other_terms_url: text("otherPermissionUrl"),
        },
    })
}
