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

use std::collections::BTreeMap;
use std::sync::Arc;

use tos_image_prototype::image;
use tos_ir::{CallTarget, Module, Op, Profile, ResourceEnvelope};
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

/// One resolved cross-module call site (ADR-0071 §2).
///
/// Integers only. The caller's site is named by position, the callee by the
/// identity the manifest minted and the index of the function in its table.
/// Nothing here is a name, so execution never performs a lookup and no export
/// table has to stay alive to serve one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Link {
    pub caller: ClosureModuleId,
    pub function: usize,
    pub block: usize,
    pub instruction: usize,
    pub callee: ClosureModuleId,
    pub callee_function: usize,
}

/// The closure's resolved cross-module links, built once (ADR-0071 §2).
///
/// Bounded by the closure's cross-module call sites rather than by any module's
/// body, and holding no strings. It can only be built after every module is
/// verified, which is the other half of why launch is eager: a link may not be
/// resolved against a module whose own verification has not happened.
#[derive(Debug)]
pub struct VerifiedClosureManifest {
    modules: usize,
    links: Vec<Link>,
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

    /// The resolved target of one call site, or `None` if the site is not a
    /// cross-module call. A lookup by position, never by name.
    pub fn resolve(
        &self,
        caller: ClosureModuleId,
        function: usize,
        block: usize,
        instruction: usize,
    ) -> Option<(ClosureModuleId, usize)> {
        self.links
            .iter()
            .find(|link| {
                link.caller == caller
                    && link.function == function
                    && link.block == block
                    && link.instruction == instruction
            })
            .map(|link| (link.callee, link.callee_function))
    }

    pub fn links(&self) -> usize {
        self.links.len()
    }

    /// Bytes the manifest occupies, heap included. It holds no strings, so this
    /// is the vector and nothing else.
    pub fn heap_bytes(&self) -> usize {
        core::mem::size_of::<VerifiedClosureManifest>()
            + self.links.capacity() * core::mem::size_of::<Link>()
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

/// What launch produced.
pub struct Launched {
    pub records: Vec<VerifiedModuleRecord>,
    pub manifest: VerifiedClosureManifest,
    /// Peak arena over the whole sequential launch.
    pub peak: usize,
    /// The largest single module's materialized cost seen during launch.
    pub largest_module: usize,
    /// Scaffolding released before launch returned, reported so the claim that
    /// it was released is a number rather than a promise.
    pub scaffolding_released: usize,
}

/// One pending cross-module link, before the callee is known.
///
/// Launch-time scaffolding: it holds an export **name**, which is exactly the
/// variable-length thing the manifest must not keep.
struct PendingLink {
    caller: usize,
    function: usize,
    block: usize,
    instruction: usize,
    module_name: String,
    export_name: String,
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
    snapshot: &ResolutionSnapshot,
    limits: &Limits,
    entry: usize,
    entry_function_name: &str,
    arena_committed: fn() -> usize,
    arena_frontier: fn() -> usize,
) -> Result<Launched, Failure> {
    let mut records: Vec<VerifiedModuleRecord> = Vec::with_capacity(images.len());
    // Scaffolding. Both of these hold names, and both are released before this
    // function returns.
    let mut exports: Vec<BTreeMap<String, usize>> = Vec::with_capacity(images.len());
    let mut names: Vec<String> = Vec::with_capacity(images.len());
    let mut pending: Vec<PendingLink> = Vec::new();

    let mut peak = arena_frontier();
    let mut largest_module = 0usize;

    for (position, bytes) in images.iter().enumerate() {
        let before = arena_committed();

        // §5's ordering, at launch as well: the artifact digest is computed over
        // the exact snapshot that is then parsed.
        let artifact_digest = tos_hash::sha256(bytes);

        let module = image::parse(bytes, limits).map_err(|error| Failure::Parser {
            module: position,
            error,
        })?;
        let receipt = tos_verifier::verify(&module, snapshot, limits).map_err(|finding| {
            Failure::Verifier {
                module: position,
                code: finding.code,
            }
        })?;

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
        records.push(VerifiedModuleRecord {
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

        // Scaffolding, gathered while the module is materialized because it is
        // the only moment it can be.
        names.push(module.header.module_name.clone());
        exports.push(export_table(&module));
        collect_pending(&module, position, &mut pending);

        // And released. This is the line the whole §1 claim rests on.
        drop(module);
        peak = peak.max(arena_frontier());
    }

    // Every module verified: only now can links be resolved.
    let position_of: BTreeMap<&str, usize> = names
        .iter()
        .enumerate()
        .map(|(at, name)| (name.as_str(), at))
        .collect();
    let mut links = Vec::with_capacity(pending.len());
    for link in &pending {
        let callee = *position_of
            .get(link.module_name.as_str())
            .ok_or(Failure::WrongModule {
                module: link.caller,
            })?;
        let callee_function =
            *exports[callee]
                .get(link.export_name.as_str())
                .ok_or(Failure::WrongModule {
                    module: link.caller,
                })?;
        links.push(Link {
            caller: ClosureModuleId(link.caller),
            function: link.function,
            block: link.block,
            instruction: link.instruction,
            callee: ClosureModuleId(callee),
            callee_function,
        });
    }
    let entry_function = *exports[entry]
        .get(entry_function_name)
        .ok_or(Failure::WrongModule { module: entry })?;

    let manifest = VerifiedClosureManifest {
        modules: images.len(),
        links,
        entry: ClosureModuleId(entry),
        entry_function,
    };

    // The export tables and the pending links go here, and nothing later can
    // ask "what does this module export": execution follows resolved links.
    let scaffolding = scaffolding_bytes(&exports, &names, &pending);
    drop(position_of);
    drop(exports);
    drop(names);
    drop(pending);

    Ok(Launched {
        records,
        manifest,
        peak,
        largest_module,
        scaffolding_released: scaffolding,
    })
}

fn export_table(module: &Module) -> BTreeMap<String, usize> {
    let mut table = BTreeMap::new();
    for (at, function) in module.functions.iter().enumerate() {
        table
            .entry(function.signature.name.clone())
            .or_insert_with(|| at);
    }
    table
}

fn collect_pending(module: &Module, position: usize, pending: &mut Vec<PendingLink>) {
    for (function, definition) in module.functions.iter().enumerate() {
        for (block, body) in definition.blocks.iter().enumerate() {
            for (instruction, step) in body.instructions.iter().enumerate() {
                let Op::Call {
                    target: CallTarget::Imported { import, name },
                    ..
                } = &step.op
                else {
                    continue;
                };
                let Some(declared) = module.imports.get(*import) else {
                    continue;
                };
                pending.push(PendingLink {
                    caller: position,
                    function,
                    block,
                    instruction,
                    module_name: declared.module_name.clone(),
                    export_name: name.clone(),
                });
            }
        }
    }
}

fn scaffolding_bytes(
    exports: &[BTreeMap<String, usize>],
    names: &[String],
    pending: &[PendingLink],
) -> usize {
    let mut total = 0usize;
    for table in exports {
        for key in table.keys() {
            total += key.len() + core::mem::size_of::<String>() + core::mem::size_of::<usize>();
        }
    }
    for name in names {
        total += name.len() + core::mem::size_of::<String>();
    }
    for link in pending {
        total +=
            link.module_name.len() + link.export_name.len() + core::mem::size_of::<PendingLink>();
    }
    total
}
