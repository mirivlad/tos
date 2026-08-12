// SPDX-License-Identifier: GPL-3.0-or-later
//! Guard lifetimes: `E1402_INVALID_GUARD_LIFETIME` (ADR-0036).
//!
//! A lock grants a guard, and the guard is the thing with rules. docs/41
//! section 4 says a guard "cannot await, cross a task boundary, or be dropped
//! after its lock resource disappears", and docs/40 section 6 lists a lock
//! guard among the values that are not `Transferable`. ADR-0036 gives those
//! rules a name to attach to — `MutexGuard<T>`, `ReadGuard<T>`, `WriteGuard<T>`
//! — and one diagnostic with an `operation` field saying which rule was broken.
//!
//! **A guard is recognised by where it came from, never by spelling.** A value
//! is a guard because a lock operation on a `Mutex<T>` or `RwLock<T>` produced
//! it, or because a binding declares one of the three guard types. ADR-0035
//! forbids inferring it from the constructor name of the object involved, and
//! this slice does not: `something.lock()` on anything that is not a `Mutex<T>`
//! yields no guard here, because the typing slice gives it no guard type.
//!
//! **Precedence.** A guard crossing a task or closure boundary is reported
//! here, with `operation=task_boundary`, and not as
//! `E1304_INVALID_TASK_CAPTURE` or `E1305_INVALID_CLOSURE_CAPTURE`
//! (ADR-0036 section 5). The capture codes keep their meaning for every other
//! non-`Transferable` value.
//!
//! **What this slice can see.** A guard's scope is its binding's block, and a
//! move transfers the guard *and the release obligation with it* — so a guard
//! handed to a helper is not an escape, and releasing on a move would make a
//! guard unusable. Liveness is therefore tracked as "declared in an enclosing
//! block and not yet moved out", which is what the accepted scope rule says a
//! guard's extent is.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::parser::{Block, Expression, ExpressionForm, Schema, Span, Statement, StatementForm};
use crate::typing::{binding_types, Type};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

/// The three guard type constructors of ADR-0036 section 1.
const GUARDS: [&str; 3] = ["MutexGuard", "ReadGuard", "WriteGuard"];

/// The prohibited operations, exactly as ADR-0036 section 5 names them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Escape {
    HeldAcrossAwait,
    Returned,
    Aggregate,
    Channel,
    TaskBoundary,
    LockOutlived,
}

impl Escape {
    fn symbol(self) -> &'static str {
        match self {
            Escape::HeldAcrossAwait => "held_across_await",
            Escape::Returned => "returned",
            Escape::Aggregate => "aggregate",
            Escape::Channel => "channel",
            Escape::TaskBoundary => "task_boundary",
            Escape::LockOutlived => "lock_outlived",
        }
    }
}

/// One live guard: what it is, where it was taken, and what it locked.
#[derive(Clone, Debug)]
struct Guard {
    /// The guard type as written in the type surface, e.g. `MutexGuard`.
    kind: String,
    /// Where the guard was acquired. A lifetime finding that does not say
    /// where the lifetime started cannot be acted on.
    acquired: usize,
    /// The name of the binding holding the synchronization object, when the
    /// guard was taken from one directly.
    lock: Option<String>,
    /// Whether the guard has been moved out of this binding.
    moved: bool,
}

pub(crate) fn check_guards(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let types = binding_types(source, schema);
    check_guards_with(source, schema, &types)
}

/// The same slice, given the binding types rather than deriving them.
pub(crate) fn check_guards_with<'source>(
    source: &'source SourceUnit,
    schema: &'source Schema,
    types: &'source BTreeMap<usize, Type>,
) -> Vec<Diagnostic> {
    let mut checker = GuardChecker {
        source,
        types,
        scopes: Vec::new(),
        diagnostics: Vec::new(),
    };
    for function in schema.functions() {
        checker.scopes.push(BTreeMap::new());
        for parameter in function.signature().parameters() {
            let name = parameter.name();
            // Every binding is recorded, not only the guards: a lock operation
            // is recognised from its receiver's type, so the receiver has to be
            // resolvable to the declaration the typing slice typed.
            let guard = guard_kind(checker.types.get(&name.start())).map(|kind| Guard {
                kind,
                acquired: name.start(),
                lock: None,
                moved: false,
            });
            checker.declare(name.text(source).to_string(), name.start(), guard);
        }
        checker.walk_block(function.body());
        checker.scopes.pop();
    }
    checker.diagnostics
}

/// The guard constructor a type names, when it names one.
fn guard_kind(ty: Option<&Type>) -> Option<String> {
    match ty {
        Some(Type::Constructed(name, _)) if GUARDS.contains(&name.as_str()) => Some(name.clone()),
        _ => None,
    }
}

/// What one name in scope refers to.
struct Entry {
    /// The offset of the binding's declaration, which keys the typing slice's
    /// map. A name alone cannot be typed: two blocks may bind the same name.
    declared_at: usize,
    /// The guard this name holds, when it holds one.
    guard: Option<Guard>,
}

struct GuardChecker<'source> {
    source: &'source SourceUnit,
    types: &'source BTreeMap<usize, Type>,
    /// Bindings in scope, innermost last.
    scopes: Vec<BTreeMap<String, Entry>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> GuardChecker<'source> {
    fn declare(&mut self, name: String, declared_at: usize, guard: Option<Guard>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, Entry { declared_at, guard });
        }
    }

    fn entry(&self, name: &str) -> Option<&Entry> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn lookup(&self, name: &str) -> Option<&Guard> {
        self.entry(name)
            .and_then(|entry| entry.guard.as_ref())
            .filter(|guard| !guard.moved)
    }

    fn mark_moved(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(entry) = scope.get_mut(name) {
                if let Some(guard) = entry.guard.as_mut() {
                    guard.moved = true;
                }
                return;
            }
        }
    }

    /// Every guard still live in any enclosing scope.
    fn live(&self) -> Vec<Guard> {
        self.scopes
            .iter()
            .flat_map(|scope| scope.values())
            .filter_map(|entry| entry.guard.as_ref())
            .filter(|guard| !guard.moved)
            .cloned()
            .collect()
    }

    fn report(&mut self, guard: &Guard, escape: Escape, span: Span) {
        let diagnostic = Diagnostic::new(
            "E1402_INVALID_GUARD_LIFETIME",
            Severity::Error,
            Stage::Type,
            span,
            self.source,
        )
        .with_field("operation", escape.symbol())
        .with_field("guard", guard.kind.clone())
        .with_field("acquired_at", guard.acquired);
        self.diagnostics.push(diagnostic);
    }

    /// The guard an expression denotes, when it denotes one.
    ///
    /// Either a name bound to a live guard, or a lock operation's own result
    /// used directly — `send(mutex.lock())` escapes exactly as much as
    /// `send(guard)` does.
    fn guard_of(&self, expression: &Expression) -> Option<Guard> {
        match expression.form() {
            ExpressionForm::Name => self.lookup(expression.span().text(self.source)).cloned(),
            ExpressionForm::Call => {
                let kind = self.acquired_kind(expression)?;
                Some(Guard {
                    kind,
                    acquired: expression.span().start(),
                    lock: expression
                        .callee()
                        .and_then(|callee| callee.inner())
                        .filter(|receiver| receiver.form() == ExpressionForm::Name)
                        .map(|receiver| receiver.span().text(self.source).to_string()),
                    moved: false,
                })
            }
            _ => None,
        }
    }

    /// The type a lock operation call yields, when the call is one.
    ///
    /// The typing slice records the type of every *binding*; a call used
    /// directly has no binding, so its guard-ness is recovered from the shape
    /// the typing slice recognises: a field callee naming a lock operation.
    fn acquired_kind(&self, call: &Expression) -> Option<String> {
        let callee = call.callee()?;
        if callee.form() != ExpressionForm::Field {
            return None;
        }
        let operation = callee.name()?.text(self.source);
        let receiver = callee.inner()?;
        if receiver.form() != ExpressionForm::Name {
            return None;
        }
        // The *receiver's* type decides. A `.lock()` written on anything that
        // is not a `Mutex<T>` yields no guard here, which is the whole point:
        // ADR-0035 forbids inferring a guard from a spelling.
        let Type::Constructed(object, _) = self.type_of_name(receiver.span().text(self.source))?
        else {
            return None;
        };
        match (object.as_str(), operation) {
            ("Mutex", "lock") => Some(String::from("MutexGuard")),
            ("RwLock", "read") => Some(String::from("ReadGuard")),
            ("RwLock", "write") => Some(String::from("WriteGuard")),
            _ => None,
        }
    }

    /// The recorded type of the binding this name refers to.
    fn type_of_name(&self, name: &str) -> Option<&Type> {
        let entry = self.entry(name)?;
        self.types.get(&entry.declared_at)
    }

    fn walk_block(&mut self, block: &Block) {
        self.scopes.push(BTreeMap::new());
        for statement in block.statements() {
            self.walk_statement(statement);
        }
        self.scopes.pop();
    }

    fn walk_statement(&mut self, statement: &Statement) {
        if let Some(expression) = statement.expression() {
            self.walk_expression(expression);
            if statement.form() == StatementForm::Return {
                if let Some(guard) = self.guard_of(expression) {
                    self.report(&guard, Escape::Returned, expression.span());
                }
            }
        }
        if let Some(target) = statement.target() {
            self.walk_expression(target);
        }
        // A `let` extends the scope. A binding that holds a guard extends the
        // set of live guards; one that takes a guard from another binding ends
        // that binding's obligation, because a move carries the release
        // obligation with it (ADR-0036 section 4).
        if statement.form() == StatementForm::Let {
            if let Some(name) = statement.pattern().and_then(|pattern| pattern.name()) {
                let initializer = statement.expression();
                if let Some(source_name) = initializer
                    .filter(|value| value.form() == ExpressionForm::Name)
                    .map(|value| value.span().text(self.source).to_string())
                {
                    if self.lookup(&source_name).is_some() {
                        self.mark_moved(&source_name);
                    }
                }
                let guard = guard_kind(self.types.get(&name.start())).map(|kind| Guard {
                    kind,
                    acquired: name.start(),
                    lock: initializer
                        .and_then(|value| value.callee())
                        .and_then(|callee| callee.inner())
                        .filter(|receiver| receiver.form() == ExpressionForm::Name)
                        .map(|receiver| receiver.span().text(self.source).to_string()),
                    moved: false,
                });
                self.declare(name.text(self.source).to_string(), name.start(), guard);
            }
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
            // `await` is a prefix operator (docs/39 section 5), not a form of
            // its own, so it is recognised by the operator it carries.
            ExpressionForm::Unary if expression.operator_text(self.source) == Some("await") => {
                for guard in self.live() {
                    self.report(&guard, Escape::HeldAcrossAwait, expression.span());
                }
            }
            ExpressionForm::Spawn | ExpressionForm::Closure => {
                if let Some(body) = expression.body() {
                    let mut free: Vec<(String, Span)> = Vec::new();
                    collect_names(self.source, body, &mut free);
                    for (name, span) in free {
                        if let Some(guard) = self.lookup(&name).cloned() {
                            self.report(&guard, Escape::TaskBoundary, span);
                        }
                    }
                }
            }
            ExpressionForm::Call => {
                self.check_call(expression);
            }
            ExpressionForm::Tuple | ExpressionForm::Array => {
                for element in expression.elements() {
                    if let Some(guard) = self.guard_of(element) {
                        self.report(&guard, Escape::Aggregate, element.span());
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
            if !matches!(
                expression.form(),
                ExpressionForm::Spawn | ExpressionForm::Closure
            ) {
                self.walk_block(body);
            }
        }
    }

    /// A call is where a guard is placed into an aggregate, sent through a
    /// channel, or where its lock is moved out from under it.
    fn check_call(&mut self, call: &Expression) {
        let channel = call
            .callee()
            .filter(|callee| callee.form() == ExpressionForm::Field)
            .and_then(|callee| callee.name())
            .map(|name| matches!(name.text(self.source), "send" | "try_send"))
            .unwrap_or(false);
        let constructor = call
            .arguments()
            .iter()
            .any(|argument| argument.name().is_some());

        for argument in call.arguments() {
            let value = argument.value();
            if let Some(guard) = self.guard_of(value) {
                let escape = if channel {
                    Escape::Channel
                } else if constructor {
                    Escape::Aggregate
                } else {
                    // An ordinary call takes the guard and its release
                    // obligation with it, which ADR-0036 section 4 permits.
                    continue;
                };
                self.report(&guard, escape, value.span());
                continue;
            }
            // Moving the synchronization object out while a guard it granted is
            // still live leaves the guard naming nothing.
            if value.form() == ExpressionForm::Name {
                let moved = value.span().text(self.source).to_string();
                if let Some(guard) = self
                    .live()
                    .into_iter()
                    .find(|guard| guard.lock.as_deref() == Some(moved.as_str()))
                {
                    self.report(&guard, Escape::LockOutlived, value.span());
                }
            }
        }
    }
}

/// Every name a block mentions, with the span of each mention.
fn collect_names(source: &SourceUnit, block: &Block, out: &mut Vec<(String, Span)>) {
    for statement in block.statements() {
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            collect_expression_names(source, expression, out);
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            collect_names(source, nested, out);
        }
        if let Some(chained) = statement.else_if() {
            for expression in [chained.target(), chained.expression()]
                .into_iter()
                .flatten()
            {
                collect_expression_names(source, expression, out);
            }
        }
        for branch in statement.branches() {
            collect_names(source, branch.body(), out);
        }
    }
}

fn collect_expression_names(
    source: &SourceUnit,
    expression: &Expression,
    out: &mut Vec<(String, Span)>,
) {
    if expression.form() == ExpressionForm::Name {
        out.push((
            expression.span().text(source).to_string(),
            expression.span(),
        ));
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
        collect_expression_names(source, child, out);
    }
    for argument in expression.arguments() {
        collect_expression_names(source, argument.value(), out);
    }
    for element in expression.elements() {
        collect_expression_names(source, element, out);
    }
    if let Some(body) = expression.body() {
        collect_names(source, body, out);
    }
}
