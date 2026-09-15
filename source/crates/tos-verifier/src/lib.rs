// SPDX-License-Identifier: GPL-3.0-or-later
//! The independent verifier for `tos-ir/v1` (docs/43 sections 4 and 5).
//!
//! docs/43 section 4 is explicit that the verifier does not trust the
//! frontend's claims. It rechecks table bounds and schema identity, nominal
//! type references, control-flow targets, operand types, call and effect
//! signatures, import and capability declarations, affine value and borrow
//! state, profile restrictions, resource accounting, task scope, atomic
//! orders, unsafe interface IDs, and source-map identity and spans.
//!
//! Independence here is structural, as section 5 requires. This crate depends
//! on [`tos_ir`] for the declarative schema and on nothing else: no frontend,
//! no AST, no type-checker result, no callback. It reaches its own conclusion
//! by traversing the module value it was handed, and the module value carries
//! no field that could stand in for that traversal. An alternate or optimized
//! frontend is untrusted at this boundary in exactly the same way.
//!
//! The result is either a [`VerifiedModule`] receipt bound to the digest of the
//! module the verifier actually saw, or one deterministic primary `V20xx`
//! finding. An engine accepts IR only with a receipt for that exact digest.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

// The test harness is a host program by construction, so it keeps `std`.
#[cfg(test)]
extern crate std;

use alloc::collections::BTreeSet;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tos_ir::{
    AtomicOp, Block, CallTarget, CapabilitySource, Constant, Function, Instruction, MemoryOrder,
    Module, Op, Operand, Profile, SourceMapEntry, Terminator, TypeDef, TypeId,
};

mod image;
mod limits;
mod representation;

// The linked predeclared table's own digests, computed by this crate's build
// script from the crate this build actually links.
include!(concat!(env!("OUT_DIR"), "/predeclared_digests.rs"));

/// **The predeclared contract content this verifier is written for**
/// (`docs/43` §5, ADR-0088 §6).
///
/// `docs/43` §5 permits a shared declarative type/interface table "only if its
/// content digest is input to both components". This is the verifier's input.
/// It is stated here, in the verifier's own source, and compared below against
/// the digest of the table this build links — so a table edited under this
/// component fails **its compilation**, not its test suite. The frontend states
/// the same values independently and neither reads the other's.
pub const ACCEPTED_PREDECLARED_DIGESTS: [(u32, &str); 5] = [
    (
        0,
        "849bd9de6a7c0c485ffbcc972791a5768c724a4165d67087fd1961e863f7f555",
    ),
    (
        1,
        "26848cb863d7727c9b6787e141da156df418ea57af0471863a5767fe9f170bb3",
    ),
    (
        2,
        "c5e8ffbb3467768e15a235b57c56bdde1a359cb707ad5f33ee45f721b357c2a5",
    ),
    (
        3,
        "a4bad47aac6cdb361ced38352fde05acfedb47d4a6c331b8cd4762fd3daec02c",
    ),
    (
        4,
        "9d496d76b8e8b25157547fe6a5d81fb3c32c0dc7f331d3e19779ee1e2271a17c",
    ),
];

/// Whether two digest tables state the same thing, evaluated at compile time.
///
/// Written as index loops over bytes because that is what a `const fn` can do:
/// the point is not the comparison but *when* it happens.
const fn digests_agree(stated: &[(u32, &str); 5], linked: &[(u32, &str); 5]) -> bool {
    let mut entry = 0;
    while entry < 5 {
        if stated[entry].0 != linked[entry].0 {
            return false;
        }
        let one = stated[entry].1.as_bytes();
        let other = linked[entry].1.as_bytes();
        if one.len() != other.len() {
            return false;
        }
        let mut byte = 0;
        while byte < one.len() {
            if one[byte] != other[byte] {
                return false;
            }
            byte += 1;
        }
        entry += 1;
    }
    true
}

/// **The binding, and it is fail-closed at compile time.** A predeclared
/// contract whose content is not the content this verifier was written for is
/// not a contract this verifier may read: the two would be checking artifacts
/// against different languages while each believed it held the agreed one.
const _: () = assert!(
    digests_agree(&ACCEPTED_PREDECLARED_DIGESTS, &LINKED_PREDECLARED_DIGESTS),
    "the linked tos-predeclared table is not the contract this verifier states"
);

pub use image::{
    verify_image, verify_image_in_closure, ImageRefusal, VerifiedArtifactEvidence, VerifiedImage,
};
pub use limits::Limits;
pub use representation::{Representation, Represented, REPRESENTED};

/// Which verifier produced a receipt.
pub const VERIFIER_IDENTITY: &str = "tos-verifier-reference/0.1.0";

/// A receipt that this exact module passed this exact verifier.
///
/// docs/43 section 5: only the verifier emits one, and it binds to the complete
/// module digest. A frontend cannot mark a cache verified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedModule {
    pub module_digest: String,
    pub schema_id: String,
    pub verifier_identity: String,
    pub module_name: String,
    pub source_set: String,
    pub content_id: String,
    pub dependency_digest: String,
    pub profile: Profile,
    pub resource_envelope: tos_ir::ResourceEnvelope,
    pub capability_interface_digest: String,
    pub source_map_digest: String,
}

/// One deterministic verifier finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    pub code: &'static str,
    pub detail: String,
    /// Where in the module the finding sits, for an engine's audit record.
    pub location: String,
    /// Ordered causal entries, outermost first.
    pub causes: Vec<Finding>,
}

impl Finding {
    fn new(code: &'static str, location: impl ToString, detail: impl ToString) -> Finding {
        Finding {
            code,
            detail: detail.to_string(),
            location: location.to_string(),
            causes: Vec::new(),
        }
    }
}

/// What the verifier is told about the world outside the module.
///
/// docs/43 section 5: the verifier consumes untrusted IR plus a declared
/// module-resolution and capability-interface snapshot. The snapshot is
/// declared input, not something the verifier discovers: it never inspects an
/// ambient directory, the network or the environment.
///
/// **The facts are the same; the representation is bounded.** docs/43 requires
/// the full declared snapshot — every declared imported module's resolved
/// identity, its *whole* export surface, and the declared capability
/// interfaces — but it does not require a particular Rust collection. This held
/// `BTreeMap<String, BTreeSet<String>>` with an owned `String` per name, which
/// measured `156.83 MiB` at the V1 worst case: 255 imported modules each
/// exporting as much as a conforming source unit allows. The names themselves
/// were `7.93 MiB` of that.
///
/// So the names are packed end to end and everything else is a span into them.
/// Nothing is dropped: this is not "the exports the caller happens to use",
/// which would make the verifier's answer depend on the question.
#[derive(Clone, Debug, Default)]
pub struct ResolutionSnapshot {
    /// Module names, content identities and capability interfaces, end to end.
    text: Vec<u8>,
    /// Sorted by module name.
    modules: Vec<ModuleFacts>,
    /// Export names, module-major and sorted within each module.
    ///
    /// Kept apart from `text` so that an export costs **one `u32`** of metadata
    /// and nothing else: a name runs from its own offset to the next one, so no
    /// length is stored at all. At the V1 worst case — 255 modules of 6 745
    /// exports — that is the difference between `6.6 MiB` of metadata and the
    /// `13.1 MiB` a `{start, length}` pair costs. There is no narrow integer
    /// here and no packing: `size_of::<u32>()` is four bytes with no padding to
    /// argue about, which a two-field struct could not promise.
    export_text: Vec<u8>,
    /// One offset per export, plus a terminator.
    export_offsets: Vec<u32>,
    /// Sorted declared capability interfaces.
    capabilities: Vec<Span>,
}

/// A stretch of [`ResolutionSnapshot::text`].
///
/// Used for module names, content identities and capability interfaces, whose
/// lengths no accepted contract bounds tightly enough to narrow. Export names
/// do not use it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Span {
    start: u32,
    length: u32,
}

/// What the declared resolution says about one module.
#[derive(Clone, Copy, Debug)]
struct ModuleFacts {
    name: Span,
    content_id: Span,
    exports_at: u32,
    exports_len: u32,
    /// Whether the snapshot states this module's export surface at all.
    ///
    /// "States an empty surface" and "says nothing about the exports" are
    /// different facts and the import check treats them differently, so a flag
    /// carries the difference that a missing map entry used to.
    surface: bool,
}

impl ResolutionSnapshot {
    fn slice(&self, span: Span) -> &str {
        let start = span.start as usize;
        let end = start + span.length as usize;
        core::str::from_utf8(&self.text[start..end]).unwrap_or("")
    }

    fn export(&self, at: usize) -> &str {
        let start = self.export_offsets[at] as usize;
        let end = self.export_offsets[at + 1] as usize;
        core::str::from_utf8(&self.export_text[start..end]).unwrap_or("")
    }

    fn find(&self, module: &str) -> Option<&ModuleFacts> {
        let at = self
            .modules
            .binary_search_by(|facts| self.slice(facts.name).cmp(module))
            .ok()?;
        self.modules.get(at)
    }

    /// Whether any module resolution was declared at all.
    ///
    /// An empty snapshot means "no declared resolution", which the import check
    /// treats as nothing to compare against. That is the behaviour this
    /// replaced and it is unchanged.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// How many modules the declared resolution names.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// What identity the declared resolution says a module name resolved to.
    pub fn resolved_content_id(&self, module: &str) -> Option<&str> {
        self.find(module).map(|facts| self.slice(facts.content_id))
    }

    /// The declared export surface of a module, if the snapshot states one.
    ///
    /// `None` means the snapshot says nothing about that module's exports —
    /// distinct from an empty surface, which says it exports nothing.
    pub fn export_surface(&self, module: &str) -> Option<ExportSurface<'_>> {
        let facts = self.find(module)?;
        if !facts.surface {
            return None;
        }
        Some(ExportSurface {
            snapshot: self,
            at: facts.exports_at,
            length: facts.exports_len,
        })
    }

    /// Whether any capability contract was declared.
    pub fn declares_capabilities(&self) -> bool {
        !self.capabilities.is_empty()
    }

    /// Whether the declared contract provides an interface.
    pub fn provides_capability(&self, interface: &str) -> bool {
        self.capabilities
            .binary_search_by(|span| self.slice(*span).cmp(interface))
            .is_ok()
    }

    /// Bytes this snapshot occupies, heap included. Reported so that a bound on
    /// it can be measured rather than argued.
    pub fn heap_bytes(&self) -> usize {
        core::mem::size_of::<ResolutionSnapshot>()
            + self.text.capacity()
            + self.modules.capacity() * core::mem::size_of::<ModuleFacts>()
            + self.export_text.capacity()
            + self.export_offsets.capacity() * core::mem::size_of::<u32>()
            + self.capabilities.capacity() * core::mem::size_of::<Span>()
    }

    /// Export-name bytes, and the metadata carrying them. For measurement.
    ///
    /// Content rather than capacity: what is being reported is what the
    /// representation costs per export, not how a `Vec` happened to grow.
    pub fn export_bytes(&self) -> (usize, usize) {
        (
            self.export_text.len(),
            self.export_offsets.len() * core::mem::size_of::<u32>(),
        )
    }
}

/// One module's declared export surface, whole.
pub struct ExportSurface<'a> {
    snapshot: &'a ResolutionSnapshot,
    at: u32,
    length: u32,
}

impl ExportSurface<'_> {
    /// Whether this exact module declares this exact export.
    ///
    /// A binary search over the module's own sorted range. No fast path skips
    /// it: the surface is complete, so a name that is not in it is a name the
    /// module does not export.
    pub fn contains(&self, name: &str) -> bool {
        let start = self.at as usize;
        let end = start + self.length as usize;
        let mut low = start;
        let mut high = end;
        while low < high {
            let middle = low + (high - low) / 2;
            match self.snapshot.export(middle).cmp(name) {
                core::cmp::Ordering::Less => low = middle + 1,
                core::cmp::Ordering::Greater => high = middle,
                core::cmp::Ordering::Equal => return true,
            }
        }
        false
    }

    pub fn len(&self) -> usize {
        self.length as usize
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

/// Builds a [`ResolutionSnapshot`] without a `String` per name.
///
/// Modules are declared one at a time and a module's exports are added to the
/// module last declared, so each module's export names land contiguously in the
/// packed export text. `build` establishes the ordering and is the only place
/// that does.
///
/// **Exports already in order cost nothing to accept.** A reader of a stored
/// declared resolution has them sorted; `build` checks, and only rewrites the
/// export text when it has to. That is what keeps the transient peak of
/// building a worst-case snapshot from doubling it.
#[derive(Clone, Debug, Default)]
pub struct DeclaredResolution {
    text: Vec<u8>,
    modules: Vec<ModuleFacts>,
    export_text: Vec<u8>,
    export_offsets: Vec<u32>,
    capabilities: Vec<Span>,
}

impl DeclaredResolution {
    pub fn new() -> DeclaredResolution {
        DeclaredResolution::default()
    }

    /// Reserves room for a declared resolution of known size.
    ///
    /// A caller that already knows how much it is about to declare — a reader
    /// of a stored resolution knows before it starts — builds without a single
    /// reallocation. Measured on the V1 worst case, growing instead cost a
    /// transient on top of the finished snapshot, which is a peak paid for
    /// nothing.
    pub fn reserve(&mut self, modules: usize, exports: usize, text: usize, export_text: usize) {
        self.text
            .reserve_exact(text.saturating_sub(self.text.len()));
        self.modules
            .reserve_exact(modules.saturating_sub(self.modules.len()));
        self.export_text
            .reserve_exact(export_text.saturating_sub(self.export_text.len()));
        self.export_offsets
            .reserve_exact((exports + 1).saturating_sub(self.export_offsets.len()));
    }

    fn pack(&mut self, value: &str) -> Span {
        let span = Span {
            start: self.text.len() as u32,
            length: value.len() as u32,
        };
        self.text.extend_from_slice(value.as_bytes());
        span
    }

    /// Declares a module and what its name resolved to. Exports added after
    /// this belong to it.
    pub fn module(&mut self, name: &str, content_id: &str) -> &mut DeclaredResolution {
        let name = self.pack(name);
        let content_id = self.pack(content_id);
        let exports_at = self.export_offsets.len() as u32;
        self.modules.push(ModuleFacts {
            name,
            content_id,
            exports_at,
            exports_len: 0,
            surface: false,
        });
        self
    }

    /// States that the module most recently declared has a known export
    /// surface, even if it is empty.
    pub fn exports_declared(&mut self) -> &mut DeclaredResolution {
        if let Some(facts) = self.modules.last_mut() {
            facts.surface = true;
        }
        self
    }

    /// Declares one export of the module most recently declared.
    pub fn export(&mut self, name: &str) -> &mut DeclaredResolution {
        self.export_offsets.push(self.export_text.len() as u32);
        self.export_text.extend_from_slice(name.as_bytes());
        if let Some(facts) = self.modules.last_mut() {
            facts.exports_len += 1;
            facts.surface = true;
        }
        self
    }

    /// Declares a capability interface the contract provides.
    pub fn capability(&mut self, interface: &str) -> &mut DeclaredResolution {
        let span = self.pack(interface);
        self.capabilities.push(span);
        self
    }

    pub fn build(self) -> ResolutionSnapshot {
        let DeclaredResolution {
            text,
            mut modules,
            export_text,
            mut export_offsets,
            mut capabilities,
        } = self;
        // The terminator, so every name runs to the next offset.
        export_offsets.push(export_text.len() as u32);

        fn read_export<'body>(offsets: &[u32], body: &'body [u8], at: usize) -> &'body str {
            let start = offsets[at] as usize;
            let end = offsets[at + 1] as usize;
            core::str::from_utf8(&body[start..end]).unwrap_or("")
        }

        let mut ordered = true;
        for facts in &modules {
            let start = facts.exports_at as usize;
            let end = start + facts.exports_len as usize;
            for at in start + 1..end {
                if read_export(&export_offsets, &export_text, at - 1)
                    > read_export(&export_offsets, &export_text, at)
                {
                    ordered = false;
                    break;
                }
            }
            if !ordered {
                break;
            }
        }

        let (export_text, export_offsets) = if ordered {
            (export_text, export_offsets)
        } else {
            // One rewrite, and only when the caller did not hand them over in
            // order.
            let mut order: Vec<u32> = (0..export_offsets.len() as u32 - 1).collect();
            for facts in &modules {
                let start = facts.exports_at as usize;
                let end = start + facts.exports_len as usize;
                order[start..end].sort_by(|left, right| {
                    read_export(&export_offsets, &export_text, *left as usize).cmp(read_export(
                        &export_offsets,
                        &export_text,
                        *right as usize,
                    ))
                });
            }
            let mut packed = Vec::with_capacity(export_text.len());
            let mut offsets = Vec::with_capacity(export_offsets.len());
            for at in &order {
                offsets.push(packed.len() as u32);
                let start = export_offsets[*at as usize] as usize;
                let end = export_offsets[*at as usize + 1] as usize;
                packed.extend_from_slice(&export_text[start..end]);
            }
            offsets.push(packed.len() as u32);
            (packed, offsets)
        };

        let read = |span: Span| -> &str {
            let start = span.start as usize;
            core::str::from_utf8(&text[start..start + span.length as usize]).unwrap_or("")
        };
        modules.sort_by(|left, right| read(left.name).cmp(read(right.name)));
        capabilities.sort_by(|left, right| read(*left).cmp(read(*right)));
        capabilities.dedup_by(|left, right| read(*left) == read(*right));
        ResolutionSnapshot {
            text,
            modules,
            export_text,
            export_offsets,
            capabilities,
        }
    }
}

/// Verifies an untrusted module against a declared snapshot.
///
/// The validation order is the one docs/43 section 5 fixes, and it stops at the
/// first primary finding: a later check reading a table an earlier one rejected
/// would be reporting a consequence, not a defect.
/// The verifier's steps, in the order [`verify`] runs them.
///
/// Named so a caller can run one at a time and see what each costs. Which step
/// costs what is invisible from a total, and optimising a verifier by guess is
/// how a verifier stops verifying.
pub const VERIFY_STEPS: [&str; 9] = [
    "limits",
    "schema",
    "source_identity",
    "table_order",
    "types_and_imports",
    "control_flow",
    "ownership_and_profile",
    "tasks_sync_atomics_unsafe",
    "source_maps",
];

/// Runs one named step against untrusted IR.
///
/// The verifier still derives everything it needs from the module itself; this
/// only exposes the sequence it already runs. No clock lives here — the crate is
/// `no_std` and a caller that wants a timing brings its own.
pub fn verify_step(
    name: &str,
    module: &Module,
    snapshot: &ResolutionSnapshot,
    limits: &Limits,
) -> Option<Result<(), Finding>> {
    Some(match name {
        "limits" => check_limits(module, limits),
        "schema" => check_schema(module),
        "source_identity" => check_source_identity(module),
        "table_order" => check_table_order(module),
        "types_and_imports" => check_types_and_imports(module, snapshot),
        "control_flow" => check_control_flow(module, snapshot),
        "ownership_and_profile" => check_ownership_and_profile(module),
        "tasks_sync_atomics_unsafe" => check_tasks_sync_atomics_unsafe(module),
        "source_maps" => check_source_maps(module),
        _ => return None,
    })
}

pub fn verify(
    module: &Module,
    snapshot: &ResolutionSnapshot,
    limits: &Limits,
) -> Result<VerifiedModule, Finding> {
    verify_in_closure(module, snapshot, limits, &mut NoDependencies)
}

/// The same obligations, plus the ones that need the rest of the closure.
///
/// **This is what a launch calls.** An imported call can only be checked
/// against the dependency that was verified for it, so the obligation lives
/// where the dependency is reachable — and `verify` above is the same function
/// told that nothing else has been verified, which is the honest answer for a
/// caller checking one artifact on its own.
pub fn verify_in_closure(
    module: &Module,
    snapshot: &ResolutionSnapshot,
    limits: &Limits,
    dependencies: &mut dyn VerifiedDependencies,
) -> Result<VerifiedModule, Finding> {
    check_limits(module, limits)?;
    check_schema(module)?;
    check_source_identity(module)?;
    check_table_order(module)?;
    check_export_projection(module)?;
    check_types_and_imports(module, snapshot)?;
    check_control_flow(module, snapshot)?;
    check_ownership_and_profile(module)?;
    check_tasks_sync_atomics_unsafe(module)?;
    check_source_maps(module)?;
    check_imported_calls(module, dependencies, limits)?;

    Ok(VerifiedModule {
        module_digest: tos_ir::module_digest(module),
        schema_id: module.header.schema_id.clone(),
        verifier_identity: VERIFIER_IDENTITY.to_string(),
        module_name: module.header.module_name.clone(),
        source_set: module.header.source_set.clone(),
        content_id: module.header.content_id.clone(),
        dependency_digest: module.header.dependency_digest.clone(),
        profile: module.header.profile,
        resource_envelope: module.header.resource_envelope.clone(),
        capability_interface_digest: module.header.capability_interface_digest.clone(),
        source_map_digest: source_map_digest(&module.source_map),
    })
}

// ------------------------------------------------------------------ step 1

/// Envelope, byte and table-count limits, before anything expensive.
fn check_limits(module: &Module, limits: &Limits) -> Result<(), Finding> {
    let checks: [(&str, usize, usize); 6] = [
        ("types", module.types.len(), limits.table_entries),
        ("constants", module.constants.len(), limits.table_entries),
        ("functions", module.functions.len(), limits.table_entries),
        ("imports", module.imports.len(), limits.modules),
        (
            "capability imports",
            module.capability_imports.len(),
            limits.table_entries,
        ),
        (
            "source map",
            module.source_map.len(),
            limits.source_map_entries,
        ),
    ];
    for (name, actual, ceiling) in checks {
        if actual > ceiling {
            return Err(Finding::new(
                "V2001_LIMIT",
                name,
                alloc::format!("{actual} entries exceeds the ceiling of {ceiling}"),
            ));
        }
    }
    if module.imports.len() as u128 > module.header.resource_envelope.imports {
        return Err(Finding::new(
            "V2001_LIMIT",
            "imports",
            "more imports than the declared resource envelope allows",
        ));
    }
    for (index, function) in module.functions.iter().enumerate() {
        if function.blocks.len() > limits.blocks_per_function {
            return Err(Finding::new(
                "V2001_LIMIT",
                alloc::format!("function {index}"),
                "more basic blocks than the ceiling allows",
            ));
        }
        if function.signature.parameters.len() > limits.parameters {
            return Err(Finding::new(
                "V2001_LIMIT",
                alloc::format!("function {index}"),
                "more parameters than the ceiling allows",
            ));
        }
        for (block_index, block) in function.blocks.iter().enumerate() {
            if block.instructions.len() > limits.instructions_per_block {
                return Err(Finding::new(
                    "V2001_LIMIT",
                    alloc::format!("function {index} block {block_index}"),
                    "more instructions than the ceiling allows",
                ));
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ step 2

/// Whether the artifact's declared language version is TOS Core 1.`minor` or
/// later.
///
/// Read from the header the module carries rather than from what this verifier
/// happens to implement: `docs/42` §1's rule is that a module receives the
/// language *its header declared*, and a form of a later minor in an earlier
/// module is refused however capable the tool that met it. A version this
/// verifier does not recognise has already been refused by `check_schema`, so
/// an unparsable one here answers `false` — the fail-closed direction.
fn declares_minor_at_least(version: &str, minor: u32) -> bool {
    match version.split_once('.') {
        Some(("1", declared)) => declared
            .parse::<u32>()
            .is_ok_and(|declared| declared >= minor),
        _ => false,
    }
}

fn check_schema(module: &Module) -> Result<(), Finding> {
    let header = &module.header;
    if header.schema_id != tos_ir::SCHEMA_ID {
        return Err(Finding::new(
            "V2002_SCHEMA",
            "header.schema_id",
            alloc::format!("expected {}, found {}", tos_ir::SCHEMA_ID, header.schema_id),
        ));
    }
    // One schema, two source-language minors (ADR-0080 §5). The artifact says
    // which its module declared, and the verifier accepts any this schema
    // represents — an unknown one is refused rather than assumed.
    if !tos_ir::LANGUAGE_VERSIONS.contains(&header.language_version.as_str()) {
        return Err(Finding::new(
            "V2002_SCHEMA",
            "header.language_version",
            alloc::format!(
                "expected one of {:?}, found {}",
                tos_ir::LANGUAGE_VERSIONS,
                header.language_version
            ),
        ));
    }
    if header.unicode_normalization_baseline != tos_ir::UNICODE_BASELINE {
        return Err(Finding::new(
            "V2002_SCHEMA",
            "header.unicode_normalization_baseline",
            "the declared Unicode baseline is not the one this language version fixes",
        ));
    }
    if header.source_map_revision != tos_ir::SOURCE_MAP_REVISION {
        return Err(Finding::new(
            "V2002_SCHEMA",
            "header.source_map_revision",
            "unknown source-map revision",
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------ step 3

fn check_source_identity(module: &Module) -> Result<(), Finding> {
    let header = &module.header;
    for (field, value) in [
        ("content_id", &header.content_id),
        ("dependency_digest", &header.dependency_digest),
        (
            "capability_interface_digest",
            &header.capability_interface_digest,
        ),
    ] {
        if !value.starts_with("sha256:") {
            return Err(Finding::new(
                "V2003_SOURCE_IDENTITY",
                alloc::format!("header.{field}"),
                "identity is not a named digest",
            ));
        }
    }
    if header.module_name.is_empty() || header.path.is_empty() {
        return Err(Finding::new(
            "V2003_SOURCE_IDENTITY",
            "header",
            "a module must name itself and its canonical path",
        ));
    }
    // docs/42 section 1: a module name maps to exactly one canonical path.
    let expected = alloc::format!("{}.tos", header.module_name.replace('.', "/"));
    if !header.path.ends_with(&expected) {
        return Err(Finding::new(
            "V2003_SOURCE_IDENTITY",
            "header.path",
            alloc::format!("{} does not map to {}", header.module_name, header.path),
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------ step 4

fn check_table_order(module: &Module) -> Result<(), Finding> {
    // docs/43 section 2: exported signatures are ordered by name.
    for pair in module.exports.windows(2) {
        if pair[0].name > pair[1].name {
            return Err(Finding::new(
                "V2004_TABLE_ORDER",
                "exports",
                alloc::format!("{} follows {}", pair[1].name, pair[0].name),
            ));
        }
    }
    // docs/43 section 2: functions are ordered by fully qualified source name.
    for pair in module.functions.windows(2) {
        if pair[0].signature.name > pair[1].signature.name {
            return Err(Finding::new(
                "V2004_TABLE_ORDER",
                "functions",
                alloc::format!(
                    "{} follows {}",
                    pair[1].signature.name,
                    pair[0].signature.name
                ),
            ));
        }
    }
    // Source-map entries are ordered by source unit then byte start and end.
    for pair in module.source_map.windows(2) {
        let left = (&pair[0].path, pair[0].byte_start, pair[0].byte_end);
        let right = (&pair[1].path, pair[1].byte_start, pair[1].byte_end);
        if left > right {
            return Err(Finding::new(
                "V2004_TABLE_ORDER",
                "source map",
                "entries are not ordered by unit then byte range",
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ step 5

fn check_types_and_imports(module: &Module, snapshot: &ResolutionSnapshot) -> Result<(), Finding> {
    for (index, definition) in module.types.iter().enumerate() {
        for referenced in referenced_types(definition) {
            if !module.has_type(referenced) {
                return Err(Finding::new(
                    "V2010_TYPE",
                    alloc::format!("type {index}"),
                    alloc::format!("references type {referenced} outside the table"),
                ));
            }
        }
        if let TypeDef::Nominal {
            module_content_id,
            export_name,
            kind,
            fields,
            variants,
        } = definition
        {
            if export_name.is_empty() {
                return Err(Finding::new(
                    "V2010_TYPE",
                    alloc::format!("type {index}"),
                    "a nominal type must record its export name",
                ));
            }
            // A nominal type of this module records the module it came from; a
            // type reached through an import is identified by its path and is
            // resolved by the source-set step, not forged here.
            let _ = module_content_id;
            match kind {
                tos_ir::NominalKind::Record => {
                    if !variants.is_empty() {
                        return Err(Finding::new(
                            "V2010_TYPE",
                            alloc::format!("type {index}"),
                            "a record declares variants",
                        ));
                    }
                }
                tos_ir::NominalKind::Enum => {
                    if !fields.is_empty() {
                        return Err(Finding::new(
                            "V2010_TYPE",
                            alloc::format!("type {index}"),
                            "an enum declares record fields",
                        ));
                    }
                }
            }
        }
    }
    for (index, import) in module.imports.iter().enumerate() {
        if import.module_name.is_empty() {
            return Err(Finding::new(
                "V2012_IMPORT",
                alloc::format!("import {index}"),
                "an import names no module",
            ));
        }
        if snapshot.is_empty() {
            continue;
        }
        let Some(resolved) = snapshot.resolved_content_id(&import.module_name) else {
            return Err(Finding::new(
                "V2012_IMPORT",
                alloc::format!("import {index}"),
                alloc::format!(
                    "{} is not in the declared resolution snapshot",
                    import.module_name
                ),
            ));
        };
        // The frontend states what each import resolved to; the snapshot states
        // what the source set actually provides. A module claiming an identity
        // the snapshot does not agree with is claiming a resolution that did
        // not happen, and the verifier is here precisely so the frontend's word
        // is not the last one.
        if !import.module_content_id.is_empty() && import.module_content_id != resolved {
            return Err(Finding::new(
                "V2012_IMPORT",
                alloc::format!("import {index}"),
                alloc::format!(
                    "{} resolved to {} in the snapshot, and the module claims {}",
                    import.module_name,
                    resolved,
                    import.module_content_id
                ),
            ));
        }
    }
    for (index, import) in module.capability_imports.iter().enumerate() {
        if !module.has_type(import.ty) {
            return Err(Finding::new(
                "V2013_CAPABILITY",
                alloc::format!("capability import {index}"),
                "the declared handle type is outside the table",
            ));
        }
        // A capability handle is opaque: its type must be the capability type
        // for the interface it names, not a scalar wearing the same index.
        match module.type_of(import.ty) {
            Some(TypeDef::Capability(interface)) if *interface == import.interface => {}
            _ => {
                return Err(Finding::new(
                    "V2013_CAPABILITY",
                    alloc::format!("capability import {index}"),
                    alloc::format!("{} is not typed as its own interface", import.interface),
                ))
            }
        }
        if snapshot.declares_capabilities() && !snapshot.provides_capability(&import.interface) {
            return Err(Finding::new(
                "V2013_CAPABILITY",
                alloc::format!("capability import {index}"),
                alloc::format!(
                    "{} is not in the declared capability contract",
                    import.interface
                ),
            ));
        }
    }
    Ok(())
}

fn referenced_types(definition: &TypeDef) -> Vec<TypeId> {
    match definition {
        TypeDef::Option(inner)
        | TypeDef::Task(inner)
        | TypeDef::TaskResult(inner)
        | TypeDef::Shared(inner)
        | TypeDef::Region(inner)
        | TypeDef::DmaRegion(inner)
        | TypeDef::Mutex(inner)
        | TypeDef::RwLock(inner)
        | TypeDef::Channel(inner)
        | TypeDef::Slice(inner)
        | TypeDef::Array(inner, _) => alloc::vec![*inner],
        TypeDef::Result(ok, error) => alloc::vec![*ok, *error],
        TypeDef::Tuple(elements) => elements.clone(),
        TypeDef::Function(parameters, result) => {
            let mut all = parameters.clone();
            all.push(*result);
            all
        }
        TypeDef::Nominal {
            fields, variants, ..
        } => {
            let mut all = fields.clone();
            for variant in variants {
                all.extend(variant.payload.iter().copied());
            }
            all
        }
        _ => Vec::new(),
    }
}

// ------------------------------------------------------------------ step 6

fn check_control_flow(module: &Module, snapshot: &ResolutionSnapshot) -> Result<(), Finding> {
    for (index, function) in module.functions.iter().enumerate() {
        let at = || alloc::format!("function {index}");
        if function.blocks.is_empty() {
            return Err(Finding::new("V2011_CFG", at(), "a function has no blocks"));
        }
        for ty in &function.signature.parameters {
            if !module.has_type(ty.ty) {
                return Err(Finding::new(
                    "V2010_TYPE",
                    at(),
                    "a parameter type is outside the table",
                ));
            }
        }
        if !module.has_type(function.signature.result) {
            return Err(Finding::new(
                "V2010_TYPE",
                at(),
                "the result type is outside the table",
            ));
        }
        for ty in &function.values {
            if !module.has_type(*ty) {
                return Err(Finding::new(
                    "V2010_TYPE",
                    at(),
                    "a value type is outside the table",
                ));
            }
        }
        for (block_index, block) in function.blocks.iter().enumerate() {
            let at = || alloc::format!("function {index} block {block_index}");
            check_block(module, snapshot, function, block, &at)?;
        }
    }
    Ok(())
}

fn check_block(
    module: &Module,
    snapshot: &ResolutionSnapshot,
    function: &Function,
    block: &Block,
    at: &dyn Fn() -> String,
) -> Result<(), Finding> {
    for instruction in &block.instructions {
        check_instruction(module, snapshot, function, instruction, at)?;
    }
    let count = function.blocks.len();
    for target in terminator_targets(&block.terminator) {
        if target >= count {
            return Err(Finding::new(
                "V2011_CFG",
                at(),
                alloc::format!("a terminator names block {target} of {count}"),
            ));
        }
    }
    for operand in terminator_operands(&block.terminator) {
        check_operand(module, function, &operand, at)?;
    }
    if let Terminator::MatchEnum { subject, arms } = &block.terminator {
        let ty = operand_type(module, function, subject);
        let expected = match ty.and_then(|ty| module.type_of(ty)) {
            Some(TypeDef::Nominal { variants, .. }) => variants.len(),
            Some(TypeDef::Option(_))
            | Some(TypeDef::Result(_, _))
            | Some(TypeDef::TaskResult(_)) => 2,
            _ => 0,
        };
        let covered: BTreeSet<usize> = arms.iter().map(|(variant, _)| *variant).collect();
        if covered.len() != arms.len() {
            return Err(Finding::new(
                "V2011_CFG",
                at(),
                "a match arm map names a variant twice",
            ));
        }
        for variant in 0..expected {
            if !covered.contains(&variant) {
                return Err(Finding::new(
                    "V2011_CFG",
                    at(),
                    alloc::format!("the match arm map leaves variant {variant} uncovered"),
                ));
            }
        }
    }
    Ok(())
}

fn check_instruction(
    module: &Module,
    snapshot: &ResolutionSnapshot,
    function: &Function,
    instruction: &Instruction,
    at: &dyn Fn() -> String,
) -> Result<(), Finding> {
    if !module.has_type(instruction.ty) {
        return Err(Finding::new(
            "V2010_TYPE",
            at(),
            "an instruction type is outside the table",
        ));
    }
    if let Some(result) = instruction.result {
        if result >= function.values.len() {
            return Err(Finding::new(
                "V2011_CFG",
                at(),
                alloc::format!("an instruction defines value {result} outside the table"),
            ));
        }
        if function.values[result] != instruction.ty {
            return Err(Finding::new(
                "V2010_TYPE",
                at(),
                "an instruction result disagrees with the value table",
            ));
        }
    }
    for operand in operands_of(&instruction.op) {
        check_operand(module, function, &operand, at)?;
    }
    for place in places_of(&instruction.op) {
        if place.root >= function.values.len() {
            return Err(Finding::new(
                "V2011_CFG",
                at(),
                alloc::format!("a place names value {} outside the table", place.root),
            ));
        }
        for step in &place.path {
            if let tos_ir::PlaceStep::DynamicIndex(value) = step {
                if *value >= function.values.len() {
                    return Err(Finding::new(
                        "V2011_CFG",
                        at(),
                        alloc::format!("an index names value {value} outside the table"),
                    ));
                }
            }
        }
    }
    // **What an indexed access through a region reads, checked rather than
    // believed** (ADR-0081 §2).
    //
    // A runtime serving `region[i]` takes the **element width** from the
    // instruction's declared type: it has no other way to know how many bytes
    // one element is, because the region's extent is the host's and the index
    // is a position rather than an offset. So an artifact claiming a `u64` read
    // through a `DmaRegion<u8>` would be asking a host to move eight bytes for a
    // one-byte element, and every bound the host then checks would be checking
    // the wrong number.
    //
    // The verifier already had everything needed to refuse that — the type
    // table, the value table and the place — and simply never projected the
    // place. It does now, for the region families, which are the roots whose
    // element width leaves the artifact.
    if let Some(place) = places_of(&instruction.op).first() {
        // **An index step only applies to something with elements.** The check
        // below reads a region's element type from the place, and a run serving
        // an indexed access decides it is a region from the *root's runtime
        // value* — so a place that indexes a root admitting no index would put
        // the two out of step, and the artifact would be asking a host to index
        // an authority that has no elements at all.
        if !indexable(module, function, place) {
            return Err(Finding::new(
                "V2010_TYPE",
                at(),
                "a place indexes a value of a type that has no elements",
            ));
        }
        if let Some(element) = region_element(module, function, place) {
            // **A write needs a mutably granted region** (ADR-0037 §1,
            // ADR-0081 §2). The frontend refuses one in source, and a host
            // refuses one against a mapping that is not writable — but neither
            // is the artifact's own proof, and an artifact that asked a host to
            // store into a read-only grant would be asking on the strength of a
            // type it does not have.
            if matches!(instruction.op, Op::Write { .. })
                && !matches!(
                    module.type_of(function.values[place.root]),
                    Some(TypeDef::RegionMut(_)) | Some(TypeDef::DmaRegionMut(_))
                )
            {
                return Err(Finding::new(
                    "V2021_REGION",
                    at(),
                    "an indexed write goes through a region that was not granted mutably",
                ));
            }
            let claimed = match &instruction.op {
                // A write moves the operand's type into the element, so that is
                // the type to compare. A read, a move and a borrow answer with
                // the element, so the instruction's own type is.
                Op::Write { value, .. } => operand_type(module, function, value),
                _ => Some(instruction.ty),
            };
            if claimed != Some(element) {
                return Err(Finding::new(
                    "V2010_TYPE",
                    at(),
                    "an indexed region access does not carry the region's element type",
                ));
            }
        }
    }
    // **A place may not be relabelled** (docs/43 §4: the verifier rechecks
    // operand types). A read, a move and a borrow answer with the location's
    // own type; a write puts a value of that type into it. The region case was
    // settled above against the region's element, so this is every other place.
    if region_element_of(module, function, instruction).is_none() {
        if let Some(place) = place_of(&instruction.op) {
            if let Some(projected) = projected_place_type(module, function, place) {
                let claimed = match &instruction.op {
                    Op::Write { value, .. } => operand_type(module, function, value),
                    Op::Read { .. } | Op::Move { .. } | Op::Borrow { .. } => Some(instruction.ty),
                    _ => None,
                };
                if let Some(claimed) = claimed {
                    if claimed != projected {
                        return Err(Finding::new(
                            "V2010_TYPE",
                            at(),
                            "an access carries a type the place it names does not hold",
                        ));
                    }
                }
            }
            // A place this cannot derive is left unchecked rather than refused.
            // The families it covers are the ones the artifact carries enough
            // evidence for; a variant payload is not one of them, and refusing
            // what it cannot derive would reject valid modules rather than
            // prove anything about invalid ones.
        }
    }
    match &instruction.op {
        // **The DMA ordering operation, proved from the artifact** (ADR-0086
        // §15). Nothing here is taken from a producer-supplied string: the
        // operand's type is read out of the function's value table, and the
        // direction is a closed discriminator the container already refused to
        // invent.
        Op::DmaSync { region, direction } => {
            let _ = direction;
            let Some(ty) = operand_type(module, function, region) else {
                return Err(Finding::new(
                    "V2021_REGION",
                    at(),
                    "a DMA synchronisation names an operand with no type",
                ));
            };
            // Obligation 3 and 4: the exact type is one of the two DMA region
            // constructors. A `Region`, an `MmioRegion`, a
            // `Capability("platform.dma.Region")`, an aggregate, a `Shared`, a
            // task, a guard and every scalar are refused here, together, by
            // asking what the type *is* rather than what it resembles.
            if !matches!(
                module.type_of(ty),
                Some(TypeDef::DmaRegion(_) | TypeDef::DmaRegionMut(_))
            ) {
                return Err(Finding::new(
                    "V2021_REGION",
                    at(),
                    "a DMA synchronisation names an operand that is not a DmaRegion",
                ));
            }
            // Obligation 6: the operation produces `unit`. An artifact that
            // claimed a value from it would have a value the engine never
            // writes.
            if !matches!(module.type_of(instruction.ty), Some(TypeDef::Unit)) {
                return Err(Finding::new(
                    "V2021_REGION",
                    at(),
                    "a DMA synchronisation is typed as something other than unit",
                ));
            }
            // Obligation 1, **checked last** for ADR-0085 §17.7's reason: every
            // rule above is wrong about the operation at any minor, and only an
            // artifact that is right about all of them has its version left as
            // the one thing wrong with it.
            if !declares_minor_at_least(&module.header.language_version, 4) {
                return Err(Finding::new(
                    "V2021_REGION",
                    at(),
                    alloc::format!(
                        "a DMA synchronisation is not a form of declared language version {}",
                        module.header.language_version
                    ),
                ));
            }
        }
        // **The device-access obligations ADR-0081 §8 states, met here.**
        //
        // > The verifier independently proves the operand is exactly an MMIO
        // > region kind, that a write names the mutable form, and that the
        // > enclosing artifact declares a language version in which the
        // > operation exists.
        //
        // The decision said so and the implementation did not do it: these two
        // operations had no arm at all, so a forged artifact could read through
        // an ordinary `Region`, write through a read-only `MmioRegion`, offset
        // by an `i64`, write a `bool`, claim any result, use a width no
        // hardware access has, or perform the whole thing in a module declaring
        // 1.0. Each of those is refused below, and each has a forged-IR
        // negative.
        //
        // These are static type, version and shape checks. The runtime bounds
        // and alignment checks an access performs are a different obligation
        // and are untouched.
        Op::MmioRead {
            region,
            offset,
            width,
            little_endian,
        } => {
            check_device_access(
                module,
                function,
                instruction,
                &[region, offset],
                *width,
                false,
                *little_endian,
                at,
            )?;
        }
        Op::MmioWrite {
            region,
            offset,
            value,
            width,
            little_endian,
        } => {
            check_device_access(
                module,
                function,
                instruction,
                &[region, offset, value],
                *width,
                true,
                *little_endian,
                at,
            )?;
        }
        Op::Call { target, operands } => match target {
            CallTarget::Local(index) => {
                if *index >= module.functions.len() {
                    return Err(Finding::new(
                        "V2011_CFG",
                        at(),
                        "a call names a function outside the table",
                    ));
                }
                // docs/43 §4: "A call names a declared imported or local
                // function signature and supplies an **exact ordered operand
                // list**", and the verifier "rechecks ... operand types,
                // call/effect signatures". The signature is right here in the
                // module, so there is nothing to take a producer's word for.
                let signature = &module.functions[*index].signature;
                check_signature(
                    module,
                    function,
                    instruction,
                    &signature.parameters,
                    signature.result,
                    operands,
                    at,
                )?;
            }
            CallTarget::Imported { import, name } => {
                let Some(imported) = module.imports.get(*import) else {
                    return Err(Finding::new(
                        "V2012_IMPORT",
                        at(),
                        "a call names an import outside the table",
                    ));
                };
                // docs/43 section 4: a call names a declared imported or local
                // function signature. Whether the imported module has that
                // export is knowable only from the snapshot, so it is checked
                // exactly when the snapshot says.
                if let Some(exports) = snapshot.export_surface(&imported.module_name) {
                    if !exports.contains(name) {
                        return Err(Finding::new(
                            "V2012_IMPORT",
                            at(),
                            alloc::format!("{} does not export {name}", imported.module_name),
                        ));
                    }
                }
            }
            // **No longer an empty branch** (ADR-0088). `docs/39` §2 fixes the
            // predeclared namespace and the contract states each operation's
            // minor, arity, operand rules and result; until this arm existed a
            // forged artifact could call `to_u8` with no operands, with three,
            // with a `bool`, or claim any result at all, and every check here
            // passed it. The contract is declarative data whose content digest
            // is an input to this component (`docs/43` §5); the answers below
            // are still derived from this artifact's own type table.
            CallTarget::Predeclared(name) => {
                check_predeclared(module, function, instruction, name, operands, at)?;
            }
        },
        Op::Const(constant) => {
            if *constant >= module.constants.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at(),
                    "an instruction names a constant outside the table",
                ));
            }
        }
        Op::Spawn { body, captures } => {
            if *body >= module.functions.len() {
                return Err(Finding::new(
                    "V2030_TASK_SCOPE",
                    at(),
                    "a spawn names a body outside the function table",
                ));
            }
            if captures.len() != module.functions[*body].signature.parameters.len() {
                return Err(Finding::new(
                    "V2030_TASK_SCOPE",
                    at(),
                    "a spawned body is given a different number of captures than it declares",
                ));
            }
        }
        Op::RegisterCleanup { body } => {
            if *body >= module.functions.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at(),
                    "a cleanup names a body outside the function table",
                ));
            }
        }
        Op::RunCleanups { calls } => {
            // docs/41 section 6 bounds live cleanups by the declared envelope,
            // so an exit cannot run more of them than the module reserved.
            if calls.len() as u128 > module.header.resource_envelope.cleanup {
                return Err(Finding::new(
                    "V2022_RESOURCE",
                    at(),
                    alloc::format!(
                        "{} cleanups at one exit exceeds the declared limit of {}",
                        calls.len(),
                        module.header.resource_envelope.cleanup
                    ),
                ));
            }
            for call in calls {
                if call.body >= module.functions.len() {
                    return Err(Finding::new(
                        "V2011_CFG",
                        at(),
                        "a cleanup names a body outside the function table",
                    ));
                }
                if module.functions[call.body].signature.parameters.len() != call.captures.len() {
                    return Err(Finding::new(
                        "V2011_CFG",
                        at(),
                        "a cleanup is given a different number of operands than it declares",
                    ));
                }
            }
        }
        Op::Closure { body, captures } => {
            if *body >= module.functions.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at(),
                    "a closure names a body outside the function table",
                ));
            }
            // A closure carries exactly what it declares: nothing reaches its
            // body by ambient scope, and nothing it needs is missing.
            let declared = module.functions[*body].signature.parameters.len();
            if captures.len() > declared {
                return Err(Finding::new(
                    "V2011_CFG",
                    at(),
                    "a closure carries more captures than its body declares",
                ));
            }
        }
        Op::CallValue { callee, operands } => {
            let ty = operand_type(module, function, callee);
            let Some(TypeDef::Function(parameters, result)) = ty.and_then(|ty| module.type_of(ty))
            else {
                return Err(Finding::new(
                    "V2010_TYPE",
                    at(),
                    "a value call names an operand that is not of function type",
                ));
            };
            // **The same obligation as a named call, from the callee's type
            // rather than from its signature.** `TypeDef::Function` carries the
            // ordered parameter types and the result, which is everything
            // docs/43 §4 requires an exact operand list to be checked against —
            // so a value call is verifiable here and is verified, rather than
            // being taken on trust because the callee is a value.
            //
            // What this deliberately does **not** do is distinguish `Owned`,
            // `SharedBorrow` and `MutableBorrow`: a function type encodes no
            // `PassMode`, and that erasure is a recorded contract question
            // rather than something to decide by implication here.
            if operands.len() != parameters.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at(),
                    alloc::format!(
                        "a value call supplies {} operand(s) to a callee of {} parameter(s)",
                        operands.len(),
                        parameters.len()
                    ),
                ));
            }
            for (position, (operand, declared)) in operands.iter().zip(parameters).enumerate() {
                let Some(actual) = operand_type(module, function, operand) else {
                    return Err(Finding::new(
                        "V2010_TYPE",
                        at(),
                        alloc::format!("operand {position} of a value call has no type"),
                    ));
                };
                if actual != *declared {
                    return Err(Finding::new(
                        "V2010_TYPE",
                        at(),
                        alloc::format!(
                            "operand {position} of a value call is not the declared parameter type"
                        ),
                    ));
                }
            }
            if instruction.ty != *result {
                return Err(Finding::new(
                    "V2010_TYPE",
                    at(),
                    "a value call declares a result that is not the callee's",
                ));
            }
        }
        Op::Capability { capabilities, .. } => {
            if capabilities.is_empty() {
                return Err(Finding::new(
                    "V2013_CAPABILITY",
                    at(),
                    "a capability operation names no capability at all",
                ));
            }
            // **Every position, not only the first.** ADR-0078 makes the source
            // of each capability explicit, and the checks below are per
            // position for the reason that decision gives: a rule that held of
            // the operation's own capability and not of the second would move
            // the hole one position along rather than close it.
            for (position, source) in capabilities.iter().enumerate() {
                let interface = match source {
                    // An import is an index into a table this module declared,
                    // and its interface is that declaration's.
                    CapabilitySource::Import(index) => {
                        let Some(import) = module.capability_imports.get(*index) else {
                            return Err(Finding::new(
                                "V2013_CAPABILITY",
                                at(),
                                "a capability operation names an import outside the table",
                            ));
                        };
                        // **Which sources a representation admits, checked
                        // beside the derivation and not inside it** (ADR-0085
                        // §4a, §7). An import derives its declared interface
                        // perfectly well; what is wrong is that a
                        // representation no import can *be* admits `Value`
                        // only, and there is nowhere in an
                        // `import capability` for the element type or the
                        // mutability of such a value to come from. No frontend
                        // emits this, which is exactly why the verifier has to
                        // refuse it on its own rather than assume it away.
                        let representation = representation::representation_of(&import.interface);
                        if !representation::startup_importable(representation) {
                            return Err(Finding::new(
                                "V2013_CAPABILITY",
                                at(),
                                alloc::format!(
                                    "capability position {position} is filled by an import of \
                                     {}, which no import can produce a value of",
                                    import.interface
                                ),
                            ));
                        }
                        import.interface.clone()
                    }
                    // A value's interface is **its own type**, checked against
                    // the artifact rather than taken from a frontend's word.
                    // A scalar, a constant of any kind, a value of a nominal
                    // record type or one outside the table is refused here: it
                    // is not a capability, so it cannot fill a capability
                    // position.
                    CapabilitySource::Value(operand) => {
                        let ty = operand_type(module, function, operand);
                        match ty.and_then(|ty| module.type_of(ty)) {
                            Some(TypeDef::Capability(interface)) => {
                                // **The interface's own path is not always a
                                // member of the family that represents it**
                                // (ADR-0085 §17.1). Where the representation is
                                // something other than `AsInterface`, a value
                                // of `Capability(I)` is not a value of `I` at
                                // all, and admitting it here would be admitting
                                // the one form §4.3 exists to exclude.
                                if !matches!(
                                    representation::representation_of(interface),
                                    representation::Representation::AsInterface
                                ) {
                                    return Err(Finding::new(
                                        "V2013_CAPABILITY",
                                        at(),
                                        alloc::format!(
                                            "capability position {position} is filled by a \
                                             capability of {interface}, which is not how a \
                                             capability of that interface is represented"
                                        ),
                                    ));
                                }
                                interface.clone()
                            }
                            // **The arms ADR-0085 §7 adds, fed by this crate's
                            // own closed table and by nothing else.** The
                            // operand's type comes from the artifact's own type
                            // table and the map from this crate's `REPRESENTED`
                            // — neither is the producer's word, and neither is
                            // `unsafe_interface`, which is the claim being
                            // checked below.
                            //
                            // It answers with one interface rather than a set
                            // because a family belongs to at most one
                            // (`SYSTEM_INTERFACE_V1` §4.3 rule 1). That
                            // uniqueness is what makes this a derivation at all
                            // rather than a predicate that had to be inverted.
                            Some(other) => match representation::interface_of(other) {
                                Some(path) => String::from(path),
                                None => {
                                    return Err(Finding::new(
                                        "V2013_CAPABILITY",
                                        at(),
                                        alloc::format!(
                                            "capability position {position} is filled by a \
                                             value that is not of any capability type"
                                        ),
                                    ))
                                }
                            },
                            None => {
                                return Err(Finding::new(
                                    "V2013_CAPABILITY",
                                    at(),
                                    alloc::format!(
                                        "capability position {position} is filled by a value \
                                         that is not of any capability type"
                                    ),
                                ))
                            }
                        }
                    }
                };
                // The instruction says two things about which interface it
                // reaches: the source at position zero, and the accepted
                // interface ID docs/43 §3 asks it to carry. They are checked
                // against each other because an artifact that named one
                // interface while acting through a capability of another would
                // pass every other check here — the source resolves, the
                // interface was imported, the function declared it — and still
                // be performing an operation on authority of the wrong type.
                if position == 0 {
                    if let Some(declared) = &instruction.unsafe_interface {
                        if &interface != declared {
                            return Err(Finding::new(
                                "V2013_CAPABILITY",
                                at(),
                                alloc::format!(
                                    "an operation declares {declared} and is performed \
                                     through {interface}"
                                ),
                            ));
                        }
                    }
                }
                // And the enclosing function admits reaching that interface.
                // `docs/42` §2 requires the enclosing `uses` effect to match,
                // and it requires it of every authority the operation acts
                // through — which is what makes this the per-position exact
                // interface check for a runtime-sourced capability, where there
                // is no import declaration to compare against.
                if !function.signature.effects.iter().any(|e| e == &interface) {
                    return Err(Finding::new(
                        "V2033_UNSAFE",
                        at(),
                        alloc::format!(
                            "capability position {position} acts through {interface}, \
                             which this function does not declare"
                        ),
                    ));
                }
                // Every capability an operation requires is a separate
                // authority (ADR-0063), so each is named separately and none
                // may be the same one twice. A repeat would be one grant
                // standing in for two, which is how "reply here and wait there"
                // becomes "reply here and wait here" without anything in the
                // artifact saying so. It is the *source* that is compared, so
                // one runtime value used twice is refused exactly as one import
                // used twice is.
                if capabilities[..position].contains(source) {
                    return Err(Finding::new(
                        "V2013_CAPABILITY",
                        at(),
                        "an operation names one capability more than once",
                    ));
                }
                // **The language minor is a verifier obligation and not only a
                // frontend one** (ADR-0085 §13, §17.7). A module receives the
                // language its header declared, so an artifact whose header
                // says 1.2 and whose body fills a capability position by the
                // capability-representation rule is refused here — a
                // hand-written artifact never met the frontend that would have
                // refused it in source.
                //
                // **Checked last of the position's rules on purpose.**
                // Everything above is wrong about the operation itself: the
                // wrong source, the wrong representation, an interface the
                // artifact misnames, an effect the function never declared.
                // Only an artifact that is right about all of those has one
                // thing left wrong with it, and that thing is its version — so
                // this reports the version rather than masking a defect that
                // would be there at any minor.
                if !matches!(
                    representation::representation_of(&interface),
                    representation::Representation::AsInterface
                ) && !declares_minor_at_least(&module.header.language_version, 3)
                {
                    return Err(Finding::new(
                        "V2013_CAPABILITY",
                        at(),
                        alloc::format!(
                            "capability position {position} is filled by the capability \
                             representation of {interface}, which the declared language \
                             version {} does not have",
                            module.header.language_version
                        ),
                    ));
                }
            }
        }
        _ => {}
    }
    if instruction.source >= module.source_map.len() {
        return Err(Finding::new(
            "V2040_SOURCE_MAP",
            at(),
            "an instruction has no source-map entry",
        ));
    }
    Ok(())
}

fn check_operand(
    module: &Module,
    function: &Function,
    operand: &Operand,
    at: &dyn Fn() -> String,
) -> Result<(), Finding> {
    match operand {
        Operand::Value(value) => {
            if *value >= function.values.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at(),
                    alloc::format!("an operand names value {value} outside the table"),
                ));
            }
        }
        Operand::Constant(constant) => {
            if *constant >= module.constants.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at(),
                    alloc::format!("an operand names constant {constant} outside the table"),
                ));
            }
        }
    }
    Ok(())
}

/// One device-memory access, against the contract the artifact's declared
/// language version selects (ADR-0081 §7–§8, ADR-0088 §5).
///
/// **The width and the byte order are looked up, not compared to a list kept
/// here.** An instruction carries a width and an order, and the question is
/// whether that pair is one an accepted operation produces: a 3-byte access, a
/// 16-byte one, or a big-endian one is a device access the language has no
/// operation for, whatever else is right about the instruction. Asking the
/// shared declarative table is what keeps a second width table out of the
/// verifier — and the table is data, so this is still the verifier's own
/// derivation.
///
/// Findings are `V2021_REGION`, which is the code this family already uses for
/// `Op::Share` and `Op::DmaSync`, including for their result-type and version
/// obligations. One operation family, one code, and the message says which
/// obligation failed.
#[allow(clippy::too_many_arguments)]
fn check_device_access(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    operands: &[&Operand],
    width: u8,
    writes: bool,
    little_endian: bool,
    at: &dyn Fn() -> String,
) -> Result<(), Finding> {
    let Some(minor) = declared_minor(&module.header.language_version) else {
        return Err(Finding::new(
            "V2002_SCHEMA",
            at(),
            "a device access is in an artifact whose language version is unreadable",
        ));
    };
    let Some(operation) = tos_predeclared::mmio_operation(width, writes, little_endian) else {
        return Err(Finding::new(
            "V2021_REGION",
            at(),
            alloc::format!(
                "a device {} of width {width} and {} byte order is no accepted operation",
                if writes { "write" } else { "read" },
                if little_endian {
                    "little-endian"
                } else {
                    "big-endian"
                }
            ),
        ));
    };
    // The IR fixes these operand counts structurally, so this can only fail if
    // the schema and the contract have drifted apart — which is exactly when a
    // silent mismatch would be worst.
    if operands.len() != operation.arity() {
        return Err(Finding::new(
            "V2011_CFG",
            at(),
            alloc::format!(
                "a device access carries {} operand(s) where {} takes {}",
                operands.len(),
                operation.name,
                operation.arity()
            ),
        ));
    }
    let mut actual = Vec::new();
    for (position, operand) in operands.iter().enumerate() {
        let Some(resolved) = operand_definition(module, function, operand) else {
            return Err(Finding::new(
                "V2021_REGION",
                at(),
                alloc::format!(
                    "{} of a device access has no type",
                    device_position(position, writes)
                ),
            ));
        };
        actual.push(resolved);
    }
    for (position, rule) in operation.parameters.iter().enumerate() {
        if !satisfies(module, *rule, &actual, position) {
            return Err(Finding::new(
                "V2021_REGION",
                at(),
                alloc::format!(
                    "{} of {} is not what the operation requires",
                    device_position(position, writes),
                    operation.name
                ),
            ));
        }
    }
    let Some(declared) = module.type_of(instruction.ty) else {
        return Err(Finding::new(
            "V2021_REGION",
            at(),
            "a device access declares a result outside the type table",
        ));
    };
    // A read answers exact `u64` at every width and a write answers `unit` —
    // ADR-0081 §7's carrier rule, which the amendment states and the table
    // carries. The width belongs to the transaction, never to the value's type.
    if !result_is(module, operation.result, &actual, declared) {
        return Err(Finding::new(
            "V2021_REGION",
            at(),
            alloc::format!(
                "a device {} is typed as something other than {}",
                if writes { "write" } else { "read" },
                if writes { "unit" } else { "u64" }
            ),
        ));
    }
    // Checked last for ADR-0085 §17.7's reason: every rule above is wrong about
    // the access at any minor, and only an artifact right about all of them has
    // its version left as the one thing wrong with it.
    if operation.minimum_minor > minor {
        return Err(Finding::new(
            "V2021_REGION",
            at(),
            alloc::format!(
                "a device access is not a form of declared language version {}",
                module.header.language_version
            ),
        ));
    }
    Ok(())
}

/// What an operand position of a device access is, for a reader of a finding.
fn device_position(position: usize, writes: bool) -> &'static str {
    match (position, writes) {
        (0, _) => "the region",
        (1, _) => "the byte offset",
        (2, true) => "the written value",
        _ => "an operand",
    }
}

/// What a launch knows about the modules it has already verified.
///
/// **The production authority is `tos-residency::launch`**, which builds one of
/// these from its own successful verifications and from the closure source it
/// was handed. Nothing outside this crate can mint a
/// [`VerifiedArtifactEvidence`], so an implementation of this trait can only
/// hand back evidence some verification actually produced: the trait is a way
/// of *reaching* what a launch already proved, never a way of asserting it.
///
/// The image is offered through a callback rather than returned, so a launch
/// whose images live behind a shared handle lends the bytes instead of copying
/// a third of a megabyte per imported dependency.
pub trait VerifiedDependencies {
    /// Whether a closure is being stated at all.
    ///
    /// `false` means "nothing has been verified alongside this module", which
    /// is the honest answer for a caller checking one artifact on its own, and
    /// is the same shape as an empty `ResolutionSnapshot`: there is nothing to
    /// compare against, so the closure obligations do not apply.
    ///
    /// **A launch always answers `true`**, which is what makes a dependency it
    /// cannot produce evidence for a refusal there rather than a check quietly
    /// skipped. The two cases must not be inferred from the same silence.
    fn states_closure(&self) -> bool;

    /// The evidence for a module of this name, if this launch verified one.
    fn evidence(&self, module_name: &str) -> Option<VerifiedArtifactEvidence>;

    /// Lends that module's immutable image to `read`, if it can be obtained.
    fn read_image(&self, module_name: &str, read: &mut dyn FnMut(&[u8]));

    /// Records this module's unique caller-to-dependency relationships and
    /// answers the closure's running total (`docs/44` §2, ADR-0090).
    ///
    /// **The count lives with the launch and the ceiling lives here.** The
    /// number of authenticated dependency reopens a closure demands is exactly
    /// the number of these edges, so the component that performs them keeps the
    /// counter — one `usize`, alive for the launch only — and the component that
    /// owns the accepted limits decides when it is too many.
    fn add_edges(&mut self, count: usize) -> usize;
}

/// A verifier asked about a module outside any closure.
///
/// For a caller checking one artifact on its own — a test, a tool, a fixture.
/// It states that nothing has been verified, so the closure obligations below
/// have nothing to check against and are not checked. It is **not** the
/// production path: `tos-residency::launch` supplies a real one.
pub struct NoDependencies;

impl VerifiedDependencies for NoDependencies {
    fn states_closure(&self) -> bool {
        false
    }

    fn add_edges(&mut self, _count: usize) -> usize {
        0
    }

    fn evidence(&self, _module_name: &str) -> Option<VerifiedArtifactEvidence> {
        None
    }

    fn read_image(&self, _module_name: &str, _read: &mut dyn FnMut(&[u8])) {}
}

/// `Module::exports` is the canonical public projection of `Module::functions`.
///
/// `docs/43` §2 names the section "Imports and exported signatures", and what
/// the runtime actually enters is a function: `Closure::export_of` resolves a
/// name against the resident module's own `functions` filtered by visibility,
/// never against this table. So a table that disagreed with that projection
/// would be a description of a module nobody executes — and until this check
/// existed, the verifier compared the two not at all and an artifact could
/// carry any export table it liked above any bodies it liked.
///
/// The comparison is **full `Signature` equality**, every field the schema
/// represents: name, visibility, parameters with their types *and* modes, the
/// result, the async flag and the declared effects. That does not make an
/// imported call mode-aware — modes are still not compared at a call — it makes
/// the authenticated export table a true description of the function execution
/// will enter.
///
/// `V2004_TABLE_ORDER` owns it because what is wrong is the table: it is not
/// the canonical form of the section, whether the defect is a missing entry, an
/// extra one, a private function published, a duplicate, or a field that does
/// not match the body it claims to describe.
fn check_export_projection(module: &Module) -> Result<(), Finding> {
    let expected: Vec<&tos_ir::Signature> = module
        .functions
        .iter()
        .filter(|function| function.signature.visibility == tos_ir::Visibility::Public)
        .map(|function| &function.signature)
        .collect();
    if expected.len() != module.exports.len() {
        return Err(Finding::new(
            "V2004_TABLE_ORDER",
            "exports",
            alloc::format!(
                "the module declares {} public function(s) and exports {} signature(s)",
                expected.len(),
                module.exports.len()
            ),
        ));
    }
    for (position, (declared, exported)) in expected.iter().zip(&module.exports).enumerate() {
        if *declared != exported {
            return Err(Finding::new(
                "V2004_TABLE_ORDER",
                alloc::format!("exports[{position}]"),
                alloc::format!(
                    "the exported signature of {} is not the signature of the function it names",
                    exported.name
                ),
            ));
        }
    }
    Ok(())
}

/// One imported call of the module being verified, indexed once.
struct ImportedCall<'a> {
    function: usize,
    import: usize,
    name: &'a str,
    operands: &'a [Operand],
    result: TypeId,
    at: String,
}

/// Every imported named call in the caller, found in one pass.
///
/// **One scan, not one per dependency.** A caller importing many modules would
/// otherwise be walked once for each of them; this walks its instruction stream
/// once and groups what it finds. The index is bounded by the caller's own
/// accepted instruction ceiling and dies with this verification.
fn imported_calls(module: &Module) -> Vec<ImportedCall<'_>> {
    let mut found = Vec::new();
    for (index, function) in module.functions.iter().enumerate() {
        for (block_index, block) in function.blocks.iter().enumerate() {
            for instruction in &block.instructions {
                if let Op::Call {
                    target: CallTarget::Imported { import, name },
                    operands,
                } = &instruction.op
                {
                    found.push(ImportedCall {
                        function: index,
                        import: *import,
                        name: name.as_str(),
                        operands,
                        result: instruction.ty,
                        at: alloc::format!("functions[{index}].blocks[{block_index}]"),
                    });
                }
            }
        }
    }
    found
}

/// Whether a type of this module is the same type as one of a dependency's.
///
/// **Structure and nominal identity, across two type tables.** A type id is a
/// position in the table that holds it, so nothing can be compared by index:
/// the two graphs are walked together. A nominal type is equal only when the
/// module that *defined* it is the same module — the content identity, not the
/// spelling — so two records named `Point` declared by two modules stay two
/// types however alike their fields are.
fn types_agree(
    caller: &Module,
    caller_ty: TypeId,
    dependency: &[TypeDef],
    dependency_ty: TypeId,
    depth: usize,
) -> bool {
    if depth > tos_ir::MAX_TYPE_DEPTH {
        return false;
    }
    let (Some(here), Some(there)) = (caller.type_of(caller_ty), dependency.get(dependency_ty))
    else {
        return false;
    };
    let pair = |a: &TypeId, b: &TypeId| types_agree(caller, *a, dependency, *b, depth + 1);
    let list = |a: &[TypeId], b: &[TypeId]| {
        a.len() == b.len() && a.iter().zip(b).all(|(one, other)| pair(one, other))
    };
    match (here, there) {
        (TypeDef::Unit, TypeDef::Unit)
        | (TypeDef::Bool, TypeDef::Bool)
        | (TypeDef::Size, TypeDef::Size)
        | (TypeDef::Duration, TypeDef::Duration)
        | (TypeDef::Text, TypeDef::Text)
        | (TypeDef::Bytes, TypeDef::Bytes)
        | (TypeDef::ConversionError, TypeDef::ConversionError)
        | (TypeDef::MmioRegion, TypeDef::MmioRegion)
        | (TypeDef::MmioRegionMut, TypeDef::MmioRegionMut)
        | (TypeDef::Event, TypeDef::Event)
        | (TypeDef::Semaphore, TypeDef::Semaphore)
        | (TypeDef::Barrier, TypeDef::Barrier)
        | (TypeDef::Latch, TypeDef::Latch)
        | (TypeDef::AtomicBool, TypeDef::AtomicBool)
        | (TypeDef::AtomicU32, TypeDef::AtomicU32)
        | (TypeDef::AtomicU64, TypeDef::AtomicU64) => true,
        (TypeDef::Int(one), TypeDef::Int(other)) => one == other,
        (TypeDef::Option(a), TypeDef::Option(b))
        | (TypeDef::Task(a), TypeDef::Task(b))
        | (TypeDef::TaskResult(a), TypeDef::TaskResult(b))
        | (TypeDef::Shared(a), TypeDef::Shared(b))
        | (TypeDef::Region(a), TypeDef::Region(b))
        | (TypeDef::DmaRegion(a), TypeDef::DmaRegion(b))
        | (TypeDef::RegionMut(a), TypeDef::RegionMut(b))
        | (TypeDef::DmaRegionMut(a), TypeDef::DmaRegionMut(b))
        | (TypeDef::Mutex(a), TypeDef::Mutex(b))
        | (TypeDef::RwLock(a), TypeDef::RwLock(b))
        | (TypeDef::MutexGuard(a), TypeDef::MutexGuard(b))
        | (TypeDef::ReadGuard(a), TypeDef::ReadGuard(b))
        | (TypeDef::WriteGuard(a), TypeDef::WriteGuard(b))
        | (TypeDef::Channel(a), TypeDef::Channel(b))
        | (TypeDef::Slice(a), TypeDef::Slice(b)) => pair(a, b),
        (TypeDef::Result(a, e), TypeDef::Result(b, f)) => pair(a, b) && pair(e, f),
        (TypeDef::Array(a, n), TypeDef::Array(b, m)) => n == m && pair(a, b),
        (TypeDef::Tuple(a), TypeDef::Tuple(b)) => list(a, b),
        (TypeDef::Function(a, r), TypeDef::Function(b, s)) => list(a, b) && pair(r, s),
        (TypeDef::Capability(a), TypeDef::Capability(b)) => a == b,
        (
            TypeDef::Nominal {
                module_content_id: one_module,
                export_name: one_name,
                kind: one_kind,
                fields: one_fields,
                variants: one_variants,
            },
            TypeDef::Nominal {
                module_content_id: other_module,
                export_name: other_name,
                kind: other_kind,
                fields: other_fields,
                variants: other_variants,
            },
        ) => {
            one_module == other_module
                && one_name == other_name
                && one_kind == other_kind
                && list(one_fields, other_fields)
                && one_variants.len() == other_variants.len()
                && one_variants
                    .iter()
                    .zip(other_variants)
                    .all(|(a, b)| a.name == b.name && list(&a.payload, &b.payload))
        }
        _ => false,
    }
}

/// Imported calls, against the exact dependency artifact this launch verified.
///
/// **The declared resolution is not the authority here and never was one.** A
/// `ResolutionSnapshot` is what a producer *says* the closure resolved to, and
/// until this check existed a caller was admitted because its import named a
/// module the snapshot happened to describe — so a hostile declaration naming
/// an export the real module does not have, or agreeing perfectly with a forged
/// caller about a signature the real module does not declare, was accepted and
/// discovered, if at all, by the engine.
///
/// What is authority is the dependency artifact this same launch already
/// verified. ADR-0071 §5's reload rule applies exactly: the evidence minted by
/// that verification names the artifact digest, the bytes offered are hashed
/// here and compared with it, and only bytes that *are* that artifact are
/// parsed. Semantic verification is not repeated — it already happened, to
/// these bytes.
///
/// The reconstructed surface is transient: one dependency is open at a time and
/// the decoded prefix is dropped before the next is opened, so nothing about a
/// module's export types survives this function, let alone the launch.
fn check_imported_calls(
    module: &Module,
    dependencies: &mut dyn VerifiedDependencies,
    limits: &Limits,
) -> Result<(), Finding> {
    if !dependencies.states_closure() {
        return Ok(());
    }
    // **The topology quota, counted by the component that pays it** (ADR-0090).
    // An edge is one unique `(caller, resolved dependency)` pair — two bindings
    // of one module are one relationship, authenticated once however many call
    // sites reach it — and the running total is checked before this module's
    // dependencies are opened, so a closure above the ceiling is refused at the
    // edge that crosses it rather than after the work the rest of it wanted.
    let mut distinct: Vec<&str> = module
        .imports
        .iter()
        .map(|import| import.module_name.as_str())
        .collect();
    distinct.sort_unstable();
    distinct.dedup();
    let edges = dependencies.add_edges(distinct.len());
    if edges > limits.closure_edges {
        return Err(Finding::new(
            "V2001_LIMIT",
            "resolved closure direct dependency edges",
            alloc::format!(
                "{edges} unique caller-to-dependency edge(s) exceeds the ceiling of {}",
                limits.closure_edges
            ),
        ));
    }
    let calls = imported_calls(module);
    // A dependency-first closure is what makes this check possible at all
    // (ADR-0071 §1): every module a caller imports has been verified before the
    // caller is. A forward dependency — and therefore any cycle — is refused
    // here rather than found by a second algorithm.
    for (index, import) in module.imports.iter().enumerate() {
        let Some(evidence) = dependencies.evidence(&import.module_name) else {
            return Err(Finding::new(
                "V2012_IMPORT",
                alloc::format!("import {index}"),
                alloc::format!(
                    "{} has not been verified earlier in this closure",
                    import.module_name
                ),
            ));
        };
        if !import.module_content_id.is_empty()
            && !evidence.states_content_id(&import.module_content_id)
        {
            return Err(Finding::new(
                "V2012_IMPORT",
                alloc::format!("import {index}"),
                alloc::format!(
                    "{} was verified as another content identity than the one this module claims",
                    import.module_name
                ),
            ));
        }
    }
    // One dependency open at a time, in import order, and only those a call
    // actually reaches.
    //
    // **Once per dependency, not once per binding** (ADR-0090 §2a). Two
    // bindings of one module are one relationship: the quota above counted them
    // once, and this is the work that count is a bound on, so it is paid once
    // too. Every call site reaching any binding of a dependency is checked
    // against the single surface its one authenticated reopen reconstructs.
    let mut opened: Vec<&str> = Vec::new();
    for (index, import) in module.imports.iter().enumerate() {
        let name = import.module_name.as_str();
        if opened.contains(&name) {
            continue;
        }
        if !calls
            .iter()
            .any(|call| module.imports[call.import].module_name == import.module_name)
        {
            continue;
        }
        opened.push(name);
        let evidence = dependencies
            .evidence(&import.module_name)
            .expect("the loop above refused an import without evidence");
        let mut outcome: Result<(), Finding> = Err(Finding::new(
            "V2012_IMPORT",
            alloc::format!("import {index}"),
            alloc::format!(
                "the image of {} is not available to authenticate",
                import.module_name
            ),
        ));
        dependencies.read_image(&import.module_name, &mut |image| {
            outcome = check_against_dependency(module, &calls, index, &evidence, image, limits);
        });
        outcome?;
    }
    Ok(())
}

/// The authenticated reopen, for one dependency of one caller.
fn check_against_dependency(
    module: &Module,
    calls: &[ImportedCall<'_>],
    import: usize,
    evidence: &VerifiedArtifactEvidence,
    image: &[u8],
    limits: &Limits,
) -> Result<(), Finding> {
    let name = &module.imports[import].module_name;
    // **Before any export datum is read.** Wrong bytes are a refusal, never a
    // fallback and never a reparse of something else.
    if !evidence.authenticates(image) {
        return Err(Finding::new(
            "V2012_IMPORT",
            alloc::format!("import {import}"),
            alloc::format!("the image offered for {name} is not the artifact this launch verified"),
        ));
    }
    let prefix =
        tos_image::parse_export_prefix(image, &image::parse_limits(limits)).map_err(|error| {
            Finding::new(
                "V2012_IMPORT",
                alloc::format!("import {import}"),
                alloc::format!("the verified artifact of {name} did not reconstruct: {error:?}"),
            )
        })?;
    // Every call site that reaches this dependency through **any** of the
    // caller's bindings of it, against the one surface just reconstructed.
    for call in calls
        .iter()
        .filter(|call| module.imports[call.import].module_name == *name)
    {
        let Some(signature) = prefix
            .exports
            .iter()
            .find(|signature| signature.name == call.name)
        else {
            return Err(Finding::new(
                "V2012_IMPORT",
                call.at.clone(),
                alloc::format!("{name} does not export {}", call.name),
            ));
        };
        if call.operands.len() != signature.parameters.len() {
            return Err(Finding::new(
                "V2011_CFG",
                call.at.clone(),
                alloc::format!(
                    "a call supplies {} operand(s) to {name}.{}, which takes {}",
                    call.operands.len(),
                    call.name,
                    signature.parameters.len()
                ),
            ));
        }
        for (position, (operand, declared)) in
            call.operands.iter().zip(&signature.parameters).enumerate()
        {
            let caller = &module.functions[call.function];
            let Some(actual) = operand_type(module, caller, operand) else {
                return Err(Finding::new(
                    "V2010_TYPE",
                    call.at.clone(),
                    alloc::format!("operand {position} of {name}.{} has no type", call.name),
                ));
            };
            if !types_agree(module, actual, &prefix.types, declared.ty, 0) {
                return Err(Finding::new(
                    "V2010_TYPE",
                    call.at.clone(),
                    alloc::format!(
                        "operand {position} of {name}.{} is not the declared parameter type",
                        call.name
                    ),
                ));
            }
        }
        // The export's result is already the call-site result: an `async fn`
        // declared `-> T` is lowered with `Task<T>` and the export table
        // records what was lowered, so nothing is reconstructed here.
        if !types_agree(module, call.result, &prefix.types, signature.result, 0) {
            return Err(Finding::new(
                "V2010_TYPE",
                call.at.clone(),
                alloc::format!(
                    "a call to {name}.{} declares a result the export does not produce",
                    call.name
                ),
            ));
        }
    }
    Ok(())
}

/// The declared minor of a language version this schema represents.
///
/// Read from the header the module carries, because `docs/42` §1's rule is that
/// a module receives the language *its header declared*. An unparsable version
/// has already been refused by `check_schema`; answering `None` here is the
/// fail-closed direction for a caller that reaches it anyway.
fn declared_minor(version: &str) -> Option<u32> {
    match version.split_once('.') {
        Some(("1", declared)) => declared.parse::<u32>().ok(),
        _ => None,
    }
}

/// One predeclared call, against the contract the artifact's own declared
/// language version selects (ADR-0088 §5).
///
/// Six obligations, and the order they are checked in is deliberate. The name,
/// the form, the count, the operands and the result are all things wrong with
/// the *operation*; the declared minor is checked last, so that an artifact
/// which is right about all of them has its version left as the one thing wrong
/// with it — the rule ADR-0085 §17.7 settled for the capability gate and
/// ADR-0086 for the DMA one.
fn check_predeclared(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    name: &str,
    operands: &[Operand],
    at: &dyn Fn() -> String,
) -> Result<(), Finding> {
    let Some(minor) = declared_minor(&module.header.language_version) else {
        return Err(Finding::new(
            "V2002_SCHEMA",
            at(),
            "a predeclared call is in an artifact whose language version is unreadable",
        ));
    };
    let contract = tos_predeclared::Contract::at_minor(minor);
    let operation = match contract.lookup(name) {
        tos_predeclared::Lookup::Unknown => {
            return Err(Finding::new(
                "V2011_CFG",
                at(),
                alloc::format!("a call names {name}, which is no predeclared operation"),
            ))
        }
        // Available or not at this minor, everything below is true of the
        // operation itself; the minor is the last thing asked.
        tos_predeclared::Lookup::RequiresMinor(operation)
        | tos_predeclared::Lookup::Available(operation) => operation,
    };
    // **The form is part of the contract.** `share`, the device accesses and
    // the DMA ordering points each lower to their own verifier-visible
    // operation, for reasons `docs/43` §3, ADR-0081 §8 and ADR-0086 §4 give,
    // and an artifact writing one as an opaque `Call` is hiding exactly what
    // those decisions put in the open.
    if operation.form != tos_predeclared::Form::Call {
        return Err(Finding::new(
            "V2011_CFG",
            at(),
            alloc::format!("{name} is written as a call, and it has its own operation"),
        ));
    }
    if operands.len() != operation.arity() {
        return Err(Finding::new(
            "V2011_CFG",
            at(),
            alloc::format!(
                "a call supplies {} operand(s) to {name}, which takes {}",
                operands.len(),
                operation.arity()
            ),
        ));
    }
    let mut actual = Vec::new();
    for (position, operand) in operands.iter().enumerate() {
        let Some(resolved) = operand_definition(module, function, operand) else {
            return Err(Finding::new(
                "V2010_TYPE",
                at(),
                alloc::format!("operand {position} of {name} has no type"),
            ));
        };
        actual.push(resolved);
    }
    for (position, rule) in operation.parameters.iter().enumerate() {
        if !satisfies(module, *rule, &actual, position) {
            return Err(Finding::new(
                "V2010_TYPE",
                at(),
                alloc::format!("operand {position} of {name} is not the type its rule requires"),
            ));
        }
    }
    let Some(declared) = module.type_of(instruction.ty) else {
        return Err(Finding::new(
            "V2010_TYPE",
            at(),
            alloc::format!("a call to {name} declares a result outside the type table"),
        ));
    };
    if !result_is(module, operation.result, &actual, declared) {
        return Err(Finding::new(
            "V2010_TYPE",
            at(),
            alloc::format!("a call to {name} declares a result the operation does not produce"),
        ));
    }
    if operation.minimum_minor > minor {
        return Err(Finding::new(
            "V2011_CFG",
            at(),
            alloc::format!(
                "{name} is not an operation of declared language version {}",
                module.header.language_version
            ),
        ));
    }
    Ok(())
}

/// The exact type of one operand, as a **definition** and, where the artifact
/// has one, as an identifier too.
///
/// [`operand_type`] answers with a `TypeId`, which a constant may not have: a
/// constant carries its own type and nothing in `tos-ir/v1` requires that type
/// to be interned merely because a constant of it appears. For a check
/// comparing against a declared parameter's `TypeId` that is the right answer —
/// a type the callee's signature names is in the table by construction. For a
/// rule that asks what *class* a type is in it is the wrong one, because "not
/// interned" is not "not an integer", and a verifier that refused `to_u8(300u64)`
/// for it would be refusing a correct artifact.
fn operand_definition(
    module: &Module,
    function: &Function,
    operand: &Operand,
) -> Option<(Option<TypeId>, TypeDef)> {
    match operand {
        Operand::Value(value) => {
            let ty = function.values.get(*value).copied()?;
            Some((Some(ty), module.type_of(ty)?.clone()))
        }
        Operand::Constant(constant) => {
            let definition = match module.constants.get(*constant)? {
                Constant::Unit => TypeDef::Unit,
                Constant::Bool(_) => TypeDef::Bool,
                Constant::Int(kind, _) => TypeDef::Int(*kind),
                Constant::Size(_) => TypeDef::Size,
                Constant::Duration(_) => TypeDef::Duration,
                Constant::Text(_) => TypeDef::Text,
                Constant::Bytes(_) => TypeDef::Bytes,
            };
            let id = module.types.iter().position(|ty| *ty == definition);
            Some((id, definition))
        }
    }
}

/// Whether the operand at a position satisfies the contract's rule for it.
///
/// Every answer comes from this artifact's own types. A rule that names a class
/// — "any exact integer" — is a question about what the type *is*, not about
/// what a producer called it, and a rule relating two positions is answered by
/// comparing the two derived types rather than two claims.
fn satisfies(
    module: &Module,
    rule: tos_predeclared::ParameterRule,
    actual: &[(Option<TypeId>, TypeDef)],
    position: usize,
) -> bool {
    use tos_predeclared::ParameterRule as Rule;
    let Some((id, definition)) = actual.get(position) else {
        return false;
    };
    match rule {
        Rule::Integer => matches!(definition, TypeDef::Int(_)),
        Rule::IntegerOrSize => matches!(definition, TypeDef::Int(_) | TypeDef::Size),
        Rule::Size => matches!(definition, TypeDef::Size),
        Rule::Exactly(kind) => matches!(definition, TypeDef::Int(found) if *found == kind),
        // The same type, and an integer one: "the same exact integer type" is
        // two requirements and an artifact may fail either.
        Rule::SameAs(other) => {
            matches!(definition, TypeDef::Int(_))
                && actual.get(other).map(|(_, other)| other) == Some(definition)
        }
        Rule::MmioRegion => matches!(definition, TypeDef::MmioRegion | TypeDef::MmioRegionMut),
        Rule::MmioRegionMut => matches!(definition, TypeDef::MmioRegionMut),
        Rule::DmaRegion => matches!(definition, TypeDef::DmaRegion(_) | TypeDef::DmaRegionMut(_)),
        // The one rule whose answer needs the type graph rather than the type,
        // so it needs an identifier to walk from. A constant has no region and
        // no guard anywhere under it, which is the reason it may have none.
        Rule::Shareable => match id {
            Some(ty) => {
                !region_facts(module, *ty).is_some_and(|(shareable, _)| !shareable)
                    && transitively_immutable(module, *ty, 0)
            }
            None => true,
        },
    }
}

/// Whether the instruction's declared result is the one the contract states.
fn result_is(
    module: &Module,
    rule: tos_predeclared::ResultRule,
    actual: &[(Option<TypeId>, TypeDef)],
    declared: &TypeDef,
) -> bool {
    use tos_predeclared::ResultRule as Rule;
    let of = |position: usize| actual.get(position).map(|(_, definition)| definition);
    match rule {
        Rule::Unit => matches!(declared, TypeDef::Unit),
        Rule::Integer(kind) => matches!(declared, TypeDef::Int(found) if *found == kind),
        Rule::Conversion(kind) => match declared {
            TypeDef::Result(ok, error) => {
                matches!(module.type_of(*ok), Some(TypeDef::Int(found)) if *found == kind)
                    && matches!(module.type_of(*error), Some(TypeDef::ConversionError))
            }
            _ => false,
        },
        Rule::SameAsOperand(position) => of(position) == Some(declared),
        Rule::SharedOfOperand(position) => match declared {
            TypeDef::Shared(inner) => module.type_of(*inner) == of(position),
            _ => false,
        },
    }
}

/// The exact ordered operand list docs/43 §4 requires, checked against a
/// signature the module itself declares.
///
/// Three obligations, and they are three because a call can be wrong in three
/// independent ways: it can supply the wrong *number* of operands, an operand
/// of the wrong *type*, or claim a *result* the callee does not produce. The
/// third matters as much as the others — an instruction's result type is what
/// every later instruction reads the slot as, so a call that declared someone
/// else's result would let verified arithmetic run on a value of a type nothing
/// produced.
///
/// **Modes are not compared, deliberately.** `Parameter` carries a `PassMode`
/// and this looks only at `ty`, because what a caller must supply is a value of
/// the parameter's type whichever mode it is passed in; whether the modes
/// themselves are recoverable at a call is a separate, recorded question about
/// the function type, not one to settle by implication here.
fn check_signature(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    parameters: &[tos_ir::Parameter],
    result: TypeId,
    operands: &[Operand],
    at: &dyn Fn() -> String,
) -> Result<(), Finding> {
    if operands.len() != parameters.len() {
        return Err(Finding::new(
            "V2011_CFG",
            at(),
            alloc::format!(
                "a call supplies {} operand(s) to a callee of {} parameter(s)",
                operands.len(),
                parameters.len()
            ),
        ));
    }
    for (position, (operand, declared)) in operands.iter().zip(parameters).enumerate() {
        let Some(actual) = operand_type(module, function, operand) else {
            return Err(Finding::new(
                "V2010_TYPE",
                at(),
                alloc::format!("operand {position} of a call has no type"),
            ));
        };
        if actual != declared.ty {
            return Err(Finding::new(
                "V2010_TYPE",
                at(),
                alloc::format!("operand {position} of a call is not the declared parameter type"),
            ));
        }
    }
    if instruction.ty != result {
        return Err(Finding::new(
            "V2010_TYPE",
            at(),
            "a call declares a result that is not the callee's",
        ));
    }
    Ok(())
}

/// The exact type of the location a place denotes, derived from the artifact.
///
/// **Derived, never trusted.** The instruction carries a declared type; this
/// works out the only legal one from the root's recorded type and the steps the
/// place actually takes. Where the two disagree the artifact is relabelling a
/// location — declaring that a read of a record field yields the record, or
/// that an element of an array of `i64` is an `i32` — and every instruction
/// downstream then operates under a static assumption the run does not meet.
///
/// `None` means the place is not one this can type, which for a verifier is the
/// same answer as wrong: it will not sign what it cannot derive.
/// The place an access names, when the operation is one.
fn place_of(op: &Op) -> Option<&tos_ir::Place> {
    match op {
        Op::Read { place }
        | Op::Move { place }
        | Op::Write { place, .. }
        | Op::Borrow { place, .. } => Some(place),
        _ => None,
    }
}

/// Whether this instruction's place is served by a region, which the check
/// above already settled against the region's element type.
fn region_element_of(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
) -> Option<TypeId> {
    let place = place_of(&instruction.op)?;
    region_element(module, function, place)
}

fn projected_place_type(
    module: &Module,
    function: &Function,
    place: &tos_ir::Place,
) -> Option<TypeId> {
    let mut current = function.values.get(place.root).copied()?;
    for step in &place.path {
        let indexed = matches!(
            step,
            tos_ir::PlaceStep::Index(_) | tos_ir::PlaceStep::DynamicIndex(_)
        );
        current = match (module.type_of(current), step) {
            (Some(TypeDef::Nominal { fields, .. }), tos_ir::PlaceStep::Field(index)) => {
                fields.get(*index).copied()?
            }
            (Some(TypeDef::Tuple(elements)), tos_ir::PlaceStep::Field(index)) => {
                elements.get(*index).copied()?
            }
            (Some(TypeDef::Array(element, _)), _) if indexed => *element,
            (Some(TypeDef::Slice(element)), _) if indexed => *element,
            (Some(TypeDef::Region(element)), _)
            | (Some(TypeDef::RegionMut(element)), _)
            | (Some(TypeDef::DmaRegion(element)), _)
            | (Some(TypeDef::DmaRegionMut(element)), _)
                if indexed =>
            {
                *element
            }
            // **A variant payload is not derivable from the place.** A
            // `Result`'s payload is `Field(0)` whichever arm it is, and which
            // arm that is belongs to the match that bound it, not to the place.
            // So this says it cannot derive one rather than guessing `ok` and
            // calling an `Err` binding a relabel.
            _ => return None,
        };
    }
    Some(current)
}

fn operand_type(module: &Module, function: &Function, operand: &Operand) -> Option<TypeId> {
    match operand {
        Operand::Value(value) => function.values.get(*value).copied(),
        Operand::Constant(constant) => match module.constants.get(*constant)? {
            Constant::Unit => module.types.iter().position(|ty| *ty == TypeDef::Unit),
            Constant::Bool(_) => module.types.iter().position(|ty| *ty == TypeDef::Bool),
            Constant::Int(kind, _) => module
                .types
                .iter()
                .position(|ty| *ty == TypeDef::Int(*kind)),
            Constant::Size(_) => module.types.iter().position(|ty| *ty == TypeDef::Size),
            Constant::Duration(_) => module.types.iter().position(|ty| *ty == TypeDef::Duration),
            Constant::Text(_) => module.types.iter().position(|ty| *ty == TypeDef::Text),
            Constant::Bytes(_) => module.types.iter().position(|ty| *ty == TypeDef::Bytes),
        },
    }
}

// ------------------------------------------------------------------ step 7

/// Affine state and profile restrictions.
///
/// docs/40 makes a non-`Copy` value moved when it is moved, so a second move of
/// the same place on one straight-line path is a defect the verifier finds
/// without trusting the frontend's ownership pass. `Copy` is recomputed from
/// the type graph by [`tos_ir::Module::is_copy`], never read from an
/// annotation.
fn check_ownership_and_profile(module: &Module) -> Result<(), Finding> {
    for (index, function) in module.functions.iter().enumerate() {
        for (block_index, block) in function.blocks.iter().enumerate() {
            let at = || alloc::format!("function {index} block {block_index}");
            let mut moved: Vec<&tos_ir::Place> = Vec::new();
            for instruction in &block.instructions {
                if let Op::Move { place } = &instruction.op {
                    let ty = function.values.get(place.root).copied().unwrap_or(0);
                    if module.is_copy(ty) && place.path.is_empty() {
                        continue;
                    }
                    if moved.iter().any(|existing| overlaps(existing, place)) {
                        return Err(Finding::new(
                            "V2020_OWNERSHIP",
                            at(),
                            alloc::format!("value {} is moved twice on one path", place.root),
                        ));
                    }
                    moved.push(place);
                }
            }
        }
        if module.header.profile == Profile::Bootstrap {
            if function.signature.is_async {
                return Err(Finding::new(
                    "V2023_PROFILE",
                    alloc::format!("function {index}"),
                    "an async function is Full-profile only",
                ));
            }
            for block in &function.blocks {
                for instruction in &block.instructions {
                    // docs/44 section 7 lists the Full-only constructs; a
                    // Bootstrap module may not contain their IR either.
                    let full_only = match &instruction.op {
                        Op::Await { .. } => Some("await"),
                        Op::Closure { .. } => Some("a closure"),
                        _ => None,
                    };
                    if let Some(what) = full_only {
                        return Err(Finding::new(
                            "V2023_PROFILE",
                            alloc::format!("function {index}"),
                            alloc::format!("{what} is Full-profile only"),
                        ));
                    }
                }
            }
        }
    }
    if module.header.profile == Profile::Bootstrap && module.header.resource_envelope.workers > 1 {
        return Err(Finding::new(
            "V2022_RESOURCE",
            "header.resource_envelope",
            "Bootstrap accepts workers: 1 only",
        ));
    }
    Ok(())
}

fn overlaps(one: &tos_ir::Place, other: &tos_ir::Place) -> bool {
    if one.root != other.root {
        return false;
    }
    let shorter = one.path.len().min(other.path.len());
    one.path[..shorter]
        .iter()
        .zip(&other.path[..shorter])
        .all(|(left, right)| match (left, right) {
            (tos_ir::PlaceStep::Field(a), tos_ir::PlaceStep::Field(b)) => a == b,
            (tos_ir::PlaceStep::Index(Some(a)), tos_ir::PlaceStep::Index(Some(b))) => a == b,
            // An index that is not a compile-time constant may name any
            // element, so it overlaps every other index step.
            (
                tos_ir::PlaceStep::Index(_) | tos_ir::PlaceStep::DynamicIndex(_),
                tos_ir::PlaceStep::Index(_) | tos_ir::PlaceStep::DynamicIndex(_),
            ) => true,
            _ => false,
        })
}

// ------------------------------------------------------------------ step 8

/// Task scope, atomic orders and unsafe interface claims.
///
/// docs/41 section 2 requires every spawned child to be consumed before its
/// scope exits. The obligation travels with the handle: a `spawn` creates one,
/// a move or read of the place holding it passes it on to whatever the
/// instruction produces, and joining, awaiting, returning or handing it to
/// another operation discharges it. `cancel` alone does not, which is the one
/// case docs/41 states outright.
/// The ADR-0037 facts of a region type, reached from the IR type table alone.
///
/// The mode is a distinct constructor in the IR, so the verifier reads it
/// rather than inferring it. Both DMA variants are conservative in V1: a
/// shareable `DmaRegion<T>` could become a `Shared<DmaRegion<T>>`, and a
/// `Shared<T>` is `Copy`, so the handle could be copied into several tasks.
fn region_facts(module: &Module, ty: TypeId) -> Option<(bool, bool)> {
    // (shareable, transferable)
    match module.types.get(ty) {
        Some(TypeDef::Region(_)) => Some((true, true)),
        Some(TypeDef::RegionMut(_)) => Some((false, false)),
        Some(TypeDef::DmaRegion(_)) | Some(TypeDef::DmaRegionMut(_)) => Some((false, false)),
        _ => None,
    }
}

/// Whether a type is transitively immutable, as `share` requires.
fn transitively_immutable(module: &Module, ty: TypeId, depth: usize) -> bool {
    if depth > tos_ir::MAX_TYPE_DEPTH {
        return false;
    }
    match module.types.get(ty) {
        None => false,
        Some(TypeDef::RegionMut(_)) | Some(TypeDef::DmaRegionMut(_)) => false,
        Some(TypeDef::MutexGuard(_))
        | Some(TypeDef::ReadGuard(_))
        | Some(TypeDef::WriteGuard(_)) => false,
        Some(TypeDef::Option(inner))
        | Some(TypeDef::Task(inner))
        | Some(TypeDef::TaskResult(inner))
        | Some(TypeDef::Shared(inner))
        | Some(TypeDef::Region(inner))
        | Some(TypeDef::DmaRegion(inner))
        | Some(TypeDef::Mutex(inner))
        | Some(TypeDef::RwLock(inner))
        | Some(TypeDef::Channel(inner))
        | Some(TypeDef::Slice(inner))
        | Some(TypeDef::Array(inner, _)) => transitively_immutable(module, *inner, depth + 1),
        Some(TypeDef::Result(ok, error)) => {
            transitively_immutable(module, *ok, depth + 1)
                && transitively_immutable(module, *error, depth + 1)
        }
        Some(TypeDef::Tuple(elements)) => elements
            .iter()
            .all(|element| transitively_immutable(module, *element, depth + 1)),
        Some(TypeDef::Nominal {
            fields, variants, ..
        }) => {
            fields
                .iter()
                .all(|field| transitively_immutable(module, *field, depth + 1))
                && variants.iter().all(|variant| {
                    variant
                        .payload
                        .iter()
                        .all(|payload| transitively_immutable(module, *payload, depth + 1))
                })
        }
        Some(_) => true,
    }
}

/// The ADR-0037 region rules, reached by this verifier's own traversal.
///
/// The checker enforces the same rules over source. Neither takes the other's
/// word for it: docs/43 section 5 forbids the frontend's success from being an
/// input here, so a region rule only the checker could catch is a rule an
/// alternate frontend could skip.
fn check_regions(module: &Module) -> Result<(), Finding> {
    for (index, function) in module.functions.iter().enumerate() {
        for (block_index, block) in function.blocks.iter().enumerate() {
            for instruction in &block.instructions {
                let at = || alloc::format!("functions[{index}].blocks[{block_index}]");
                match &instruction.op {
                    Op::Share { operand } => {
                        let Operand::Value(id) = operand else {
                            return Err(Finding::new(
                                "V2021_REGION",
                                at(),
                                "share applied to a constant",
                            ));
                        };
                        let Some(ty) = function.values.get(*id).copied() else {
                            return Err(Finding::new("V2021_REGION", at(), "share of no value"));
                        };
                        if region_facts(module, ty).is_some_and(|(shareable, _)| !shareable) {
                            return Err(Finding::new(
                                "V2021_REGION",
                                at(),
                                "share of a region that is not shareable",
                            ));
                        }
                        if !transitively_immutable(module, ty, 0) {
                            return Err(Finding::new(
                                "V2021_REGION",
                                at(),
                                "share of a value that is not transitively immutable",
                            ));
                        }
                    }
                    Op::Spawn { captures, .. } | Op::Closure { captures, .. } => {
                        for capture in captures {
                            let Operand::Value(id) = capture else {
                                continue;
                            };
                            let Some(ty) = function.values.get(*id).copied() else {
                                continue;
                            };
                            if region_facts(module, ty)
                                .is_some_and(|(_, transferable)| !transferable)
                            {
                                return Err(Finding::new(
                                    "V2021_REGION",
                                    at(),
                                    "a region that is not Transferable crosses a task or closure boundary",
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// Whether a type is one of the three ADR-0036 lock guards.
fn is_guard(module: &Module, ty: TypeId) -> bool {
    matches!(
        module.types.get(ty),
        Some(TypeDef::MutexGuard(_)) | Some(TypeDef::ReadGuard(_)) | Some(TypeDef::WriteGuard(_))
    )
}

/// The guard type an operand carries, when it carries one.
fn guard_operand(module: &Module, function: &Function, operand: &Operand) -> Option<TypeId> {
    let Operand::Value(id) = operand else {
        return None;
    };
    let ty = *function.values.get(*id)?;
    is_guard(module, ty).then_some(ty)
}

/// The guard rules of ADR-0036, reached by this verifier's own traversal.
///
/// docs/43 section 5 forbids taking the frontend's word for anything, so this
/// restates the rules over IR rather than trusting that a checker ran: a guard
/// rule the checker enforces and the verifier does not is a rule an alternate
/// frontend could skip. The IR carries a type for every value, so a guard is
/// identified from the type table rather than from any name.
fn check_guard_lifetimes(module: &Module) -> Result<(), Finding> {
    for (index, function) in module.functions.iter().enumerate() {
        for (block_index, block) in function.blocks.iter().enumerate() {
            let mut held: bool = false;
            for instruction in &block.instructions {
                let at = || alloc::format!("functions[{index}].blocks[{block_index}]");
                // An acquisition must name a real synchronization object and
                // produce the guard that object grants. Checking it here is
                // what stops a forged module from calling anything a lock.
                if let Op::Lock { object, mode } = &instruction.op {
                    let object_ty = match object {
                        Operand::Value(id) => function.values.get(*id).copied(),
                        Operand::Constant(_) => None,
                    };
                    let expected = match (object_ty.and_then(|ty| module.types.get(ty)), mode) {
                        (Some(TypeDef::Mutex(inner)), tos_ir::LockMode::Mutex) => {
                            Some(TypeDef::MutexGuard(*inner))
                        }
                        (Some(TypeDef::RwLock(inner)), tos_ir::LockMode::Read) => {
                            Some(TypeDef::ReadGuard(*inner))
                        }
                        (Some(TypeDef::RwLock(inner)), tos_ir::LockMode::Write) => {
                            Some(TypeDef::WriteGuard(*inner))
                        }
                        _ => None,
                    };
                    let produced = instruction.result.and_then(|id| {
                        function.values.get(id).and_then(|ty| module.types.get(*ty))
                    });
                    if expected.is_none() || produced != expected.as_ref() {
                        return Err(Finding::new(
                            "V2031_SYNC",
                            at(),
                            "a lock operation does not name a synchronization object or does not produce its guard",
                        ));
                    }
                }
                let escaping = match &instruction.op {
                    Op::Spawn { captures, .. } => Some(("task_boundary", captures)),
                    Op::Closure { captures, .. } => Some(("task_boundary", captures)),
                    Op::Aggregate { operands, .. } => Some(("aggregate", operands)),
                    Op::Variant { operands, .. } => Some(("aggregate", operands)),
                    _ => None,
                };
                if let Some((operation, operands)) = escaping {
                    for operand in operands {
                        if guard_operand(module, function, operand).is_some() {
                            return Err(Finding::new(
                                "V2031_SYNC",
                                at(),
                                alloc::format!("a guard operand escapes: {operation}"),
                            ));
                        }
                    }
                }
                // A guard produced in this block is live from here on: the IR
                // is in SSA form within a block, so a later await in the same
                // block is an await while it is held.
                if let Some(result) = instruction.result {
                    if function
                        .values
                        .get(result)
                        .is_some_and(|ty| is_guard(module, *ty))
                    {
                        held = true;
                    }
                }
                if held && matches!(instruction.op, Op::Await { .. }) {
                    return Err(Finding::new(
                        "V2031_SYNC",
                        at(),
                        "a guard is live across an await",
                    ));
                }
            }
            if let Terminator::Return(Some(operand)) = &block.terminator {
                if guard_operand(module, function, operand).is_some() {
                    return Err(Finding::new(
                        "V2031_SYNC",
                        alloc::format!("functions[{index}].blocks[{block_index}]"),
                        "a guard operand escapes: returned",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn check_tasks_sync_atomics_unsafe(module: &Module) -> Result<(), Finding> {
    check_guard_lifetimes(module)?;
    check_regions(module)?;
    for (index, function) in module.functions.iter().enumerate() {
        let at = || alloc::format!("function {index}");
        let mut pending: BTreeSet<usize> = BTreeSet::new();
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Op::Atomic {
                    operation,
                    order,
                    failure_order,
                    ..
                } = &instruction.op
                {
                    check_atomic(*operation, *order, *failure_order, &at)?;
                }
                if let Some(interface) = &instruction.unsafe_interface {
                    // An operation reaching an accepted interface schema
                    // (ADR-0060). The verifier does not carry the schema — a
                    // verifier that knew which interfaces exist would be a
                    // second place they are declared — it proves what the
                    // artifact must say about itself, which is docs/43 §3's
                    // "effect/right/interface match": the function making the
                    // call **declared** this interface as an effect.
                    //
                    // **It no longer requires a capability import** (ADR-0080).
                    // An import is a *request*, answered before the first
                    // instruction; an effect is a *declaration* of which class
                    // of authority may be exercised. They coincided while an
                    // import was the only way to come to hold a capability, and
                    // stopped coinciding when operations began returning them:
                    // a module that claims a PCI function reaches
                    // `platform.pci.FunctionConfig` through a value, and no
                    // import can answer a request for an object that does not
                    // exist until the claim runs.
                    //
                    // Nothing is weakened by dropping it. Which capability
                    // fills a position is proved per position below — an
                    // `Import` against the module's own table, a `Value`
                    // against its exact nominal type — so authority is still
                    // checked twice and is still checked against the artifact
                    // rather than the frontend's word.
                    if !function.signature.effects.iter().any(|e| e == interface) {
                        return Err(Finding::new(
                            "V2033_UNSAFE",
                            at(),
                            alloc::format!("{interface} is reached without being declared"),
                        ));
                    }
                }
                if matches!(instruction.op, Op::Cancel { .. }) {
                    continue;
                }
                // Whatever this instruction touches, it takes the handle from.
                let mut carried = false;
                for operand in operands_of(&instruction.op) {
                    if let Operand::Value(value) = operand {
                        carried |= pending.remove(&value);
                    }
                }
                for place in places_of(&instruction.op) {
                    carried |= pending.remove(&place.root);
                }
                // A move or read of the whole place passes the handle to what
                // this instruction produces; a join or await ends it.
                let passes_on = matches!(
                    instruction.op,
                    Op::Move { .. } | Op::Read { .. } | Op::Borrow { .. }
                );
                if let Some(result) = instruction.result {
                    // A spawn creates the obligation; a move or read of a place
                    // that held one passes it to what this produces.
                    let holds =
                        matches!(instruction.op, Op::Spawn { .. }) || (carried && passes_on);
                    if holds {
                        pending.insert(result);
                    }
                }
            }
            for operand in terminator_operands(&block.terminator) {
                if let Operand::Value(value) = operand {
                    pending.remove(&value);
                }
            }
        }
        if let Some(task) = pending.iter().next() {
            return Err(Finding::new(
                "V2030_TASK_SCOPE",
                at(),
                alloc::format!("value {task} is a child that leaves its scope unconsumed"),
            ));
        }
    }
    Ok(())
}

fn check_atomic(
    operation: AtomicOp,
    order: MemoryOrder,
    failure_order: Option<MemoryOrder>,
    at: &dyn Fn() -> String,
) -> Result<(), Finding> {
    let accepted = match operation {
        AtomicOp::Load => matches!(
            order,
            MemoryOrder::Relaxed | MemoryOrder::Acquire | MemoryOrder::SeqCst
        ),
        AtomicOp::Store => matches!(
            order,
            MemoryOrder::Relaxed | MemoryOrder::Release | MemoryOrder::SeqCst
        ),
        _ => true,
    };
    if !accepted {
        return Err(Finding::new(
            "V2032_ATOMIC_ORDER",
            at(),
            alloc::format!(
                "{} does not accept {}",
                operation_name(operation),
                order.spelled()
            ),
        ));
    }
    let Some(failure) = failure_order else {
        return Ok(());
    };
    if operation != AtomicOp::CompareExchange {
        return Err(Finding::new(
            "V2032_ATOMIC_ORDER",
            at(),
            "only compare_exchange carries a failure order",
        ));
    }
    if !matches!(
        failure,
        MemoryOrder::Relaxed | MemoryOrder::Acquire | MemoryOrder::SeqCst
    ) {
        return Err(Finding::new(
            "V2032_ATOMIC_ORDER",
            at(),
            alloc::format!("a failure order may not be {}", failure.spelled()),
        ));
    }
    if failure.rank() > order.rank() {
        return Err(Finding::new(
            "V2032_ATOMIC_ORDER",
            at(),
            alloc::format!(
                "failure order {} is stronger than success order {}",
                failure.spelled(),
                order.spelled()
            ),
        ));
    }
    Ok(())
}

fn operation_name(operation: AtomicOp) -> &'static str {
    match operation {
        AtomicOp::Load => "load",
        AtomicOp::Store => "store",
        AtomicOp::Swap => "swap",
        AtomicOp::FetchAdd => "fetch_add",
        AtomicOp::FetchSub => "fetch_sub",
        AtomicOp::FetchAnd => "fetch_and",
        AtomicOp::FetchOr => "fetch_or",
        AtomicOp::FetchXor => "fetch_xor",
        AtomicOp::CompareExchange => "compare_exchange",
    }
}

// ------------------------------------------------------------------ step 9

fn check_source_maps(module: &Module) -> Result<(), Finding> {
    let header = &module.header;
    for (index, entry) in module.source_map.iter().enumerate() {
        // Built only when a finding needs it. Formatting a location for every
        // entry allocates and runs `core::fmt` tens of thousands of times to
        // describe a place nothing is wrong with.
        let at = || alloc::format!("source map {index}");
        if entry.byte_start > entry.byte_end {
            return Err(Finding::new(
                "V2040_SOURCE_MAP",
                at(),
                "a span runs backwards",
            ));
        }
        // docs/43 section 6: the identity in an entry is the module's own. A
        // mismatch is exactly how a forged map claims another module's source.
        if entry.content_id != header.content_id
            || entry.path != header.path
            || entry.source_set != header.source_set
        {
            return Err(Finding::new(
                "V2040_SOURCE_MAP",
                at(),
                "an entry claims a source identity the header does not",
            ));
        }
        if entry.language_version != header.language_version
            || entry.profile != header.profile
            || entry.unicode_normalization_baseline != header.unicode_normalization_baseline
        {
            return Err(Finding::new(
                "V2040_SOURCE_MAP",
                at(),
                "an entry disagrees with the header about the language contract",
            ));
        }
        if let Some(parent) = entry.derived_from {
            if parent >= module.source_map.len() {
                return Err(Finding::new(
                    "V2040_SOURCE_MAP",
                    at(),
                    "a derivation parent is outside the table",
                ));
            }
        }
    }
    for (index, function) in module.functions.iter().enumerate() {
        if function.source >= module.source_map.len() {
            return Err(Finding::new(
                "V2040_SOURCE_MAP",
                alloc::format!("function {index}"),
                "a function has no source-map entry",
            ));
        }
        for (block_index, block) in function.blocks.iter().enumerate() {
            if block.source >= module.source_map.len() {
                return Err(Finding::new(
                    "V2040_SOURCE_MAP",
                    alloc::format!("function {index} block {block_index}"),
                    "a block has no source-map entry",
                ));
            }
        }
    }
    Ok(())
}

/// A digest over the source map alone, so a receipt binds the map it checked.
/// Exposed so its cost can be measured apart from the module digest.
pub fn source_map_digest_of(entries: &[SourceMapEntry]) -> String {
    source_map_digest(entries)
}

/// The source-map digest, hashed as it is produced.
///
/// The byte sequence and its order are unchanged; what is gone is the buffer
/// that used to hold all of it first. On a ceiling-sized module that buffer was
/// `3.63 MiB`, and after `tos_ir::module_digest` stopped materializing its own
/// stream it was the whole of what verification cost above the decoded module.
/// A verifier's memory should not be a function of how much source map a module
/// carries when nothing is looked at twice.
fn source_map_digest(entries: &[SourceMapEntry]) -> String {
    let mut state = tos_hash::Sha256::new();
    for entry in entries {
        for text in [
            entry.source_set.as_str(),
            entry.path.as_str(),
            entry.content_id.as_str(),
            entry.frontend_identity.as_str(),
        ] {
            state.update(&(text.len() as u64).to_be_bytes());
            state.update(text.as_bytes());
        }
        state.update(&(entry.byte_start as u64).to_be_bytes());
        state.update(&(entry.byte_end as u64).to_be_bytes());
    }
    let digest = state.finalize();
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    alloc::format!(
        "sha256:{}",
        core::str::from_utf8(&hex).expect("hex output is ASCII")
    )
}

// ------------------------------------------------------------------ shared

fn terminator_targets(terminator: &Terminator) -> Vec<usize> {
    match terminator {
        Terminator::Return(_) | Terminator::Trap(_) => Vec::new(),
        Terminator::Branch { target, .. } => alloc::vec![*target],
        Terminator::BranchIf {
            true_target,
            false_target,
            ..
        } => alloc::vec![*true_target, *false_target],
        Terminator::MatchEnum { arms, .. } => arms.iter().map(|(_, target)| *target).collect(),
        Terminator::PropagateError { ok_target, .. } => alloc::vec![*ok_target],
    }
}

fn terminator_operands(terminator: &Terminator) -> Vec<Operand> {
    match terminator {
        Terminator::Return(value) => value.iter().cloned().collect(),
        Terminator::Branch { arguments, .. } => arguments.clone(),
        Terminator::BranchIf {
            condition,
            true_arguments,
            false_arguments,
            ..
        } => {
            let mut all = alloc::vec![condition.clone()];
            all.extend(true_arguments.iter().cloned());
            all.extend(false_arguments.iter().cloned());
            all
        }
        Terminator::MatchEnum { subject, .. } => alloc::vec![subject.clone()],
        Terminator::PropagateError { result, .. } => alloc::vec![result.clone()],
        Terminator::Trap(_) => Vec::new(),
    }
}

fn operands_of(op: &Op) -> Vec<Operand> {
    match op {
        Op::Aggregate { operands, .. } | Op::Variant { operands, .. } => operands.clone(),
        Op::Write { value, .. } => alloc::vec![value.clone()],
        Op::Binary { left, right, .. } => alloc::vec![left.clone(), right.clone()],
        Op::Unary { operand, .. } | Op::Widen { operand, .. } => alloc::vec![operand.clone()],
        Op::Call { operands, .. } => operands.clone(),
        Op::Spawn { captures, .. } => captures.clone(),
        Op::Join { task } | Op::Await { task } | Op::Cancel { task } => alloc::vec![task.clone()],
        Op::Atomic {
            target, operands, ..
        } => {
            let mut all = alloc::vec![target.clone()];
            all.extend(operands.iter().cloned());
            all
        }
        Op::Capability { operands, .. } => operands.clone(),
        // ADR-0086 §15 obligation 2. The region is an ordinary operand for
        // every rule that applies to one, including naming a value the table
        // has.
        Op::DmaSync { region, .. } => alloc::vec![region.clone()],
        Op::Resource { amount, .. } => alloc::vec![amount.clone()],
        Op::Closure { captures, .. } => captures.clone(),
        Op::CallValue { callee, operands } => {
            let mut all = alloc::vec![callee.clone()];
            all.extend(operands.iter().cloned());
            all
        }
        Op::RunCleanups { calls } => calls
            .iter()
            .flat_map(|call| call.captures.iter().cloned())
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether every index step of a place applies to something with elements.
///
/// An array, a slice and the four region families have elements; a capability,
/// a record, a scalar and a handle do not. A step whose root is one of those is
/// refused rather than projected, because a projection that cannot be taken is
/// not a place — and the run below would otherwise decide what to do from the
/// root's runtime shape instead of from its declared type.
///
/// A root or a field the table cannot resolve answers `true`: that is a
/// different defect with a finding of its own, and reporting it twice would
/// name the wrong one first.
fn indexable(module: &Module, function: &Function, place: &tos_ir::Place) -> bool {
    let Some(mut current) = function.values.get(place.root).copied() else {
        return true;
    };
    for step in &place.path {
        let Some(ty) = module.type_of(current) else {
            return true;
        };
        current = match step {
            tos_ir::PlaceStep::Field(index) => match ty {
                TypeDef::Nominal { fields, .. } => match fields.get(*index) {
                    Some(field) => *field,
                    None => return true,
                },
                TypeDef::Tuple(elements) => match elements.get(*index) {
                    Some(element) => *element,
                    None => return true,
                },
                // A variant payload is reached by a field step too, and the
                // shapes it may have are the enum's rather than one type's.
                _ => return true,
            },
            tos_ir::PlaceStep::Index(_) | tos_ir::PlaceStep::DynamicIndex(_) => match ty {
                TypeDef::Array(element, _)
                | TypeDef::Slice(element)
                | TypeDef::Region(element)
                | TypeDef::RegionMut(element)
                | TypeDef::DmaRegion(element)
                | TypeDef::DmaRegionMut(element) => *element,
                TypeDef::Bytes => return true,
                _ => return false,
            },
        };
    }
    true
}

/// The element type an indexed access through a region reads or writes, when the
/// place is one.
///
/// `None` for every place that does not project a region by an index step —
/// a field of a record, an element of an array, or the region value itself with
/// no projection at all. Those are checked, where they are checked, by the rules
/// that own them; this answers only the question whose answer leaves the
/// artifact.
///
/// **The last index step decides**, because that is the one whose element the
/// access is of. A place reaching further — a field of an element — is not an
/// access of the element and is left alone.
fn region_element(module: &Module, function: &Function, place: &tos_ir::Place) -> Option<TypeId> {
    let mut current = function.values.get(place.root).copied()?;
    let mut element = None;
    for step in &place.path {
        let indexed = matches!(
            step,
            tos_ir::PlaceStep::Index(_) | tos_ir::PlaceStep::DynamicIndex(_)
        );
        let inner = match module.type_of(current) {
            Some(TypeDef::Region(inner))
            | Some(TypeDef::RegionMut(inner))
            | Some(TypeDef::DmaRegion(inner))
            | Some(TypeDef::DmaRegionMut(inner))
                if indexed =>
            {
                element = Some(*inner);
                *inner
            }
            Some(TypeDef::Array(inner, _)) | Some(TypeDef::Slice(inner)) if indexed => {
                element = None;
                *inner
            }
            Some(TypeDef::Nominal { fields, .. }) => match step {
                tos_ir::PlaceStep::Field(index) => {
                    element = None;
                    *fields.get(*index)?
                }
                _ => return None,
            },
            Some(TypeDef::Tuple(elements)) => match step {
                tos_ir::PlaceStep::Field(index) => {
                    element = None;
                    *elements.get(*index)?
                }
                _ => return None,
            },
            _ => return None,
        };
        current = inner;
    }
    element
}

fn places_of(op: &Op) -> Vec<&tos_ir::Place> {
    match op {
        Op::Read { place }
        | Op::Move { place }
        | Op::Write { place, .. }
        | Op::Borrow { place, .. }
        | Op::Drop { place } => alloc::vec![place],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    fn declared() -> ResolutionSnapshot {
        let mut building = DeclaredResolution::new();
        building
            .module("set.b", "sha256:bbb")
            .exports_declared()
            .export("zeta")
            .export("alpha")
            .export("mid");
        building.module("set.c", "sha256:ccc");
        building.module("set.a", "sha256:aaa").exports_declared();
        building.capability("system.time.Clock");
        building.capability("system.audit.Logger");
        building.build()
    }

    #[test]
    fn a_module_resolves_to_the_identity_it_was_declared_with() {
        let snapshot = declared();
        assert_eq!(snapshot.resolved_content_id("set.b"), Some("sha256:bbb"));
        assert_eq!(snapshot.resolved_content_id("set.c"), Some("sha256:ccc"));
        assert_eq!(snapshot.resolved_content_id("set.absent"), None);
        assert_eq!(snapshot.len(), 3);
        assert!(!snapshot.is_empty());
    }

    /// The distinction a missing map entry used to carry, and the one place
    /// this representation could quietly have changed the verifier's answer.
    #[test]
    fn an_unstated_export_surface_is_not_an_empty_one() {
        let snapshot = declared();
        assert!(
            snapshot.export_surface("set.c").is_none(),
            "a module declared without an export surface states nothing about its exports"
        );
        let empty = snapshot
            .export_surface("set.a")
            .expect("set.a states an empty surface");
        assert!(empty.is_empty());
        assert!(!empty.contains("anything"));
    }

    #[test]
    fn an_export_surface_answers_for_the_exact_name() {
        let snapshot = declared();
        let surface = snapshot.export_surface("set.b").expect("stated");
        assert_eq!(surface.len(), 3);
        for name in ["alpha", "mid", "zeta"] {
            assert!(surface.contains(name), "{name} was declared");
        }
        for name in ["alph", "alphaa", "beta", ""] {
            assert!(!surface.contains(name), "{name} was not declared");
        }
    }

    /// One `u32` per export and nothing else.
    ///
    /// A `{start, length}` pair would be eight bytes — `size_of` says so, and a
    /// narrower second field would not help because alignment would pad it back
    /// up. Deriving a name's end from the next offset removes the field
    /// instead of shrinking it, so there is nothing left to pad.
    #[test]
    fn an_export_costs_four_bytes_of_metadata() {
        assert_eq!(core::mem::size_of::<u32>(), 4);
        assert_eq!(core::mem::size_of::<Span>(), 8);

        let mut building = DeclaredResolution::new();
        building.module("set.a", "sha256:aaa").exports_declared();
        let count = 4096;
        for at in 0..count {
            building.export(&alloc::format!("e{at:06}"));
        }
        let snapshot = building.build();
        let (text, metadata) = snapshot.export_bytes();
        assert_eq!(text, count * 7, "seven bytes a name, packed with no gaps");
        assert_eq!(
            metadata,
            (count + 1) * 4,
            "one offset per export plus a terminator"
        );

        let surface = snapshot.export_surface("set.a").expect("stated");
        assert_eq!(surface.len(), count);
        assert!(surface.contains("e000000"));
        assert!(surface.contains(&alloc::format!("e{:06}", count - 1)));
        assert!(!surface.contains("e999999"));
    }

    /// Unsorted input is accepted and ordered, and sorted input is not rewritten.
    #[test]
    fn exports_are_ordered_however_they_arrive() {
        let mut building = DeclaredResolution::new();
        building
            .module("set.a", "sha256:aaa")
            .exports_declared()
            .export("zulu")
            .export("alpha")
            .export("mike");
        building
            .module("set.b", "sha256:bbb")
            .exports_declared()
            .export("alpha")
            .export("mike")
            .export("zulu");
        let snapshot = building.build();
        for module in ["set.a", "set.b"] {
            let surface = snapshot.export_surface(module).expect("stated");
            assert_eq!(surface.len(), 3);
            for name in ["alpha", "mike", "zulu"] {
                assert!(surface.contains(name), "{module} exports {name}");
            }
            for name in ["al", "alphaa", "november"] {
                assert!(!surface.contains(name), "{module} does not export {name}");
            }
        }
    }

    /// docs/44 §2 caps identifier bytes at 128, and an export name is a source
    /// identifier — but this representation does not depend on that.
    ///
    /// The bound is recorded because it would be load-bearing for any design
    /// that stored a narrow length. This one stores no length, so a name of any
    /// size a `u32` offset can address is representable, and the test says so
    /// rather than leaving a reader to wonder what happens at 129 bytes.
    #[test]
    fn an_export_name_longer_than_an_identifier_is_still_exact() {
        let long = alloc::string::String::from_utf8(alloc::vec![b'x'; 4096]).expect("ascii");
        let mut building = DeclaredResolution::new();
        building
            .module("set.a", "sha256:aaa")
            .exports_declared()
            .export("short")
            .export(&long);
        let snapshot = building.build();
        let surface = snapshot.export_surface("set.a").expect("stated");
        assert!(surface.contains(&long));
        assert!(surface.contains("short"));
        assert!(!surface.contains(&long[..4095]));
    }

    #[test]
    fn capability_interfaces_answer_exactly() {
        let snapshot = declared();
        assert!(snapshot.declares_capabilities());
        assert!(snapshot.provides_capability("system.time.Clock"));
        assert!(snapshot.provides_capability("system.audit.Logger"));
        assert!(!snapshot.provides_capability("system.time.Clockwork"));
        assert!(!ResolutionSnapshot::default().declares_capabilities());
    }

    /// **The predeclared contract this verifier was built against** (`docs/43`
    /// §5, ADR-0088 §6).
    ///
    /// The condition on a shared declarative table is that its content digest
    /// is an input to *both* components. This is this component's input: the
    /// values are written here, in the verifier, and compared with the table's
    /// own canonical digest. The frontend side states the same values in its
    /// own tests and neither reads them from the other. A table edited under
    /// either component fails that component's build, which is what stops two
    /// implementations from quietly holding two contracts.
    const PREDECLARED_CONTRACT: [(u32, &str); 5] = [
        (
            0,
            "849bd9de6a7c0c485ffbcc972791a5768c724a4165d67087fd1961e863f7f555",
        ),
        (
            1,
            "26848cb863d7727c9b6787e141da156df418ea57af0471863a5767fe9f170bb3",
        ),
        (
            2,
            "c5e8ffbb3467768e15a235b57c56bdde1a359cb707ad5f33ee45f721b357c2a5",
        ),
        (
            3,
            "a4bad47aac6cdb361ced38352fde05acfedb47d4a6c331b8cd4762fd3daec02c",
        ),
        (
            4,
            "9d496d76b8e8b25157547fe6a5d81fb3c32c0dc7f331d3e19779ee1e2271a17c",
        ),
    ];

    #[test]
    fn the_predeclared_contract_is_the_one_this_verifier_was_built_against() {
        for (minor, expected) in PREDECLARED_CONTRACT {
            let contract = tos_predeclared::Contract::at_minor(minor);
            let hex =
                alloc::string::String::from_utf8(tos_predeclared::hex_digest(&contract).to_vec())
                    .expect("hex is ASCII");
            assert_eq!(hex, expected, "TOS Core 1.{minor}");
        }
    }

    /// The minor is read from the artifact's own header, and an unreadable one
    /// selects no contract at all rather than a plausible one.
    #[test]
    fn a_declared_minor_is_read_from_the_header() {
        assert_eq!(declared_minor("1.4"), Some(4));
        assert_eq!(declared_minor("1.0"), Some(0));
        assert_eq!(declared_minor("2.0"), None);
        assert_eq!(declared_minor("1.x"), None);
        assert_eq!(declared_minor("nonsense"), None);
    }
}
