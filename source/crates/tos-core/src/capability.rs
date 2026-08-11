// SPDX-License-Identifier: GPL-3.0-or-later
//! Declared authority: effect sets and capability nonconstructibility.
//!
//! docs/40 section 3 makes a function pure with respect to authority unless its
//! `uses [ ... ]` set names imported capability parameters or capability
//! values. Two rules follow, and both are checked here:
//!
//! - an operation that requires a capability is type-correct only if that
//!   capability's name is in the enclosing function's effect set;
//! - calling a function requires the caller's effect set to include every
//!   effect the callee requires.
//!
//! Either failure is `E1501_UNDECLARED_CAPABILITY_EFFECT`. Because every
//! function declares its own set and every call is checked against it, the
//! declared set of a well-formed module is its transitive set: there is no
//! separate inference step, and no ambient authority can appear.
//!
//! docs/40 section 3 and docs/42 section 4 also make a capability opaque and
//! nonconstructible. An integer, string, cast, deserialization, record literal
//! or unsafe block cannot mint one, and constructing or casting one is
//! `E1502_FORGED_CAPABILITY`.
//!
//! **Boundary.** docs/40 section 3 routes an `as` with the other opaque handles
//! — region, DMA region, task, synchronization object, function, closure — to
//! "the corresponding nonconstructible-type error", which no accepted document
//! names. Nothing is reported for those rather than borrowing this code for a
//! condition it does not cover.
//!
//! **Boundary.** An effect name in a callee declared by another module names
//! that module's own capability binding. Matching such a set across modules
//! needs the callee module's imports, which a single-module check does not
//! have, so a call to an imported function is not checked here. Within a module
//! the names denote the same bindings and the comparison is exact.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::parser::{Block, Expression, ExpressionForm, ImportKind, Schema, Span, Statement};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

pub(crate) fn check_capabilities(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut capabilities: BTreeMap<String, String> = BTreeMap::new();
    for import in schema.outline().prefix().imports() {
        if import.kind() != ImportKind::Capability {
            continue;
        }
        let path: Vec<&str> = import
            .path()
            .iter()
            .map(|segment| segment.text(source))
            .collect();
        capabilities.insert(import.binding().text(source).to_string(), path.join("."));
    }
    let paths: BTreeSet<String> = capabilities.values().cloned().collect();

    let mut declared: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for function in schema.functions() {
        let effects: BTreeSet<String> = function
            .signature()
            .effects()
            .iter()
            .map(|effect| effect.text(source).to_string())
            .collect();
        declared.insert(function.signature().name().text(source), effects);
    }

    let mut checker = CapabilityChecker {
        source,
        capabilities,
        paths,
        declared,
        effects: BTreeSet::new(),
        diagnostics: Vec::new(),
    };
    for function in schema.functions() {
        checker.effects = function
            .signature()
            .effects()
            .iter()
            .map(|effect| effect.text(source).to_string())
            .collect();
        checker.walk_block(function.body());
    }
    checker.diagnostics
}

struct CapabilityChecker<'source> {
    source: &'source SourceUnit,
    /// Capability binding name to the interface path it was imported from.
    capabilities: BTreeMap<String, String>,
    /// Every imported capability interface path, for recognising its type.
    paths: BTreeSet<String>,
    /// Each module function's declared effect set.
    declared: BTreeMap<&'source str, BTreeSet<String>>,
    /// The effect set of the function currently being walked.
    effects: BTreeSet<String>,
    diagnostics: Vec<Diagnostic>,
}

impl CapabilityChecker<'_> {
    fn report(&mut self, code: &'static str, span: Span) -> Diagnostic {
        Diagnostic::new(code, Severity::Error, Stage::Effect, span, self.source)
    }

    /// Reports a use of a capability the enclosing function does not declare.
    fn require_effect(&mut self, name: &str, span: Span) {
        if !self.capabilities.contains_key(name) || self.effects.contains(name) {
            return;
        }
        let interface = self.capabilities[name].clone();
        let diagnostic = self
            .report("E1501_UNDECLARED_CAPABILITY_EFFECT", span)
            .with_field("capability", name.to_string())
            .with_field("interface", interface)
            .with_field("required_by", "operation");
        self.diagnostics.push(diagnostic);
    }

    /// Reports a call whose callee needs authority the caller does not declare.
    fn require_callee_effects(&mut self, callee: &str, span: Span) {
        let Some(required) = self.declared.get(callee) else {
            return;
        };
        let missing: Vec<String> = required.difference(&self.effects).cloned().collect();
        for name in missing {
            let interface = self
                .capabilities
                .get(&name)
                .cloned()
                .unwrap_or_else(|| String::from("<unresolved>"));
            let diagnostic = self
                .report("E1501_UNDECLARED_CAPABILITY_EFFECT", span)
                .with_field("capability", name)
                .with_field("interface", interface)
                .with_field("required_by", callee.to_string());
            self.diagnostics.push(diagnostic);
        }
    }

    /// Whether a written path names an imported capability interface.
    fn is_capability_path(&self, path: &str) -> bool {
        self.paths.contains(path)
    }

    fn report_forgery(&mut self, path: String, how: &'static str, span: Span) {
        let diagnostic = self
            .report("E1502_FORGED_CAPABILITY", span)
            .with_field("interface", path)
            .with_field("operation", how);
        self.diagnostics.push(diagnostic);
    }

    fn walk_block(&mut self, block: &Block) {
        for statement in block.statements() {
            self.walk_statement(statement);
        }
    }

    fn walk_statement(&mut self, statement: &Statement) {
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            self.walk_expression(expression);
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            self.walk_block(nested);
        }
        if let Some(chained) = statement.else_if() {
            self.walk_statement(chained);
        }
        for branch in statement.branches() {
            self.walk_block(branch.body());
        }
    }

    fn walk_expression(&mut self, expression: &Expression) {
        match expression.form() {
            ExpressionForm::Name => {
                self.require_effect(expression.span().text(self.source), expression.span());
            }
            ExpressionForm::Cast => {
                if let Some(target) = expression.cast_type() {
                    let written = target.text(self.source).to_string();
                    if self.is_capability_path(&written) {
                        self.report_forgery(written, "cast", expression.span());
                    }
                }
            }
            ExpressionForm::Call => {
                if let Some(callee) = expression.callee() {
                    let written = spelled_path(self.source, callee);
                    if let Some(written) = written {
                        if self.is_capability_path(&written) {
                            self.report_forgery(written, "construct", expression.span());
                        } else if callee.form() == ExpressionForm::Name {
                            self.require_callee_effects(
                                callee.span().text(self.source),
                                expression.span(),
                            );
                        }
                    }
                }
            }
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
            self.walk_expression(child);
        }
        for argument in expression.arguments() {
            self.walk_expression(argument.value());
        }
        for element in expression.elements() {
            self.walk_expression(element);
        }
        if let Some(body) = expression.body() {
            self.walk_block(body);
        }
    }
}

/// The dotted path an expression spells, when it is written as one.
///
/// `system.time.Clock` parses as nested field accesses, so a capability
/// interface used as a constructor or a type is recovered by reassembling it.
fn spelled_path(source: &SourceUnit, expression: &Expression) -> Option<String> {
    match expression.form() {
        ExpressionForm::Name => Some(expression.span().text(source).to_string()),
        ExpressionForm::Field => {
            let base = spelled_path(source, expression.inner()?)?;
            let name = expression.name()?.text(source);
            Some(alloc::format!("{base}.{name}"))
        }
        _ => None,
    }
}
