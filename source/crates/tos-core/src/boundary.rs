// SPDX-License-Identifier: GPL-3.0-or-later
//! The unsafe and FFI boundary (docs/40 section 7, docs/42 section 5).
//!
//! V1 reserves `extern` and `unsafe` syntax so the boundary is visible from the
//! first implementation, without admitting any external calling contract. An
//! `extern` item has no accepted interface schema to name, so it is rejected as
//! `E1801_FFI_NOT_AVAILABLE`; docs/42 states plainly that no build flag, host
//! library or unsafe block can enable it.
//!
//! An `unsafe { ... }` block must open with a line comment beginning `SAFETY:`
//! that names its local preconditions; a missing rationale is
//! `E1802_UNSAFE_RATIONALE_REQUIRED`. The lexer discards comments, so the
//! rationale is read from the source text of the block itself.

use std::vec::Vec;

use crate::parser::{Block, Expression, Schema, Statement, StatementForm};
use crate::{Diagnostic, Severity, SourceUnit, Span, Stage};

pub(crate) fn check_boundary(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for signature in schema.extern_functions() {
        diagnostics.push(
            Diagnostic::new(
                "E1801_FFI_NOT_AVAILABLE",
                Severity::Error,
                Stage::Effect,
                signature.span(),
                source,
            )
            .with_field("item", signature.name().text(source)),
        );
    }
    for function in schema.functions() {
        check_block(source, function.body(), &mut diagnostics);
    }
    diagnostics
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
