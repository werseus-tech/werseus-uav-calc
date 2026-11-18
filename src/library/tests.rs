// SPDX-License-Identifier: AGPL-3.0-or-later
//! Headless tests for the component library. Files are only written below a temp directory.

use super::samples::examples;
use super::*;

fn motor(name: &str) -> Motor {
    examples().motors[1].clone().with_name(name)
}

trait WithName {
    fn with_name(self, name: &str) -> Self;
}

impl WithName for Motor {
    fn with_name(mut self, name: &str) -> Self {
        self.name = name.into();
        self.source = Source::User;
        self
    }
}

fn user_lib() -> ComponentLibrary {
    let mut lib = ComponentLibrary::default();
    lib.add(motor("My motor")).unwrap();
    let mut b = examples().batteries[0].clone();
    b.name = "My pack".into();
    b.source = Source::User;
    lib.add(b).unwrap();
    lib
}

#[test]
fn json_round_trip() {
    let lib = user_lib();
    let back = ComponentLibrary::from_json(&lib.to_json()).unwrap();
    assert_eq!(lib, back);
    // Imported source and optional fields survive too.
    let mut lib = examples();
    lib.propellers[0].ct = Some(0.1);
    lib.escs[0].burst_a = None;
    lib.motors[0].mount = None;
    lib.motors[1].source = Source::Imported("friend.json".into());
    assert_eq!(ComponentLibrary::from_json(&lib.to_json()).unwrap(), lib);
}

#[test]
fn schema_version_is_checked() {
    let mut v: serde_json::Value = serde_json::from_str(&user_lib().to_json()).unwrap();
    v["schema_version"] = (SCHEMA_VERSION + 1).into();
    assert_eq!(
        ComponentLibrary::from_json(&v.to_string()),
        Err(LibraryError::Schema {
            found: SCHEMA_VERSION + 1
        })
    );
    v["schema_version"] = 0.into();
    assert!(matches!(
        ComponentLibrary::from_json(&v.to_string()),
        Err(LibraryError::Schema { found: 0 })
    ));
    v.as_object_mut().unwrap().remove("schema_version");
    assert!(matches!(
        ComponentLibrary::from_json(&v.to_string()),
        Err(LibraryError::Parse(_))
    ));
    assert!(matches!(
        ComponentLibrary::from_json("not json"),
        Err(LibraryError::Parse(_))
    ));
    assert_eq!(ComponentLibrary::default().schema_version, SCHEMA_VERSION);
}

#[test]
fn validation_rejects_bad_values() {
    let good = motor("ok");
    assert!(good.validate().is_empty());

    let mut m = good.clone();
    m.kv = f64::NAN;
    let e = m.validate();
    assert!(
        e.iter()
            .any(|s| s.contains("kv") && s.contains("not a number"))
    );

    let mut m = good.clone();
    m.mass_g = -5.0;
    assert!(m.validate().iter().any(|s| s.contains("mass")));

    let mut m = good.clone();
    m.max_current_a = f64::INFINITY;
    assert!(!m.validate().is_empty());

    let mut m = good.clone();
    m.kv = 1.0e9;
    assert!(m.validate().iter().any(|s| s.contains("plausible range")));

    let mut m = good.clone();
    m.name = "  ".into();
    assert!(m.validate().iter().any(|s| s.contains("name")));

    let mut p = examples().propellers[0].clone();
    p.blades = 0;
    p.ct = Some(-0.1);
    assert_eq!(p.validate().len(), 2);

    let mut e = examples().escs[0].clone();
    e.cells_min = 6;
    e.cells_max = 3;
    e.burst_a = Some(1.0);
    assert!(e.validate().len() >= 2);

    let mut b = examples().batteries[0].clone();
    b.cells_s = 0;
    b.capacity_mah = 0.0;
    assert!(b.validate().len() >= 2);

    // The library refuses to add or save bad records.
    let mut lib = ComponentLibrary::default();
    let mut bad = good.clone();
    bad.r_internal_ohm = -1.0;
    assert!(lib.add(bad).is_err());
    assert!(lib.is_empty());
}

#[test]
fn bad_values_in_a_file_are_reported() {
    let mut lib = user_lib();
    lib.motors[0].mass_g = -1.0;
    let text = serde_json::to_string(&lib).unwrap();
    assert!(matches!(
        ComponentLibrary::from_json(&text),
        Err(LibraryError::Invalid(_))
    ));
    let dir = tempfile::tempdir().unwrap();
    assert!(lib.save(&dir.path().join("x.json")).is_err());
    assert!(!dir.path().join("x.json").exists());
}

#[test]
fn add_replace_remove_with_unique_names() {
    let mut lib = ComponentLibrary::default();
    lib.add(motor("A")).unwrap();
    assert!(
        lib.add(motor(" a ")).is_err(),
        "names compare case-insensitively"
    );
    lib.add(motor("B")).unwrap();
    assert!(lib.replace("A", motor("B")).is_err());
    lib.replace("A", motor("C")).unwrap();
    assert!(lib.find::<Motor>("c").is_some());
    assert!(lib.find::<Motor>("A").is_none());
    assert!(lib.replace("missing", motor("Z")).is_err());
    assert!(lib.remove::<Motor>("C"));
    assert!(!lib.remove::<Motor>("C"));
    assert_eq!(lib.motors.len(), 1);
}

#[test]
fn import_merges_and_detects_duplicates() {
    let mut mine = user_lib();
    let mut theirs = user_lib();
    // Same name, different values: conflict. New name: added. Invalid: skipped.
    theirs.motors[0].kv += 100.0;
    theirs.motors.push(motor("Friend motor"));
    let mut bad = motor("Broken");
    bad.mass_g = f64::NAN;
    theirs.motors.push(bad);

    let rep = mine.merge(theirs, "friend.json");
    assert_eq!(
        rep.added,
        vec![(Category::Motor, "Friend motor".to_string())]
    );
    assert_eq!(
        rep.conflicts,
        vec![(Category::Motor, "My motor".to_string())]
    );
    assert_eq!(
        rep.duplicates,
        vec![(Category::Battery, "My pack".to_string())]
    );
    assert_eq!(rep.invalid.len(), 1);
    assert_eq!(
        mine.find::<Motor>("Friend motor").unwrap().source,
        Source::Imported("friend.json".into())
    );
    // Existing records are untouched.
    assert_eq!(mine.find::<Motor>("My motor").unwrap().source, Source::User);

    // Importing the same file again adds nothing.
    let again = mine.merge(user_lib(), "friend.json");
    assert!(again.added.is_empty());
    assert_eq!(mine.motors.len(), 2);
}

#[test]
fn export_then_import_gives_identical_records() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = user_lib();
    lib.add(examples().propellers[2].clone().tap_user())
        .unwrap();
    let file = dir.path().join("share.json");
    let keys = vec![
        (Category::Motor, "My motor".to_string()),
        (Category::Propeller, "Example 9x4.7x2".to_string()),
    ];
    assert_eq!(lib.export_file(&keys, &file).unwrap(), 2);

    let mut other = ComponentLibrary::default();
    let rep = other.import_file(&file).unwrap();
    assert_eq!(rep.added.len(), 2);
    assert!(rep.invalid.is_empty() && rep.conflicts.is_empty());
    assert_eq!(other.batteries.len(), 0, "only selected items are exported");

    let src = Source::Imported("share.json".into());
    let mut want = lib.select(&keys);
    want.motors[0].source = src.clone();
    want.propellers[0].source = src;
    assert_eq!(other, want);
}

trait TapUser {
    fn tap_user(self) -> Self;
}

impl TapUser for Propeller {
    fn tap_user(mut self) -> Self {
        self.source = Source::User;
        self
    }
}

#[test]
fn import_rejects_unsupported_files_without_changes() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("new.json");
    std::fs::write(&f, r#"{"schema_version": 99}"#).unwrap();
    let mut lib = user_lib();
    let before = lib.clone();
    assert!(matches!(
        lib.import_file(&f),
        Err(LibraryError::Schema { found: 99 })
    ));
    assert!(lib.import_file(&dir.path().join("missing.json")).is_err());
    assert_eq!(lib, before);
}

#[test]
fn save_and_load_use_the_given_directory() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join(store::FILE_NAME);
    assert_eq!(
        ComponentLibrary::load(&path).unwrap(),
        ComponentLibrary::default()
    );
    let lib = user_lib();
    lib.save(&path).unwrap();
    assert_eq!(ComponentLibrary::load(&path).unwrap(), lib);
    assert!(!path.with_extension("json.tmp").exists());
    assert!(store::default_path().ends_with(store::FILE_NAME));
}

#[test]
fn sample_list_is_valid_and_labelled() {
    let ex = examples();
    assert!(ex.validate().is_empty(), "{:?}", ex.validate());
    assert_eq!(ex.motors.len(), 5);
    assert_eq!(ex.propellers.len(), 5);
    assert_eq!(ex.escs.len(), 5);
    assert_eq!(ex.batteries.len(), 5);
    assert!(ex.motors.iter().all(|m| m.source == Source::Example
        && m.notes.contains("check the datasheet")
        && m.mount.is_some()));
    assert!(
        ex.batteries
            .iter()
            .all(|b| b.notes.contains("check the datasheet"))
    );
}
