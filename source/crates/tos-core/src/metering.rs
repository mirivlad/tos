// SPDX-License-Identifier: GPL-3.0-or-later
//! Loop metering (docs/41 section 6).
//!
//! Loop back edges consume fuel. A verifier-visible loop may either have a
//! statically proven finite bound or consume fuel; a loop with neither is
//! `E1701_UNMETERED_LOOP`.
//!
//! Both halves of that disjunction are decided from the source alone:
//!
//! - fuel is the declared `fuel` limit of the module's resource envelope. A
//!   module declaring `fuel: 0` has no fuel for any back edge to consume, so
//!   nothing in it is metered by fuel.
//! - a `for` iterates a finite sequence, so its iteration count is bounded by
//!   that sequence's length whatever the value turns out to be. A `while` or a
//!   bare `loop` has no such bound in V1: its condition is an ordinary
//!   expression, and V1 has no termination annotation.
//!
//! A module that declares no `fuel` key at all is already reported as
//! `E1700_RESOURCE_DECLARATION_REQUIRED`; its loops are not reported a second
//! time for a consequence of the same defect.

use alloc::vec::Vec;

use crate::parser::{Block, Schema, Statement, StatementForm};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

pub(crate) fn check_metering(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let Some(fuel) = declared_fuel(source, schema) else {
        return Vec::new();
    };
    if fuel > 0 {
        return Vec::new();
    }
    let mut diagnostics = Vec::new();
    for function in schema.functions() {
        walk_block(source, function.body(), &mut diagnostics);
    }
    diagnostics
}

/// The declared `fuel` limit, when the module declares one as an integer.
fn declared_fuel(source: &SourceUnit, schema: &Schema) -> Option<u128> {
    for limit in schema.outline().resource().limits() {
        if limit.name().text(source) != "fuel" {
            continue;
        }
        let text = limit.value().text(source);
        let digits: alloc::string::String =
            text.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() || digits.len() != text.len() {
            // A size literal or a malformed value is not a fuel budget;
            // `E1704_UNKNOWN_RESOURCE_LIMIT` owns that.
            return None;
        }
        return digits.parse().ok();
    }
    None
}

fn walk_block(source: &SourceUnit, block: &Block, out: &mut Vec<Diagnostic>) {
    for statement in block.statements() {
        walk_statement(source, statement, out);
    }
}

fn walk_statement(source: &SourceUnit, statement: &Statement, out: &mut Vec<Diagnostic>) {
    if matches!(statement.form(), StatementForm::While | StatementForm::Loop) {
        out.push(
            Diagnostic::new(
                "E1701_UNMETERED_LOOP",
                Severity::Error,
                Stage::Resource,
                statement.span(),
                source,
            )
            .with_field(
                "form",
                if statement.form() == StatementForm::While {
                    "while"
                } else {
                    "loop"
                },
            )
            .with_field("fuel", "0"),
        );
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        walk_block(source, nested, out);
    }
    if let Some(chained) = statement.else_if() {
        walk_statement(source, chained, out);
    }
    for branch in statement.branches() {
        walk_block(source, branch.body(), out);
    }
    for expression in [statement.target(), statement.expression()]
        .into_iter()
        .flatten()
    {
        walk_expression(source, expression, out);
    }
}

fn walk_expression(
    source: &SourceUnit,
    expression: &crate::parser::Expression,
    out: &mut Vec<Diagnostic>,
) {
    // Iteratively: a flat operator chain is as deep as it is long. This slice
    // walked a body before the subexpressions, and still does.
    crate::walk::walk_tree(expression, true, |node| {
        if let crate::walk::Node::Block(block) = node {
            walk_block(source, block, out);
        }
        crate::walk::Descend::Children
    });
}
