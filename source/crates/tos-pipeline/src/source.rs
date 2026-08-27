// SPDX-License-Identifier: GPL-3.0-or-later
//! Where canonical source comes from (ADR-0072 §6).
//!
//! Not module search. A provider answers exactly one question — *given an
//! identity this preparation's own resolution produced, return the canonical
//! bytes* — and there is no second question it can be asked.
//!
//! ```text
//! resolved exact source closure
//!       -> opaque SourceModuleId
//!       -> SourceProvider
//!       -> immutable canonical source snapshot
//! ```
//!
//! Two stages, and the order is what makes the discipline hold.
//!
//! **Resolution** may read the source set's listing — that is what resolution
//! *is*. Its result is a closed membership: a [`SourceClosureManifest`] and the
//! opaque [`SourceModuleId`]s it mints, and nothing else can mint one.
//!
//! **Materialization** then hands back bytes for an identity that already
//! exists, and the content is checked against what resolution saw. If the source
//! is gone, has changed, or the provider returns something else, preparation
//! fails. There is no search for an alternative, because there is nothing to
//! search: a module outside the closure has no identifier, so asking for one is
//! not refused — it cannot be spelled.
//!
//! A provider returns **source**. Never a receipt, never a `Module`, never a
//! conclusion about an image. What the bytes mean is decided downstream by the
//! verifier, and by nothing the provider says.

use alloc::string::String;
use alloc::vec::Vec;

use crate::Unit;

/// One entry of a provider's declared source set.
///
/// **Provider-local, and not executable authority.** A catalog entry exists
/// before any closure has been resolved: it names something the source set
/// declares it has, which is what resolution needs in order to decide what the
/// closure *is*. It is opaque so that it cannot be confused with an identity
/// the resolution produced, and it is a different type from
/// [`SourceModuleId`] for exactly that reason.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceEntryId(usize);

impl SourceEntryId {
    /// Mints an entry identity. A provider names its own entries; nothing else
    /// does.
    pub fn at(position: usize) -> SourceEntryId {
        SourceEntryId(position)
    }

    pub fn position(self) -> usize {
        self.0
    }
}

/// What a provider says it has, without saying what is in it.
///
/// **Metadata only.** No bytes: a catalog that carried source would make
/// enumerating the set cost the set, which is the residency ADR-0072 §6 exists
/// to avoid. A provider that keeps its corpus resident anyway — Capsule v1 does,
/// because that is its contract — may do so; the interface does not require it
/// of any other backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceCatalogEntry<'a> {
    pub id: SourceEntryId,
    /// Canonical repository path, module-root relative.
    pub path: &'a str,
}

/// One module of an exact resolved source closure.
///
/// Opaque, and **minted only by [`SourceClosureManifest`]**. Not a path, not a
/// module name, not something that can be constructed from text or guessed.
/// This is what makes closure widening unrepresentable rather than merely
/// checked.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceModuleId(usize);

impl SourceModuleId {
    /// Only for reporting and for indexing this preparation's own tables. A
    /// number a human can read is not authority: it cannot be turned back into
    /// a `SourceModuleId`.
    pub fn position(self) -> usize {
        self.0
    }
}

/// One member of a resolved source closure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMember {
    /// Which catalog entry this member came from. The only bridge from a
    /// resolved identity back to something a provider will answer.
    pub entry: SourceEntryId,
    /// The canonical repository path the unit is stored at.
    pub path: String,
    /// The identity resolution computed from the unit's normalized bytes.
    pub content_id: String,
}

/// The exact resolved source closure's membership, built once.
///
/// Bounded by the closure ceiling and by nothing else, and closed. It holds the
/// modules the entry can actually reach — **not** every entry the catalog
/// offered. A source set may declare a hundred modules and a closure contain
/// three; the other ninety-seven have a catalog entry and no `SourceModuleId`,
/// which is the difference between what a provider knows about and what an
/// execution is.
#[derive(Clone, Debug)]
pub struct SourceClosureManifest {
    members: Vec<SourceMember>,
}

impl SourceClosureManifest {
    /// Mints the membership from what resolution actually produced.
    ///
    /// Crate-private: the manifest is built by the preparation, from identities
    /// it computed itself, and there is no other constructor.
    pub(crate) fn of(members: Vec<SourceMember>) -> SourceClosureManifest {
        SourceClosureManifest { members }
    }

    pub fn modules(&self) -> usize {
        self.members.len()
    }

    /// The identity at a position of this closure.
    pub fn module(&self, position: usize) -> Option<SourceModuleId> {
        (position < self.members.len()).then_some(SourceModuleId(position))
    }

    pub fn member(&self, id: SourceModuleId) -> Option<&SourceMember> {
        self.members.get(id.0)
    }

    pub fn members(&self) -> &[SourceMember] {
        &self.members
    }
}

/// Why a provider could not answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceRefusal {
    /// The provider has nothing for an identity this closure contains.
    Absent { path: String },
    /// The bytes came back, and they are not the bytes resolution saw.
    Changed {
        path: String,
        resolved: String,
        found: String,
    },
}

impl SourceRefusal {
    /// A stable reason token, for a caller reporting this over an event log.
    pub fn symbol(&self) -> &'static str {
        match self {
            SourceRefusal::Absent { .. } => "source-absent",
            SourceRefusal::Changed { .. } => "source-changed",
        }
    }

    /// Which unit it is about.
    pub fn path(&self) -> &str {
        match self {
            SourceRefusal::Absent { path } | SourceRefusal::Changed { path, .. } => path,
        }
    }
}

/// Supplies canonical source for modules of an exact resolved closure.
///
/// There is no enumeration beyond the listing resolution reads, no path or name
/// lookup after it, no fallback to a filesystem, a network or an environment,
/// and no way to return anything but bytes.
pub trait SourceProvider {
    /// What this source set declares it has, as metadata.
    ///
    /// Read before a membership exists, because resolution cannot decide what
    /// the closure is without knowing what the set offers. It carries no source:
    /// enumerating a set must not cost the set.
    fn catalog(&self) -> Vec<SourceCatalogEntry<'_>>;

    /// The canonical bytes of one catalog entry.
    ///
    /// Immutable, and the same bytes on every call — a provider that answered
    /// differently the second time is caught by the identity check in
    /// [`materialize`]. Returning `None` fails whatever asked; it does not start
    /// a search.
    fn source(&self, id: SourceEntryId) -> Option<&[u8]>;
}

/// A provider over units a caller already holds.
///
/// The host and test shape: a source set that is already in memory, offered as
/// it is. It is **not** evidence of a freestanding persistent-source backend —
/// ADR-0072 §9 leaves that undecided — and no measurement taken through it
/// should be reported as one.
pub struct SliceSourceProvider<'a> {
    units: &'a [Unit<'a>],
}

impl<'a> SliceSourceProvider<'a> {
    pub fn new(units: &'a [Unit<'a>]) -> SliceSourceProvider<'a> {
        SliceSourceProvider { units }
    }
}

impl SourceProvider for SliceSourceProvider<'_> {
    fn catalog(&self) -> Vec<SourceCatalogEntry<'_>> {
        self.units
            .iter()
            .enumerate()
            .map(|(position, unit)| SourceCatalogEntry {
                id: SourceEntryId::at(position),
                path: unit.path,
            })
            .collect()
    }

    fn source(&self, id: SourceEntryId) -> Option<&[u8]> {
        self.units.get(id.position()).map(|unit| unit.bytes)
    }
}

/// Obtains one member's source and checks that it is what resolution saw.
///
/// The identity is **recomputed from the bytes** rather than remembered about
/// them: a provider that returned different content under the same identity is
/// caught here, which is the whole reason materialization is a separate stage
/// from resolution.
pub(crate) fn materialize<'a>(
    provider: &'a dyn SourceProvider,
    manifest: &SourceClosureManifest,
    id: SourceModuleId,
) -> Result<&'a [u8], SourceRefusal> {
    let Some(member) = manifest.member(id) else {
        // Unreachable through the public interface: an id is minted by this
        // manifest and by nothing else. Answered rather than asserted, because
        // a total function is cheaper than a proof that no caller reaches it.
        return Err(SourceRefusal::Absent {
            path: String::new(),
        });
    };
    let Some(bytes) = provider.source(member.entry) else {
        return Err(SourceRefusal::Absent {
            path: member.path.clone(),
        });
    };
    let Some(found) = identity_of(bytes) else {
        return Err(SourceRefusal::Changed {
            path: member.path.clone(),
            resolved: member.content_id.clone(),
            found: String::from("<not transport-valid>"),
        });
    };
    if found != member.content_id {
        return Err(SourceRefusal::Changed {
            path: member.path.clone(),
            resolved: member.content_id.clone(),
            found,
        });
    }
    Ok(bytes)
}

/// The identity of a unit's bytes, as resolution computes it.
///
/// `None` when the bytes are not transport-valid, which is a different failure
/// from a mismatch and is reported as one.
pub(crate) fn identity_of(bytes: &[u8]) -> Option<String> {
    crate::SourceReader::read(bytes)
        .ok()
        .map(|source| crate::content_id(source.bytes()))
}
