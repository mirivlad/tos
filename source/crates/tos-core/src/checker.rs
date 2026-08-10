// SPDX-License-Identifier: GPL-3.0-or-later
//! Static checks over a parsed TOS Core module.
//!
//! This is the first slice of step 3 of the docs/44 section 6 order. It owns
//! the checks that need only the module's own declarations: the resource
//! envelope required by docs/41 section 6, and named-field uniqueness from
//! docs/39 section 5. Name resolution, types, effects and ownership are later
//! slices and are not performed here.
//!
//! Every diagnostic carries a code registered in docs/44 section 7. A check
//! that cannot yet be performed reports nothing rather than guessing.

use std::collections::BTreeMap;
use std::vec::Vec;

use crate::parser::{Block, Expression, RecordField, Schema, Statement};
use crate::{Diagnostic, Severity, SourceUnit, Span, Stage};

/// Resource keys every module must declare, with the literal class each one
/// takes (docs/41 section 6).
const REQUIRED_LIMITS: [(&str, LimitKind); 10] = [
    ("fuel", LimitKind::Integer),
    ("stack", LimitKind::Size),
    ("allocation", LimitKind::Size),
    ("tasks", LimitKind::Integer),
    ("workers", LimitKind::Integer),
    ("sync", LimitKind::Integer),
    ("shared", LimitKind::Size),
    ("cleanup", LimitKind::Integer),
    ("recursion", LimitKind::Integer),
    ("imports", LimitKind::Integer),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LimitKind {
    Integer,
    Size,
}

impl LimitKind {
    /// Whether a literal's source text belongs to this class.
    ///
    /// The lexer already separated integer from size literals, so this only
    /// has to tell the two apart: a size literal ends in its unit suffix.
    fn accepts(self, text: &str) -> bool {
        let is_size = text.ends_with('B')
            || text.ends_with("KiB")
            || text.ends_with("MiB")
            || text.ends_with("GiB");
        match self {
            LimitKind::Integer => {
                !is_size && text.chars().next().is_some_and(|c| c.is_ascii_digit())
            }
            LimitKind::Size => is_size,
        }
    }
}

pub struct Checker;

impl Checker {
    /// Runs every implemented static check over one parsed module.
    ///
    /// The schema must have parsed without error diagnostics; checking a
    /// partial tree would report consequences of a syntax error as semantic
    /// findings.
    pub fn check(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        check_resource_envelope(source, schema, &mut diagnostics);
        check_record_fields(source, schema, &mut diagnostics);
        diagnostics
    }
}

fn diagnostic(code: &'static str, stage: Stage, span: Span, source: &SourceUnit) -> Diagnostic {
    Diagnostic::new(code, Severity::Error, stage, span, source)
}

/// Checks the module resource declaration against docs/41 section 6.
fn check_resource_envelope(source: &SourceUnit, schema: &Schema, out: &mut Vec<Diagnostic>) {
    let resource = schema.outline().resource();
    let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
    for limit in resource.limits() {
        let name = limit.name().text(source);
        if let Some(first) = seen.get(name) {
            out.push(
                diagnostic(
                    "E1703_DUPLICATE_RESOURCE_DECLARATION",
                    Stage::Resource,
                    limit.name(),
                    source,
                )
                .with_field("key", name)
                .with_field("first_declared_at", first.start()),
            );
            continue;
        }
        seen.insert(name, limit.name());

        let Some((_, kind)) = REQUIRED_LIMITS.iter().find(|(key, _)| *key == name) else {
            // A module may declare stricter named limits, but an unrecognized
            // key is not one of them: docs/41 names no extension mechanism.
            out.push(
                diagnostic(
                    "E1704_UNKNOWN_RESOURCE_LIMIT",
                    Stage::Resource,
                    limit.name(),
                    source,
                )
                .with_field("key", name),
            );
            continue;
        };
        if !kind.accepts(limit.value().text(source)) {
            out.push(
                diagnostic(
                    "E1704_UNKNOWN_RESOURCE_LIMIT",
                    Stage::Resource,
                    limit.value(),
                    source,
                )
                .with_field("key", name)
                .with_field(
                    "expected",
                    if *kind == LimitKind::Size {
                        "size"
                    } else {
                        "integer"
                    },
                ),
            );
        }
    }

    for (key, _) in REQUIRED_LIMITS {
        if seen.contains_key(key) {
            continue;
        }
        out.push(
            diagnostic(
                "E1700_RESOURCE_DECLARATION_REQUIRED",
                Stage::Resource,
                resource.span(),
                source,
            )
            .with_field("key", key),
        );
    }
}

/// Checks that named field lists declare each name once (docs/39 section 5).
fn check_record_fields(source: &SourceUnit, schema: &Schema, out: &mut Vec<Diagnostic>) {
    for record in schema.records() {
        check_field_list(source, record.fields(), out);
    }
    for declaration in schema.enums() {
        for variant in declaration.variants() {
            check_field_list(source, variant.fields(), out);
        }
    }
    let mut findings = Vec::new();
    walk_expressions(schema, &mut |expression| {
        check_named_arguments(source, expression, &mut findings);
    });
    out.extend(findings);
}

fn check_field_list(source: &SourceUnit, fields: &[RecordField], out: &mut Vec<Diagnostic>) {
    let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
    for field in fields {
        let name = field.name().text(source);
        match seen.get(name) {
            Some(first) => out.push(duplicate_field(source, field.name(), name, *first)),
            None => {
                seen.insert(name, field.name());
            }
        }
    }
}

fn duplicate_field(source: &SourceUnit, span: Span, name: &str, first: Span) -> Diagnostic {
    diagnostic("E1205_DUPLICATE_RECORD_FIELD", Stage::Type, span, source)
        .with_field("field", name)
        .with_field("first_declared_at", first.start())
}

/// Checks that a named argument list supplies each field once.
///
/// docs/39 section 5 makes named construction exact-once, so the same rule
/// covers a declared field list and a constructor argument list.
fn check_named_arguments(source: &SourceUnit, expression: &Expression, out: &mut Vec<Diagnostic>) {
    let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
    for argument in expression.arguments() {
        let Some(span) = argument.name() else {
            continue;
        };
        let name = span.text(source);
        match seen.get(name) {
            Some(first) => out.push(duplicate_field(source, span, name, *first)),
            None => {
                seen.insert(name, span);
            }
        }
    }
}

/// Visits every expression reachable from a module's items.
fn walk_expressions(schema: &Schema, visit: &mut impl FnMut(&Expression)) {
    for declaration in schema.consts() {
        walk_expression(declaration.value(), visit);
    }
    for function in schema.functions() {
        walk_block(function.body(), visit);
    }
}

fn walk_block(block: &Block, visit: &mut impl FnMut(&Expression)) {
    for statement in block.statements() {
        walk_statement(statement, visit);
    }
}

fn walk_statement(statement: &Statement, visit: &mut impl FnMut(&Expression)) {
    if let Some(target) = statement.target() {
        walk_expression(target, visit);
    }
    if let Some(expression) = statement.expression() {
        walk_expression(expression, visit);
    }
    if let Some(body) = statement.body() {
        walk_block(body, visit);
    }
    if let Some(body) = statement.else_body() {
        walk_block(body, visit);
    }
    if let Some(nested) = statement.else_if() {
        walk_statement(nested, visit);
    }
    for branch in statement.branches() {
        walk_block(branch.body(), visit);
    }
}

fn walk_expression(expression: &Expression, visit: &mut impl FnMut(&Expression)) {
    visit(expression);
    for child in [
        expression.left(),
        expression.right(),
        expression.inner(),
        expression.callee(),
    ]
    .into_iter()
    .flatten()
    {
        walk_expression(child, visit);
    }
    for argument in expression.arguments() {
        walk_expression(argument.value(), visit);
    }
    for element in expression.elements() {
        walk_expression(element, visit);
    }
    if let Some(body) = expression.body() {
        walk_block(body, visit);
    }
}
