// SPDX-License-Identifier: GPL-3.0-or-later
//! Structured task scopes and atomic order legality (docs/41 sections 2 and 5).
//!
//! - `E1401_UNJOINED_TASK` — a scope is left with a spawned child still
//!   unconsumed.
//! - `E1410_INVALID_ATOMIC_ORDER` — an atomic operation is given an order it
//!   does not accept.
//!
//! docs/41 section 2 makes `parallel { ... }` a lexical task scope and requires
//! every spawned child to be joined or otherwise consumed before that scope
//! exits. `cancel` is an idempotent cooperative request that consumes no
//! ownership, so it explicitly does not discharge the obligation: the parent
//! still joins the cancelled handle.
//!
//! A `spawn` that no `parallel` block encloses is checked against its enclosing
//! return scope instead, which is the same rule applied to the scope that owns
//! the handle. A `spawn` whose value is not bound at all can never be consumed
//! and is reported where it stands.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::parser::{Block, Expression, ExpressionForm, Schema, Span, Statement, StatementForm};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

/// The five order values docs/41 section 5 defines, strongest last.
///
/// The rank orders them for the one comparison the contract states: the failure
/// order of `compare_exchange` may not be stronger than its success order.
fn order_rank(name: &str) -> Option<u8> {
    match name {
        "Relaxed" => Some(0),
        "Acquire" | "Release" => Some(1),
        "AcqRel" => Some(2),
        "SeqCst" => Some(3),
        _ => None,
    }
}

/// What kind of atomic operation a method name denotes, and where its orders
/// sit in the argument list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AtomicOperation {
    /// `load(order)`
    Load,
    /// `store(value, order)`
    Store,
    /// `swap(value, order)` and `fetch_*(value, order)`
    ReadModifyWrite,
    /// `compare_exchange(expected, desired, success, failure)`
    CompareExchange,
}

fn atomic_operation(name: &str) -> Option<AtomicOperation> {
    match name {
        "load" => Some(AtomicOperation::Load),
        "store" => Some(AtomicOperation::Store),
        "swap" => Some(AtomicOperation::ReadModifyWrite),
        "fetch_add" | "fetch_sub" | "fetch_and" | "fetch_or" | "fetch_xor" => {
            Some(AtomicOperation::ReadModifyWrite)
        }
        "compare_exchange" => Some(AtomicOperation::CompareExchange),
        _ => None,
    }
}

impl AtomicOperation {
    /// The orders this operation accepts in its primary order position.
    fn accepts(self, order: &str) -> bool {
        match self {
            AtomicOperation::Load => matches!(order, "Relaxed" | "Acquire" | "SeqCst"),
            AtomicOperation::Store => matches!(order, "Relaxed" | "Release" | "SeqCst"),
            AtomicOperation::ReadModifyWrite | AtomicOperation::CompareExchange => {
                order_rank(order).is_some()
            }
        }
    }

    fn spelled(self) -> &'static str {
        match self {
            AtomicOperation::Load => "load",
            AtomicOperation::Store => "store",
            AtomicOperation::ReadModifyWrite => "read-modify-write",
            AtomicOperation::CompareExchange => "compare_exchange",
        }
    }

    /// Which arguments carry orders: the index and whether it is the failure
    /// order of a compare-exchange.
    fn order_positions(self) -> &'static [usize] {
        match self {
            AtomicOperation::Load => &[0],
            AtomicOperation::Store | AtomicOperation::ReadModifyWrite => &[1],
            AtomicOperation::CompareExchange => &[2, 3],
        }
    }
}

pub(crate) fn check_concurrency(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut checker = ConcurrencyChecker {
        source,
        diagnostics: Vec::new(),
    };
    for function in schema.functions() {
        // A function body is the scope that owns any child no `parallel` block
        // encloses.
        checker.check_scope(function.body());
        checker.walk_block_for_orders(function.body());
    }
    checker.diagnostics
}

struct ConcurrencyChecker<'source> {
    source: &'source SourceUnit,
    diagnostics: Vec<Diagnostic>,
}

impl ConcurrencyChecker<'_> {
    fn report(&mut self, code: &'static str, span: Span) -> Diagnostic {
        Diagnostic::new(code, Severity::Error, Stage::Type, span, self.source)
    }

    // ---------------------------------------------------------- task scopes

    /// Checks one task scope and, recursively, every scope nested inside it.
    fn check_scope(&mut self, block: &Block) {
        let mut spawned: BTreeMap<String, Span> = BTreeMap::new();
        let mut unbound: Vec<Span> = Vec::new();
        let mut consumed: BTreeSet<String> = BTreeSet::new();
        collect_scope(
            self.source,
            block,
            &mut spawned,
            &mut unbound,
            &mut consumed,
        );

        for (name, at) in spawned {
            if consumed.contains(&name) {
                continue;
            }
            let diagnostic = self
                .report("E1401_UNJOINED_TASK", at)
                .with_field("task", name)
                .with_field("reason", "left the scope unconsumed");
            self.diagnostics.push(diagnostic);
        }
        for at in unbound {
            let diagnostic = self
                .report("E1401_UNJOINED_TASK", at)
                .with_field("reason", "the child handle is never bound");
            self.diagnostics.push(diagnostic);
        }

        for nested in nested_scopes(block) {
            self.check_scope(nested);
        }
    }

    // -------------------------------------------------------- atomic orders

    fn walk_block_for_orders(&mut self, block: &Block) {
        for statement in block.statements() {
            self.walk_statement_for_orders(statement);
        }
    }

    fn walk_statement_for_orders(&mut self, statement: &Statement) {
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            self.walk_expression_for_orders(expression);
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            self.walk_block_for_orders(nested);
        }
        if let Some(chained) = statement.else_if() {
            self.walk_statement_for_orders(chained);
        }
        for branch in statement.branches() {
            self.walk_block_for_orders(branch.body());
        }
    }

    fn walk_expression_for_orders(&mut self, expression: &Expression) {
        // Iteratively: a flat operator chain is as deep as it is long.
        crate::walk::walk_tree(expression, false, |node| {
            match node {
                crate::walk::Node::Block(block) => self.walk_block_for_orders(block),
                crate::walk::Node::Expression(expression) => {
                    if expression.form() == ExpressionForm::Call {
                        self.check_atomic_call(expression);
                    }
                }
            }
            crate::walk::Descend::Children
        });
    }

    /// Checks an atomic operation's order arguments.
    ///
    /// The operation is recognised by its method name together with an argument
    /// written as one of the five predeclared order values, which no other V1
    /// construct spells. A call that names no order is left to the type slice.
    fn check_atomic_call(&mut self, expression: &Expression) {
        let Some(callee) = expression.callee() else {
            return;
        };
        if callee.form() != ExpressionForm::Field {
            return;
        }
        let Some(method) = callee.name() else {
            return;
        };
        let Some(operation) = atomic_operation(method.text(self.source)) else {
            return;
        };
        let arguments = expression.arguments();
        let mut orders: Vec<(usize, &str, Span)> = Vec::new();
        for position in operation.order_positions() {
            let Some(argument) = arguments.get(*position) else {
                continue;
            };
            let value = argument.value();
            if value.form() != ExpressionForm::Name {
                continue;
            }
            let text = value.span().text(self.source);
            if order_rank(text).is_none() {
                continue;
            }
            orders.push((*position, text, value.span()));
        }
        if orders.is_empty() {
            return;
        }
        for (position, order, at) in &orders {
            let is_failure = operation == AtomicOperation::CompareExchange && *position == 3;
            let accepted = if is_failure {
                matches!(*order, "Relaxed" | "Acquire" | "SeqCst")
            } else {
                operation.accepts(order)
            };
            if accepted {
                continue;
            }
            let diagnostic = self
                .report("E1410_INVALID_ATOMIC_ORDER", *at)
                .with_field("operation", operation.spelled())
                .with_field("order", order.to_string())
                .with_field("position", if is_failure { "failure" } else { "order" });
            self.diagnostics.push(diagnostic);
        }
        // docs/41 section 5: the failure order may not be stronger than the
        // success order.
        if operation != AtomicOperation::CompareExchange {
            return;
        }
        let success = orders.iter().find(|(position, _, _)| *position == 2);
        let failure = orders.iter().find(|(position, _, _)| *position == 3);
        let (Some((_, success, _)), Some((_, failure, at))) = (success, failure) else {
            return;
        };
        let (Some(success_rank), Some(failure_rank)) = (order_rank(success), order_rank(failure))
        else {
            return;
        };
        if failure_rank <= success_rank {
            return;
        }
        let diagnostic = self
            .report("E1410_INVALID_ATOMIC_ORDER", *at)
            .with_field("operation", "compare_exchange")
            .with_field("order", failure.to_string())
            .with_field("position", "failure")
            .with_field("success_order", success.to_string());
        self.diagnostics.push(diagnostic);
    }
}

/// The `parallel` blocks directly inside a scope, which are its nested scopes.
fn nested_scopes(block: &Block) -> Vec<&Block> {
    let mut found = Vec::new();
    collect_nested_scopes(block, &mut found);
    found
}

fn collect_nested_scopes<'ast>(block: &'ast Block, out: &mut Vec<&'ast Block>) {
    for statement in block.statements() {
        // A closure or spawned body is its own return scope, so a child spawned
        // inside it belongs to that scope rather than this one.
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            collect_expression_scopes(expression, out);
        }
        if statement.form() == StatementForm::Parallel {
            if let Some(body) = statement.body() {
                out.push(body);
            }
            continue;
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            collect_nested_scopes(nested, out);
        }
        if let Some(chained) = statement.else_if() {
            collect_nested_scopes_in_statement(chained, out);
        }
        for branch in statement.branches() {
            collect_nested_scopes(branch.body(), out);
        }
    }
}

fn collect_expression_scopes<'ast>(expression: &'ast Expression, out: &mut Vec<&'ast Block>) {
    // Iteratively: a flat operator chain is as deep as it is long. The bodies
    // are collected, not entered — the caller walks them.
    crate::walk::walk_tree(expression, true, |node| {
        if let crate::walk::Node::Block(block) = node {
            out.push(block);
        }
        crate::walk::Descend::Children
    });
}

fn collect_nested_scopes_in_statement<'ast>(
    statement: &'ast Statement,
    out: &mut Vec<&'ast Block>,
) {
    if statement.form() == StatementForm::Parallel {
        if let Some(body) = statement.body() {
            out.push(body);
        }
        return;
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        collect_nested_scopes(nested, out);
    }
    if let Some(chained) = statement.else_if() {
        collect_nested_scopes_in_statement(chained, out);
    }
    for branch in statement.branches() {
        collect_nested_scopes(branch.body(), out);
    }
}

/// Collects what one task scope spawns and what it consumes.
///
/// A nested `parallel` block owns its own children, so it is not descended
/// into; a nested ordinary block, branch or loop body belongs to this scope and
/// is.
fn collect_scope(
    source: &SourceUnit,
    block: &Block,
    spawned: &mut BTreeMap<String, Span>,
    unbound: &mut Vec<Span>,
    consumed: &mut BTreeSet<String>,
) {
    for statement in block.statements() {
        if statement.form() == StatementForm::Parallel {
            continue;
        }
        // `cancel task;` is a cooperative request that consumes no ownership,
        // so it does not discharge the obligation and its operand is not a use.
        if statement.form() != StatementForm::Cancel {
            for expression in [statement.target(), statement.expression()]
                .into_iter()
                .flatten()
            {
                collect_consumption(source, expression, consumed);
            }
        }
        if statement.form() == StatementForm::Let {
            if let (Some(pattern), Some(initializer)) =
                (statement.pattern(), statement.expression())
            {
                if is_spawn(initializer) {
                    if let Some(name) = pattern.name() {
                        spawned.insert(name.text(source).to_string(), initializer.span());
                    }
                }
            }
        } else if statement.form() == StatementForm::Expression {
            if let Some(expression) = statement.expression() {
                if is_spawn(expression) {
                    unbound.push(expression.span());
                }
            }
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            collect_scope(source, nested, spawned, unbound, consumed);
        }
        if let Some(chained) = statement.else_if() {
            collect_scope_in_statement(source, chained, spawned, unbound, consumed);
        }
        for branch in statement.branches() {
            collect_scope(source, branch.body(), spawned, unbound, consumed);
        }
    }
}

fn collect_scope_in_statement(
    source: &SourceUnit,
    statement: &Statement,
    spawned: &mut BTreeMap<String, Span>,
    unbound: &mut Vec<Span>,
    consumed: &mut BTreeSet<String>,
) {
    if statement.form() == StatementForm::Parallel {
        return;
    }
    if statement.form() != StatementForm::Cancel {
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            collect_consumption(source, expression, consumed);
        }
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        collect_scope(source, nested, spawned, unbound, consumed);
    }
    if let Some(chained) = statement.else_if() {
        collect_scope_in_statement(source, chained, spawned, unbound, consumed);
    }
    for branch in statement.branches() {
        collect_scope(source, branch.body(), spawned, unbound, consumed);
    }
}

fn is_spawn(expression: &Expression) -> bool {
    match expression.form() {
        ExpressionForm::Spawn => true,
        ExpressionForm::Group => expression.inner().is_some_and(is_spawn),
        _ => false,
    }
}

/// Names an expression consumes: the operand of `join` or `await`, and any
/// name passed to a call or carried by a return, which moves the handle out.
fn collect_consumption(source: &SourceUnit, expression: &Expression, out: &mut BTreeSet<String>) {
    // Iteratively: a flat operator chain is as deep as it is long. A nested body
    // is not this slice's to read, so a block reached from an expression is
    // ignored exactly as it was before.
    crate::walk::walk_tree(expression, false, |node| {
        if let crate::walk::Node::Expression(expression) = node {
            consumed_by(source, expression, out);
        }
        crate::walk::Descend::Children
    });
}

fn consumed_by(source: &SourceUnit, expression: &Expression, out: &mut BTreeSet<String>) {
    match expression.form() {
        ExpressionForm::Unary => {
            let operator = expression.operator_text(source);
            if matches!(operator, Some("join") | Some("await")) {
                if let Some(name) = operand_name(source, expression.inner()) {
                    out.insert(name);
                }
            }
        }
        ExpressionForm::Name => {
            // A handle named in an ordinary value position leaves this scope by
            // being moved into whatever consumes it.
            out.insert(expression.span().text(source).to_string());
        }
        _ => {}
    }
}

fn operand_name(source: &SourceUnit, operand: Option<&Expression>) -> Option<String> {
    let operand = operand?;
    match operand.form() {
        ExpressionForm::Name => Some(operand.span().text(source).to_string()),
        ExpressionForm::Group => operand_name(source, operand.inner()),
        _ => None,
    }
}
