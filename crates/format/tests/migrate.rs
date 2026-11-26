//! Per-extension migrations (ADR 0087): a test-only chain moves a frozen fixture through two versions, versions
//! outside the chain are refused by name, and every fixture directory holds one fixture per version.

use std::path::{Path, PathBuf};

use civbb_format::migrate::{MigrateError, STEPS, Step, migrate_with, newest, newest_in};
use civbb_format::state::StateTable;
use serde_json::{Map, Value, json};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture(extension: &str, version: u64) -> Value {
    let path = fixtures().join(extension).join(format!("v{version}.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

fn rename(o: &mut Map<String, Value>, from: &str, to: &str) -> Result<(), String> {
    let v = o.remove(from).ok_or(format!("no `{from}`"))?;
    o.insert(to.into(), v);
    Ok(())
}

/// A test-only version 2 of `CIVBB_state` calls `entries` `items`; version 3 calls them `entries` again.
const TEST_STEPS: &[Step] = &[
    Step {
        extension: "CIVBB_state",
        from: 2,
        run: |o| rename(o, "items", "entries"),
    },
    Step {
        extension: "CIVBB_state",
        from: 1,
        run: |o| rename(o, "entries", "items"),
    },
];

#[test]
fn chain_migrates_a_frozen_fixture() {
    assert_eq!(newest_in(TEST_STEPS, "CIVBB_state"), 3);
    assert_eq!(newest_in(TEST_STEPS, "CIVBB_avatar"), 1);
    let v1 = fixture("CIVBB_state", 1);
    let mut expected = v1.as_object().unwrap().clone();
    expected.remove("version");
    // Version 1 runs both steps, in order of `from`.
    let migrated = migrate_with(TEST_STEPS, "CIVBB_state", &v1).unwrap();
    assert_eq!(migrated, expected);
    let table: StateTable = serde_json::from_value(Value::Object(migrated)).unwrap();
    assert!(!table.entries.is_empty());
    // Version 2 runs only the second.
    let mut v2 = expected.clone();
    rename(&mut v2, "entries", "items").unwrap();
    v2.insert("version".into(), 2.into());
    assert_eq!(
        migrate_with(TEST_STEPS, "CIVBB_state", &Value::Object(v2)).unwrap(),
        expected
    );
    // Version 3 is read as is.
    let mut v3 = v1.clone();
    v3["version"] = 3.into();
    assert_eq!(migrate_with(TEST_STEPS, "CIVBB_state", &v3).unwrap(), expected);
}

#[test]
fn versions_outside_the_chain_are_refused() {
    let state = |version: Value| json!({"version": version, "entries": []});
    let unsupported = |found: &str| MigrateError::Unsupported {
        extension: "CIVBB_state".into(),
        found: found.into(),
        newest: 3,
    };
    for (version, found) in [
        (json!(4), "4"),
        (json!(0), "0"),
        (json!("1"), "\"1\""),
        (json!(1.5), "1.5"),
    ] {
        assert_eq!(
            migrate_with(TEST_STEPS, "CIVBB_state", &state(version)),
            Err(unsupported(found))
        );
    }
    let e = migrate_with(TEST_STEPS, "CIVBB_state", &json!({"entries": []})).unwrap_err();
    assert_eq!(e, MigrateError::NoVersion("CIVBB_state".into()));
    assert_eq!(e.to_string(), "CIVBB_state: no version");
    let e = migrate_with(TEST_STEPS, "CIVBB_state", &json!([])).unwrap_err();
    assert_eq!(e, MigrateError::NotObject("CIVBB_state".into()));
    // A step that cannot read its input names the extension and version.
    let e = migrate_with(TEST_STEPS, "CIVBB_state", &json!({"version": 1})).unwrap_err();
    assert_eq!(e.to_string(), "CIVBB_state: migrating version 1: no `entries`");
    // A gap in the steps is reported, not skipped.
    let e = migrate_with(&TEST_STEPS[..1], "CIVBB_state", &state(json!(1))).unwrap_err();
    assert_eq!(
        e,
        MigrateError::MissingStep {
            extension: "CIVBB_state".into(),
            from: 1
        }
    );
}

/// Each extension's steps read versions 1 to newest - 1, once each.
#[test]
fn steps_have_no_gaps() {
    for s in STEPS {
        let mut froms: Vec<u64> = STEPS
            .iter()
            .filter(|t| t.extension == s.extension)
            .map(|t| t.from)
            .collect();
        froms.sort_unstable();
        assert_eq!(froms, (1..newest(s.extension)).collect::<Vec<_>>(), "{}", s.extension);
    }
}

/// Every extension's directory holds `v1.json` to `v<newest>.json` and nothing else (ADR 0087 point 2).
#[test]
fn one_fixture_per_version() {
    let mut extensions = 0;
    for dir in std::fs::read_dir(fixtures()).unwrap() {
        let dir = dir.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let extension = dir.file_name().unwrap().to_str().unwrap().to_owned();
        let mut files: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|f| f.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort_unstable();
        let mut expected: Vec<String> = (1..=newest(&extension)).map(|v| format!("v{v}.json")).collect();
        expected.sort_unstable();
        assert_eq!(files, expected, "{extension}");
        for v in 1..=newest(&extension) {
            assert_eq!(fixture(&extension, v)["version"], v, "{extension} v{v}");
        }
        extensions += 1;
    }
    assert_eq!(extensions, 8);
}
