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
/// rather than a rule to remember: an `Arc` cannot be written through after it
/// is handed over, so the time-of-check to time-of-use window a mutable
/// provider buffer would open is not expressible in this interface.
///
/// The bytes are held **outside the measured arena**, on the host allocator.
/// That is not a trick to flatter a number: ADR-0071 §8 accounts image bytes as
/// whole-machine residency and not as process grant, because in TOS an image is
/// capsule or cache storage mapped into the address space rather than allocated
/// from `RuntimeMemoryGrantV1`. Putting them anywhere else would make a grant
/// bound untestable.
pub type Snapshot = Arc<HostBytes>;

/// An immutable byte buffer on the host allocator, outside the measured arena.
pub struct HostBytes {
    pointer: core::ptr::NonNull<u8>,
    length: usize,
}

impl HostBytes {
    pub fn new(bytes: &[u8]) -> HostBytes {
        if bytes.is_empty() {
            return HostBytes {
                pointer: core::ptr::NonNull::dangling(),
                length: 0,
            };
        }
        let layout = core::alloc::Layout::from_size_align(bytes.len(), 1)
            .expect("a byte buffer layout is valid");
        // SAFETY: the layout is non-zero-sized, and `System` is the host
        // allocator, unaffected by the measured global allocator.
        let pointer = unsafe { std::alloc::GlobalAlloc::alloc(&std::alloc::System, layout) };
        let pointer = core::ptr::NonNull::new(pointer).expect("the host allocator has room");
        // SAFETY: `pointer` owns `bytes.len()` freshly allocated bytes and the
        // source slice does not overlap it.
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.as_ptr(), bytes.len()) };
        HostBytes {
            pointer,
            length: bytes.len(),
        }
    }
}

impl core::ops::Deref for HostBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        if self.length == 0 {
            return &[];
        }
        // SAFETY: `pointer` owns `length` initialized bytes for as long as
        // `self` lives, and nothing hands out a mutable reference to them.
        unsafe { core::slice::from_raw_parts(self.pointer.as_ptr(), self.length) }
    }
}

impl Drop for HostBytes {
    fn drop(&mut self) {
        if self.length == 0 {
            return;
        }
        let layout = core::alloc::Layout::from_size_align(self.length, 1)
            .expect("the layout it was allocated with");
        // SAFETY: `pointer` came from `System` with this exact layout and is
        // released exactly once.
        unsafe {
            std::alloc::GlobalAlloc::dealloc(&std::alloc::System, self.pointer.as_ptr(), layout)
        };
    }
}

// SAFETY: the buffer is immutable after construction and owns its allocation.
unsafe impl Send for HostBytes {}
// SAFETY: the buffer is immutable after construction.
unsafe impl Sync for HostBytes {}

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

impl BoundedName {
    /// The stored bytes, without validating them again.
    ///
    /// Comparison is byte-wise rather than through `as_str`: UTF-8 orders the
    /// same way as its bytes, so the order is identical, and a lookup does not
    /// re-validate two strings that were validated when they were built.
    fn bytes(&self) -> &[u8] {
        &self.bytes[..self.length as usize]
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

/// The same conversion, for identities read out of a module's own artifact.
pub fn identity_digest(text: &str) -> [u8; 32] {
    fixed_digest(text)
}

fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// One member of the verified closure (ADR-0071 §2, revised twice).
///
/// The **exact resolved-module identity** the resolver contract uses, and no
/// more: docs/42 resolution maps a declared module name to a content identity,
/// and `V2012_IMPORT` checks an import against *both* — a name the snapshot
/// provides, and the content ID that name resolved to. So membership keys on
/// the pair. A content ID alone is not promised anywhere to be the whole
/// resolved identity, and this does not assume it is.
///
/// Fixed size, and there are at most 256 of them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Member {
    pub name: BoundedName,
    pub content_id: [u8; 32],
    pub position: u32,
}

/// The exact verified closure's membership, built once (§2).
///
/// **This is all the permanent manifest holds.** Not import slots, not call
/// sites: a manifest exists to fix *which modules this execution may reach* and
/// to be the only thing that mints an identity for one. Both of those are
/// properties of the closure, so the manifest is bounded by the closure ceiling
/// — at most 256 members — and by nothing else.
///
/// An earlier form held one entry per declared import slot, bounded at
/// `256 x 255`. That bound was defensible but the structure was not: an import
/// slot is a property of a *module*, and a module's own artifact already states
/// what its imports resolved to. Duplicating that into a permanent structure
/// made the manifest grow with something it does not decide.
///
/// A caller's `import slot -> ClosureModuleId` mapping is therefore resolved
/// when the caller becomes resident, against this membership, and is **resident
/// module-derived state under §7** — inside the byte bound, gone when the module
/// goes. So is `export name -> function index` inside the callee. Neither is
/// module search: membership is fixed before the first instruction and the
/// provider cannot widen it (§3).
#[derive(Debug)]
pub struct VerifiedClosureManifest {
    /// Sorted by identity, for a bounded lookup.
    members: Vec<Member>,
    entry: ClosureModuleId,
    entry_function: usize,
}

impl VerifiedClosureManifest {
    /// The only place a `ClosureModuleId` comes into existence.
    fn mint(&self, position: usize) -> Option<ClosureModuleId> {
        (position < self.members.len()).then_some(ClosureModuleId(position))
    }

    pub fn module(&self, position: usize) -> Option<ClosureModuleId> {
        self.mint(position)
    }

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

    /// Bytes the manifest occupies, heap included. Fixed-size records and
    /// nothing else.
    pub fn heap_bytes(&self) -> usize {
        core::mem::size_of::<VerifiedClosureManifest>()
            + self.members.capacity() * core::mem::size_of::<Member>()
    }
}

/// A membership table built from identities alone, for measuring lookup cost.
///
/// The positions it hands out belong to no execution, and no provider will ever
/// see them. It exists so that resolving against a full-ceiling closure can be
/// measured without building 256 modules.
pub fn membership_probe(members: &[(String, [u8; 32])]) -> VerifiedClosureManifest {
    let mut table: Vec<Member> = members
        .iter()
        .enumerate()
        .filter_map(|(position, (name, content_id))| {
            Some(Member {
                name: BoundedName::new(name)?,
                content_id: *content_id,
                position: position as u32,
            })
        })
        .collect();
    table.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.content_id.cmp(&right.content_id))
    });
    VerifiedClosureManifest {
        members: table,
        entry: ClosureModuleId(0),
        entry_function: 0,
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
    // Nothing crosses a module boundary during launch any more. The closure's
    // membership is built from the records afterwards, and a caller's import
    // slots are resolved when the caller is resident (§2, §7).
    let mut entry_function: Option<usize> = None;

    let mut peak = arena_frontier();
    let mut largest_module = 0usize;
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

    // Dependency order or its reverse no longer matters: nothing is pending.
    for (position, bytes) in images.iter().enumerate() {
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
        records[position] = Some(VerifiedModuleRecord {
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
        });
        mark("record", Some(position), &mut marks);

        if position == entry {
            entry_function = module
                .functions
                .iter()
                .position(|function| function.signature.name == entry_function_name);
        }

        // And released. This is the line the whole §1 claim rests on.
        drop(module);
        peak = peak.max(arena_frontier());
        mark("module released", Some(position), &mut marks);
    }

    let records: Vec<VerifiedModuleRecord> = records
        .into_iter()
        .map(|record| record.expect("every position was verified"))
        .collect();

    // Membership, from the identities the verifier itself produced.
    let mut members: Vec<Member> = records
        .iter()
        .enumerate()
        .map(|(position, record)| Member {
            name: record.name,
            content_id: record.content_id,
            position: position as u32,
        })
        .collect();
    members.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.content_id.cmp(&right.content_id))
    });
    for pair in members.windows(2) {
        if pair[0].name == pair[1].name && pair[0].content_id == pair[1].content_id {
            return Err(Failure::WrongModule {
                module: pair[1].position as usize,
            });
        }
    }

    let manifest = VerifiedClosureManifest {
        members,
        entry: ClosureModuleId(entry),
        entry_function: entry_function.ok_or(Failure::WrongModule { module: entry })?,
    };
    peak = peak.max(arena_frontier());
    mark("manifest built", None, &mut marks);
    mark("scaffolding released", None, &mut marks);

    Ok(Launched {
        marks,
        records,
        manifest,
        peak,
        largest_module,
        scaffolding_released: 0,
    })
}
