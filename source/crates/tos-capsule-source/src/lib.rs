// SPDX-License-Identifier: GPL-3.0-or-later
//! Canonical source, as a boot capsule carries it (ADR-0072 §6, ADR-0073).
//!
//! One backend of the `SourceProvider` interface, and the one a boot actually
//! has: Capsule v1 maps its whole payload by contract, so every unit this
//! provider hands back is a window into bytes that are already there.
//!
//! ```text
//! capsule payload, mapped
//!       -> path table              what the set declares it has
//!       -> SourceCatalogEntry      metadata, no bytes
//!       -> SourceSnapshot::Borrowed  a window, never a copy
//! ```
//!
//! **Nothing is copied and no corpus is assembled.** A catalog entry is a name
//! and a position; a snapshot is a slice. Enumerating the set costs the path
//! table, not the payload, which is the residency property ADR-0072 §6 asks a
//! provider for — and the reason this is a separate crate is that the interface
//! belongs to the reference path while a storage format does not.
//!
//! **Every `.tos` file is a unit of the set, and nothing else is.** The rest of
//! a capsule — the version marker, the licence notice — never claimed to be a
//! module, and a set that offered them would ask the frontend to parse a file
//! that is not source. This is the same rule the nucleus applies when it builds
//! a boot's source set, stated once here instead.
//!
//! **This provider decides nothing about what it returns.** It answers with
//! bytes; whether they parse, check, lower or verify is decided downstream, and
//! a capsule that passed its own parser is still hostile source.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

use alloc::vec::Vec;

use tos_capsule::Capsule;
use tos_pipeline::{SourceCatalogEntry, SourceEntryId, SourceProvider, SourceSnapshot};

/// The extension a capsule file must carry to be offered as source.
const SOURCE_EXTENSION: &str = ".tos";

/// Canonical source for one build, over a validated capsule.
///
/// It borrows the capsule and holds nothing else: no index, no table of its
/// own, no materialized unit. Two providers over the same capsule are the same
/// provider, and dropping one frees nothing the other needs.
pub struct CapsuleSourceProvider<'a> {
    capsule: Capsule<'a>,
}

impl<'a> CapsuleSourceProvider<'a> {
    /// Offers a validated capsule's source files as a set.
    ///
    /// The capsule must already have passed `tos_capsule::parse`: this type
    /// reads its tables, and a structure nobody validated is not something to
    /// read tables from. That check is the capsule parser's and is not repeated
    /// here.
    pub fn over(capsule: Capsule<'a>) -> CapsuleSourceProvider<'a> {
        CapsuleSourceProvider { capsule }
    }

    /// The capsule this reads from.
    pub fn capsule(&self) -> &Capsule<'a> {
        &self.capsule
    }

    /// The module-root-relative path of a capsule entry, when it is source.
    ///
    /// `None` for anything that is not a `.tos` file and for a name that is not
    /// text: a path is what docs/42 §1 derives a module name from, so bytes no
    /// module name could be derived from name nothing this set offers.
    fn source_path(&self, position: usize) -> Option<&'a str> {
        let file = self.capsule.file_at(position)?;
        let name = core::str::from_utf8(file.name).ok()?;
        if !name.ends_with(SOURCE_EXTENSION) {
            return None;
        }
        // A capsule path is absolute; a module-root-relative one is what the
        // frontend derives a module name from, so the leading separator is not
        // part of it.
        Some(name.strip_prefix('/').unwrap_or(name))
    }
}

impl SourceProvider for CapsuleSourceProvider<'_> {
    /// What the capsule declares it has, as metadata.
    ///
    /// The identity of an entry is its position in the capsule's own path
    /// table, so an entry the catalog skipped keeps its position rather than
    /// shifting the ones after it. Nothing outside this provider can act on
    /// that number: [`SourceEntryId`] is opaque, and a resolution mints its own
    /// identities from what it decided the closure is.
    fn catalog(&self) -> Vec<SourceCatalogEntry<'_>> {
        let count = self.capsule.path_table_count() as usize;
        (0..count)
            .filter_map(|position| {
                self.source_path(position).map(|path| SourceCatalogEntry {
                    id: SourceEntryId::at(position),
                    path,
                })
            })
            .collect()
    }

    /// One entry's canonical bytes, as a window into the mapped payload.
    ///
    /// Borrowed rather than owned, because the capsule is resident by contract:
    /// a provider that copied here would hold a second copy of source that is
    /// already in memory. An identity the catalog did not offer is answered with
    /// `None` rather than with whatever happens to be at that position.
    fn source(&self, id: SourceEntryId) -> Option<SourceSnapshot<'_>> {
        let position = id.position();
        self.source_path(position)?;
        self.capsule
            .file_at(position)
            .map(|file| SourceSnapshot::Borrowed(file.content))
    }
}

#[cfg(test)]
mod tests;
