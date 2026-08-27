// SPDX-License-Identifier: GPL-3.0-or-later
//! Rendering a run as text a human or a harness can read.
//!
//! docs/41 section 7 fixes what a diagnostic must *carry*, not how it is
//! printed, so this is a presentation of the accepted model rather than part of
//! it. Everything printed here is read from the structured value; nothing is
//! recomputed, and no field is invented for display.
//!
//! One rendering serves the nucleus over serial and a hosted test, which is
//! deliberate: a boot log and a test expectation that are produced by different
//! code eventually disagree, and then neither is evidence for the other.

use alloc::format;
use alloc::string::String;

use tos_core::Diagnostic;
use tos_engine::Value;

use crate::{Completion, Run, Site};

/// A diagnostic in one line: code, severity, stage, span, then its fields.
///
/// The byte span is printed alongside line and column because the span is the
/// normative locator (docs/41 section 7) and the position is derived from it.
pub fn diagnostic(diagnostic: &Diagnostic) -> String {
    let mut out = format!(
        "{} severity={} stage={} bytes={}..{} at={}:{}",
        diagnostic.code(),
        diagnostic.severity().symbol(),
        diagnostic.stage().symbol(),
        diagnostic.span().start(),
        diagnostic.span().end(),
        diagnostic.start().line(),
        diagnostic.start().column(),
    );
    if let Some(module) = diagnostic.module() {
        out.push_str(&format!(" module={} path={}", module.name(), module.path()));
    }
    for field in diagnostic.fields() {
        out.push_str(&format!(" {}={}", field.key(), field.value()));
    }
    out
}

/// A value in a form that round-trips through the eye: no type is guessed and
/// no aggregate is flattened, so two different values never print the same.
pub fn value(value: &Value) -> String {
    match value {
        Value::Unit => String::from("unit"),
        Value::Bool(flag) => format!("bool:{flag}"),
        Value::Int(kind, number) => format!("{}:{number}", int_kind(*kind)),
        Value::Size(number) => format!("size:{number}"),
        Value::Duration(number) => format!("duration:{number}"),
        Value::Text(text) => format!("text:{text}"),
        Value::Bytes(bytes) => format!("bytes:{}", bytes.len()),
        Value::Aggregate(parts) => {
            let mut out = String::from("(");
            for (index, part) in parts.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&self::value(part));
            }
            out.push(')');
            out
        }
        Value::Variant { index, payload } => {
            let mut out = format!("variant{index}(");
            for (position, part) in payload.iter().enumerate() {
                if position > 0 {
                    out.push(',');
                }
                out.push_str(&self::value(part));
            }
            out.push(')');
            out
        }
        Value::Closure { body, captures } => format!("closure:{body}/{}", captures.len()),
        Value::Task {
            body,
            captures,
            cancelled,
        } => format!("task:{body}/{}/{cancelled}", captures.len()),
        // The one value that does not round-trip through the eye, on purpose.
        // `docs/42` §2 keeps the concrete handle representation out of source
        // maps, audit logs and cache identity, and this renderer feeds all
        // three. Two different capabilities print the same because the
        // difference between them is not a reader's to have.
        Value::Capability(_) => String::from("capability"),
    }
}

fn int_kind(kind: tos_ir::IntKind) -> &'static str {
    match kind {
        tos_ir::IntKind::I8 => "i8",
        tos_ir::IntKind::I16 => "i16",
        tos_ir::IntKind::I32 => "i32",
        tos_ir::IntKind::I64 => "i64",
        tos_ir::IntKind::U8 => "u8",
        tos_ir::IntKind::U16 => "u16",
        tos_ir::IntKind::U32 => "u32",
        tos_ir::IntKind::U64 => "u64",
    }
}

/// A source site as `path:line:column-line:column`.
pub fn site(site: &Site) -> String {
    format!(
        "{}:{}:{}-{}:{}",
        site.path,
        site.start.line(),
        site.start.column(),
        site.end.line(),
        site.end.column()
    )
}

/// What a run consumed against what the module declared.
pub fn accounting(completion: &Completion) -> String {
    let accounting = &completion.accounting;
    format!(
        "fuel={}/{} depth={}/{} tasks={}/{} allocation={}/{} cleanup={}/{} workers={}/{} \
         shared={}/{} sync={}/{}",
        accounting.fuel_used,
        accounting.fuel_limit,
        accounting.max_call_depth,
        accounting.recursion_limit,
        accounting.tasks_started,
        accounting.task_limit,
        accounting.allocation_peak,
        accounting.allocation_limit,
        accounting.cleanups_peak,
        accounting.cleanup_limit,
        accounting.workers_reserved,
        accounting.worker_limit,
        // Appended after the fields the accepted contract requires, which is
        // exactly what its extension rule admits.
        accounting.shared_peak,
        accounting.shared_limit,
        accounting.sync_peak,
        accounting.sync_limit,
    )
}

/// Every event a run produces, in order, without a line terminator.
///
/// These are the `TOS.RUN.*` identifiers described in
/// `docs/evidence/STAGE2_RUNTIME_EVENTS.md`, whose promotion to an accepted
/// interface contract is ADR-0042 (Proposed). The vocabulary lives with
/// the runtime that emits it rather than with any one consumer, so the nucleus
/// writing them to serial and a hosted test asserting on them are reading the
/// same producer — a boot log and a test expectation built by different code
/// eventually disagree, and then neither is evidence for the other.
///
/// The last event always states the outcome, so a truncated log is detectable.
pub fn events(run: &Run) -> alloc::vec::Vec<String> {
    let mut out = alloc::vec::Vec::new();
    match run {
        Run::SourceRejected {
            code,
            byte_offset,
            path,
        } => {
            // `path` is appended after the fields the accepted contract
            // requires, which is exactly what its extension rule admits: a set
            // has more than one unit, and an offset without a unit names
            // nothing.
            out.push(format!(
                "TOS.RUN.REFUSED stage=read code={code} byte={byte_offset} path={}",
                field(path)
            ));
        }
        Run::Diagnosed { stage, diagnostics } => {
            for entry in diagnostics {
                out.push(format!("TOS.RUN.DIAGNOSTIC {}", diagnostic(entry)));
            }
            out.push(format!(
                "TOS.RUN.REFUSED stage={} count={}",
                stage.symbol(),
                diagnostics.len()
            ));
        }
        Run::NotLowered(gap) => out.push(format!(
            "TOS.RUN.REFUSED stage=lower construct={} bytes={}..{}",
            gap.construct, gap.byte_start, gap.byte_end
        )),
        Run::Unverified(finding) => {
            out.push(format!(
                "TOS.RUN.REFUSED stage=verify code={} at={} detail={}",
                finding.code,
                field(&finding.location),
                field(&finding.detail)
            ));
        }
        Run::Refused(refusal) => out.push(format!(
            "TOS.RUN.REFUSED stage=execute reason={}{}",
            refusal_reason(refusal),
            refusal_fields(refusal)
        )),
        Run::Trapped { code, detail, at } => {
            let where_at = at
                .as_ref()
                .map(site)
                .unwrap_or_else(|| String::from("<unmapped>"));
            out.push(format!(
                "TOS.RUN.TRAP code={code} at={where_at} detail={}",
                field(detail)
            ));
        }
        Run::Completed(completion) => {
            out.push(format!(
                "TOS.RUN.VERIFIED module={} digest={} verifier={}",
                completion.receipt.module_name,
                completion.receipt.module_digest,
                field(&completion.receipt.verifier_identity)
            ));
            out.push(format!("TOS.RUN.ACCOUNTING {}", accounting(completion)));
            out.push(format!(
                "TOS.RUN.COMPLETED value={}",
                field(&value(&completion.value))
            ));
        }
    }
    out
}

/// A field value, made safe for a one-line `key=value` event.
///
/// The event log's discipline is one event per line with whitespace separating
/// fields, so a value carrying a space or a newline would silently invent
/// fields or events. Substitution is visible in the log rather than silent
/// truncation, and it applies only to values that can carry free text at all —
/// a code or a digest never changes.
fn field(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_whitespace() || character.is_control() {
                '_'
            } else {
                character
            }
        })
        .collect()
}

/// Why a run was refused, as one token.
///
/// A token and not a sentence: `reason=` is a field, and a value with spaces in
/// it is two fields to any reader that splits on them. What the reason is *about*
/// goes in [`refusal_fields`], where each part is a field of its own and can be
/// searched for by name.
fn refusal_reason(refusal: &tos_engine::Refusal) -> &'static str {
    match refusal {
        tos_engine::Refusal::EntryNotResident(_) => "entry-not-resident",
        tos_engine::Refusal::NoSuchEntry(_) => "no-such-entry",
        tos_engine::Refusal::EntryArity { .. } => "entry-arity",
        tos_engine::Refusal::CapabilityDenied { .. } => "capability-denied",
    }
}

/// What that reason is about, appended after it as ordinary fields.
fn refusal_fields(refusal: &tos_engine::Refusal) -> String {
    match refusal {
        // The identity and the check that refused it, as separate fields: a
        // run that could not reach its own entry module has to say which module
        // and why, and a reader has to be able to search for either.
        tos_engine::Refusal::EntryNotResident(failure) => {
            format!(
                " module={} check={}",
                residency_module(failure),
                residency_check(failure)
            )
        }
        tos_engine::Refusal::NoSuchEntry(name) => format!(" entry={}", field(name)),
        tos_engine::Refusal::EntryArity { expected, actual } => {
            format!(" expected={expected} actual={actual}")
        }
        // Named by binding *and* interface: the binding is what the source calls
        // it and what a reader searches for, the interface is what was wanted.
        // `PROCESS_IDENTITY_V1` §7.3 asks a denial to name itself, and one
        // without the binding would name only a type.
        tos_engine::Refusal::CapabilityDenied { binding, interface } => {
            format!(" binding={} interface={}", field(binding), field(interface))
        }
    }
}

/// Which closure module a residency failure is about.
fn residency_module(failure: &tos_residency::Failure) -> usize {
    match failure {
        tos_residency::Failure::Missing(module) => *module,
        tos_residency::Failure::ArtifactDigest { module }
        | tos_residency::Failure::Parser { module, .. }
        | tos_residency::Failure::Verifier { module, .. }
        | tos_residency::Failure::WrongModule { module }
        | tos_residency::Failure::NoEntryFunction { module }
        | tos_residency::Failure::OverResidencyBound { module, .. } => *module,
    }
}

/// Which check refused it, as a stable token.
fn residency_check(failure: &tos_residency::Failure) -> &'static str {
    match failure {
        tos_residency::Failure::Missing(_) => "provider-has-no-image",
        tos_residency::Failure::ArtifactDigest { .. } => "artifact-digest",
        tos_residency::Failure::Parser { .. } => "image-parser",
        tos_residency::Failure::Verifier { .. } => "verifier",
        tos_residency::Failure::WrongModule { .. } => "not-in-closure",
        tos_residency::Failure::NoEntryFunction { .. } => "no-entry-function",
        tos_residency::Failure::OverResidencyBound { .. } => "residency-bound",
    }
}
