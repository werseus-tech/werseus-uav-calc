// SPDX-License-Identifier: AGPL-3.0-or-later
//! Component library: mechanical standards, electrical component records, and the user's own
//! JSON library with import and export. There is no bundled vendor database; people enter their
//! own parts or import files shared by others. See [`samples`] for the small example list.

pub mod records;
pub mod samples;
pub mod standards;
pub mod store;

pub use records::{Battery, Chemistry, Esc, Motor, Propeller, Record, Source};
pub use store::{Category, ComponentLibrary, ImportReport, LibraryError, SCHEMA_VERSION};

#[cfg(test)]
mod tests;
