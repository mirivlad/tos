// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded verified-module residency (ADR-0071, Accepted 2026-08-27).
//!
//! An execution verifies its **exact resolved closure** once, before the first
//! instruction, one module at a time. What survives each module is a fixed-size
//! [`VerifiedModuleRecord`]; what survives the closure is its membership, in a
//! [`VerifiedClosureManifest`]. Everything else — the decoded module, its
//! import mapping, its export index — is resident state that is released when
//! the module is evicted.
//!
//! ```text
//! launch:   image -> hostile parse -> verify -> record -> release module
//!           ... once per module, in order ...
//!           -> membership manifest
//!
//! run:      caller import slot -> resident import map -> ClosureModuleId
//!           -> provider -> immutable snapshot -> artifact digest against the
//!              trusted record -> bounded parse -> resident export index
//! ```
//!
//! The properties this crate exists to hold, each of them structural rather
//! than remembered:
//!
//! - the manifest holds **membership only**. Not import slots, not call sites:
//!   a module's own verified artifact already states them, and copying either
//!   into a permanent structure would make the manifest grow with something it
//!   does not decide;
//! - a module's identity is the **pair the resolver contract uses** — a declared
//!   name and the content identity it resolved to. `V2012_IMPORT` checks an
//!   import against both;
//! - [`ClosureModuleId`] cannot be constructed outside the manifest. A request
//!   for a module outside the closure is not rejected; it has no
//!   representation;
//! - a [`ModuleProvider`] takes only a `ClosureModuleId`, cannot enumerate, and
//!   returns bytes. It never returns a receipt or any other conclusion — a
//!   cache may supply bytes, never conclusions;
//! - a reload is **byte identity** against the record this execution's own
//!   launch produced. The semantic verifier does not run again, and the
//!   snapshot that is hashed is the snapshot that is parsed;
//! - residency is bounded by **count and by module-derived bytes** — image,
//!   decoded module, derived indexes and bookkeeping, all of it — and eviction
//!   is deterministic and never consults free memory.
//!
//! There is no path, no search and no name-based fallback anywhere in this
//! crate.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

use alloc::sync::Arc;
use alloc::vec::Vec;

use tos_image::ImageError;
use tos_ir::Module;
use tos_verifier::{Finding, ImageRefusal, Limits, ResolutionSnapshot};

mod launch;
mod resident;

pub use launch::{launch, ClosureSource, Launched};
pub use resident::{Ledger, Residency, ResidencyLimits, Traffic};

/// An immutable image snapshot.
///
/// ADR-0071 §5 requires that the bytes which are hashed and the bytes which are
/// then parsed and executed be **one immutable snapshot**. That is a type here
/// rather than a rule to remember: an `Arc<[u8]>` cannot be written through
/// after it is handed over, so the time-of-check to time-of-use window a
/// mutable provider buffer would open is not expressible in this interface.
pub type ImageSnapshot = Arc<[u8]>;

/// The provider's only key.
///
/// Opaque, and **minted only by [`VerifiedClosureManifest`]**. Not a module
/// name, not a path, not a content ID, not a semantic digest — nothing that can
/// be constructed from text, parsed out of an image, or guessed.
///
/// This is what makes closure widening unrepresentable rather than merely
/// checked: a request for a module outside the closure is not rejected by a
/// validation someone might forget to write, because there is no identifier for
/// it and no way to make one. The field is private and the type exposes no
/// constructor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClosureModuleId(usize);

impl ClosureModuleId {
    /// Only for reporting and for indexing this execution's own tables. A
    /// number a human can read is not authority: the value cannot be turned
    /// back into a `ClosureModuleId`.
    pub fn position(self) -> usize {
        self.0
    }
}

/// Supplies image bytes for modules of this execution's closure, and nothing
/// else.
///
/// The whole authority: *given an identity this execution's own launch minted,
/// return bytes that claim to be that module's image.*
///
/// There is no enumeration — a component that could list what it holds could be
/// asked what else exists, and what else exists is not this execution's
/// business. There is no way to return a receipt, a record or any other
/// conclusion: the method returns bytes, and what they mean is decided by the
/// artifact digest in the trusted record and by nothing the provider says.
pub trait ModuleProvider {
    fn image(&self, id: ClosureModuleId) -> Option<ImageSnapshot>;
}

/// A bounded name. Fixed storage, so a record containing one is still fixed
/// size.
#[derive(Clone, Copy)]
pub struct BoundedName {
    bytes: [u8; 96],
    length: u8,
}

impl BoundedName {
    pub fn new(text: &str) -> Option<BoundedName> {
        let source = text.as_bytes();
        if source.len() > 96 {
            return None;
        }
        let mut bytes = [0u8; 96];
        bytes[..source.len()].copy_from_slice(source);
        Some(BoundedName {
            bytes,
            length: source.len() as u8,
        })
    }

    /// The stored bytes, without validating them again.
    ///
    /// Comparison is byte-wise rather than through `as_str`: UTF-8 orders the
    /// same way as its bytes, so the order is identical, and a lookup does not
    /// re-validate two strings that were validated when they were built.
    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.length as usize]
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.bytes()).unwrap_or("")
    }
}

impl PartialEq for BoundedName {
    fn eq(&self, other: &BoundedName) -> bool {
        self.bytes() == other.bytes()
    }
}

impl Eq for BoundedName {}

impl PartialOrd for BoundedName {
    fn partial_cmp(&self, other: &BoundedName) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BoundedName {
    fn cmp(&self, other: &BoundedName) -> core::cmp::Ordering {
        self.bytes().cmp(other.bytes())
    }
}

impl core::fmt::Debug for BoundedName {
    fn fmt(&self, out: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(out, "{:?}", self.as_str())
    }
}

/// The ten declared limits, as fixed numbers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Envelope {
    pub limits: [u128; 10],
}

impl Envelope {
    fn of(envelope: &tos_ir::ResourceEnvelope) -> Envelope {
        Envelope {
            limits: [
                envelope.fuel,
                envelope.stack,
                envelope.allocation,
                envelope.tasks,
                envelope.workers,
                envelope.sync,
                envelope.shared,
                envelope.cleanup,
                envelope.recursion,
                envelope.imports,
            ],
        }
    }
}

/// What survives one module's verification (ADR-0071 §2).
///
/// **Fixed size, with no heap and no variable-length field.** `size_of` is the
/// whole cost, which is the property the design rests on: releasing a
/// materialized `Module` must free megabytes and retain a constant.
///
/// Digest-shaped fields already in `sha256:<hex>` form are stored as their 32
/// bytes; any other text is stored as the digest of that text. Either way the
/// field is 32 bytes and comparison stays exact.
///
/// Note what is **not** here: no export surface, no name-to-function map, no
/// list of anything. Those grow with the module, and a record carrying one
/// could not be called fixed size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedModuleRecord {
    pub semantic_digest: [u8; 32],
    pub artifact_digest: [u8; 32],
    pub verifier_identity: [u8; 32],
    pub content_id: [u8; 32],
    pub dependency_digest: [u8; 32],
    pub capability_interface_digest: [u8; 32],
    pub source_map_digest: [u8; 32],
    pub name: BoundedName,
    pub source_set: BoundedName,
    pub profile: tos_ir::Profile,
    pub envelope: Envelope,
}

/// `sha256:<hex>` to bytes, or the digest of the text when it is not one.
pub(crate) fn fixed_digest(text: &str) -> [u8; 32] {
    if let Some(hex) = text.strip_prefix("sha256:") {
        if hex.len() == 64 {
            let mut bytes = [0u8; 32];
            let raw = hex.as_bytes();
            let mut ok = true;
            for (at, slot) in bytes.iter_mut().enumerate() {
                match (nibble(raw[at * 2]), nibble(raw[at * 2 + 1])) {
                    (Some(high), Some(low)) => *slot = (high << 4) | low,
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                return bytes;
            }
        }
    }
    tos_hash::sha256(text.as_bytes())
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// One member of the verified closure.
///
/// The **exact resolved-module identity** the resolver contract uses: docs/42
/// resolution maps a declared module name to a content identity, and
/// `V2012_IMPORT` checks an import against *both*. So membership keys on the
/// pair. A content ID alone is not promised anywhere to be the whole resolved
/// identity, and this does not assume it is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Member {
    pub name: BoundedName,
    pub content_id: [u8; 32],
    pub position: u32,
}

/// The exact verified closure's membership, built once (ADR-0071 §2).
///
/// **This is all the permanent manifest holds**, so it is bounded by the
/// closure ceiling and by nothing else — at most 256 members.
#[derive(Clone, Debug)]
pub struct VerifiedClosureManifest {
    members: Vec<Member>,
    entry: ClosureModuleId,
    entry_function: usize,
}

impl VerifiedClosureManifest {
    pub fn modules(&self) -> usize {
        self.members.len()
    }

    pub fn entry(&self) -> (ClosureModuleId, usize) {
        (self.entry, self.entry_function)
    }

    /// Which member of the closure an exact resolved identity names.
    ///
    /// A binary search over at most 256 fixed-size records. It cannot answer
    /// with anything outside the closure, because the table holds nothing else
    /// and `ClosureModuleId` has no other constructor.
    pub fn resolve(&self, name: &str, content_id: &[u8; 32]) -> Option<ClosureModuleId> {
        let at = self
            .members
            .binary_search_by(|member| {
                member
                    .name
                    .bytes()
                    .cmp(name.as_bytes())
                    .then_with(|| member.content_id.cmp(content_id))
            })
            .ok()?;
        Some(ClosureModuleId(self.members[at].position as usize))
    }

    /// The identity at a position of this closure, for a caller that already
    /// holds one of its own identifiers.
    pub fn module(&self, position: usize) -> Option<ClosureModuleId> {
        (position < self.members.len()).then_some(ClosureModuleId(position))
    }

    pub fn members(&self) -> &[Member] {
        &self.members
    }

    /// Bytes the manifest occupies, heap included.
    pub fn heap_bytes(&self) -> usize {
        core::mem::size_of::<VerifiedClosureManifest>()
            + self.members.capacity() * core::mem::size_of::<Member>()
    }
}

/// Why a launch or a reload failed. Refusal is the only behaviour (ADR-0071 §9).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Failure {
    /// The provider had nothing for an identity this execution needs.
    Missing(usize),
    /// The snapshot's sha-256 is not the trusted artifact digest — stale,
    /// corrupted or substituted. Detected **before** parsing.
    ArtifactDigest { module: usize },
    /// The bytes did not survive the parser.
    Parser { module: usize, error: ImageError },
    /// The semantic verifier refused the module. A launch-time condition only.
    Verifier { module: usize, finding: Finding },
    /// An import named a module the closure does not contain, or a closure
    /// contained the same identity twice.
    WrongModule { module: usize },
    /// A record field outside its bound — a name too long for the fixed record.
    Unrepresentable { module: usize, field: &'static str },
    /// A module needs more resident state than the declared bound allows, even
    /// alone. An execution must be able to make progress with one resident
    /// module; if it cannot, that is a refusal and not an eviction.
    OverResidencyBound { module: usize, bytes: usize },
}

impl Failure {
    pub(crate) fn from_refusal(module: usize, refusal: ImageRefusal) -> Failure {
        match refusal {
            ImageRefusal::Parser(error) => Failure::Parser { module, error },
            ImageRefusal::Verifier(finding) => Failure::Verifier { module, finding },
        }
    }
}

/// What a launch needs beside the images: the declared resolution, per module.
///
/// docs/43 §5 makes the snapshot declared input. It is passed per module rather
/// than for the closure, because each module's verification consults only the
/// modules it imports, and a launch that held the closure's whole export
/// surface would be holding 255 modules' worth of it for nothing.
pub type Resolution<'a> = &'a dyn Fn(usize) -> ResolutionSnapshot;

/// The declared limits a launch verifies under.
pub type VerifierLimits = Limits;

/// The module a record describes, for callers that want to compare identities.
pub fn module_identity(module: &Module) -> ([u8; 32], Option<BoundedName>) {
    (
        fixed_digest(&module.header.content_id),
        BoundedName::new(&module.header.module_name),
    )
}

#[cfg(test)]
mod tests;
