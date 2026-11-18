// SPDX-License-Identifier: AGPL-3.0-or-later
//! The user's component library: JSON storage, validation, import (merge) and export.
//!
//! The same file format is used for the personal library on disk and for files people share.

use super::records::{Battery, Esc, Motor, Propeller, Record, Source};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// Current file format version. Files with a newer version are rejected, not guessed at.
pub const SCHEMA_VERSION: u32 = 1;

/// File name of the personal library inside the app data directory.
pub const FILE_NAME: &str = "component_library.json";

/// Default location of the personal library (next to the app's other data).
pub fn default_path() -> PathBuf {
    crate::support::app_data_dir().join(FILE_NAME)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Motor,
    Propeller,
    Esc,
    Battery,
}

impl Category {
    pub const ALL: [Category; 4] = [Self::Motor, Self::Propeller, Self::Esc, Self::Battery];
}

#[derive(Clone, Debug, PartialEq)]
pub enum LibraryError {
    Io(String),
    Parse(String),
    /// File written by a different (newer or invalid) schema version.
    Schema {
        found: u32,
    },
    Invalid(Vec<String>),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "file error: {e}"),
            Self::Parse(e) => write!(f, "not a valid component library file: {e}"),
            Self::Schema { found } => write!(
                f,
                "unsupported schema version {found} (this app reads version {SCHEMA_VERSION})"
            ),
            Self::Invalid(list) => write!(f, "invalid values: {}", list.join("; ")),
        }
    }
}

impl std::error::Error for LibraryError {}

/// Result of an import.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    /// Items added to the library.
    pub added: Vec<(Category, String)>,
    /// Items skipped because an identical record already exists.
    pub duplicates: Vec<(Category, String)>,
    /// Items skipped because the same name exists with different values.
    pub conflicts: Vec<(Category, String)>,
    /// Items skipped because their values are invalid (with the reasons).
    pub invalid: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentLibrary {
    pub schema_version: u32,
    #[serde(default)]
    pub motors: Vec<Motor>,
    #[serde(default)]
    pub propellers: Vec<Propeller>,
    #[serde(default)]
    pub escs: Vec<Esc>,
    #[serde(default)]
    pub batteries: Vec<Battery>,
}

impl Default for ComponentLibrary {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            motors: Vec::new(),
            propellers: Vec::new(),
            escs: Vec::new(),
            batteries: Vec::new(),
        }
    }
}

/// A record kind stored in the library.
pub trait Item: Record {
    const CATEGORY: Category;
    fn list(lib: &ComponentLibrary) -> &Vec<Self>;
    fn list_mut(lib: &mut ComponentLibrary) -> &mut Vec<Self>;
}

macro_rules! item {
    ($ty:ty, $cat:expr, $field:ident) => {
        impl Item for $ty {
            const CATEGORY: Category = $cat;
            fn list(lib: &ComponentLibrary) -> &Vec<Self> {
                &lib.$field
            }
            fn list_mut(lib: &mut ComponentLibrary) -> &mut Vec<Self> {
                &mut lib.$field
            }
        }
    };
}
item!(Motor, Category::Motor, motors);
item!(Propeller, Category::Propeller, propellers);
item!(Esc, Category::Esc, escs);
item!(Battery, Category::Battery, batteries);

fn norm(name: &str) -> String {
    name.trim().to_lowercase()
}

impl ComponentLibrary {
    pub fn len(&self) -> usize {
        self.motors.len() + self.propellers.len() + self.escs.len() + self.batteries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every problem in the library, including duplicate names within a category.
    pub fn validate(&self) -> Vec<String> {
        fn one<T: Item>(lib: &ComponentLibrary, out: &mut Vec<String>) {
            let mut seen = std::collections::HashSet::new();
            for it in T::list(lib) {
                out.extend(it.validate());
                if !seen.insert(norm(it.name())) {
                    out.push(format!("duplicate name \"{}\"", it.name().trim()));
                }
            }
        }
        let mut out = Vec::new();
        one::<Motor>(self, &mut out);
        one::<Propeller>(self, &mut out);
        one::<Esc>(self, &mut out);
        one::<Battery>(self, &mut out);
        out
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("library serialises")
    }

    /// Parses and fully validates a library file's text.
    pub fn from_json(text: &str) -> Result<Self, LibraryError> {
        let lib = Self::parse_unchecked(text)?;
        let problems = lib.validate();
        if problems.is_empty() {
            Ok(lib)
        } else {
            Err(LibraryError::Invalid(problems))
        }
    }

    /// Parses and checks the schema version, but not the values.
    fn parse_unchecked(text: &str) -> Result<Self, LibraryError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| LibraryError::Parse(e.to_string()))?;
        let found = value
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| LibraryError::Parse("missing schema_version".into()))?;
        let found = u32::try_from(found).unwrap_or(u32::MAX);
        if found == 0 || found > SCHEMA_VERSION {
            return Err(LibraryError::Schema { found });
        }
        serde_json::from_value(value).map_err(|e| LibraryError::Parse(e.to_string()))
    }

    /// Loads the library from `path`. A missing file gives an empty library.
    pub fn load(path: &Path) -> Result<Self, LibraryError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_json(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(LibraryError::Io(e.to_string())),
        }
    }

    /// Validates, then writes to `path` through a temporary file so a crash cannot truncate it.
    pub fn save(&self, path: &Path) -> Result<(), LibraryError> {
        let problems = self.validate();
        if !problems.is_empty() {
            return Err(LibraryError::Invalid(problems));
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| LibraryError::Io(e.to_string()))?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.to_json()).map_err(|e| LibraryError::Io(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| LibraryError::Io(e.to_string()))
    }

    /// Adds a new record after validating it; names must be unique within the category.
    pub fn add<T: Item>(&mut self, item: T) -> Result<(), Vec<String>> {
        let mut problems = item.validate();
        let n = norm(item.name());
        if T::list(self).iter().any(|x| norm(x.name()) == n) {
            problems.push(format!("\"{}\" already exists", item.name().trim()));
        }
        if problems.is_empty() {
            T::list_mut(self).push(item);
            Ok(())
        } else {
            Err(problems)
        }
    }

    /// Replaces the record called `old_name` (the name may change, but not onto another record).
    pub fn replace<T: Item>(&mut self, old_name: &str, item: T) -> Result<(), Vec<String>> {
        let mut problems = item.validate();
        let (old, new) = (norm(old_name), norm(item.name()));
        let list = T::list(self);
        let Some(idx) = list.iter().position(|x| norm(x.name()) == old) else {
            return Err(vec![format!("\"{}\" no longer exists", old_name.trim())]);
        };
        if new != old && list.iter().any(|x| norm(x.name()) == new) {
            problems.push(format!("\"{}\" already exists", item.name().trim()));
        }
        if problems.is_empty() {
            T::list_mut(self)[idx] = item;
            Ok(())
        } else {
            Err(problems)
        }
    }

    /// Removes the record called `name`; returns whether something was removed.
    pub fn remove<T: Item>(&mut self, name: &str) -> bool {
        let n = norm(name);
        let list = T::list_mut(self);
        let before = list.len();
        list.retain(|x| norm(x.name()) != n);
        list.len() != before
    }

    pub fn find<T: Item>(&self, name: &str) -> Option<&T> {
        let n = norm(name);
        T::list(self).iter().find(|x| norm(x.name()) == n)
    }

    /// Merges `incoming` (read from `file_name`) into this library. Imported records are tagged
    /// `Imported(file_name)`. Identical records are skipped as duplicates, records with the same
    /// name but different values are reported as conflicts and left alone, invalid ones skipped.
    pub fn merge(&mut self, incoming: ComponentLibrary, file_name: &str) -> ImportReport {
        fn one<T: Item>(
            dst: &mut ComponentLibrary,
            src: Vec<T>,
            file_name: &str,
            rep: &mut ImportReport,
        ) {
            for mut it in src {
                let name = it.name().trim().to_string();
                let problems = it.validate();
                if !problems.is_empty() {
                    rep.invalid.extend(problems);
                    continue;
                }
                it.set_source(Source::Imported(file_name.to_string()));
                match dst.find::<T>(&name) {
                    Some(existing) => {
                        let mut probe = it.clone();
                        probe.set_source(existing.source().clone());
                        if probe == *existing {
                            rep.duplicates.push((T::CATEGORY, name));
                        } else {
                            rep.conflicts.push((T::CATEGORY, name));
                        }
                    }
                    None => {
                        T::list_mut(dst).push(it);
                        rep.added.push((T::CATEGORY, name));
                    }
                }
            }
        }
        let mut rep = ImportReport::default();
        one(self, incoming.motors, file_name, &mut rep);
        one(self, incoming.propellers, file_name, &mut rep);
        one(self, incoming.escs, file_name, &mut rep);
        one(self, incoming.batteries, file_name, &mut rep);
        rep
    }

    /// Reads a shared file and merges it. The library is only changed when the file is readable
    /// and has a supported schema; invalid records inside it are skipped and reported.
    pub fn import_file(&mut self, path: &Path) -> Result<ImportReport, LibraryError> {
        let text = std::fs::read_to_string(path).map_err(|e| LibraryError::Io(e.to_string()))?;
        let incoming = Self::parse_unchecked(&text)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        Ok(self.merge(incoming, &name))
    }

    /// A new library holding copies of the named records only.
    pub fn select(&self, keys: &[(Category, String)]) -> ComponentLibrary {
        fn one<T: Item>(src: &ComponentLibrary, keys: &[(Category, String)]) -> Vec<T> {
            T::list(src)
                .iter()
                .filter(|x| {
                    keys.iter()
                        .any(|(c, n)| *c == T::CATEGORY && norm(n) == norm(x.name()))
                })
                .cloned()
                .collect()
        }
        ComponentLibrary {
            schema_version: SCHEMA_VERSION,
            motors: one(self, keys),
            propellers: one(self, keys),
            escs: one(self, keys),
            batteries: one(self, keys),
        }
    }

    /// Writes the selected records to a shareable file.
    pub fn export_file(
        &self,
        keys: &[(Category, String)],
        path: &Path,
    ) -> Result<usize, LibraryError> {
        let sel = self.select(keys);
        sel.save(path)?;
        Ok(sel.len())
    }
}
