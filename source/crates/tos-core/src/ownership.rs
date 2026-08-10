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

use crate::flow::{join_option, BorrowKind, BorrowRecord, Certainty, Flow, Region, State};
use crate::parser::{
    Block, BorrowMode, Expression, ExpressionForm, ImportKind, Pattern, PatternForm, Schema, Span,
    Statement, StatementForm,
};
use crate::place::{BindingId, Place, Segment};
use crate::typing::{binding_types, record_fields, Type};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

/// Why a value may not cross a task or closure boundary (docs/40 section 6).
///
/// Only the two the frontend can establish on its own are listed. docs/41
/// distinguishes a synchronization object such as `Mutex<T>` from the affine
/// guard a lock operation yields, and it is the guard that may not transfer, so
/// the object's type constructor proves nothing. Likewise docs/40 section 6
/// makes a `Region<T>`'s shareability and mutability a fact of its capability
/// contract, which this slice cannot see: inventing a diagnostic from the type
/// constructor alone would be a guess.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NonTransferable {
    Borrow,
    Capability,
}

impl NonTransferable {
    fn reason(self) -> &'static str {
        match self {
            NonTransferable::Borrow => "borrow",
            NonTransferable::Capability => "non-transferable capability",
        }
    }
}

/// A guard on loop iteration. The lattice is monotone and finite, so the fixed
/// point is reached well before this; the bound only stops a lattice mistake
/// from becoming a hang.
const MAX_LOOP_ROUNDS: usize = 16;

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
        let state = State::entry();
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
        let _ = checker.walk_block(function.body(), state);
        checker.scopes.pop();
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

    fn declare(&mut self, name: Span, barrier: Option<NonTransferable>) {
        let id = name.start();
        let ty = self.types.get(&id).cloned().unwrap_or(Type::Unknown);
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

    /// Evaluates the base and index expressions of an assignment target.
    ///
    /// docs/40 section 4 evaluates them left to right before the right side.
    /// The target place itself is not read: an assignment writes it rather than
    /// using its old value.
    fn evaluate_place_address(&mut self, target: &Expression, state: &mut State) {
        match target.form() {
            ExpressionForm::Name => {}
            ExpressionForm::Group => {
                if let Some(inner) = target.inner() {
                    self.evaluate_place_address(inner, state);
                }
            }
            ExpressionForm::Field => {
                if let Some(inner) = target.inner() {
                    self.evaluate_place_address(inner, state);
                }
            }
            ExpressionForm::Index => {
                if let Some(inner) = target.inner() {
                    self.evaluate_place_address(inner, state);
                }
                if let Some(index) = target.right() {
                    self.walk_expression(index, state);
                }
            }
            _ => self.walk_expression(target, state),
        }
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
        collect_free(self.source, body, &declared, &mut free, &mut assigned);

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

    fn walk_block(&mut self, block: &Block, state: State) -> Flow {
        self.push_scope();
        let depth = self.scopes.len();
        let mut flow = Flow::normal(state);
        for statement in block.statements() {
            let Some(current) = flow.normal.take() else {
                // Everything after a break, continue or return is unreachable.
                break;
            };
            let next = self.walk_statement(statement, current);
            flow = Flow {
                normal: next.normal,
                breaks: join_option(flow.breaks, next.breaks),
                continues: join_option(flow.continues, next.continues),
                returns: join_option(flow.returns, next.returns),
            };
        }
        let ids: Vec<BindingId> = self
            .scopes
            .last()
            .map(|scope| scope.iter().map(|(_, id)| *id).collect())
            .unwrap_or_default();
        for state in [
            &mut flow.normal,
            &mut flow.breaks,
            &mut flow.continues,
            &mut flow.returns,
        ]
        .into_iter()
        .flatten()
        {
            state.end_block_borrows(depth);
            state.forget(&ids);
        }
        self.scopes.pop();
        flow
    }

    fn walk_statement(&mut self, statement: &Statement, state: State) -> Flow {
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
                Flow::normal(state)
            }
            StatementForm::Assignment => {
                // docs/40 section 4: the place base and index evaluate first,
                // left to right, then the right side, then the write happens.
                if let Some(target) = statement.target() {
                    self.evaluate_place_address(target, &mut state);
                }
                if let Some(expression) = statement.expression() {
                    self.consume(expression, &mut state);
                }
                if let Some(target) = statement.target() {
                    self.check_write(target, &mut state);
                    // Writing a place gives it a value again.
                    if let Some((place, _)) = self.place_of(target) {
                        state
                            .moves
                            .retain(|record| !place.is_prefix_of(&record.place));
                    }
                }
                state.end_statement_borrows();
                Flow::normal(state)
            }
            StatementForm::Return => {
                if let Some(expression) = statement.expression() {
                    self.consume(expression, &mut state);
                }
                state.end_statement_borrows();
                Flow::returning(state)
            }
            StatementForm::Break => Flow::breaking(state),
            StatementForm::Continue => Flow::continuing(state),
            StatementForm::Expression | StatementForm::Cancel => {
                if let Some(expression) = statement.expression() {
                    self.walk_expression(expression, &mut state);
                }
                state.end_statement_borrows();
                Flow::normal(state)
            }
            StatementForm::If => self.walk_if(statement, state),
            StatementForm::Match => self.walk_match(statement, state),
            StatementForm::While | StatementForm::For | StatementForm::Loop => {
                self.walk_loop(statement, state)
            }
            StatementForm::Parallel | StatementForm::Unsafe => match statement.body() {
                Some(body) => self.walk_block(body, state),
                None => Flow::normal(state),
            },
            // docs/40 section 5 registers a `defer` body now and runs it at
            // scope exit. Which ownership state it observes there — what it
            // captures, whether its cleanup consumes, and what the enclosing
            // scope may still use in between — is not stated, so this slice
            // analyses nothing inside it rather than choosing a semantics.
            StatementForm::Defer => Flow::normal(state),
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

    fn walk_if(&mut self, statement: &Statement, state: State) -> Flow {
        let mut entry = state;
        if let Some(head) = statement.expression() {
            self.walk_expression(head, &mut entry);
        }
        entry.end_statement_borrows();

        let taken = match statement.body() {
            Some(body) => self.walk_block(body, entry.clone()),
            None => Flow::normal(entry.clone()),
        };
        let alternative = if let Some(block) = statement.else_body() {
            self.walk_block(block, entry)
        } else if let Some(nested) = statement.else_if() {
            self.walk_statement(nested, entry)
        } else {
            // Without an else the condition may simply be false.
            Flow::normal(entry)
        };
        Flow::join(taken, alternative)
    }

    fn walk_match(&mut self, statement: &Statement, state: State) -> Flow {
        let mut entry = state;
        if let Some(head) = statement.expression() {
            // docs/40 section 5: patterns bind by move unless the subject is an
            // immutable Copy value, so the subject itself is consumed.
            self.consume(head, &mut entry);
        }
        entry.end_statement_borrows();

        let mut joined: Option<Flow> = None;
        for branch in statement.branches() {
            self.push_scope();
            self.bind_pattern(branch.pattern(), None, &mut entry.clone());
            let outcome = self.walk_block(branch.body(), entry.clone());
            self.scopes.pop();
            joined = Some(match joined {
                Some(existing) => Flow::join(existing, outcome),
                None => outcome,
            });
        }
        joined.unwrap_or_else(|| Flow::normal(entry))
    }

    /// Solves a loop by iterating its body to a stable entry state.
    ///
    /// The lattice is monotone — a move is never removed and a borrow never
    /// dropped by a join — and a body mentions finitely many places, so
    /// repeatedly folding the back edge into the entry state reaches a fixed
    /// point. The iteration runs silently and stops when the entry state stops
    /// changing, rather than assuming some fixed number of passes; a bound on
    /// the number of rounds guards against a lattice mistake turning into a
    /// hang. Only the final pass reports, so diagnostics stay deterministic and
    /// unduplicated.
    ///
    /// The back edge carries both normal completion and `continue`. The exit
    /// carries `break`, plus the entry state itself for `while` and `for`,
    /// whose head may fail on the first evaluation; a bare `loop` has no
    /// zero-iteration exit and leaves only through `break`.
    fn walk_loop(&mut self, statement: &Statement, state: State) -> Flow {
        let mut entry = state;
        if let Some(head) = statement.expression() {
            self.walk_expression(head, &mut entry);
        }
        entry.end_statement_borrows();
        let Some(body) = statement.body() else {
            return Flow::normal(entry);
        };
        let may_skip = statement.form() != StatementForm::Loop;

        self.push_scope();
        if let Some(pattern) = statement.pattern() {
            self.bind_pattern(pattern, None, &mut entry.clone());
        }

        let mut current = entry.clone();
        for _ in 0..MAX_LOOP_ROUNDS {
            let suppressed = self.diagnostics.len();
            let outcome = self.walk_block(body, current.clone());
            self.diagnostics.truncate(suppressed);
            let back = join_option(outcome.normal, outcome.continues);
            let next = match back {
                Some(back) => State::join(current.clone(), back),
                None => current.clone(),
            };
            if next.same_facts(&current) {
                break;
            }
            current = next;
        }

        let outcome = self.walk_block(body, current.clone());
        self.scopes.pop();

        let mut exit = outcome.breaks;
        if may_skip {
            exit = join_option(exit, Some(entry));
        }
        // `break` and `continue` inside this body belong to this loop, so they
        // do not escape; a `return` does.
        Flow {
            normal: exit,
            breaks: None,
            continues: None,
            returns: outcome.returns,
        }
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
            // docs/40 section 4: `&&` does not evaluate its right side after
            // false and `||` does not after true, so the right side is a
            // conditional path joined with the one that skipped it.
            ExpressionForm::Binary
                if matches!(
                    expression.operator_text(self.source),
                    Some("&&") | Some("||")
                ) =>
            {
                if let Some(left) = expression.left() {
                    self.walk_expression(left, state);
                }
                let mut taken = state.clone();
                if let Some(right) = expression.right() {
                    self.walk_expression(right, &mut taken);
                }
                *state = State::join(state.clone(), taken);
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
///
/// Declarations are lexical. A `let` is visible to the statements after it in
/// its own block, but a nested block, one `if` arm or one `match` arm gets a
/// child environment, so a name it declares never hides an outer capture in a
/// sibling arm or after the construct ends.
fn collect_free(
    source: &SourceUnit,
    block: &Block,
    declared: &BTreeSet<String>,
    free: &mut Vec<(String, Span)>,
    assigned: &mut BTreeSet<String>,
) {
    let mut scope = declared.clone();
    for statement in block.statements() {
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            free_in_expression(source, expression, &scope, free);
        }
        if statement.form() == StatementForm::Assignment {
            if let Some(target) = statement.target() {
                if let Some(root) = root_name(source, target) {
                    if !scope.contains(&root) {
                        assigned.insert(root);
                    }
                }
            }
        }
        // A `let` takes effect after its own statement, within this block only.
        if statement.form() == StatementForm::Let {
            if let Some(pattern) = statement.pattern() {
                declare_pattern(source, pattern, &mut scope);
            }
        }
        // A loop pattern scopes to its body, not to the rest of this block.
        let mut nested_scope = scope.clone();
        if statement.form() == StatementForm::For {
            if let Some(pattern) = statement.pattern() {
                declare_pattern(source, pattern, &mut nested_scope);
            }
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            collect_free(source, nested, &nested_scope, free, assigned);
        }
        if let Some(chained) = statement.else_if() {
            free_in_statement(source, chained, &scope, free, assigned);
        }
        for branch in statement.branches() {
            let mut arm = scope.clone();
            declare_pattern(source, branch.pattern(), &mut arm);
            collect_free(source, branch.body(), &arm, free, assigned);
        }
    }
}

/// Collects from one statement, for an `else if` continuation.
fn free_in_statement(
    source: &SourceUnit,
    statement: &Statement,
    declared: &BTreeSet<String>,
    free: &mut Vec<(String, Span)>,
    assigned: &mut BTreeSet<String>,
) {
    for expression in [statement.target(), statement.expression()]
        .into_iter()
        .flatten()
    {
        free_in_expression(source, expression, declared, free);
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        collect_free(source, nested, declared, free, assigned);
    }
    if let Some(chained) = statement.else_if() {
        free_in_statement(source, chained, declared, free, assigned);
    }
    for branch in statement.branches() {
        let mut arm = declared.clone();
        declare_pattern(source, branch.pattern(), &mut arm);
        collect_free(source, branch.body(), &arm, free, assigned);
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
        // A nested closure sees the enclosing environment plus its own
        // parameters; whatever it uses freely is also free here.
        let mut inner = declared.clone();
        for parameter in expression.parameters() {
            inner.insert(parameter.name().text(source).to_string());
        }
        let mut assigned = BTreeSet::new();
        collect_free(source, body, &inner, free, &mut assigned);
    }
}
