// SPDX-License-Identifier: GPL-3.0-or-later
//! Module-level constants (docs/40 section 2, ADR-0052).
//!
//! A `const` declares a compile-time value, not a runtime object. Its
//! initializer is a constant expression, it is substituted where it is used,
//! and V1 therefore has no module-initialization phase: nothing is computed at
//! a moment, so there is no evaluation order between constants, no trap a
//! constant can raise and no resource its declaration consumes.
//!
//! This slice checks the *form* of an initializer. Whether its names resolve is
//! the names slice's question and whether its types agree is typing's; both
//! take precedence, because a form check on something that does not resolve
//! would report the wrong defect.

use alloc::vec::Vec;

use crate::parser::{Expression, ExpressionForm, Schema};
use crate::{Diagnostic, Severity, SourceUnit, Span, Stage};

pub(crate) fn check_constants(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for declaration in schema.consts() {
        check_expression(source, schema, declaration.value(), &mut diagnostics);
    }
    diagnostics
}

/// Reports the first non-constant part of an initializer.
///
/// The first, not every one: a call inside a call is one defect reported once,
/// and listing its parts would describe a tree rather than a mistake.
fn check_expression(
    source: &SourceUnit,
    schema: &Schema,
    expression: &Expression,
    out: &mut Vec<Diagnostic>,
) {
    match expression.form() {
        // A literal is the base case; a name is a constant, another module's
        // constant or a nullary variant constructor, and which one it is is the
        // names slice's question rather than this one's.
        ExpressionForm::Literal | ExpressionForm::Name => {}
        ExpressionForm::Group => {
            if let Some(inner) = expression.inner() {
                check_expression(source, schema, inner, out);
            }
        }
        ExpressionForm::Unary | ExpressionForm::Binary => {
            for operand in [expression.left(), expression.right()]
                .into_iter()
                .flatten()
            {
                check_expression(source, schema, operand, out);
            }
        }
        ExpressionForm::Tuple | ExpressionForm::Array => {
            for element in expression.elements() {
                check_expression(source, schema, element, out);
            }
        }
        // docs/39 section 5 gives calls and constructors one syntactic form, so
        // which this is depends on what the callee names. A constructor over
        // constant arguments is constant; a function call is not, because
        // calling is the one thing an initializer may not do.
        ExpressionForm::Call => {
            if constructs_a_value(source, schema, expression) {
                for argument in expression.arguments() {
                    check_expression(source, schema, argument.value(), out);
                }
            } else {
                out.push(nonconstant(source, expression.span(), "call"));
            }
        }
        ExpressionForm::Field => out.push(nonconstant(source, expression.span(), "field-access")),
        ExpressionForm::Index => out.push(nonconstant(source, expression.span(), "index")),
        ExpressionForm::Cast => out.push(nonconstant(source, expression.span(), "conversion")),
        ExpressionForm::Question => out.push(nonconstant(source, expression.span(), "error-edge")),
        ExpressionForm::Closure => out.push(nonconstant(source, expression.span(), "closure")),
        ExpressionForm::Spawn => out.push(nonconstant(source, expression.span(), "spawn")),
    }
}

/// Whether a Call/Construct form names a type constructor rather than a
/// function.
///
/// A record name, an enum variant of this module, or a predeclared constructor
/// builds a value out of its arguments and computes nothing. Anything else is
/// treated as a call, which keeps the refusal on the safe side: admitting a
/// call by mistake would let an initializer execute.
fn constructs_a_value(source: &SourceUnit, schema: &Schema, expression: &Expression) -> bool {
    let Some(callee) = expression.callee() else {
        return false;
    };
    if callee.form() != ExpressionForm::Name {
        return false;
    }
    let name = callee.span().text(source);
    if schema
        .records()
        .iter()
        .any(|record| record.name().text(source) == name)
    {
        return true;
    }
    if schema.enums().iter().any(|declaration| {
        declaration
            .variants()
            .iter()
            .any(|variant| variant.name().text(source) == name)
    }) {
        return true;
    }
    matches!(name, "Some" | "Ok" | "Err" | "Completed" | "Cancelled")
}

fn nonconstant(source: &SourceUnit, span: Span, reason: &'static str) -> Diagnostic {
    Diagnostic::new(
        "E1224_NONCONSTANT_INITIALIZER",
        Severity::Error,
        Stage::Type,
        span,
        source,
    )
    .with_field("reason", reason)
}
