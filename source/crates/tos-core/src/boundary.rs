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
    Block, Expression, ExpressionForm, FunctionSignature, ImportKind, Schema, Statement,
    StatementForm, TypeSyntax,
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
    // Which `extern` items are operations of an accepted schema, by name, so a
    // call site can be checked against the operation it reaches.
    let mut operations: BTreeMap<String, &'static interfaces::Operation> = BTreeMap::new();
    for signature in schema.extern_functions() {
        let name = signature.name().text(source);
        let Some(first) = signature.effects().first() else {
            continue;
        };
        let Some(operation) = requested
            .get(first.text(source))
            .and_then(|path| interfaces::interface(path))
            .and_then(|interface| interface.operation(name))
        else {
            continue;
        };
        operations.insert(name.to_string(), operation);
    }
    let sites = Sites {
        requested: &requested,
        operations: &operations,
    };
    for function in schema.functions() {
        check_block(source, &sites, function.body(), &mut diagnostics);
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
    // The **first** effect names the interface the operation belongs to — the
    // one the instruction records and `Signature.effects` states. An item naming
    // none has no interface at all.
    let Some(first) = effects.first() else {
        return Some("expected a capability effect");
    };
    let Some(path) = requested.get(first.text(source)) else {
        return Some("uses names no capability import of this module");
    };
    let Some(interface) = interfaces::interface(path) else {
        return Some("no accepted interface schema declares this interface");
    };
    let Some(operation) = interface.operation(signature.name().text(source)) else {
        return Some("the interface declares no operation of this name");
    };
    // And there is one effect per capability the operation requires, in the
    // order the schema declares them (ADR-0063). Fewer would leave a required
    // authority unrequested; more would name a binding the operation does not
    // take, which is authority declared and never used.
    if effects.len() != operation.capabilities.len() {
        return Some("the operation requires a different number of capability effects");
    }
    for (required, effect) in operation.capabilities.iter().zip(effects) {
        match requested.get(effect.text(source)) {
            None => return Some("uses names no capability import of this module"),
            Some(path) if path != required.interface => {
                return Some("a capability effect is not of the interface the operation requires")
            }
            Some(_) => {}
        }
    }
    let parameters = signature.parameters();
    // The capabilities come first, in the order §4 lists them: an operation is
    // reached *through* a capability, so a declaration that did not take one
    // would be an operation reachable without authority — and one that took the
    // wrong one, or took them in another order, would be reachable through
    // authority the schema did not ask for.
    if parameters.len() < operation.capabilities.len() {
        return Some("the operation requires more capabilities than are declared");
    }
    let (capabilities, values) = parameters.split_at(operation.capabilities.len());
    for (position, (required, written)) in
        operation.capabilities.iter().zip(capabilities).enumerate()
    {
        if type_text(source, written.ty()).as_deref() != Some(required.interface) {
            return Some(match position {
                0 => "the first parameter is not a capability of this interface",
                _ => "a capability parameter is not of the interface the operation requires",
            });
        }
    }
    // The first capability is the operation's own interface, which is what the
    // instruction records and what `uses` names. A schema whose first
    // requirement named some other interface would make an operation reachable
    // from an interface that does not declare it.
    if operation
        .capabilities
        .first()
        .is_some_and(|first| first.interface != interface.path)
    {
        return Some("the operation's first capability is not this interface");
    }
    if values.len() != operation.parameters.len() {
        return Some("the operation takes a different number of values");
    }
    for (declared, written) in operation.parameters.iter().zip(values) {
        if type_text(source, written.ty()).as_deref() != Some(declared.ty) {
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

/// What a call site is checked against: what this module requested, and which
/// accepted operation each `extern` name reaches.
struct Sites<'a> {
    requested: &'a BTreeMap<String, String>,
    operations: &'a BTreeMap<String, &'static interfaces::Operation>,
}

fn check_block(source: &SourceUnit, sites: &Sites<'_>, block: &Block, out: &mut Vec<Diagnostic>) {
    for statement in block.statements() {
        check_statement(source, sites, statement, out);
    }
}

fn check_statement(
    source: &SourceUnit,
    sites: &Sites<'_>,
    statement: &Statement,
    out: &mut Vec<Diagnostic>,
) {
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
        check_expression(source, sites, expression, out);
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        check_block(source, sites, nested, out);
    }
    if let Some(nested) = statement.else_if() {
        check_statement(source, sites, nested, out);
    }
    for branch in statement.branches() {
        check_block(source, sites, branch.body(), out);
    }
}

fn check_expression(
    source: &SourceUnit,
    sites: &Sites<'_>,
    expression: &Expression,
    out: &mut Vec<Diagnostic>,
) {
    // A call to an operation of an accepted schema: each capability argument
    // must name an import **of the interface that position requires**
    // (ADR-0063). Without this, an operation taking two capabilities accepts
    // them in either order — and "reply here and wait there" becomes "wait here
    // and reply there" by writing the arguments the other way round, with
    // nothing in the artifact saying so. The types alone do not catch it: a
    // capability binding used as a value has no type the checker infers.
    if let Some(callee) = expression.callee() {
        if callee.form() == ExpressionForm::Name {
            if let Some(operation) = sites.operations.get(callee.span().text(source)) {
                for (required, argument) in
                    operation.capabilities.iter().zip(expression.arguments())
                {
                    let written = argument.value();
                    let held = (written.form() == ExpressionForm::Name)
                        .then(|| sites.requested.get(written.span().text(source)))
                        .flatten();
                    if held.map(String::as_str) != Some(required.interface) {
                        out.push(
                            Diagnostic::new(
                                "E1215_ARGUMENT_TYPE_MISMATCH",
                                Severity::Error,
                                Stage::Effect,
                                written.span(),
                                source,
                            )
                            .with_field("callee", callee.span().text(source))
                            .with_field("expected", required.interface)
                            .with_field(
                                "actual",
                                held.map(String::as_str)
                                    .unwrap_or("not a capability import"),
                            ),
                        );
                    }
                }
            }
        }
    }
    for child in [
        expression.left(),
        expression.right(),
        expression.inner(),
        expression.callee(),
    ]
    .into_iter()
    .flatten()
    {
        check_expression(source, sites, child, out);
    }
    for argument in expression.arguments() {
        check_expression(source, sites, argument.value(), out);
    }
    for element in expression.elements() {
        check_expression(source, sites, element, out);
    }
    if let Some(body) = expression.body() {
        check_block(source, sites, body, out);
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
