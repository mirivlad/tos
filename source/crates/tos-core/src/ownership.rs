// SPDX-License-Identifier: GPL-3.0-or-later
//! Affine ownership: move and use after move (docs/40 section 5).
//!
//! A safe non-`Copy` value has one owner and is moved when it is assigned,
//! passed to an owning parameter, returned, or placed in an aggregate. Using it
//! afterwards is `E1301_USE_AFTER_MOVE`.
//!
//! This slice tracks whole simple bindings. A path such as `message.payload`
//! counts as a *use* of `message`, so a move followed by a field read is
//! reported, but a partial move of one field is not yet modelled and reports
//! nothing. Borrow conflicts (`E1302`, `E1303`) and capture rules (`E1304`,
//! `E1305`) are separate slices.
//!
//! Copy-ness comes from the same inference the typing slice runs, so there is
//! one source of truth for what a binding holds. An undetermined type is
//! treated as `Copy`, which means an unknown never produces a move diagnostic.

use std::collections::BTreeMap;
use std::vec::Vec;

use crate::parser::{
    Block, BorrowMode, Expression, ExpressionForm, FunctionSignature, Pattern, PatternForm, Schema,
    Span, Statement, StatementForm,
};
use crate::typing::{binding_types, Type};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

pub(crate) fn check_ownership(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let types = binding_types(source, schema);
    let mut owned: BTreeMap<&str, Vec<Type>> = BTreeMap::new();
    for signature in schema.extern_functions() {
        owned.insert(
            signature.name().text(source),
            owning_parameters(signature, &types),
        );
    }
    for function in schema.functions() {
        let signature = function.signature();
        owned.insert(
            signature.name().text(source),
            owning_parameters(signature, &types),
        );
    }

    let mut checker = OwnershipChecker {
        source,
        types: &types,
        owning: owned,
        scopes: Vec::new(),
        diagnostics: Vec::new(),
    };
    for function in schema.functions() {
        checker.push_scope();
        for parameter in function.signature().parameters() {
            // Only an owned parameter can be moved out of; a borrow cannot.
            if parameter.borrow_mode() != BorrowMode::Owned {
                continue;
            }
            let ty = types.get(&parameter.name().start()).cloned();
            checker.declare(parameter.name(), ty);
        }
        checker.visit_block(function.body());
        checker.pop_scope();
    }
    checker.diagnostics
}

/// Which parameter positions of a function take ownership.
fn owning_parameters(signature: &FunctionSignature, types: &BTreeMap<usize, Type>) -> Vec<Type> {
    signature
        .parameters()
        .iter()
        .map(|parameter| {
            if parameter.borrow_mode() == BorrowMode::Owned {
                types
                    .get(&parameter.name().start())
                    .cloned()
                    .unwrap_or(Type::Unknown)
            } else {
                // A borrowed argument is never moved, so it can never be the
                // reason a later use fails.
                Type::Unknown
            }
        })
        .collect()
}

/// One tracked binding: whether it holds an affine value and where it moved.
struct Binding {
    affine: bool,
    moved_at: Option<Span>,
}

struct OwnershipChecker<'source, 'types> {
    source: &'source SourceUnit,
    types: &'types BTreeMap<usize, Type>,
    owning: BTreeMap<&'source str, Vec<Type>>,
    scopes: Vec<BTreeMap<&'source str, Binding>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'source, 'types> OwnershipChecker<'source, 'types> {
    fn push_scope(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: Span, ty: Option<Type>) {
        let affine = ty.is_some_and(|ty| !ty.is_copy());
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name.text(self.source),
                Binding {
                    affine,
                    moved_at: None,
                },
            );
        }
    }

    fn binding_mut(&mut self, name: &str) -> Option<&mut Binding> {
        self.scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.get_mut(name))
    }

    /// Records a use of a name, reporting it when the value already moved.
    fn use_name(&mut self, span: Span) {
        let name = span.text(self.source);
        let moved_at = match self.binding_mut(name) {
            Some(binding) => binding.moved_at,
            None => return,
        };
        let Some(moved_at) = moved_at else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(
                "E1301_USE_AFTER_MOVE",
                Severity::Error,
                Stage::Ownership,
                span,
                self.source,
            )
            .with_field("binding", name)
            .with_field("moved_at", moved_at.start()),
        );
    }

    /// Records that a name's value was moved out, if it is affine.
    fn move_name(&mut self, span: Span) {
        let name = span.text(self.source);
        let Some(binding) = self.binding_mut(name) else {
            return;
        };
        if !binding.affine || binding.moved_at.is_some() {
            return;
        }
        binding.moved_at = Some(span);
    }

    /// Visits an expression in a position that takes ownership of its value.
    fn visit_moving(&mut self, expression: &'source Expression) {
        self.visit_expression(expression);
        if expression.form() == ExpressionForm::Name {
            self.move_name(expression.span());
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
                    self.visit_moving(expression);
                }
                if let Some(pattern) = statement.pattern() {
                    self.bind_pattern(pattern);
                }
            }
            StatementForm::Assignment | StatementForm::Return => {
                if let Some(target) = statement.target() {
                    self.visit_expression(target);
                }
                if let Some(expression) = statement.expression() {
                    self.visit_moving(expression);
                }
            }
            StatementForm::For => {
                if let Some(expression) = statement.expression() {
                    self.visit_expression(expression);
                }
                self.push_scope();
                if let Some(pattern) = statement.pattern() {
                    self.bind_pattern(pattern);
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
                    self.bind_pattern(branch.pattern());
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
        if statement.form() == StatementForm::Let || statement.form() == StatementForm::Assignment {
            for nested in [statement.body(), statement.else_body()]
                .into_iter()
                .flatten()
            {
                self.visit_block(nested);
            }
        }
    }

    fn bind_pattern(&mut self, pattern: &'source Pattern) {
        match pattern.form() {
            PatternForm::Name if !pattern.is_qualified() => {
                if let Some(name) = pattern.name() {
                    let ty = self.types.get(&name.start()).cloned();
                    self.declare(name, ty);
                }
            }
            PatternForm::Destructure | PatternForm::Tuple => {
                for element in pattern.elements() {
                    self.bind_pattern(element);
                }
            }
            _ => {}
        }
    }

    fn visit_expression(&mut self, expression: &'source Expression) {
        match expression.form() {
            ExpressionForm::Name => {
                self.use_name(expression.span());
                return;
            }
            ExpressionForm::Call => {
                self.visit_call(expression);
                return;
            }
            // A tuple or array literal takes ownership of each member.
            ExpressionForm::Tuple | ExpressionForm::Array => {
                for element in expression.elements() {
                    self.visit_moving(element);
                }
                return;
            }
            // A borrow reads without taking ownership.
            ExpressionForm::Unary if expression.operator_text(self.source) == Some("borrow") => {
                if let Some(inner) = expression.inner() {
                    self.visit_expression(inner);
                }
                return;
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
            self.visit_expression(child);
        }
        for element in expression.elements() {
            self.visit_expression(element);
        }
        if let Some(body) = expression.body() {
            self.visit_block(body);
        }
    }

    fn visit_call(&mut self, expression: &'source Expression) {
        if let Some(callee) = expression.callee() {
            if callee.form() != ExpressionForm::Name {
                self.visit_expression(callee);
            }
        }
        let owning = expression
            .callee()
            .filter(|callee| callee.form() == ExpressionForm::Name)
            .and_then(|callee| self.owning.get(callee.span().text(self.source)))
            .cloned();
        for (index, argument) in expression.arguments().iter().enumerate() {
            // A constructor takes ownership of every argument; a call takes it
            // only where the parameter is owned.
            let takes_ownership = match &owning {
                Some(parameters) => parameters
                    .get(index)
                    .is_some_and(|parameter| !parameter.is_copy()),
                None => true,
            };
            if takes_ownership {
                self.visit_moving(argument.value());
            } else {
                self.visit_expression(argument.value());
            }
        }
    }
}
