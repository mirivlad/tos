// SPDX-License-Identifier: GPL-3.0-or-later
//! Launch: the trusted record, the closure manifest, and the provider key.
//!
//! ADR-0071 §1–§3 and §5, built for measurement. Nothing here is production
//! code and no engine is switched onto it.
//!
//! The shape being measured is the one the ADR fixes:
//!
//! 1. every module of the exact resolved closure is verified **once, at
//!    launch**, one at a time, and its materialized `Module` is released before
//!    the next is decoded;
//! 2. what survives per module is a **fixed-size** [`VerifiedModuleRecord`] —
//!    identities, digests and a bounded envelope, and nothing that grows with
//!    the module;
//! 3. the cross-module links live in a separate [`VerifiedClosureManifest`],
//!    fully resolved, built after the whole closure is verified, after which the
//!    export lookup tables are released;
//! 4. the manifest is the only thing that mints a [`ClosureModuleId`], which is
//!    the provider's only key.

use std::sync::Arc;

use tos_image_prototype::image;
use tos_ir::{Profile, ResourceEnvelope};
use tos_verifier::{Limits, ResolutionSnapshot};

/// An immutable artifact snapshot.
///
/// ADR-0071 §5 requires that the bytes which are hashed and the bytes which are
/// then parsed and executed be **one immutable snapshot**. That is a type here
/// rather than a rule to remember: an `Arc<[u8]>` cannot be written through
/// after it is handed over, so the time-of-check to time-of-use window a mutable
/// provider buffer would open is not expressible in this interface.
pub type Snapshot = Arc<[u8]>;

/// The provider's only key.
///
/// Opaque, and **minted only by [`VerifiedClosureManifest`]**. Not a module
/// name, not a path, not a content ID, not a semantic digest — nothing that can
/// be constructed from text, parsed out of an image, or guessed.
///
/// This is what makes closure widening unrepresentable rather than merely
/// checked (§3): a request for a module outside the closure is not rejected by
/// a validation someone might forget to write, because there is no identifier
/// for it and no way to make one. The field is private and the type exposes no
/// constructor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClosureModuleId(usize);

impl ClosureModuleId {
    /// Only for reporting. A number a human can read is not authority: the
    /// value cannot be turned back into a `ClosureModuleId`.
    pub fn position(self) -> usize {
        self.0
    }
}

/// Supplies image bytes for modules of this execution's closure, and nothing
/// else.
///
/// The whole authority: *given an identity this execution's own launch minted,
/// return bytes that claim to be that module's image.* There is no
/// enumeration — a component that could list what it holds could be asked what
/// else exists, and what else exists is not this execution's business.
pub trait Provider {
    fn image(&self, id: ClosureModuleId) -> Option<Snapshot>;
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

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.length as usize]).unwrap_or("")
    }
}

impl core::fmt::Debug for BoundedName {
    fn fmt(&self, out: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(out, "{:?}", self.as_str())
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
/// list of anything. Those grow with the module, and a record carrying one could
/// not be called fixed size.
///
/// Most fields are never read by this harness, and that is correct: they are the
/// facts a verified module's identity consists of, retained because a residency
/// design that dropped what it did not immediately need would be measuring a
/// smaller record than the one a real implementation has to hold.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
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
    pub profile: Profile,
    pub envelope: ResourceEnvelope128,
}

/// The declared envelope, as ten fixed numbers.
///
/// `tos_ir::ResourceEnvelope` is the same ten values; it is restated here as a
/// `Copy` type so the record has no heap at all.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceEnvelope128 {
    pub limits: [u128; 10],
}

impl ResourceEnvelope128 {
    fn from(envelope: &ResourceEnvelope) -> ResourceEnvelope128 {
        ResourceEnvelope128 {
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

/// `sha256:<hex>` to bytes, or the digest of the text when it is not one.
fn fixed_digest(text: &str) -> [u8; 32] {
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

/// One resolved **import edge** (ADR-0071 §2, revised).
///
/// The manifest's job is to fix the exact executable closure and the provider's
/// authority before the first instruction. That needs one entry per *declared
/// import slot* — which module a caller's import names — and not one per call
/// site. The earlier per-call-site form was derived and measured, and its V1
/// upper bound was `8 257 536` links (`378 MiB`), larger than the whole
/// reference machine. Import edges are bounded by the closure instead: a module
/// may import at most the other 255 of a 256-module closure.
///
/// Which *function* of that module a call reaches is reconstructed inside the
/// callee once it is resident — the module is already fixed, so nothing about
/// that lookup can widen anything (§3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Edge {
    pub caller: ClosureModuleId,
    pub import: u32,
    pub callee: ClosureModuleId,
}

/// The closure's module membership and import topology, built once (§2).
///
/// Bounded by the closure's import edges, holding no strings and no call-site
/// positions. It can only be built after every module is verified, which is the
/// other half of why launch is eager: an edge may not be resolved against a
/// module whose own verification has not happened.
#[derive(Debug)]
pub struct VerifiedClosureManifest {
    modules: usize,
    /// Where each module's import slots begin in `edges`.
    offsets: Vec<u32>,
    /// One `ClosureModuleId` per declared import slot, in slot order.
    edges: Vec<ClosureModuleId>,
    entry: ClosureModuleId,
    entry_function: usize,
}

impl VerifiedClosureManifest {
    /// The only place a `ClosureModuleId` comes into existence.
    fn mint(&self, position: usize) -> Option<ClosureModuleId> {
        (position < self.modules).then_some(ClosureModuleId(position))
    }

    pub fn module(&self, position: usize) -> Option<ClosureModuleId> {
        self.mint(position)
    }

    pub fn modules(&self) -> usize {
        self.modules
    }

    pub fn entry(&self) -> (ClosureModuleId, usize) {
        (self.entry, self.entry_function)
    }

    /// Which module a caller's import slot names.
    ///
    /// A lookup by position in a table this execution's own launch built. It
    /// cannot answer with a module outside the closure, because the table holds
    /// nothing else and `ClosureModuleId` has no other constructor.
    pub fn edge(&self, caller: ClosureModuleId, import: usize) -> Option<ClosureModuleId> {
        let start = *self.offsets.get(caller.position())? as usize;
        let end = self
            .offsets
            .get(caller.position() + 1)
            .map(|next| *next as usize)
            .unwrap_or(self.edges.len());
        self.edges.get(start..end)?.get(import).copied()
    }

    pub fn edges(&self) -> usize {
        self.edges.len()
    }

    /// Bytes the manifest occupies, heap included. It holds no strings, so this
    /// is the two vectors and nothing else.
    pub fn heap_bytes(&self) -> usize {
        core::mem::size_of::<VerifiedClosureManifest>()
            + self.offsets.capacity() * core::mem::size_of::<u32>()
            + self.edges.capacity() * core::mem::size_of::<ClosureModuleId>()
    }
}

/// Why a launch or a reload failed. Refusal is the only behaviour (§9).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Failure {
    /// The provider had nothing for an identity this execution needs.
    Missing(usize),
    /// The snapshot's SHA-256 is not the trusted artifact digest — stale,
    /// corrupted or substituted. Detected **before** parsing.
    ArtifactDigest { module: usize },
    /// The bytes did not survive the parser.
    Parser {
        module: usize,
        error: image::ImageError,
    },
    /// The semantic verifier refused the module. A launch-time condition only.
    Verifier { module: usize, code: &'static str },
    /// The image named a module that is not the one the closure resolved.
    WrongModule { module: usize },
    /// A record field outside its bound — a name too long for the fixed record.
    Unrepresentable { module: usize, field: &'static str },
    /// The workload needed something this measurement engine does not execute.
    Unsupported(&'static str),
}

/// The arena at one named point of launch.
///
/// Both numbers, always. `committed` says what is alive; `frontier` says how far
/// the arena has been carried — and ADR-0069 sizes a grant from the second, so a
/// launch that reported only the first would be answering a question nobody
/// asked.
#[derive(Clone, Copy, Debug)]
pub struct Mark {
    pub label: &'static str,
    pub module: Option<usize>,
    pub committed: usize,
    pub frontier: usize,
}

/// What launch produced.
pub struct Launched {
    /// Every phase boundary, in order. Attribution needs the sequence, not a
    /// total: which owner the bytes belong to is invisible from a peak.
    pub marks: Vec<Mark>,
    pub records: Vec<VerifiedModuleRecord>,
    pub manifest: VerifiedClosureManifest,
    /// Peak arena over the whole sequential launch.
    pub peak: usize,
    /// The largest single module's materialized cost seen during launch.
    pub largest_module: usize,
    /// The widest the pending-link set ever got, in bytes. The only thing that
    /// crosses a module boundary during launch, and the only closure-scaled
    /// term left in it.
    pub scaffolding_released: usize,
}

/// One import edge waiting for its callee, in **fixed size**.
///
/// The callee is named by the **content ID the import itself declares**, which
/// is an exact identity the frontend already put in the IR — not a name, not a
/// hash of a name, and not a truncation of anything. Resolution is a 32-byte
/// equality against the callee's own record, so a wrong match is not improbable
/// but impossible.
///
/// One of these exists per declared import slot, so the live set is bounded by
/// the closure's import edges and shrinks as callees are reached.
#[derive(Clone, Copy, Debug)]
struct PendingEdge {
    caller: u32,
    import: u32,
    callee_content_id: [u8; 32],
}

/// Verifies the exact resolved closure, sequentially, and builds the manifest.
///
/// `images` is in dependency order, `snapshot` is the declared resolution — an
/// input, never something this function discovers (docs/43 §5). `entry` is the
/// position of the entry module and the name of its entry function.
///
/// The peak is the whole point: one module is materialized at a time and
/// released before the next is decoded, so the peak should be one module's
/// working set rather than the closure's. Whether it is, is what the caller
/// measures.
#[allow(clippy::too_many_arguments)]
pub fn launch(
    images: &[Snapshot],
    resolution: &dyn Fn(usize) -> ResolutionSnapshot,
    limits: &Limits,
    entry: usize,
    entry_function_name: &str,
    arena_committed: fn() -> usize,
    arena_frontier: fn() -> usize,
) -> Result<Launched, Failure> {
    let count = images.len();
    let mut records: Vec<Option<VerifiedModuleRecord>> = vec![None; count];
    // The only thing that crosses a module boundary during launch, and it is
    // fixed size. Every link is *consumed* when its callee is reached, so the
    // live set shrinks as launch proceeds rather than accumulating.
    // The only thing that crosses a module boundary during launch, and it is
    // fixed size and bounded by the closure's import edges.
    let mut pending: Vec<PendingEdge> = Vec::new();
    let mut slots: Vec<Vec<Option<ClosureModuleId>>> = vec![Vec::new(); count];
    let mut entry_function: Option<usize> = None;

    let mut peak = arena_frontier();
    let mut largest_module = 0usize;
    let mut widest_pending = 0usize;
    let mut marks: Vec<Mark> = Vec::with_capacity(count * 6 + 4);
    let mark = |label: &'static str, module: Option<usize>, marks: &mut Vec<Mark>| {
        marks.push(Mark {
            label,
            module,
            committed: arena_committed(),
            frontier: arena_frontier(),
        });
    };
    mark("base", None, &mut marks);

    // **Reverse dependency order — callers before callees.** The image order is
    // the topological order resolution produced, so reversing it puts every
    // caller ahead of everything it calls. By the time a module is reached,
    // every edge that will ever name it is already pending, so nothing has to
    // be remembered on the chance that someone later asks.
    for position in (0..count).rev() {
        let bytes = &images[position];
        let before = arena_committed();

        // The declared resolution, one module's slice of it. Read here and
        // dropped below, so what is live is one module's import surface rather
        // than the closure's.
        let snapshot = resolution(position);
        mark("resolution slice", Some(position), &mut marks);

        // §5's ordering, at launch as well: the artifact digest is computed
        // over the exact snapshot that is then parsed.
        let artifact_digest = tos_hash::sha256(bytes);

        let module = image::parse(bytes, limits).map_err(|error| Failure::Parser {
            module: position,
            error,
        })?;
        mark("decoded Module", Some(position), &mut marks);
        let receipt = tos_verifier::verify(&module, &snapshot, limits).map_err(|finding| {
            Failure::Verifier {
                module: position,
                code: finding.code,
            }
        })?;
        // The verifier's workspace is gone by the time it returns, so its cost
        // is invisible in `committed` and shows only in the frontier.
        mark("verifier workspace", Some(position), &mut marks);
        drop(snapshot);

        largest_module = largest_module.max(arena_committed().saturating_sub(before));
        peak = peak.max(arena_frontier());

        let name = BoundedName::new(&receipt.module_name).ok_or(Failure::Unrepresentable {
            module: position,
            field: "module_name",
        })?;
        let source_set = BoundedName::new(&receipt.source_set).ok_or(Failure::Unrepresentable {
            module: position,
            field: "source_set",
        })?;
        let record = VerifiedModuleRecord {
            semantic_digest: fixed_digest(&receipt.module_digest),
            artifact_digest,
            verifier_identity: fixed_digest(&receipt.verifier_identity),
            content_id: fixed_digest(&receipt.content_id),
            dependency_digest: fixed_digest(&receipt.dependency_digest),
            capability_interface_digest: fixed_digest(&receipt.capability_interface_digest),
            source_map_digest: fixed_digest(&receipt.source_map_digest),
            name,
            source_set,
            profile: receipt.profile,
            envelope: ResourceEnvelope128::from(&receipt.resource_envelope),
        };
        let own_content_id = record.content_id;
        records[position] = Some(record);
        mark("record", Some(position), &mut marks);

        // This module as a **callee**: every pending edge whose declared
        // content ID is this module's is resolved here, by 32-byte equality
        // against an identity the frontend recorded. No name is compared and
        // nothing is inferred.
        for at in (0..pending.len()).rev() {
            if pending[at].callee_content_id != own_content_id {
                continue;
            }
            let edge = pending[at];
            let slot = slots[edge.caller as usize]
                .get_mut(edge.import as usize)
                .ok_or(Failure::WrongModule {
                    module: edge.caller as usize,
                })?;
            *slot = Some(ClosureModuleId(position));
            pending.swap_remove(at);
        }
        if position == entry {
            entry_function = module
                .functions
                .iter()
                .position(|function| function.signature.name == entry_function_name);
        }
        mark("edges resolved", Some(position), &mut marks);

        // This module as a **caller**: one pending edge per declared import
        // slot, carrying the content ID the import itself states.
        slots[position] = vec![None; module.imports.len()];
        for (import, declared) in module.imports.iter().enumerate() {
            pending.push(PendingEdge {
                caller: position as u32,
                import: import as u32,
                callee_content_id: fixed_digest(&declared.module_content_id),
            });
        }
        widest_pending = widest_pending.max(pending.len());
        mark("pending edges", Some(position), &mut marks);

        // And released. This is the line the whole §1 claim rests on.
        drop(module);
        peak = peak.max(arena_frontier());
        mark("module released", Some(position), &mut marks);
    }

    if let Some(unresolved) = pending.first() {
        // An import naming a module the closure does not contain. The closure
        // was resolved before launch, so this is a malformed input rather than
        // a lookup that came up empty.
        return Err(Failure::WrongModule {
            module: unresolved.caller as usize,
        });
    }

    // Flattened: one `ClosureModuleId` per import slot, with per-module offsets.
    let mut offsets = Vec::with_capacity(count + 1);
    let mut edges = Vec::new();
    for module in &slots {
        offsets.push(edges.len() as u32);
        for slot in module {
            edges.push(slot.ok_or(Failure::WrongModule { module: 0 })?);
        }
    }
    offsets.push(edges.len() as u32);

    let manifest = VerifiedClosureManifest {
        modules: count,
        offsets,
        edges,
        entry: ClosureModuleId(entry),
        entry_function: entry_function.ok_or(Failure::WrongModule { module: entry })?,
    };
    peak = peak.max(arena_frontier());
    mark("manifest built", None, &mut marks);

    let scaffolding = widest_pending * core::mem::size_of::<PendingEdge>();
    drop(pending);
    drop(slots);
    mark("scaffolding released", None, &mut marks);

    Ok(Launched {
        marks,
        records: records
            .into_iter()
            .map(|record| record.expect("every position was verified"))
            .collect(),
        manifest,
        peak,
        largest_module,
        scaffolding_released: scaffolding,
    })
}
