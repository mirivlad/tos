// SPDX-License-Identifier: GPL-3.0-or-later
//! The unsafe and FFI boundary (docs/40 section 7, docs/42 section 5).
//!
//! V1 reserved `extern` syntax so the boundary would be visible from the first
//! implementation, and rejected every `extern` item because there was no
//! accepted interface schema to name. ADR-0060 supplied the first one
//! (`SYSTEM_INTERFACE_V1`), so the rejection stopped being unconditional and
//! became what docs/44 always said it was: **an `extern` item names no accepted
//! interface schema**. Everything the schema does not declare is rejected
//! exactly as before, and docs/42 §5's prohibitions are untouched — no build
//! flag, host library or `unsafe` block enables anything here.
//!
//! An accepted item is one where all four of these hold, and the diagnostic
//! names which one did not:
//!
//! - its `uses` list names capability imports of this module, so an operation
//!   cannot be reached without having requested the authority it belongs to;
//! - the interface of that import is one an accepted schema declares;
//! - that interface declares an operation of this name;
//! - and the item's parameters and result are the operation's — the first
//!   parameter being the capability itself, of the interface's own type.
//!
//! An `unsafe { ... }` block must open with a line comment beginning `SAFETY:`
//! that names its local preconditions; a missing rationale is
//! `E1802_UNSAFE_RATIONALE_REQUIRED`. The lexer discards comments, so the
//! rationale is read from the source text of the block itself.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::interfaces;
use crate::parser::{
    Block, Expression, FunctionSignature, ImportKind, Schema, Statement, StatementForm, TypeSyntax,
};
use crate::{Diagnostic, Severity, SourceUnit, Span, Stage};

pub(crate) fn check_boundary(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // What this module requested, by the name it bound: an `extern` item's
    // `uses` list names one of these, and the interface is that request's type.
    let mut requested: BTreeMap<String, String> = BTreeMap::new();
    for import in schema.outline().prefix().imports() {
        if import.kind() != ImportKind::Capability {
            continue;
        }
        let path: Vec<&str> = import
            .path()
            .iter()
            .map(|segment| segment.text(source))
            .collect();
        requested.insert(import.binding().text(source).to_string(), path.join("."));
    }
    for signature in schema.extern_functions() {
        if let Some(reason) = unavailable(source, signature, &requested) {
            diagnostics.push(
                Diagnostic::new(
                    "E1801_FFI_NOT_AVAILABLE",
                    Severity::Error,
                    Stage::Effect,
                    signature.span(),
                    source,
                )
                .with_field("item", signature.name().text(source))
                .with_field("reason", reason),
            );
        }
    }
    for function in schema.functions() {
        check_block(source, function.body(), &mut diagnostics);
    }
    diagnostics
}

/// Why this `extern` item names no accepted operation, or nothing when it does.
///
/// The reasons are ordered so that the first thing wrong is the thing reported:
/// a module that did not request the authority is told that, rather than being
/// told about a signature mismatch on an operation it could not have reached.
fn unavailable(
    source: &SourceUnit,
    signature: &FunctionSignature,
    requested: &BTreeMap<String, String>,
) -> Option<&'static str> {
    let effects = signature.effects();
    if effects.len() != 1 {
        // One operation belongs to one interface, reached through one
        // capability. An item naming none has no interface; one naming several
        // would be an operation of no single contract.
        return Some("expected exactly one capability effect");
    }
    let Some(path) = requested.get(effects[0].text(source)) else {
        return Some("uses names no capability import of this module");
    };
    let Some(interface) = interfaces::interface(path) else {
        return Some("no accepted interface schema declares this interface");
    };
    let Some(operation) = interface.operation(signature.name().text(source)) else {
        return Some("the interface declares no operation of this name");
    };
    let parameters = signature.parameters();
    // The first parameter is the capability itself, of the interface's own
    // type: an operation is reached *through* a capability, so a declaration
    // that did not take one would be an operation reachable without authority.
    let Some((capability, values)) = parameters.split_first() else {
        return Some("the first parameter must be the capability");
    };
    if type_text(source, capability.ty()).as_deref() != Some(interface.path) {
        return Some("the first parameter is not a capability of this interface");
    }
    if values.len() != operation.parameters.len() {
        return Some("the operation takes a different number of values");
    }
    for (declared, written) in operation.parameters.iter().zip(values) {
        if type_text(source, written.ty()).as_deref() != Some(*declared) {
            return Some("a value parameter is not the type the operation takes");
        }
    }
    if type_text(source, signature.result()).as_deref() != Some(operation.result) {
        return Some("the result is not the type the operation returns");
    }
    None
}

/// A written type as its dotted text, for the forms an interface may name.
///
/// Only `Name` forms: an operation's parameters are a capability of a named
/// interface and primitive values, and a schema that admitted a constructed or
/// function type here would be admitting a shape nothing declares.
fn type_text(source: &SourceUnit, ty: &TypeSyntax) -> Option<String> {
    match ty {
        TypeSyntax::Name { path, .. } => {
            let segments: Vec<&str> = path.iter().map(|segment| segment.text(source)).collect();
            Some(segments.join("."))
        }
        _ => None,
    }
}

fn check_block(source: &SourceUnit, block: &Block, out: &mut Vec<Diagnostic>) {
    for statement in block.statements() {
        check_statement(source, statement, out);
    }
}

fn check_statement(source: &SourceUnit, statement: &Statement, out: &mut Vec<Diagnostic>) {
    if statement.form() == StatementForm::Unsafe && !has_safety_rationale(source, statement.span())
    {
        out.push(
            Diagnostic::new(
                "E1802_UNSAFE_RATIONALE_REQUIRED",
                Severity::Error,
                Stage::Effect,
                statement.span(),
                source,
            )
            .with_field("expected", "leading SAFETY: line comment"),
        );
    }
    for expression in [statement.target(), statement.expression()]
        .into_iter()
        .flatten()
    {
        check_expression(source, expression, out);
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        check_block(source, nested, out);
    }
    if let Some(nested) = statement.else_if() {
        check_statement(source, nested, out);
    }
    for branch in statement.branches() {
        check_block(source, branch.body(), out);
    }
}

fn check_expression(source: &SourceUnit, expression: &Expression, out: &mut Vec<Diagnostic>) {
    for child in [
        expression.left(),
        expression.right(),
        expression.inner(),
        expression.callee(),
    ]
    .into_iter()
    .flatten()
    {
        check_expression(source, child, out);
    }
    for argument in expression.arguments() {
        check_expression(source, argument.value(), out);
    }
    for element in expression.elements() {
        check_expression(source, element, out);
    }
    if let Some(body) = expression.body() {
        check_block(source, body, out);
    }
}

/// Whether an `unsafe` block opens with a `SAFETY:` line comment.
///
/// The rationale must lead the block, so only the text between the opening
/// brace and the first other content is considered.
fn has_safety_rationale(source: &SourceUnit, span: Span) -> bool {
    let text = span.text(source);
    let Some(body) = text.find('{').map(|index| &text[index + 1..]) else {
        return false;
    };
    let leading = body.trim_start_matches([' ', '\n']);
    let Some(comment) = leading.strip_prefix("//") else {
        return false;
    };
    comment.trim_start().starts_with("SAFETY:")
}
