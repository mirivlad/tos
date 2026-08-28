// SPDX-License-Identifier: GPL-3.0-or-later
//! The reference path from canonical source to an observable result.
//!
//! Every stage TOS requires already exists as its own crate. What did not exist
//! is the composition, and the composition is where the architecture is either
//! honoured or quietly broken. This crate is that composition and nothing else:
//! it holds no language rule, no verification rule and no evaluation rule of its
//! own, so a defect here cannot invent semantics — it can only fail to run a
//! stage, which the result type makes visible.
//!
//! ```text
//! canonical source
//!   -> SourceReader   transport validity (docs/39 section 1)
//!   -> Parser         grammar
//!   -> Checker        types, ownership, effects, resources
//!   -> module set     name-to-path agreement (docs/42 section 1)
//!   -> lower_module   tos-ir/v1
//!   -> verify         independent verifier, receipt bound to the digest
//!   -> run            bounded reference engine
//! ```
//!
//! **No stage may be skipped and no stage may be trusted by another.** The
//! engine is handed the receipt the verifier produced, and the verifier is
//! handed IR and a declared snapshot — never the checker's verdict. That is why
//! this crate depends on `tos-verifier` and `tos-engine` separately rather than
//! letting the frontend hand execution a blessed module.
//!
//! **No host.** `no_std`, no I/O, no clock, no environment. Bytes and a
//! declared context arrive; a structured result comes back. A caller that wants
//! to watch progress supplies a [`Trace`]; a caller that wants to print the
//! result renders it. Both are the caller's business, which is what lets the
//! same code run inside the nucleus on the boot path and inside a hosted test.
//!
//! **Identity is computed, not asserted.** The content ID is the digest of the
//! normalized source, and the dependency and capability-interface digests are
//! digests of the *actual* resolved lists. An empty list still has a digest;
//! writing a placeholder there would make the receipt name something that was
//! never checked.

#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tos_core::{
    check_module_summaries, lower_module_in_set, Gap, LoweringInterface, ModuleContext,
    ModuleEntry, ModuleSummary, Parser, ResolvedImport, SourceReader, VerificationSurface,
};

/// The frontend types this crate's own results are made of.
///
/// `Run` hands a caller diagnostics, positions and severities; a caller that
/// cannot name them would have to depend on the frontend directly in order to
/// read a value this crate gave it, which is a dependency on the stage that
/// produced the result rather than on the result.
pub use tos_core::{Diagnostic, Position, Severity};
// The host side of the boundary an accepted interface schema defines. Re-exported
// here because this crate is the reference path's facade: a caller assembling a
// run should not have to name the engine crate to say what that run may reach.
/// The accepted interface schemas, for the host that answers a module's
/// capability requests and performs its operations (ADR-0060, ADR-0061).
pub use tos_core::interfaces;

pub mod source;

pub use source::{
    SliceSourceProvider, SourceCatalogEntry, SourceClosureManifest, SourceEntryId, SourceMember,
    SourceModuleId, SourceProvider, SourceRefusal, SourceSnapshot,
};
use tos_engine::{run_closure, Accounting, Closure, Refusal};
pub use tos_engine::{
    Handle, Reach, Request as CapabilityRequest, System, Trap, Unreachable, Value,
};
/// The integer widths a value carries, for a host building one.
pub use tos_ir::IntKind;
use tos_ir::Module;
/// The declared bounds a run holds resident (ADR-0071 section 7).
pub use tos_residency::ResidencyLimits;
use tos_residency::{
    launch, ClosureModuleId, ClosureSource, ImageSnapshot, ModuleProvider, Residency,
    VerifiedClosureManifest, VerifiedModuleRecord,
};
use tos_verifier::{Finding, Limits, ResolutionSnapshot, VerifiedModule};

/// What the pipeline is asked to run.
///
/// `path` is the canonical repository path of the source, relative to its
/// declared module root and without a leading separator: docs/42 section 1
/// derives the expected module name from it, so a path the caller invents
/// rather than reads is a name the module never claimed.
#[derive(Clone, Copy, Debug)]
pub struct Request<'a> {
    /// The declared source set this module belongs to.
    pub source_set: &'a str,
    /// Canonical repository path, module-root relative, e.g. `system/boot/init.tos`.
    pub path: &'a str,
    /// The source bytes exactly as stored.
    pub bytes: &'a [u8],
    /// The exported function to run.
    pub entry: &'a str,
}

/// One source unit of a set: where it is stored and what it says.
#[derive(Clone, Copy, Debug)]
pub struct Unit<'a> {
    /// Canonical repository path, module-root relative.
    pub path: &'a str,
    /// The source bytes exactly as stored.
    pub bytes: &'a [u8],
}

/// What the pipeline is asked to run, when it is a set rather than one module.
///
/// The entry is named by path rather than by position, because a caller that
/// hands over a directory listing has paths and not an order, and an order it
/// invented would decide which module is the program.
#[derive(Clone, Copy, Debug)]
pub struct SetRequest<'a> {
    /// The declared source set these modules belong to.
    pub source_set: &'a str,
    /// Every unit of the set, including the entry.
    pub units: &'a [Unit<'a>],
    /// The path of the unit whose exported function is run.
    pub entry_path: &'a str,
    /// The exported function to run.
    pub entry: &'a str,
}

/// Why a set could not be run at all.
///
/// Not a [`Run`]: no stage ran, nothing was refused, and nothing is wrong with
/// the source. The request itself does not describe something runnable, and a
/// caller that reported this as a refusal would be blaming a program for its
/// own mistake.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetError {
    /// No unit of the set is stored at the declared entry path.
    EntryModuleAbsent { path: String },
    /// The set is empty.
    NoUnits,
}

impl SetError {
    /// A stable reason token, for a caller reporting this over an event log.
    pub fn symbol(&self) -> &'static str {
        match self {
            SetError::EntryModuleAbsent { .. } => "entry-module-absent",
            SetError::NoUnits => "no-units",
        }
    }
}

/// A stage of the reference path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum PipelineStage {
    Read,
    Parse,
    Check,
    Resolve,
    Lower,
    Verify,
    Execute,
}

impl PipelineStage {
    /// The stable symbol a caller reports this stage by.
    pub fn symbol(self) -> &'static str {
        match self {
            PipelineStage::Read => "read",
            PipelineStage::Parse => "parse",
            PipelineStage::Check => "check",
            PipelineStage::Resolve => "resolve",
            PipelineStage::Lower => "lower",
            PipelineStage::Verify => "verify",
            PipelineStage::Execute => "execute",
        }
    }
}

/// Watches the pipeline advance.
///
/// A stage is announced *before* it runs, so the last stage a caller sees is
/// the stage that failed to return — which is the only thing that identifies a
/// non-terminating or trapping stage from outside.
pub trait Trace {
    fn entering(&mut self, stage: PipelineStage);
}

/// A [`Trace`] that records nothing, for callers that only want the result.
pub struct Silent;

impl Trace for Silent {
    fn entering(&mut self, _stage: PipelineStage) {}
}

/// A source location, resolved to line and column for reporting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Site {
    pub path: String,
    pub start: Position,
    pub end: Position,
}

/// Where a trap came from, in the terms the process itself carries.
///
/// **No source anywhere in it.** A canonical path and a byte span, both of them
/// facts the trap brought out of the run: ADR-0072 §2 makes execution an account
/// that holds a running program, and a process that needed the source text in
/// order to *report* where it trapped would be holding the build in order to
/// describe the run.
///
/// A line and a column are a different thing — a rendering of this against
/// source that some later reader happens to have. [`locate`] does that, after
/// the fact, and a run that nobody ever locates still says exactly which bytes
/// of which unit it stopped at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrapLocation {
    pub path: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// Turns a canonical span into a line and a column.
///
/// A **reporting** step, run after execution by whoever has the source. It is
/// not part of the run and nothing about the run depends on it happening.
pub fn locate(location: &TrapLocation, source_bytes: &[u8]) -> Option<Site> {
    let source = SourceReader::read(source_bytes).ok()?;
    Some(Site {
        path: location.path.clone(),
        start: Position::at(&source, location.byte_start),
        end: Position::at(&source, location.byte_end),
    })
}

/// The same, through the provider and membership a preparation produced.
///
/// The source is asked for by the identity the resolution minted, and the bytes
/// are checked against what resolution saw — the same discipline materialization
/// uses, because a location rendered against different source would name a
/// different place.
pub fn locate_in(
    location: &TrapLocation,
    provider: &dyn SourceProvider,
    closure: &SourceClosureManifest,
) -> Option<Site> {
    let position = closure
        .members()
        .iter()
        .position(|member| member.path == location.path)?;
    let id = closure.module(position)?;
    let snapshot = source::materialize(provider, closure, id).ok()?;
    locate(location, snapshot.bytes())
}

/// A completed run and everything that proves it was a real one.
#[derive(Clone, Debug)]
pub struct Completion {
    /// The receipt the verifier issued, naming this exact module.
    pub receipt: VerifiedModule,
    /// What the entry function returned.
    pub value: Value,
    /// What the run consumed against what the module declared.
    pub accounting: Accounting,
}

/// How far the reference path got, and what it produced.
///
/// Each variant is a *different stage's* refusal, kept apart on purpose: a
/// caller that collapses them cannot tell a program the frontend rejected from
/// IR the verifier rejected, and those are claims about different components.
#[derive(Clone, Debug)]
pub enum Run {
    /// The bytes are not a transport-valid source unit (docs/39 section 1).
    ///
    /// `path` names the unit that was refused. A transport refusal never
    /// reaches a diagnostic, so nothing else in the result can say which of a
    /// set's units the offset belongs to.
    SourceRejected {
        code: &'static str,
        byte_offset: usize,
        path: String,
    },
    /// The frontend refused the module, at the stage that refused it.
    ///
    /// The stage is carried rather than inferred from the diagnostics: a
    /// refusal has to name its own author even when the list it produced is
    /// empty, and inferring it from the first diagnostic would report the
    /// frontend stage that happened to speak first instead.
    Diagnosed {
        stage: PipelineStage,
        diagnostics: Vec<Diagnostic>,
    },
    /// The provider could not supply a member of the resolved closure, or
    /// supplied something other than what resolution saw (ADR-0072 §6).
    ///
    /// Its own outcome, and not a transport refusal: the bytes were readable
    /// when the closure was resolved, and what changed is the source behind an
    /// identity rather than the identity's own validity.
    SourceRefused(SourceRefusal),
    /// The source is valid and checked, and this lowerer cannot represent one
    /// of its constructs. Not a defect in the program.
    NotLowered(Gap),
    /// The independent verifier refused the IR the frontend emitted.
    Unverified(Finding),
    /// The engine refused to start: wrong receipt, no such entry, wrong arity.
    Refused(Refusal),
    /// A trap ended the run, named by the canonical span it came from.
    ///
    /// A location and not a site: the process carries a path and a byte range
    /// out of the run, and turning those into a line and a column is a
    /// reporting step for whoever still has the source ([`locate`]).
    Trapped {
        code: &'static str,
        detail: String,
        at: Option<TrapLocation>,
    },
    /// The program ran to completion.
    ///
    /// Boxed because a receipt and a returned value are far larger than any
    /// refusal, and a caller matching on this enum should not carry the cost of
    /// the successful case in every refusal it handles.
    Completed(Box<Completion>),
}

impl Run {
    /// Whether the reference path produced an executed result.
    pub fn is_completed(&self) -> bool {
        matches!(self, Run::Completed(_))
    }

    /// The stage that ended the run, or `None` when it completed.
    pub fn failed_at(&self) -> Option<PipelineStage> {
        match self {
            Run::SourceRejected { .. } => Some(PipelineStage::Read),
            Run::Diagnosed { stage, .. } => Some(*stage),
            // The source was resolved and then would not materialize, which is
            // discovered where it is needed: in the lowering pass.
            Run::SourceRefused(_) | Run::NotLowered(_) => Some(PipelineStage::Lower),
            Run::Unverified(_) => Some(PipelineStage::Verify),
            Run::Refused(_) | Run::Trapped { .. } => Some(PipelineStage::Execute),
            Run::Completed(_) => None,
        }
    }
}

/// Runs one canonical source unit through the whole reference path.
///
/// The stages are announced through `trace` in the order they are entered, and
/// the first one that refuses ends the run: a later stage reading a table an
/// earlier stage rejected would be reporting a consequence, not a defect.
pub fn execute(
    request: &Request<'_>,
    arguments: Vec<Value>,
    trace: &mut dyn Trace,
    system: &mut dyn System,
) -> Run {
    let unit = Unit {
        path: request.path,
        bytes: request.bytes,
    };
    // A single unit is always stored at its own path, so the set cannot fail
    // its precondition and there is nothing for this caller to handle.
    match execute_set(
        &SetRequest {
            source_set: request.source_set,
            units: core::slice::from_ref(&unit),
            entry_path: request.path,
            entry: request.entry,
        },
        arguments,
        trace,
        system,
    ) {
        Ok(run) => run,
        Err(_) => unreachable!("a one-unit set contains its own entry path"),
    }
}

/// Runs a source set: several canonical units, one of which is the entry.
///
/// The same reference path in the same order, over more than one module. Every
/// unit is read, parsed and checked; the set is then resolved as a set, which
/// is where a name that resolves to nothing, a module stored at a path its name
/// does not derive, and an import cycle are found. None of those can be seen
/// from inside one module, which is why resolution is its own stage rather than
/// a part of checking.
pub fn execute_set(
    request: &SetRequest<'_>,
    arguments: Vec<Value>,
    trace: &mut dyn Trace,
    system: &mut dyn System,
) -> Result<Run, SetError> {
    match prepare_from_source(request, trace, HOST_RESIDENCY)? {
        Preparation::Refused(run) => Ok(run),
        Preparation::Ready(mut prepared) => {
            trace.entering(PipelineStage::Execute);
            Ok(run_prepared(&mut prepared, arguments, system))
        }
    }
}

/// What a preparation produced.
///
/// The executable is boxed because the two answers are not the same size: a
/// prepared closure carries the resident set's own bookkeeping, and a refusal
/// carries a sentence about a stage. Boxing keeps a caller's `Result` the size
/// of the smaller one.
pub enum Preparation {
    /// An executable closure: verified, reduced to records and membership, with
    /// the launch workspace already released.
    Ready(Box<Prepared>),
    /// The closure could not be built. The [`Run`] says which stage refused it
    /// and why; none of them is `Completed`, because nothing executed.
    Refused(Run),
}

/// Builds an executable closure from canonical source (ADR-0072 §2).
///
/// **This is the launch workspace, and it ends when this returns.** Everything
/// the build needed — the source snapshots, the parse trees, the summaries, the
/// closure plan, the lowering views, the verification surfaces and one module's
/// IR at a time — is transient and is gone by the time a [`Prepared`] is handed
/// back. What crosses the boundary is exactly what ADR-0071 says survives
/// verification: the immutable images, one fixed-size record per module, the
/// closure's membership, the entry receipt and the declared envelope.
///
/// A process grant is a different account with a different owner (ADR-0072 §1),
/// and it holds a running program rather than the machinery that built one.
pub fn prepare_from_source(
    request: &SetRequest<'_>,
    trace: &mut dyn Trace,
    residency: ResidencyLimits,
) -> Result<Preparation, SetError> {
    let provider = SliceSourceProvider::new(request.units);
    prepare_from_provider(
        &provider,
        request.source_set,
        request.entry_path,
        request.entry,
        trace,
        residency,
    )
}

/// The same, from an explicit closure-bounded provider (ADR-0072 §6).
///
/// **The production shape.** Resolution reads the provider's listing once and
/// mints a closed membership; from then on the frontend asks for a member by
/// the identity that membership produced, and every answer is checked against
/// what resolution saw. A module outside the closure has no identifier, so the
/// provider cannot be asked for one.
pub fn prepare_from_provider(
    provider: &dyn SourceProvider,
    source_set: &str,
    entry_path: &str,
    entry: &str,
    trace: &mut dyn Trace,
    residency: ResidencyLimits,
) -> Result<Preparation, SetError> {
    // Metadata only. What the set offers, not what is in it.
    let catalog = provider.catalog();
    // Checked before the first stage is announced, so a request that cannot run
    // produces no stage events at all. A log that announced `read` and then
    // said the entry was missing would describe a run that never started.
    if catalog.is_empty() {
        return Err(SetError::NoUnits);
    }
    let Some(entry_index) = catalog.iter().position(|item| item.path == entry_path) else {
        return Err(SetError::EntryModuleAbsent {
            path: entry_path.to_string(),
        });
    };

    trace.entering(PipelineStage::Read);
    // **One source at a time, and nothing kept.** A `SourceUnit` is the
    // normalized copy of a unit's bytes, so a `Vec<SourceUnit>` over the closure
    // is the whole source set held twice — measured at 32 MiB for 128
    // ceiling-sized modules, before a single module has been lowered. The
    // caller already owns the bytes; what this pass produces is owned summaries,
    // and each source and each tree dies at the end of its own turn.
    let mut checking = false;
    let mut summaries: Vec<ModuleSummary> = Vec::with_capacity(catalog.len());
    let mut identities: Vec<String> = Vec::with_capacity(catalog.len());
    let mut diagnostics = Vec::new();
    let mut parsing = false;
    for item in &catalog {
        let Some(snapshot) = provider.source(item.id) else {
            return Ok(Preparation::Refused(Run::SourceRefused(
                SourceRefusal::Absent {
                    path: item.path.to_string(),
                },
            )));
        };
        let source = match SourceReader::read(snapshot.bytes()) {
            Ok(source) => source,
            Err(error) => {
                return Ok(Preparation::Refused(Run::SourceRejected {
                    code: error.code().symbol(),
                    byte_offset: error.byte_offset(),
                    // Which unit is not obvious once there is more than one,
                    // and a transport refusal never reaches a diagnostic that
                    // could carry a module identity.
                    path: item.path.to_string(),
                }));
            }
        };
        if !parsing {
            trace.entering(PipelineStage::Parse);
            parsing = true;
        }
        // Parse, check and summarize, keeping only what is owned.
        // `ModuleEntry::summarize` returns an owned summary precisely so the
        // tree can go, and `check_module_summaries` exists so set-wide
        // resolution never needs one — `docs/evidence/STAGE2_ARENA_BOUND.md`
        // measures why: a ceiling-sized module's parse tree costs about 14 MiB
        // and its summary about 0.2 MiB, and holding every tree made the arena
        // linear in the closure at seventy times the necessary slope.
        //
        // `Check` is announced when it is first entered rather than before the
        // first parse — a trace that named it earlier would name a stage that a
        // run failing in the parser never reached.
        let parsed = Parser::parse_schema(&source);
        let schema_diagnostics = parsed.diagnostics().to_vec();
        let Some(schema) = parsed.into_accepted() else {
            return Ok(Preparation::Refused(Run::Diagnosed {
                stage: PipelineStage::Parse,
                diagnostics: schema_diagnostics,
            }));
        };
        if !checking {
            trace.entering(PipelineStage::Check);
            checking = true;
        }
        // The identity resolution will hold this member to, computed here from
        // the bytes this turn actually read.
        identities.push(content_id(source.bytes()));
        let entry = ModuleEntry::new(item.path, &source, &schema);
        diagnostics.extend(entry.check());
        summaries.push(entry.summarize());
        // The tree and the normalized source both go here, at the end of this
        // module's turn. Everything after this point reads the summary.
        drop(schema);
        drop(source);
    }
    if !parsing {
        trace.entering(PipelineStage::Parse);
    }
    if !checking {
        trace.entering(PipelineStage::Check);
    }
    if diagnostics.iter().any(is_error) {
        return Ok(Preparation::Refused(Run::Diagnosed {
            stage: PipelineStage::Check,
            diagnostics,
        }));
    }

    trace.entering(PipelineStage::Resolve);
    let diagnostics = check_module_summaries(&summaries);
    if diagnostics.iter().any(is_error) {
        return Ok(Preparation::Refused(Run::Diagnosed {
            stage: PipelineStage::Resolve,
            diagnostics,
        }));
    }
    // Ordered and reachable: a module the entry cannot reach is not part of
    // what runs, and ordering it anyway would put it in the dependency digest.
    // Dependencies come first, so each module is lowered after everything it
    // imports and can be given their computed identities.
    let closure = closure_of_summaries(&summaries, entry_index);

    // The membership is minted here and nowhere else, **over the closure and not
    // over the catalog**: from identities this resolution computed, for the
    // modules the entry can actually reach. A source set may declare a hundred
    // modules and a closure contain three; the other ninety-seven keep their
    // catalog entry and never get a `SourceModuleId`. Past this line the
    // frontend asks for a member, and a module outside the closure has no
    // identifier to ask under.
    let members: Vec<SourceMember> = closure
        .iter()
        .map(|&index| SourceMember {
            entry: catalog[index].id,
            path: catalog[index].path.to_string(),
            content_id: identities[index].clone(),
        })
        .collect();
    let source_closure = SourceClosureManifest::of(members);

    trace.entering(PipelineStage::Lower);
    let names: Vec<String> = summaries
        .iter()
        .map(|summary| summary.name.clone())
        .collect();

    // **Deterministic liveness, not a cache.** The closure DAG is fully known
    // the moment resolution finishes, so when a dependency's lowering view stops
    // being needed is a fact, not a guess: it is the last position of the
    // lowering order that imports it. No eviction policy, no recency, nothing
    // consulted at run time — the view is dropped the instant its last consumer
    // is done with it, so what is live is the graph's frontier and not the
    // closure. A chain of 256 holds one; a wide fan-in holds its fan, which is
    // a real property of the graph and is measured rather than hidden.
    let last_consumer = last_consumers(&summaries, &closure, &names);

    // **One module's IR at a time.** Each is lowered, reduced to the two views
    // its readers actually read, encoded as the image the verifier will read,
    // and released. What accumulates is images and surfaces; what does not
    // accumulate is `Module`, and what does not accumulate for longer than the
    // graph requires is the lowering view. ADR-0040 bounds the whole machine,
    // not the execution phase, so holding the closure's IR until execution
    // starts would be the same retained-IR slope one stage earlier.
    let mut interfaces: Vec<Option<LoweringInterface>> = (0..catalog.len()).map(|_| None).collect();
    let mut surfaces: Vec<(usize, VerificationSurface)> = Vec::with_capacity(closure.len());
    let mut images: Vec<ImageSnapshot> = Vec::with_capacity(closure.len());
    for (position, &index) in closure.iter().enumerate() {
        // The source is normalized again, for this module alone, and dropped
        // when the module is lowered. A second pass over the same canonical
        // bytes is a second run of the same total function: `SourceReader` and
        // `Parser::parse_schema` are deterministic over them, so this is the
        // same source and the same tree the check phase saw, not a cheaper
        // substitute for either. Nothing skips the frontend and nothing skips
        // the checker — what is not done twice is *holding* the result.
        // The provider is asked for this member by the identity resolution
        // minted — position in the **closure**, not in the catalog — and what
        // comes back is checked against what resolution saw. Source that
        // vanished or changed between the two stages fails the preparation;
        // nothing looks for an alternative, and no path or module name is a
        // lookup key here.
        let member = source_closure
            .module(position)
            .expect("the lowering order walks this closure's own membership");
        let snapshot = match source::materialize(provider, &source_closure, member) {
            Ok(snapshot) => snapshot,
            Err(refusal) => return Ok(Preparation::Refused(Run::SourceRefused(refusal))),
        };
        let Ok(source) = SourceReader::read(snapshot.bytes()) else {
            // A unit that read in the read phase and not here would mean the
            // reader is not a function of its input.
            return Ok(Preparation::Refused(Run::SourceRejected {
                code: "source-not-reproducible",
                byte_offset: 0,
                path: catalog[index].path.to_string(),
            }));
        };
        let reparsed = Parser::parse_schema(&source);
        let Some(tree) = reparsed.into_accepted() else {
            // A module that parsed in the check phase and not here would mean
            // the parser is not a function of its input. It is refused rather
            // than worked around.
            return Ok(Preparation::Refused(Run::Diagnosed {
                stage: PipelineStage::Parse,
                diagnostics: Vec::new(),
            }));
        };
        let schema = &tree;
        // Each module's own dependency digest, over its own closure: a
        // dependency's identity cannot be the entry's, or two modules that
        // depend on different things would claim the same one.
        let own_closure = closure_of_summaries(&summaries, index);
        let context = ModuleContext {
            source_set: source_set.to_string(),
            path: summaries[index].path.clone(),
            content_id: content_id(source.bytes()),
            dependency_digest: closure_digest_of_summaries(&summaries, &own_closure, index),
            capability_interface_digest: list_digest(&[]),
        };
        let imports: Vec<ResolvedImport<'_>> = interfaces
            .iter()
            .enumerate()
            .filter_map(|(at, held)| {
                held.as_ref().map(|interface| ResolvedImport {
                    name: names[at].as_str(),
                    interface,
                })
            })
            .collect();
        let module = match lower_module_in_set(&source, schema, &context, &imports) {
            Ok(module) => module,
            Err(gap) => return Ok(Preparation::Refused(Run::NotLowered(gap))),
        };
        drop(imports);
        // Both views are built from the lowered IR, while it is still here, and
        // never from the source or a summary.
        interfaces[index] = Some(LoweringInterface::of(&module));
        surfaces.push((index, VerificationSurface::of(&module)));
        images.push(image_of(&module));
        // This is the line the phase rests on. Past it, this module's bodies,
        // blocks, instructions and source map are gone; what is left of it is
        // an image and two narrow views.
        drop(module);
        drop(tree);
        drop(source);

        // And this is the line the frontier rests on: every view whose last
        // consumer was this position goes now.
        for (at, held) in interfaces.iter_mut().enumerate() {
            if held.is_some() && last_consumer[at] == Some(position) {
                *held = None;
            }
        }
    }
    // The lowering views are done: the last module of the closure has been
    // lowered, so nothing will read one again.
    drop(interfaces);

    let entry_position = closure
        .iter()
        .position(|index| *index == entry_index)
        .expect("the closure always contains the entry");

    trace.entering(PipelineStage::Verify);
    // What the source set actually provides, computed from what was lowered.
    // The verifier is handed this and the IR, never the frontend's verdict:
    // an import that names a module the set does not provide, or claims an
    // identity the set disagrees with, is refused here even though the same
    // frontend produced both.
    let snapshot = snapshot_of(&surfaces);

    let prepared =
        match Prepared::launch_images(images, &snapshot, entry_position, entry, residency) {
            Ok(prepared) => prepared,
            // Every module verified and the closure is what it claimed to be; what
            // is absent is the function the caller named. That is the run failing
            // to start, so it is announced as one.
            Err(tos_residency::Failure::NoEntryFunction { .. }) => {
                trace.entering(PipelineStage::Execute);
                return Ok(Preparation::Refused(Run::Refused(Refusal::NoSuchEntry(
                    entry.to_string(),
                ))));
            }
            Err(failure) => return Ok(Preparation::Refused(launch_refusal(failure))),
        };

    // The verification surfaces go here. They were needed until the launch,
    // because the declared resolution is an input to verifying every module of
    // the closure; past it nothing reads one.
    drop(surfaces);

    // The launch workspace ends here. What is returned holds images, records,
    // the membership, the entry receipt and the declared envelope, and nothing
    // that built them.
    Ok(Preparation::Ready(Box::new(prepared)))
}

/// Runs a prepared executable closure.
///
/// The other side of ADR-0072 §2: this is the process's account. It reaches the
/// modules through the bounded resident set and never through anything the
/// preparation held.
///
/// **It takes no source and no provider.** After a preparation returns, the
/// canonical source, the request and the provider may all be dropped, and the
/// run produces the same value, the same trap and the same trap location.
pub fn run_prepared(
    prepared: &mut Prepared,
    arguments: Vec<Value>,
    system: &mut dyn System,
) -> Run {
    match prepared.run(arguments, system) {
        Err(refusal) => Run::Refused(refusal),
        Ok(Err(trap)) => Run::Trapped {
            code: trap.code,
            detail: trap.detail.clone(),
            at: location_of(&trap),
        },
        Ok(Ok(outcome)) => {
            let accounting = prepared.accounting(&outcome);
            Run::Completed(Box::new(Completion {
                receipt: prepared.receipt().clone(),
                value: outcome.value,
                accounting,
            }))
        }
    }
}

/// A verified closure, launched and ready to run.
///
/// The production path, in one object:
///
/// ```text
/// modules -> TOSIMAGE/v1 -> sequential verify_image -> records + membership
///         -> the modules are released
///         -> bounded resident set + explicit provider
/// ```
///
/// **What it holds is images, records and membership.** No `Vec<Module>`: the
/// decoded form of every module the launch saw was released as the launch went,
/// and what a run decodes again is bounded by the declared residency limits.
///
/// It is separate from the frontend on purpose. A caller that already has
/// verified IR — a measurement harness, a boot path — prepares once and runs
/// many times, and the preparation is not inside anything it measures.
pub struct Prepared {
    store: ImageStore,
    records: Vec<VerifiedModuleRecord>,
    manifest: VerifiedClosureManifest,
    receipt: VerifiedModule,
    residency: Residency,
}

impl Prepared {
    /// Encodes nothing and decodes nothing: verifies an ordered image closure,
    /// one module at a time, and binds a bounded resident set to what the launch
    /// established.
    ///
    /// **The production entry.** `images` is the **exact resolved closure** in
    /// position order and `entry_position` names the entry within it. Nothing
    /// here needs a decoded module, which is what lets the caller release each
    /// one as it is produced.
    ///
    /// **No budget arrives with the images.** The run's envelope is read out of
    /// the receipt this launch's own verifier issued, so what bounds a run is
    /// what the target verified rather than what the side that encoded the
    /// images said about it. A build that inflated a header is refused by the
    /// verifier or believed by nobody.
    pub fn launch_images(
        images: Vec<ImageSnapshot>,
        resolution: &ResolutionSnapshot,
        entry_position: usize,
        entry: &str,
        limits: ResidencyLimits,
    ) -> Result<Prepared, tos_residency::Failure> {
        let store = ImageStore { images };
        let launched = launch(
            &store,
            &|_| resolution.clone(),
            &Limits::default(),
            entry_position,
            entry,
        )?;
        let residency = Residency::new(limits, parse_limits()).map_err(|_| {
            tos_residency::Failure::OverResidencyBound {
                module: entry_position,
                bytes: 0,
            }
        })?;
        Ok(Prepared {
            store,
            records: launched.records,
            manifest: launched.manifest,
            receipt: launched.entry_receipt,
            residency,
        })
    }

    /// The same, from decoded modules a caller already holds.
    ///
    /// **A test and helper facade.** It requires the caller to hold every module
    /// of the closure at once, which is exactly what `execute_set` no longer
    /// does. Production goes through [`Prepared::launch_images`].
    ///
    /// `modules` is the exact resolved closure, dependencies first and the entry
    /// last. Nothing is discovered here: what is verified is what was handed
    /// over.
    pub fn launch(
        modules: &[&Module],
        resolution: &ResolutionSnapshot,
        entry: &str,
        limits: ResidencyLimits,
    ) -> Result<Prepared, tos_residency::Failure> {
        let images: Vec<ImageSnapshot> = modules.iter().map(|module| image_of(module)).collect();
        let entry_position = images.len().saturating_sub(1);
        Prepared::launch_images(images, resolution, entry_position, entry, limits)
    }

    /// The receipt the launch's own verifier issued for the entry module.
    pub fn receipt(&self) -> &VerifiedModule {
        &self.receipt
    }

    /// The same receipt, for a caller that is finished with the closure.
    pub fn into_receipt(self) -> VerifiedModule {
        self.receipt
    }

    /// Runs the entry function.
    ///
    /// Repeatable: the resident set carries over between runs, which is what a
    /// warm cache is, and every reload still checks the trusted artifact digest.
    pub fn run(
        &mut self,
        arguments: Vec<Value>,
        system: &mut dyn System,
    ) -> Result<Result<tos_engine::Outcome, Trap>, Refusal> {
        let mut closure = Closure::new(
            &mut self.residency,
            &self.store,
            &self.records,
            &self.manifest,
        );
        run_closure(&mut closure, arguments, system)
    }

    /// What a run cost, against the entry module's declared envelope.
    ///
    /// The envelope is the one in this launch's own receipt: the budget a run is
    /// held to is a fact the verifier established about the image, not a number
    /// that travelled beside it.
    pub fn accounting(&self, outcome: &tos_engine::Outcome) -> Accounting {
        Accounting::under(&self.receipt.resource_envelope, outcome)
    }

    /// What the resident set did.
    pub fn traffic(&self) -> tos_residency::Traffic {
        self.residency.traffic()
    }

    /// What is resident now, by component.
    pub fn ledger(&self) -> tos_residency::Ledger {
        self.residency.ledger()
    }

    /// How many modules the verified closure contains.
    pub fn modules(&self) -> usize {
        self.manifest.modules()
    }
}

/// What this host holds resident while a run executes.
///
/// A **declaration**, not a measurement: ADR-0071 section 7 bounds residency by
/// count and by module-derived bytes, and the numbers are the configuration's to
/// state. These are the host facade's own, for runs on a host allocator; the
/// freestanding runtime declares its own against `RUNTIME_GRANT`.
pub const HOST_RESIDENCY: ResidencyLimits = ResidencyLimits {
    // docs/44 section 2's closure ceiling: a run may reach every module of its
    // closure, and none of them more than once.
    modules: 256,
    bytes: 64 * 1024 * 1024,
};

/// The accepted ceilings as the reload parser's bounds.
fn parse_limits() -> tos_image::ParseLimits {
    let limits = Limits::default();
    tos_image::ParseLimits {
        table_entries: limits.table_entries,
        modules: limits.modules,
        fields: limits.fields,
        parameters: limits.parameters,
        blocks_per_function: limits.blocks_per_function,
        instructions_per_block: limits.instructions_per_block,
        source_map_entries: limits.source_map_entries,
    }
}

/// One module, as the immutable image the verifier and the engine both read.
fn image_of(module: &Module) -> ImageSnapshot {
    let (bytes, _) = tos_image::encode(module);
    ImageSnapshot::from(bytes.into_boxed_slice())
}

/// The closure's images, in position order.
///
/// The launch's `ClosureSource` and the run's `ModuleProvider` are the same
/// store, which is what a cache is: it supplies bytes. It cannot enumerate for
/// the provider and it never returns a conclusion — what the bytes mean is
/// decided by the artifact digest in the trusted record.
struct ImageStore {
    images: Vec<ImageSnapshot>,
}

impl ClosureSource for ImageStore {
    fn count(&self) -> usize {
        self.images.len()
    }

    fn image(&self, position: usize) -> Option<ImageSnapshot> {
        self.images.get(position).cloned()
    }
}

impl ModuleProvider for ImageStore {
    fn image(&self, id: ClosureModuleId) -> Option<ImageSnapshot> {
        self.images.get(id.position()).cloned()
    }
}

/// A launch that refused, as the pipeline's own outcome.
///
/// A semantic refusal is `Unverified` — the verifier looked at a module and said
/// no — and everything else is a failure of the closure the pipeline itself
/// assembled, which is a refusal of the run rather than a verdict on the source.
fn launch_refusal(failure: tos_residency::Failure) -> Run {
    match failure {
        tos_residency::Failure::Verifier { finding, .. } => Run::Unverified(finding),
        other => Run::Refused(Refusal::EntryNotResident(other)),
    }
}

/// What the resolved set provides, as the verifier is told it.
///
/// Built from the modules that were actually lowered rather than from the
/// request: a snapshot assembled from what a caller asked for would let the
/// verifier confirm the caller's own assumption.
fn snapshot_of(surfaces: &[(usize, VerificationSurface)]) -> ResolutionSnapshot {
    let mut declared = tos_verifier::DeclaredResolution::new();
    for (_, surface) in surfaces {
        declared
            .module(surface.module_name(), surface.content_id())
            .exports_declared();
        for export in surface.exports() {
            declared.export(export);
        }
        for capability in surface.capabilities() {
            declared.capability(capability);
        }
    }
    declared.build()
}

/// The last position of the lowering order that reads each module's lowering
/// view.
///
/// Computed from the resolved graph, once, before any module is lowered.
/// `None` means no later module imports it — its view dies at the end of its
/// own turn.
fn last_consumers(
    summaries: &[ModuleSummary],
    closure: &[usize],
    names: &[String],
) -> Vec<Option<usize>> {
    let mut last: Vec<Option<usize>> = (0..summaries.len()).map(|_| None).collect();
    for (position, &index) in closure.iter().enumerate() {
        for import in &summaries[index].imports {
            if let Some(at) = names.iter().position(|name| *name == import.target) {
                last[at] = Some(position);
            }
        }
    }
    last
}

/// A module's dependency closure, dependencies first, deterministically.
///
/// Depth-first over each module's imports in source order, emitting a module
/// only after everything it imports: lowering needs its dependencies already
/// lowered, and a digest needs a stable order. The same set therefore produces
/// the same order on every run and on every machine.
///
/// Resolution has already refused a cycle and an unresolvable import, so this
/// walk cannot loop and cannot be asked for a module that is not there. It
/// still guards, because a total function is cheaper than a proof that no
/// caller ever reaches it in another order.
/// The closure's order, over summaries.
///
/// This is the shape the accepted memory architecture asks for: a summary is
/// owned and a parse tree is not needed to order a closure — which is what lets
/// the trees be dropped one module at a time
/// (`docs/evidence/STAGE2_ARENA_BOUND.md`).
fn closure_of_summaries(summaries: &[ModuleSummary], entry: usize) -> Vec<usize> {
    let by_name: alloc::collections::BTreeMap<&str, usize> = summaries
        .iter()
        .enumerate()
        .map(|(index, summary)| (summary.name.as_str(), index))
        .collect();

    let mut order = Vec::new();
    let mut settled = alloc::collections::BTreeSet::new();
    let mut open = alloc::collections::BTreeSet::new();
    visit(
        entry,
        summaries,
        &by_name,
        &mut settled,
        &mut open,
        &mut order,
    );
    order
}

fn visit(
    index: usize,
    summaries: &[tos_core::ModuleSummary],
    by_name: &alloc::collections::BTreeMap<&str, usize>,
    settled: &mut alloc::collections::BTreeSet<usize>,
    open: &mut alloc::collections::BTreeSet<usize>,
    order: &mut Vec<usize>,
) {
    if settled.contains(&index) || !open.insert(index) {
        return;
    }
    for import in &summaries[index].imports {
        if let Some(&target) = by_name.get(import.target.as_str()) {
            visit(target, summaries, by_name, settled, open, order);
        }
    }
    open.remove(&index);
    settled.insert(index);
    order.push(index);
}

/// `sha256:<hex>` over the entry's resolved dependencies, name and content id.
///
/// The entry is excluded: a module's dependency digest describes what it
/// depends on, and including itself would make the digest change for a reason
/// that is already the content id.
/// The dependency digest of one module's own closure, over summaries.
fn closure_digest_of_summaries(
    summaries: &[ModuleSummary],
    closure: &[usize],
    entry: usize,
) -> String {
    let pairs: Vec<(&str, &str)> = closure
        .iter()
        .filter(|index| **index != entry)
        .map(|index| {
            (
                summaries[*index].name.as_str(),
                summaries[*index].content_id.as_str(),
            )
        })
        .collect();
    list_digest(&pairs)
}

fn is_error(diagnostic: &Diagnostic) -> bool {
    diagnostic.severity() == Severity::Error
}

/// The source a trap came from, resolved to line and column.
///
/// A trap from a dependency carries byte offsets into *that* module's text, so
/// the position is computed against the unit the source-map entry names.
/// Computing it against the entry's text would produce a line and column that
/// exist and are wrong, which is worse than none at all.
fn location_of(trap: &tos_engine::Trap) -> Option<TrapLocation> {
    // The trap carries its own span and its own canonical path. It has to: the
    // modules were released before the first instruction, so an index into a
    // source map nobody holds would name nothing — and now so is the source
    // itself gone by the time a process runs.
    let mapped = trap.site.as_deref()?;
    Some(TrapLocation {
        path: mapped.path.clone(),
        byte_start: mapped.byte_start,
        byte_end: mapped.byte_end,
    })
}

/// `sha256:<hex>` over the normalized source bytes.
pub fn content_id(bytes: &[u8]) -> String {
    named_digest(&tos_hash::sha256(bytes))
}

/// `sha256:<hex>` over an ordered list of `(name, identity)` pairs.
///
/// Length-prefixed like the module digest, so no pair of lists can produce the
/// same bytes by moving a separator into a name.
pub fn list_digest(entries: &[(&str, &str)]) -> String {
    let mut bytes: Vec<u8> = Vec::new();
    bytes.extend_from_slice(&(entries.len() as u64).to_le_bytes());
    for (name, identity) in entries {
        for field in [name, identity] {
            bytes.extend_from_slice(&(field.len() as u64).to_le_bytes());
            bytes.extend_from_slice(field.as_bytes());
        }
    }
    named_digest(&tos_hash::sha256(&bytes))
}

fn named_digest(digest: &[u8; 32]) -> String {
    let mut hex = [0u8; 64];
    tos_hash::hex(digest, &mut hex);
    let mut out = String::from("sha256:");
    // `hex` writes exactly 64 ASCII hex digits, so this cannot fail.
    out.push_str(core::str::from_utf8(&hex).unwrap_or(""));
    out
}

pub mod render;
