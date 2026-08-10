// SPDX-License-Identifier: GPL-3.0-or-later
//! Affine ownership, borrows and captures (docs/40 sections 2, 5 and 6).
//!
//! One walk answers four questions, because they are one question about the
//! same state: which places hold a value, which places are borrowed, what a
//! task may take across its boundary, and what a closure may capture.
//!
//! - `E1301_USE_AFTER_MOVE` — a place is used after its value moved out.
//! - `E1302_CONFLICTING_BORROW` — a borrow overlaps an incompatible live one.
//! - `E1303_MUTATE_WHILE_BORROWED` — a write lands on a shared-borrowed place.
//! - `E1304_INVALID_TASK_CAPTURE` — a task captures a non-transferable value.
//! - `E1305_INVALID_CLOSURE_CAPTURE` — a closure captures a forbidden value.
//!
//! State flows through structured control with [`crate::flow`]: every branch
//! runs from the same entry state and the results are joined, so a move in one
//! arm never leaks into its sibling, and a move on any reachable path blocks a
//! later use.
//!
//! Copy-ness and place types come from the typing slice, which stays the single
//! source of truth. An undetermined type is `Copy`, so an unknown never
//! produces an ownership diagnostic.
//!
//! **Layering.** This is TOS Core frontend semantic state, not a
//! language-neutral executable representation. Ownership, borrows and
//! `Transferable` are rules of the safe TOS Core language: proof the frontend
//! produces, not a precondition for a program to be representable at all.
//!
//! docs/06 makes TOS IR a versioned representation shared by supported
//! frontends, while docs/43 pins the `tos-ir/v1` schema — including its affine
//! and `Copy` verification — to TOS Core V1. Both paths the architecture allows
//! must stay open: a future versioned IR schema or profile able to carry
//! another frontend's semantics, and foreign runtime integration under docs/07
//! where that is the better fit. So nothing here may become a mandatory
//! condition of a shared IR, and `tos-ir/v1` is not thereby the universal IR
//! for an unsafe language either.
//!
//! The isolation TOS guarantees any process — address space, capabilities,
//! granted regions, verifier and runtime contract — is a separate layer that
//! does not depend on these types.

use std::collections::{BTreeMap, BTreeSet};
use std::string::{String, ToString};
use std::vec::Vec;

use crate::flow::{BorrowKind, BorrowRecord, Certainty, Region, State};
use crate::parser::{
    Block, BorrowMode, Expression, ExpressionForm, ImportKind, Pattern, PatternForm, Schema, Span,
    Statement, StatementForm,
};
use crate::place::{BindingId, Place, Segment};
use crate::typing::{binding_types, record_fields, Type};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

/// Why a value may not cross a task or closure boundary (docs/40 section 6).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NonTransferable {
    Borrow,
    LockGuard,
    Region,
    Capability,
}

impl NonTransferable {
    fn reason(self) -> &'static str {
        match self {
            NonTransferable::Borrow => "borrow",
            NonTransferable::LockGuard => "lock guard",
            NonTransferable::Region => "mutable region",
            NonTransferable::Capability => "non-transferable capability",
        }
    }
}

/// What the walker knows about one binding occurrence.
struct BindingInfo {
    name: String,
    affine: bool,
    barrier: Option<NonTransferable>,
}

pub(crate) fn check_ownership(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut checker = OwnershipChecker {
        source,
        types: binding_types(source, schema),
        fields: record_fields(source, schema),
        capabilities: schema
            .outline()
            .prefix()
            .imports()
            .iter()
            .filter(|import| import.kind() == ImportKind::Capability)
            .map(|import| import.binding().text(source).to_string())
            .collect(),
        bindings: BTreeMap::new(),
        scopes: Vec::new(),
        diagnostics: Vec::new(),
    };
    for function in schema.functions() {
        let mut state = State::entry();
        checker.push_scope();
        for parameter in function.signature().parameters() {
            let barrier = match parameter.borrow_mode() {
                BorrowMode::Owned => None,
                // A borrow parameter names someone else's value: it can be read
                // and written through, but never moved or transferred.
                _ => Some(NonTransferable::Borrow),
            };
            checker.declare(parameter.name(), barrier);
        }
        state = checker.walk_block(function.body(), state);
        let _ = state;
        checker.pop_scope(&mut State::entry());
    }
    checker.diagnostics
}

struct OwnershipChecker<'source> {
    source: &'source SourceUnit,
    types: BTreeMap<usize, Type>,
    fields: BTreeMap<String, Vec<(String, Type)>>,
    capabilities: BTreeSet<String>,
    bindings: BTreeMap<BindingId, BindingInfo>,
    scopes: Vec<Vec<(String, BindingId)>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> OwnershipChecker<'source> {
    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn pop_scope(&mut self, state: &mut State) {
        let Some(scope) = self.scopes.pop() else {
            return;
        };
        let ids: Vec<BindingId> = scope.iter().map(|(_, id)| *id).collect();
        state.forget(&ids);
    }

    fn declare(&mut self, name: Span, barrier: Option<NonTransferable>) {
        let id = name.start();
        let ty = self.types.get(&id).cloned().unwrap_or(Type::Unknown);
        let barrier = barrier.or_else(|| barrier_of(&ty));
        self.bindings.insert(
            id,
            BindingInfo {
                name: name.text(self.source).to_string(),
                affine: !ty.is_copy(),
                barrier,
            },
        );
        if let Some(scope) = self.scopes.last_mut() {
            scope.push((name.text(self.source).to_string(), id));
        }
    }

    fn resolve(&self, name: &str) -> Option<BindingId> {
        self.scopes.iter().rev().find_map(|scope| {
            scope
                .iter()
                .rev()
                .find(|(declared, _)| declared == name)
                .map(|(_, id)| *id)
        })
    }

    fn info(&self, id: BindingId) -> Option<&BindingInfo> {
        self.bindings.get(&id)
    }

    /// The type at a place, walking record fields and array elements.
    fn place_type(&self, place: &Place, segments: &[Segment]) -> Type {
        let mut current = self
            .types
            .get(&place.binding())
            .cloned()
            .unwrap_or(Type::Unknown);
        for segment in segments {
            current = match (&current, segment) {
                (Type::Nominal(name), Segment::Field(field)) => self
                    .fields
                    .get(name)
                    .and_then(|fields| {
                        fields
                            .iter()
                            .find(|(declared, _)| declared == field)
                            .map(|(_, ty)| ty.clone())
                    })
                    .unwrap_or(Type::Unknown),
                (Type::Array(element), Segment::Index(_)) => (**element).clone(),
                _ => Type::Unknown,
            };
        }
        current
    }

    /// The place an expression names, if it names one.
    ///
    /// Parentheses are transparent: docs/39 makes a group an expression form,
    /// not a value copy, so `(message)` names the same place as `message`.
    fn place_of(&self, expression: &Expression) -> Option<(Place, Vec<Segment>)> {
        match expression.form() {
            ExpressionForm::Name => {
                let id = self.resolve(expression.span().text(self.source))?;
                Some((Place::root(id), Vec::new()))
            }
            ExpressionForm::Group => self.place_of(expression.inner()?),
            ExpressionForm::Field => {
                let (base, mut segments) = self.place_of(expression.inner()?)?;
                let name = expression.name()?.text(self.source).to_string();
                segments.push(Segment::Field(name));
                Some((
                    base.extended(Segment::Field(
                        expression.name()?.text(self.source).to_string(),
                    )),
                    segments,
                ))
            }
            ExpressionForm::Index => {
                let (base, mut segments) = self.place_of(expression.inner()?)?;
                let index = expression
                    .right()
                    .and_then(|index| constant_index(index, self.source));
                segments.push(Segment::Index(index));
                Some((base.extended(Segment::Index(index)), segments))
            }
            _ => None,
        }
    }

    fn spell(&self, place: &Place) -> String {
        let name = self
            .info(place.binding())
            .map(|info| info.name.as_str())
            .unwrap_or("<binding>");
        place.spell(name)
    }

    fn report(&mut self, code: &'static str, stage: Stage, span: Span) -> Diagnostic {
        Diagnostic::new(code, Severity::Error, stage, span, self.source)
    }

    // ---------------------------------------------------------------- uses

    /// Records a read of a place, reporting a use after move.
    fn read_place(&mut self, expression: &Expression, state: &mut State) {
        let Some((place, _)) = self.place_of(expression) else {
            return;
        };
        let Some(record) = state.blocking_move(&place) else {
            return;
        };
        let moved = record.place.clone();
        let at = record.at;
        let certainty = record.certainty;
        let spelled = self.spell(&place);
        let moved_spelled = self.spell(&moved);
        let diagnostic = self
            .report("E1301_USE_AFTER_MOVE", Stage::Ownership, expression.span())
            .with_field("place", spelled)
            .with_field("moved", moved_spelled)
            .with_field("moved_at", at.start())
            .with_field(
                "certainty",
                if certainty == Certainty::Definite {
                    "definite"
                } else {
                    "on some paths"
                },
            );
        self.diagnostics.push(diagnostic);
    }

    /// Moves the value at a place out, if it is affine.
    fn move_place(&mut self, expression: &Expression, state: &mut State) {
        let Some((place, segments)) = self.place_of(expression) else {
            return;
        };
        if self.place_type(&place, &segments).is_copy() {
            return;
        }
        // A borrow names someone else's value and cannot be moved from.
        if self
            .info(place.binding())
            .is_some_and(|info| info.barrier == Some(NonTransferable::Borrow))
        {
            return;
        }
        state.record_move(place, expression.span());
    }

    /// Reads then moves, for a position that takes ownership.
    fn consume(&mut self, expression: &Expression, state: &mut State) {
        self.walk_expression(expression, state);
        self.move_place(expression, state);
    }

    // ------------------------------------------------------------- borrows

    fn take_borrow(
        &mut self,
        operand: &Expression,
        kind: BorrowKind,
        region: Region,
        span: Span,
        state: &mut State,
    ) {
        self.walk_expression(operand, state);
        let Some((place, _)) = self.place_of(operand) else {
            return;
        };
        if let Some(existing) = state.conflicting_borrow(&place, kind) {
            let existing_place = existing.place.clone();
            let existing_kind = existing.kind;
            let existing_at = existing.at;
            let spelled = self.spell(&place);
            let existing_spelled = self.spell(&existing_place);
            let diagnostic = self
                .report("E1302_CONFLICTING_BORROW", Stage::Ownership, span)
                .with_field("place", spelled)
                .with_field("borrow", borrow_word(kind))
                .with_field("conflicts_with", borrow_word(existing_kind))
                .with_field("held_place", existing_spelled)
                .with_field("held_at", existing_at.start());
            self.diagnostics.push(diagnostic);
            return;
        }
        state.record_borrow(BorrowRecord {
            place,
            kind,
            at: span,
            region,
        });
    }

    /// Reports a write that lands on a shared-borrowed place.
    fn check_write(&mut self, target: &Expression, state: &mut State) {
        let Some((place, _)) = self.place_of(target) else {
            return;
        };
        let Some(existing) = state.shared_borrow_of(&place) else {
            return;
        };
        let held = existing.place.clone();
        let at = existing.at;
        let spelled = self.spell(&place);
        let held_spelled = self.spell(&held);
        let diagnostic = self
            .report(
                "E1303_MUTATE_WHILE_BORROWED",
                Stage::Ownership,
                target.span(),
            )
            .with_field("place", spelled)
            .with_field("borrowed_place", held_spelled)
            .with_field("borrowed_at", at.start());
        self.diagnostics.push(diagnostic);
    }

    // ------------------------------------------------------------ captures

    /// Checks a closure or spawned body against the capture rules.
    ///
    /// Only the body's genuinely free names are captures; a name the body
    /// declares itself is not. A captured binding that the body assigns needs a
    /// mutable alias of the outer value, which docs/40 section 6 makes
    /// non-transferable, so it is an invalid capture rather than a move.
    fn check_captures(&mut self, expression: &Expression, state: &mut State) {
        let Some(body) = expression.body() else {
            return;
        };
        let is_task = expression.form() == ExpressionForm::Spawn;
        let code = if is_task {
            "E1304_INVALID_TASK_CAPTURE"
        } else {
            "E1305_INVALID_CLOSURE_CAPTURE"
        };

        let mut declared: BTreeSet<String> = BTreeSet::new();
        for parameter in expression.parameters() {
            declared.insert(parameter.name().text(self.source).to_string());
        }
        let mut free: Vec<(String, Span)> = Vec::new();
        let mut assigned: BTreeSet<String> = BTreeSet::new();
        collect_free(self.source, body, &mut declared, &mut free, &mut assigned);

        let mut seen: BTreeSet<String> = BTreeSet::new();
        for (name, span) in free {
            if !seen.insert(name.clone()) {
                continue;
            }
            if self.capabilities.contains(&name) {
                let diagnostic = self
                    .report(code, Stage::Ownership, span)
                    .with_field("capture", name)
                    .with_field("reason", NonTransferable::Capability.reason());
                self.diagnostics.push(diagnostic);
                continue;
            }
            let Some(id) = self.resolve(&name) else {
                continue;
            };
            let barrier = self.info(id).and_then(|info| info.barrier);
            let mutably_aliased = assigned.contains(&name);
            let reason = if mutably_aliased {
                Some("mutable binding by alias")
            } else {
                barrier.map(NonTransferable::reason)
            };
            if let Some(reason) = reason {
                let diagnostic = self
                    .report(code, Stage::Ownership, span)
                    .with_field("capture", name)
                    .with_field("reason", reason);
                self.diagnostics.push(diagnostic);
                // An invalid capture is not a transfer, so ownership stays put
                // and no use-after-move follows from it.
                continue;
            }
            let place = Place::root(id);
            if let Some(record) = state.blocking_move(&place) {
                let at = record.at;
                let spelled = self.spell(&place);
                let diagnostic = self
                    .report("E1301_USE_AFTER_MOVE", Stage::Ownership, span)
                    .with_field("place", spelled)
                    .with_field("moved_at", at.start())
                    .with_field("certainty", "definite");
                self.diagnostics.push(diagnostic);
                continue;
            }
            // A permitted affine capture transfers ownership out of the scope.
            if self.info(id).is_some_and(|info| info.affine) {
                state.record_move(place, span);
            }
        }
    }

    // --------------------------------------------------------------- walks

    fn walk_block(&mut self, block: &Block, state: State) -> State {
        self.push_scope();
        let depth = self.scopes.len();
        let mut state = state;
        for statement in block.statements() {
            if !state.reachable {
                break;
            }
            state = self.walk_statement(statement, state);
        }
        state.end_block_borrows(depth);
        self.pop_scope(&mut state);
        state
    }

    fn walk_statement(&mut self, statement: &Statement, state: State) -> State {
        let mut state = state;
        match statement.form() {
            StatementForm::Let => {
                if let Some(expression) = statement.expression() {
                    state = self.walk_binding_initializer(expression, state);
                }
                state.end_statement_borrows();
                if let Some(pattern) = statement.pattern() {
                    let borrow_kind = statement
                        .expression()
                        .and_then(|expression| borrow_of(expression, self.source));
                    self.bind_pattern(pattern, borrow_kind, &mut state);
                }
                state
            }
            StatementForm::Assignment => {
                if let Some(expression) = statement.expression() {
                    self.consume(expression, &mut state);
                }
                if let Some(target) = statement.target() {
                    self.check_write(target, &mut state);
                    // Writing a place makes it whole again.
                    if let Some((place, _)) = self.place_of(target) {
                        state
                            .moves
                            .retain(|record| !place.is_prefix_of(&record.place));
                    }
                }
                state.end_statement_borrows();
                state
            }
            StatementForm::Return => {
                if let Some(expression) = statement.expression() {
                    self.consume(expression, &mut state);
                }
                state.end_statement_borrows();
                State::unreachable()
            }
            StatementForm::Break | StatementForm::Continue => State::unreachable(),
            StatementForm::Expression | StatementForm::Cancel => {
                if let Some(expression) = statement.expression() {
                    self.walk_expression(expression, &mut state);
                }
                state.end_statement_borrows();
                state
            }
            StatementForm::If => self.walk_if(statement, state),
            StatementForm::Match => self.walk_match(statement, state),
            StatementForm::While | StatementForm::For | StatementForm::Loop => {
                self.walk_loop(statement, state)
            }
            StatementForm::Parallel | StatementForm::Defer | StatementForm::Unsafe => {
                match statement.body() {
                    Some(body) => self.walk_block(body, state),
                    None => state,
                }
            }
        }
    }

    /// A `let` initializer that is a borrow binds the borrow itself.
    fn walk_binding_initializer(&mut self, expression: &Expression, state: State) -> State {
        let mut state = state;
        match borrow_of(expression, self.source) {
            Some(kind) => {
                // A named borrow lives for the block that scopes the name.
                let region = Region::Block(self.scopes.len());
                if let Some(operand) = expression.inner() {
                    self.take_borrow(operand, kind, region, expression.span(), &mut state);
                }
            }
            None => self.consume(expression, &mut state),
        }
        state
    }

    fn bind_pattern(
        &mut self,
        pattern: &Pattern,
        borrow_kind: Option<BorrowKind>,
        state: &mut State,
    ) {
        match pattern.form() {
            PatternForm::Name if !pattern.is_qualified() => {
                if let Some(name) = pattern.name() {
                    let barrier = borrow_kind.map(|_| NonTransferable::Borrow);
                    self.declare(name, barrier);
                    let _ = state;
                }
            }
            PatternForm::Destructure | PatternForm::Tuple => {
                for element in pattern.elements() {
                    self.bind_pattern(element, borrow_kind, state);
                }
            }
            _ => {}
        }
    }

    fn walk_if(&mut self, statement: &Statement, state: State) -> State {
        let mut entry = state;
        if let Some(head) = statement.expression() {
            self.walk_expression(head, &mut entry);
        }
        entry.end_statement_borrows();

        let taken = match statement.body() {
            Some(body) => self.walk_block(body, entry.clone()),
            None => entry.clone(),
        };
        let alternative = if let Some(block) = statement.else_body() {
            self.walk_block(block, entry)
        } else if let Some(nested) = statement.else_if() {
            self.walk_statement(nested, entry)
        } else {
            // Without an else the condition may simply be false.
            entry
        };
        State::join(taken, alternative)
    }

    fn walk_match(&mut self, statement: &Statement, state: State) -> State {
        let mut entry = state;
        if let Some(head) = statement.expression() {
            // docs/40 section 5: patterns bind by move unless the subject is an
            // immutable Copy value, so the subject itself is consumed.
            self.consume(head, &mut entry);
        }
        entry.end_statement_borrows();

        let mut joined: Option<State> = None;
        for branch in statement.branches() {
            self.push_scope();
            self.bind_pattern(branch.pattern(), None, &mut entry.clone());
            let outcome = self.walk_block(branch.body(), entry.clone());
            let mut outcome = outcome;
            self.pop_scope(&mut outcome);
            joined = Some(match joined {
                Some(existing) => State::join(existing, outcome),
                None => outcome,
            });
        }
        joined.unwrap_or(entry)
    }

    /// Analyses a loop body twice so a move inside it is seen by the next
    /// iteration.
    ///
    /// Moves only accumulate, so joining the first pass's result back into the
    /// entry state and running once more reaches the fixed point. Only the
    /// second pass reports, which keeps the diagnostics deterministic and
    /// unduplicated.
    fn walk_loop(&mut self, statement: &Statement, state: State) -> State {
        let mut entry = state;
        if let Some(head) = statement.expression() {
            self.walk_expression(head, &mut entry);
        }
        entry.end_statement_borrows();
        let Some(body) = statement.body() else {
            return entry;
        };

        self.push_scope();
        if let Some(pattern) = statement.pattern() {
            self.bind_pattern(pattern, None, &mut entry.clone());
        }

        let suppressed = self.diagnostics.len();
        let first = self.walk_block(body, entry.clone());
        self.diagnostics.truncate(suppressed);
        let repeated = State::join(entry.clone(), first);

        let mut outcome = self.walk_block(body, repeated);
        self.pop_scope(&mut outcome);
        // A `while`/`for` head may fail immediately, so the entry state also
        // reaches the exit; a bare `loop` leaves only through `break`.
        State::join(entry, outcome)
    }

    fn walk_expression(&mut self, expression: &Expression, state: &mut State) {
        match expression.form() {
            ExpressionForm::Name | ExpressionForm::Field | ExpressionForm::Index => {
                self.read_place(expression, state);
                // An index expression still evaluates its index.
                if expression.form() == ExpressionForm::Index {
                    if let Some(index) = expression.right() {
                        self.walk_expression(index, state);
                    }
                }
                return;
            }
            ExpressionForm::Group => {
                if let Some(inner) = expression.inner() {
                    self.walk_expression(inner, state);
                }
                return;
            }
            ExpressionForm::Unary => {
                if let Some(kind) = borrow_of(expression, self.source) {
                    if let Some(operand) = expression.inner() {
                        self.take_borrow(
                            operand,
                            kind,
                            Region::Statement,
                            expression.span(),
                            state,
                        );
                    }
                    return;
                }
            }
            ExpressionForm::Call => {
                self.walk_call(expression, state);
                return;
            }
            ExpressionForm::Tuple | ExpressionForm::Array => {
                for element in expression.elements() {
                    self.consume(element, state);
                }
                return;
            }
            ExpressionForm::Closure | ExpressionForm::Spawn => {
                self.check_captures(expression, state);
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
            self.walk_expression(child, state);
        }
    }

    fn walk_call(&mut self, expression: &Expression, state: &mut State) {
        if let Some(callee) = expression.callee() {
            if callee.form() != ExpressionForm::Name {
                self.walk_expression(callee, state);
            }
        }
        // Every argument position takes ownership unless it is written as a
        // borrow, which the parser records as a unary operand.
        for argument in expression.arguments() {
            self.consume(argument.value(), state);
        }
    }
}

/// The borrow an expression takes, if it is one.
fn borrow_of(expression: &Expression, source: &SourceUnit) -> Option<BorrowKind> {
    if expression.form() != ExpressionForm::Unary {
        return None;
    }
    match expression.operator_text(source)? {
        "borrow" => Some(BorrowKind::Shared),
        "borrow mut" => Some(BorrowKind::Mutable),
        _ => None,
    }
}

fn borrow_word(kind: BorrowKind) -> &'static str {
    match kind {
        BorrowKind::Shared => "borrow",
        BorrowKind::Mutable => "borrow mut",
    }
}

/// Whether a type may not cross a task or closure boundary.
fn barrier_of(ty: &Type) -> Option<NonTransferable> {
    match ty {
        Type::Constructed(name, _) => match name.as_str() {
            "Mutex" | "RwLock" => Some(NonTransferable::LockGuard),
            "Region" | "DmaRegion" => Some(NonTransferable::Region),
            _ => None,
        },
        _ => None,
    }
}

/// The constant value of an index expression, when it has one.
fn constant_index(expression: &Expression, source: &SourceUnit) -> Option<i128> {
    if expression.form() != ExpressionForm::Literal {
        return None;
    }
    let text = expression.span().text(source);
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Collects the names a body uses freely, and those it assigns.
fn collect_free(
    source: &SourceUnit,
    block: &Block,
    declared: &mut BTreeSet<String>,
    free: &mut Vec<(String, Span)>,
    assigned: &mut BTreeSet<String>,
) {
    for statement in block.statements() {
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            free_in_expression(source, expression, declared, free);
        }
        if statement.form() == StatementForm::Assignment {
            if let Some(target) = statement.target() {
                if let Some(root) = root_name(source, target) {
                    if !declared.contains(&root) {
                        assigned.insert(root);
                    }
                }
            }
        }
        if statement.form() == StatementForm::Let {
            if let Some(pattern) = statement.pattern() {
                declare_pattern(source, pattern, declared);
            }
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            collect_free(source, nested, declared, free, assigned);
        }
        for branch in statement.branches() {
            declare_pattern(source, branch.pattern(), declared);
            collect_free(source, branch.body(), declared, free, assigned);
        }
    }
}

fn declare_pattern(source: &SourceUnit, pattern: &Pattern, declared: &mut BTreeSet<String>) {
    match pattern.form() {
        PatternForm::Name if !pattern.is_qualified() => {
            if let Some(name) = pattern.name() {
                declared.insert(name.text(source).to_string());
            }
        }
        PatternForm::Destructure | PatternForm::Tuple => {
            for element in pattern.elements() {
                declare_pattern(source, element, declared);
            }
        }
        _ => {}
    }
}

fn root_name(source: &SourceUnit, expression: &Expression) -> Option<String> {
    match expression.form() {
        ExpressionForm::Name => Some(expression.span().text(source).to_string()),
        ExpressionForm::Field | ExpressionForm::Index | ExpressionForm::Group => {
            root_name(source, expression.inner()?)
        }
        _ => None,
    }
}

fn free_in_expression(
    source: &SourceUnit,
    expression: &Expression,
    declared: &BTreeSet<String>,
    free: &mut Vec<(String, Span)>,
) {
    if expression.form() == ExpressionForm::Name {
        let name = expression.span().text(source).to_string();
        if !declared.contains(&name) {
            free.push((name, expression.span()));
        }
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
        free_in_expression(source, child, declared, free);
    }
    for argument in expression.arguments() {
        free_in_expression(source, argument.value(), declared, free);
    }
    for element in expression.elements() {
        free_in_expression(source, element, declared, free);
    }
    if let Some(body) = expression.body() {
        let mut inner = declared.clone();
        for parameter in expression.parameters() {
            inner.insert(parameter.name().text(source).to_string());
        }
        let mut assigned = BTreeSet::new();
        collect_free(source, body, &mut inner, free, &mut assigned);
    }
}
