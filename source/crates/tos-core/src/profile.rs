// SPDX-License-Identifier: GPL-3.0-or-later
//! Bootstrap profile enforcement (docs/42 section 3).
//!
//! `profile bootstrap` is a strict executable subset of `profile full`. A Full
//! module must never be silently accepted by a Bootstrap frontend: it reports
//! `E1702_PROFILE_NOT_SUPPORTED` naming the first forbidden feature.
//!
//! Bootstrap forbids `async fn`, `spawn async`, `await`, closures, `defer`,
//! `unsafe` and `extern`, and requires `workers: 1`. Every one of those is
//! visible in the syntax tree, so this check needs no types.

use alloc::vec::Vec;

use crate::parser::{Block, Expression, ExpressionForm, Profile, Schema, Statement, StatementForm};
use crate::{Diagnostic, Severity, SourceUnit, Span, Stage};

/// One forbidden construct, with the span that identifies it.
struct Forbidden {
    feature: &'static str,
    span: Span,
}

pub(crate) fn check_profile(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    if schema.outline().prefix().header().profile() != Profile::Bootstrap {
        return Vec::new();
    }
    let mut found = Vec::new();
    collect_module(source, schema, &mut found);

    // docs/42 section 3 asks for the first forbidden feature, so the earliest
    // one in source order is reported and the rest are left for the next run.
    let Some(first) = found.into_iter().min_by_key(|entry| entry.span.start()) else {
        return Vec::new();
    };
    alloc::vec![Diagnostic::new(
        "E1702_PROFILE_NOT_SUPPORTED",
        Severity::Error,
        Stage::Resource,
        first.span,
        source,
    )
    .with_field("feature", first.feature)
    .with_field("profile", "bootstrap")]
}

fn collect_module(source: &SourceUnit, schema: &Schema, out: &mut Vec<Forbidden>) {
    for limit in schema.outline().resource().limits() {
        if limit.name().text(source) == "workers" && limit.value().text(source) != "1" {
            out.push(Forbidden {
                feature: "workers greater than 1",
                span: limit.span(),
            });
        }
    }
    for signature in schema.extern_functions() {
        out.push(Forbidden {
            feature: "extern",
            span: signature.span(),
        });
    }
    for function in schema.functions() {
        let signature = function.signature();
        if signature.is_async() {
            out.push(Forbidden {
                feature: "async fn",
                span: signature.span(),
            });
        }
        collect_block(source, function.body(), out);
    }
    for declaration in schema.consts() {
        collect_expression(source, declaration.value(), out);
    }
}

fn collect_block(source: &SourceUnit, block: &Block, out: &mut Vec<Forbidden>) {
    for statement in block.statements() {
        collect_statement(source, statement, out);
    }
}

fn collect_statement(source: &SourceUnit, statement: &Statement, out: &mut Vec<Forbidden>) {
    match statement.form() {
        StatementForm::Defer => out.push(Forbidden {
            feature: "defer",
            span: statement.span(),
        }),
        StatementForm::Unsafe => out.push(Forbidden {
            feature: "unsafe",
            span: statement.span(),
        }),
        _ => {}
    }
    for expression in [statement.target(), statement.expression()]
        .into_iter()
        .flatten()
    {
        collect_expression(source, expression, out);
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        collect_block(source, nested, out);
    }
    if let Some(nested) = statement.else_if() {
        collect_statement(source, nested, out);
    }
    for branch in statement.branches() {
        collect_block(source, branch.body(), out);
    }
}

fn collect_expression(source: &SourceUnit, expression: &Expression, out: &mut Vec<Forbidden>) {
    // Iteratively: a flat operator chain is as deep as it is long.
    crate::walk::walk_tree(expression, false, |node| {
        match node {
            crate::walk::Node::Block(block) => collect_block(source, block, out),
            crate::walk::Node::Expression(expression) => inspect(source, expression, out),
        }
        crate::walk::Descend::Children
    });
}

fn inspect(source: &SourceUnit, expression: &Expression, out: &mut Vec<Forbidden>) {
    match expression.form() {
        ExpressionForm::Closure => out.push(Forbidden {
            feature: "closure",
            span: expression.span(),
        }),
        // `spawn parallel` has defined serialized Bootstrap semantics; only
        // `spawn async` is Full-only.
        ExpressionForm::Spawn if expression.operator_text(source) == Some("async") => {
            out.push(Forbidden {
                feature: "spawn async",
                span: expression.span(),
            })
        }
        ExpressionForm::Unary if expression.operator_text(source) == Some("await") => {
            out.push(Forbidden {
                feature: "await",
                span: expression.span(),
            })
        }
        _ => {}
    }
}
