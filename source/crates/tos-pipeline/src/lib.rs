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
    check_module_set, lower_module, Checker, Diagnostic, Gap, ModuleContext, ModuleEntry, Parser,
    Position, Severity, SourceReader, SourceUnit,
};
use tos_engine::{run, Accounting, Refusal, Value};
use tos_ir::Module;
use tos_verifier::{verify, Finding, Limits, ResolutionSnapshot, VerifiedModule};

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
    SourceRejected {
        code: &'static str,
        byte_offset: usize,
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
    /// The source is valid and checked, and this lowerer cannot represent one
    /// of its constructs. Not a defect in the program.
    NotLowered(Gap),
    /// The independent verifier refused the IR the frontend emitted.
    Unverified(Finding),
    /// The engine refused to start: wrong receipt, no such entry, wrong arity.
    Refused(Refusal),
    /// A trap ended the run, named by the source it came from.
    Trapped {
        code: &'static str,
        detail: String,
        at: Option<Site>,
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
            Run::NotLowered(_) => Some(PipelineStage::Lower),
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
pub fn execute(request: &Request<'_>, arguments: Vec<Value>, trace: &mut dyn Trace) -> Run {
    trace.entering(PipelineStage::Read);
    let source = match SourceReader::read(request.bytes) {
        Ok(source) => source,
        Err(error) => {
            return Run::SourceRejected {
                code: error.code().symbol(),
                byte_offset: error.byte_offset(),
            }
        }
    };

    trace.entering(PipelineStage::Parse);
    let parsed = Parser::parse_schema(&source);
    let diagnostics = parsed.diagnostics().to_vec();
    let Some(schema) = parsed.into_accepted() else {
        return Run::Diagnosed {
            stage: PipelineStage::Parse,
            diagnostics,
        };
    };

    trace.entering(PipelineStage::Check);
    let diagnostics = Checker::check(&source, &schema);
    if diagnostics.iter().any(is_error) {
        return Run::Diagnosed {
            stage: PipelineStage::Check,
            diagnostics,
        };
    }

    trace.entering(PipelineStage::Resolve);
    let entry = ModuleEntry::new(request.path, &source, &schema);
    let diagnostics = check_module_set(core::slice::from_ref(&entry));
    if diagnostics.iter().any(is_error) {
        return Run::Diagnosed {
            stage: PipelineStage::Resolve,
            diagnostics,
        };
    }

    trace.entering(PipelineStage::Lower);
    let context = ModuleContext {
        source_set: request.source_set.to_string(),
        path: request.path.to_string(),
        content_id: content_id(source.bytes()),
        // Both digests are over the resolved list, which is empty for a module
        // with no imports. An empty list has a digest; a placeholder would make
        // the receipt name a resolution that never happened.
        dependency_digest: list_digest(&[]),
        capability_interface_digest: list_digest(&[]),
    };
    let module = match lower_module(&source, &schema, &context) {
        Ok(module) => module,
        Err(gap) => return Run::NotLowered(gap),
    };

    trace.entering(PipelineStage::Verify);
    let snapshot = ResolutionSnapshot::default();
    let receipt = match verify(&module, &snapshot, &Limits::default()) {
        Ok(receipt) => receipt,
        Err(finding) => return Run::Unverified(finding),
    };

    trace.entering(PipelineStage::Execute);
    match run(&module, &receipt, request.entry, arguments) {
        Err(refusal) => Run::Refused(refusal),
        Ok(Err(trap)) => Run::Trapped {
            code: trap.code,
            detail: trap.detail.clone(),
            at: site_of(&module, &source, &trap),
        },
        Ok(Ok(outcome)) => {
            let accounting = Accounting::of(&module, &outcome);
            Run::Completed(Box::new(Completion {
                receipt,
                value: outcome.value,
                accounting,
            }))
        }
    }
}

fn is_error(diagnostic: &Diagnostic) -> bool {
    diagnostic.severity() == Severity::Error
}

/// The source a trap came from, resolved to line and column.
fn site_of(module: &Module, source: &SourceUnit, trap: &tos_engine::Trap) -> Option<Site> {
    let entry = tos_engine::trap_source(module, trap)?;
    Some(Site {
        path: entry.path.clone(),
        start: Position::at(source, entry.byte_start),
        end: Position::at(source, entry.byte_end),
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
