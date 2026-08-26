// SPDX-License-Identifier: GPL-3.0-or-later
//! Match exhaustiveness (docs/40 section 5, ADR-0033).
//!
//! A `match` must be exhaustive for an enum, `Option` or `Result`; a missing
//! case is `E1220_NONEXHAUSTIVE_MATCH`. An `_` arm is exhaustive, and so is a
//! bare binding arm, because ADR-0033 makes a bare name a binding exactly when
//! it is not a variant of the expected type — and a binding matches anything.
//!
//! Exhaustiveness needs the scrutinee's type. This slice determines it where
//! the source states it: a parameter's declared type, or a `let` binding with a
//! type annotation. A scrutinee whose type is not stated is not analysed, so
//! this reports only what the declarations make certain and never invents a
//! missing case.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use crate::parser::{
    Block, Expression, ExpressionForm, Pattern, PatternForm, Schema, Statement, StatementForm,
    TypeSyntax,
};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

/// The fixed variants of the predeclared sum types (docs/40 section 1).
const PREDECLARED_SUMS: [(&str, [&str; 2]); 3] = [
    ("Option", ["Some", "None"]),
    ("Result", ["Ok", "Err"]),
    ("TaskResult", ["Completed", "Cancelled"]),
];

pub(crate) fn check_exhaustiveness(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut enums: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for declaration in schema.enums() {
        enums.insert(
            declaration.name().text(source),
            declaration
                .variants()
                .iter()
                .map(|variant| variant.name().text(source))
                .collect(),
        );
    }
    let mut checker = ExhaustivenessChecker {
        source,
        enums,
        bindings: Vec::new(),
        diagnostics: Vec::new(),
    };
    for function in schema.functions() {
        checker.push_scope();
        for parameter in function.signature().parameters() {
            let name = parameter.name().text(source);
            if let Some(subject) = subject_of(source, parameter.ty()) {
                checker.declare(name, subject);
            }
        }
        checker.visit_block(function.body());
        checker.pop_scope();
    }
    checker.diagnostics
}

/// The name of the sum type a scrutinee has, when the syntax states it.
fn subject_of<'source>(
    source: &'source SourceUnit,
    ty: &'source TypeSyntax,
) -> Option<&'source str> {
    match ty {
        TypeSyntax::Name { path, .. } => Some(path.last()?.text(source)),
        TypeSyntax::Constructed { name, .. } => Some(name.text(source)),
        _ => None,
    }
}

struct ExhaustivenessChecker<'source> {
    source: &'source SourceUnit,
    enums: BTreeMap<&'source str, Vec<&'source str>>,
    bindings: Vec<BTreeMap<&'source str, &'source str>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> ExhaustivenessChecker<'source> {
    fn push_scope(&mut self) {
        self.bindings.push(BTreeMap::new());
    }

    fn pop_scope(&mut self) {
        self.bindings.pop();
    }

    fn declare(&mut self, name: &'source str, subject: &'source str) {
        if let Some(scope) = self.bindings.last_mut() {
            scope.insert(name, subject);
        }
    }

    fn lookup(&self, name: &str) -> Option<&'source str> {
        self.bindings
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    /// The declared variants of a sum type, local or predeclared.
    fn variants_of(&self, subject: &str) -> Option<Vec<&'source str>> {
        if let Some(variants) = self.enums.get(subject) {
            return Some(variants.clone());
        }
        PREDECLARED_SUMS
            .iter()
            .find(|(name, _)| *name == subject)
            .map(|(_, variants)| variants.to_vec())
    }

    fn visit_block(&mut self, block: &'source Block) {
        self.push_scope();
        for statement in block.statements() {
            self.visit_statement(statement);
        }
        self.pop_scope();
    }

    fn visit_statement(&mut self, statement: &'source Statement) {
        if statement.form() == StatementForm::Match {
            self.check_match(statement);
        }
        if statement.form() == StatementForm::Let {
            if let (Some(pattern), Some(ty)) = (statement.pattern(), statement.declared_type()) {
                if pattern.form() == PatternForm::Name && !pattern.is_qualified() {
                    if let (Some(name), Some(subject)) =
                        (pattern.name(), subject_of(self.source, ty))
                    {
                        self.declare(name.text(self.source), subject);
                    }
                }
            }
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            self.visit_block(nested);
        }
        if let Some(nested) = statement.else_if() {
            self.visit_statement(nested);
        }
        for branch in statement.branches() {
            self.visit_block(branch.body());
        }
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            self.visit_expression(expression);
        }
    }

    fn visit_expression(&mut self, expression: &'source Expression) {
        // Iteratively: a flat operator chain is as deep as it is long. This
        // slice walked a body before the subexpressions, and still does.
        crate::walk::walk_tree(expression, true, |node| {
            if let crate::walk::Node::Block(block) = node {
                self.visit_block(block);
            }
            crate::walk::Descend::Children
        });
    }

    fn check_match(&mut self, statement: &'source Statement) {
        let Some(head) = statement.expression() else {
            return;
        };
        if head.form() != ExpressionForm::Name {
            // Only a named scrutinee has a stated type in this slice.
            return;
        }
        let Some(subject) = self.lookup(head.span().text(self.source)) else {
            return;
        };
        let Some(declared) = self.variants_of(subject) else {
            return;
        };

        let mut covered: BTreeSet<&str> = BTreeSet::new();
        for branch in statement.branches() {
            match self.coverage(branch.pattern(), &declared) {
                Coverage::Everything => return,
                Coverage::Variant(name) => {
                    covered.insert(name);
                }
                Coverage::Nothing => {}
            }
        }
        let missing: Vec<&str> = declared
            .iter()
            .copied()
            .filter(|variant| !covered.contains(variant))
            .collect();
        if missing.is_empty() {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                "E1220_NONEXHAUSTIVE_MATCH",
                Severity::Error,
                Stage::Type,
                statement.span(),
                self.source,
            )
            .with_field("subject", subject)
            .with_field("missing", missing.join(", "))
            .with_field("missing_count", missing.len()),
        );
    }

    /// What one arm pattern covers, under the ADR-0033 resolution rule.
    fn coverage(&self, pattern: &'source Pattern, declared: &[&'source str]) -> Coverage<'source> {
        match pattern.form() {
            PatternForm::Wildcard => Coverage::Everything,
            PatternForm::Name | PatternForm::Destructure => {
                let Some(name) = pattern.name() else {
                    return Coverage::Nothing;
                };
                let spelled = name.text(self.source);
                if let Some(variant) = declared.iter().find(|variant| **variant == spelled) {
                    return Coverage::Variant(variant);
                }
                if pattern.is_qualified() || pattern.form() == PatternForm::Destructure {
                    // A constructor path that is not a variant of this type
                    // covers nothing; ADR-0033 keeps it from binding.
                    return Coverage::Nothing;
                }
                // A bare name that is not a variant of the expected type binds,
                // and a binding matches every value.
                Coverage::Everything
            }
            PatternForm::Tuple => Coverage::Nothing,
        }
    }
}

enum Coverage<'source> {
    Everything,
    Variant(&'source str),
    Nothing,
}
