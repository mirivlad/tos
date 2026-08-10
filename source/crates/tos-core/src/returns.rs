// SPDX-License-Identifier: GPL-3.0-or-later
//! Return-completeness checking for TOS Core return scopes (docs/40 section 5).
//!
//! An executable block has no tail expression, so `return expression;` is the
//! only normal value return. Every reachable normal completion path of a
//! function with a non-`unit` declared return type must execute one; reaching
//! the end of such a function is `E1221_MISSING_RETURN`. A closure or spawned
//! body follows the same rule against its inferred result: mixing a value
//! return with a normal fallthrough is the same error.
//!
//! The analysis is reachability only. It answers "can control reach the end of
//! this block", which needs no types. Each return scope is analysed on its own:
//! a `return` inside a closure or spawned body targets that body, so nested
//! scopes are never followed from an enclosing one.

use std::vec::Vec;

use crate::parser::{Block, Expression, ExpressionForm, Schema, Statement, StatementForm};
use crate::{Diagnostic, Severity, SourceUnit, Span, Stage};

pub(crate) fn check_returns(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for function in schema.functions() {
        let signature = function.signature();
        if signature.result().text(source) != "unit" && completes_normally(function.body()) {
            diagnostics.push(missing_return(source, signature.name(), "function"));
        }
        for scope in nested_scopes(function.body()) {
            check_nested_scope(source, scope, &mut diagnostics);
        }
    }
    diagnostics
}

/// A closure or spawned body has result `unit` only when no path returns a
/// value, so a value return alongside a reachable fallthrough is an error.
fn check_nested_scope(source: &SourceUnit, scope: &Expression, out: &mut Vec<Diagnostic>) {
    let Some(body) = scope.body() else {
        return;
    };
    if returns_a_value(body) && completes_normally(body) {
        let kind = if scope.form() == ExpressionForm::Closure {
            "closure"
        } else {
            "task"
        };
        out.push(missing_return(source, scope.span(), kind));
    }
    for nested in nested_scopes(body) {
        check_nested_scope(source, nested, out);
    }
}

fn missing_return(source: &SourceUnit, span: Span, scope: &'static str) -> Diagnostic {
    Diagnostic::new(
        "E1221_MISSING_RETURN",
        Severity::Error,
        Stage::Type,
        span,
        source,
    )
    .with_field("scope", scope)
}

/// Whether control can reach the end of `block`.
///
/// A statement that cannot complete normally makes everything after it
/// unreachable, so the block cannot complete normally either.
fn completes_normally(block: &Block) -> bool {
    block.statements().iter().all(statement_completes_normally)
}

fn statement_completes_normally(statement: &Statement) -> bool {
    match statement.form() {
        StatementForm::Return | StatementForm::Break | StatementForm::Continue => false,
        StatementForm::If => {
            let taken = statement.body().is_some_and(completes_normally);
            let alternative = match (statement.else_body(), statement.else_if()) {
                (Some(block), _) => completes_normally(block),
                (None, Some(nested)) => statement_completes_normally(nested),
                // Without an else branch the condition may simply be false.
                (None, None) => true,
            };
            taken || alternative
        }
        StatementForm::Match => {
            // Exhaustiveness is E1220 and belongs to the type slice. Assuming
            // it here can only miss an error, never invent one: a
            // non-exhaustive match is reported by that check instead.
            let branches = statement.branches();
            branches.is_empty()
                || branches
                    .iter()
                    .any(|branch| completes_normally(branch.body()))
        }
        // A loop ends only through a `break` that targets it.
        StatementForm::Loop => statement.body().is_some_and(breaks_out),
        // The head may fail on the first evaluation.
        StatementForm::While | StatementForm::For => true,
        // These wrap a block without changing where control goes next.
        StatementForm::Parallel | StatementForm::Unsafe => {
            statement.body().is_some_and(completes_normally)
        }
        _ => true,
    }
}

/// Whether a loop body contains a `break` that targets this loop.
///
/// A `break` inside a nested loop belongs to that loop, so nested loop bodies
/// are not searched.
fn breaks_out(block: &Block) -> bool {
    block.statements().iter().any(statement_breaks_out)
}

fn statement_breaks_out(statement: &Statement) -> bool {
    match statement.form() {
        StatementForm::Break => true,
        StatementForm::Loop | StatementForm::While | StatementForm::For => false,
        StatementForm::If => {
            statement.body().is_some_and(breaks_out)
                || statement.else_body().is_some_and(breaks_out)
                || statement.else_if().is_some_and(statement_breaks_out)
        }
        StatementForm::Match => statement
            .branches()
            .iter()
            .any(|branch| breaks_out(branch.body())),
        StatementForm::Parallel | StatementForm::Unsafe | StatementForm::Defer => {
            statement.body().is_some_and(breaks_out)
        }
        _ => false,
    }
}

/// Whether this return scope returns a value on some path.
fn returns_a_value(block: &Block) -> bool {
    block.statements().iter().any(statement_returns_a_value)
}

fn statement_returns_a_value(statement: &Statement) -> bool {
    if statement.form() == StatementForm::Return {
        return statement.expression().is_some();
    }
    let in_body = statement.body().is_some_and(returns_a_value);
    let in_else = statement.else_body().is_some_and(returns_a_value);
    let in_else_if = statement.else_if().is_some_and(statement_returns_a_value);
    let in_branches = statement
        .branches()
        .iter()
        .any(|branch| returns_a_value(branch.body()));
    in_body || in_else || in_else_if || in_branches
}

/// Collects the closure and spawned bodies directly inside one return scope.
fn nested_scopes(block: &Block) -> Vec<&Expression> {
    let mut scopes = Vec::new();
    collect_block_scopes(block, &mut scopes);
    scopes
}

fn collect_block_scopes<'tree>(block: &'tree Block, out: &mut Vec<&'tree Expression>) {
    for statement in block.statements() {
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            collect_expression_scopes(expression, out);
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            collect_block_scopes(nested, out);
        }
        if let Some(nested) = statement.else_if() {
            collect_block_scopes_of_statement(nested, out);
        }
        for branch in statement.branches() {
            collect_block_scopes(branch.body(), out);
        }
    }
}

fn collect_block_scopes_of_statement<'tree>(
    statement: &'tree Statement,
    out: &mut Vec<&'tree Expression>,
) {
    for expression in [statement.target(), statement.expression()]
        .into_iter()
        .flatten()
    {
        collect_expression_scopes(expression, out);
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        collect_block_scopes(nested, out);
    }
    if let Some(nested) = statement.else_if() {
        collect_block_scopes_of_statement(nested, out);
    }
}

fn collect_expression_scopes<'tree>(
    expression: &'tree Expression,
    out: &mut Vec<&'tree Expression>,
) {
    if matches!(
        expression.form(),
        ExpressionForm::Closure | ExpressionForm::Spawn
    ) {
        // The body belongs to this scope; its own nested scopes are collected
        // when that scope is analysed.
        out.push(expression);
        return;
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
        collect_expression_scopes(child, out);
    }
    for argument in expression.arguments() {
        collect_expression_scopes(argument.value(), out);
    }
    for element in expression.elements() {
        collect_expression_scopes(element, out);
    }
}
