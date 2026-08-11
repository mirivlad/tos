// SPDX-License-Identifier: GPL-3.0-or-later
//! Assignment mutability checking (docs/40 section 2).
//!
//! `let name = ...` creates an immutable binding and `let mut name = ...` a
//! mutable one. Assignment requires a mutable binding or a place reached
//! through one active mutable borrow; assigning to a nonmutable place is
//! `E1201_ASSIGN_TO_IMMUTABLE`.
//!
//! A place is a name followed by field and index suffixes, so mutability is
//! decided by the root name alone. The parser already rejects any other
//! assignment target, and the root of a place is always a `Name`.
//!
//! Borrow tracking belongs to the ownership slice. This slice reports only what
//! the binding forms make certain: a `borrow mut` parameter is mutable, a
//! `borrow` or owned parameter is not, and a `let` without `mut` is not.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::parser::{
    Block, BorrowMode, Expression, ExpressionForm, Pattern, PatternForm, Schema, Span, Statement,
    StatementForm,
};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

pub(crate) fn check_mutability(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut checker = MutabilityChecker {
        source,
        scopes: Vec::new(),
        diagnostics: Vec::new(),
    };
    for function in schema.functions() {
        checker.push_scope();
        for parameter in function.signature().parameters() {
            // Only an exclusive borrow makes a parameter assignable.
            let mutable = parameter.borrow_mode() == BorrowMode::Mutable;
            checker.declare(parameter.name().text(source), mutable);
        }
        checker.visit_block(function.body());
        checker.pop_scope();
    }
    checker.diagnostics
}

struct MutabilityChecker<'source> {
    source: &'source SourceUnit,
    scopes: Vec<BTreeMap<&'source str, bool>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> MutabilityChecker<'source> {
    fn push_scope(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &'source str, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, mutable);
        }
    }

    /// The mutability of the innermost binding of `name`, if it has one.
    fn lookup(&self, name: &str) -> Option<bool> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }

    /// The name a place is rooted at.
    fn place_root(expression: &'source Expression) -> Option<Span> {
        match expression.form() {
            ExpressionForm::Name => Some(expression.span()),
            ExpressionForm::Field | ExpressionForm::Index => {
                expression.inner().and_then(Self::place_root)
            }
            _ => None,
        }
    }

    fn check_assignment(&mut self, target: &'source Expression) {
        let Some(root) = Self::place_root(target) else {
            return;
        };
        let name = root.text(self.source);
        // An unbound name is E1202 from the resolver; reporting it again as an
        // immutability finding would double-report one mistake.
        let Some(mutable) = self.lookup(name) else {
            return;
        };
        if mutable {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                "E1201_ASSIGN_TO_IMMUTABLE",
                Severity::Error,
                Stage::Type,
                target.span(),
                self.source,
            )
            .with_field("binding", name)
            .with_field("declared_at", root.start()),
        );
    }

    /// Declares every name a pattern binds, with the `let` mutability.
    fn bind_pattern(&mut self, pattern: &'source Pattern, mutable: bool) {
        match pattern.form() {
            PatternForm::Wildcard => {}
            PatternForm::Name if pattern.is_qualified() => {}
            PatternForm::Name => {
                if let Some(name) = pattern.name() {
                    self.declare(name.text(self.source), mutable);
                }
            }
            PatternForm::Destructure | PatternForm::Tuple => {
                for element in pattern.elements() {
                    self.bind_pattern(element, mutable);
                }
            }
        }
    }

    fn visit_block(&mut self, block: &'source Block) {
        self.push_scope();
        for statement in block.statements() {
            self.visit_statement(statement);
        }
        self.pop_scope();
    }

    fn visit_statement(&mut self, statement: &'source Statement) {
        match statement.form() {
            StatementForm::Let => {
                if let Some(expression) = statement.expression() {
                    self.visit_expression(expression);
                }
                if let Some(pattern) = statement.pattern() {
                    self.bind_pattern(pattern, statement.is_mutable());
                }
            }
            StatementForm::Assignment => {
                if let Some(target) = statement.target() {
                    self.check_assignment(target);
                    self.visit_expression(target);
                }
                if let Some(expression) = statement.expression() {
                    self.visit_expression(expression);
                }
            }
            StatementForm::For => {
                if let Some(expression) = statement.expression() {
                    self.visit_expression(expression);
                }
                self.push_scope();
                if let Some(pattern) = statement.pattern() {
                    // A loop binding is immutable: docs/39 gives `for` no `mut`.
                    self.bind_pattern(pattern, false);
                }
                if let Some(body) = statement.body() {
                    self.visit_block(body);
                }
                self.pop_scope();
            }
            StatementForm::Match => {
                if let Some(expression) = statement.expression() {
                    self.visit_expression(expression);
                }
                for branch in statement.branches() {
                    self.push_scope();
                    self.bind_pattern(branch.pattern(), false);
                    self.visit_block(branch.body());
                    self.pop_scope();
                }
            }
            _ => {
                for expression in [statement.target(), statement.expression()]
                    .into_iter()
                    .flatten()
                {
                    self.visit_expression(expression);
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
            }
        }
    }

    fn visit_expression(&mut self, expression: &'source Expression) {
        if expression.form() == ExpressionForm::Closure {
            self.push_scope();
            for parameter in expression.parameters() {
                let mutable = parameter.borrow_mode() == BorrowMode::Mutable;
                self.declare(parameter.name().text(self.source), mutable);
            }
            if let Some(body) = expression.body() {
                self.visit_block(body);
            }
            self.pop_scope();
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
            self.visit_expression(child);
        }
        for argument in expression.arguments() {
            self.visit_expression(argument.value());
        }
        for element in expression.elements() {
            self.visit_expression(element);
        }
        if let Some(body) = expression.body() {
            self.visit_block(body);
        }
    }
}
