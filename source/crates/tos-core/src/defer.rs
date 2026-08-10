// SPDX-License-Identifier: GPL-3.0-or-later
//! Restrictions on a `defer` body (docs/40 section 5).
//!
//! `defer` registers a lexically scoped cleanup block that runs whenever its
//! enclosing block exits — normally, by `return`, by `?`, by `break`, or after
//! cancellation. A cleanup block that could itself divert control or start new
//! work would make that ordering unanalysable, so a defer body cannot `return`,
//! `break`, `continue`, `await`, `join`, spawn work, or acquire a new resource.
//! Violations are `E1225_INVALID_DEFER`.
//!
//! Six of those seven are visible in the syntax tree and are checked here.
//! "Acquire a new resource" is a typed property of the operation being called
//! and belongs to the slice that resolves types; this module reports nothing
//! for it rather than guessing which calls allocate.

use std::vec::Vec;

use crate::parser::{Block, Expression, ExpressionForm, Schema, Statement, StatementForm};
use crate::{Diagnostic, Severity, SourceUnit, Span, Stage};

pub(crate) fn check_defer_bodies(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for function in schema.functions() {
        walk_block(source, function.body(), &mut diagnostics);
    }
    diagnostics
}

fn walk_block(source: &SourceUnit, block: &Block, out: &mut Vec<Diagnostic>) {
    for statement in block.statements() {
        walk_statement(source, statement, out);
    }
}

fn walk_statement(source: &SourceUnit, statement: &Statement, out: &mut Vec<Diagnostic>) {
    if statement.form() == StatementForm::Defer {
        if let Some(body) = statement.body() {
            let mut found = Vec::new();
            forbidden_in_block(source, body, false, &mut found);
            for (operation, span) in found {
                out.push(
                    Diagnostic::new(
                        "E1225_INVALID_DEFER",
                        Severity::Error,
                        Stage::Type,
                        span,
                        source,
                    )
                    .with_field("operation", operation),
                );
            }
        }
    }
    for expression in [statement.target(), statement.expression()]
        .into_iter()
        .flatten()
    {
        walk_expression(source, expression, out);
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        walk_block(source, nested, out);
    }
    if let Some(nested) = statement.else_if() {
        walk_statement(source, nested, out);
    }
    for branch in statement.branches() {
        walk_block(source, branch.body(), out);
    }
}

fn walk_expression(source: &SourceUnit, expression: &Expression, out: &mut Vec<Diagnostic>) {
    for child in [
        expression.left(),
        expression.right(),
        expression.inner(),
        expression.callee(),
    ]
    .into_iter()
    .flatten()
    {
        walk_expression(source, child, out);
    }
    for argument in expression.arguments() {
        walk_expression(source, argument.value(), out);
    }
    for element in expression.elements() {
        walk_expression(source, element, out);
    }
    if let Some(body) = expression.body() {
        walk_block(source, body, out);
    }
}

/// Collects the forbidden operations of one defer body.
///
/// `in_loop` records whether the position sits inside a loop that the defer
/// body itself contains: a `break` or `continue` there targets that loop rather
/// than diverting out of the cleanup block, so it is allowed.
fn forbidden_in_block(
    source: &SourceUnit,
    block: &Block,
    in_loop: bool,
    out: &mut Vec<(&'static str, Span)>,
) {
    for statement in block.statements() {
        forbidden_in_statement(source, statement, in_loop, out);
    }
}

fn forbidden_in_statement(
    source: &SourceUnit,
    statement: &Statement,
    in_loop: bool,
    out: &mut Vec<(&'static str, Span)>,
) {
    match statement.form() {
        StatementForm::Return => out.push(("return", statement.span())),
        StatementForm::Break if !in_loop => out.push(("break", statement.span())),
        StatementForm::Continue if !in_loop => out.push(("continue", statement.span())),
        _ => {}
    }
    let inside_loop = in_loop
        || matches!(
            statement.form(),
            StatementForm::Loop | StatementForm::While | StatementForm::For
        );
    for expression in [statement.target(), statement.expression()]
        .into_iter()
        .flatten()
    {
        forbidden_in_expression(source, expression, out);
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        forbidden_in_block(source, nested, inside_loop, out);
    }
    if let Some(nested) = statement.else_if() {
        forbidden_in_statement(source, nested, inside_loop, out);
    }
    for branch in statement.branches() {
        forbidden_in_block(source, branch.body(), inside_loop, out);
    }
}

fn forbidden_in_expression(
    source: &SourceUnit,
    expression: &Expression,
    out: &mut Vec<(&'static str, Span)>,
) {
    match expression.form() {
        ExpressionForm::Spawn => {
            out.push(("spawn", expression.span()));
            // A spawned body is its own return scope; its contents are not the
            // defer body's diversions.
            return;
        }
        // A closure body is a separate return scope, so a `return` inside it
        // does not divert out of the cleanup block.
        ExpressionForm::Closure => return,
        ExpressionForm::Unary => match expression.operator_text(source) {
            Some("await") => out.push(("await", expression.span())),
            Some("join") => out.push(("join", expression.span())),
            _ => {}
        },
        _ => {}
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
        forbidden_in_expression(source, child, out);
    }
    for argument in expression.arguments() {
        forbidden_in_expression(source, argument.value(), out);
    }
    for element in expression.elements() {
        forbidden_in_expression(source, element, out);
    }
}
