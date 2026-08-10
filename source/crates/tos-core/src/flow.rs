// SPDX-License-Identifier: GPL-3.0-or-later
//! Ownership and borrow state, and how it flows through structured control.
//!
//! TOS Core has no `goto`, so a general control-flow graph is not needed: every
//! branch point is an `if`, a `match` or a loop, and each one is analysed by
//! running its alternatives from the *same* entry state and joining the
//! results. That is what makes
//!
//! ```tos
//! if (ready) { take(message); } else { take(message); }
//! ```
//!
//! correct — the second arm starts from the entry state, not from whatever the
//! first arm left behind.
//!
//! Control leaves a statement through one of four channels — normal
//! fallthrough, `break`, `continue`, `return` — and they must not be collapsed
//! into a single reachability flag. A `break` carries its state to the loop's
//! exit, a `continue` carries it to the loop's back edge, and a `return`
//! carries it out of the return scope; losing any of them would silently drop
//! the facts that path established.
//!
//! The lattice is small and monotone: a move, once recorded, is never removed,
//! and a borrow live on either path is live after the join. A use is rejected
//! when a move reaches it on *any* path, so `Definite` and `Maybe` differ only
//! in what the diagnostic says, never in whether it fires. Monotone and finite
//! is what lets a loop be solved by iterating to stability rather than by
//! assuming a fixed number of passes.
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

use std::vec::Vec;

use crate::parser::Span;
use crate::place::{BindingId, Place};

/// How certainly a move has happened at a program point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Certainty {
    /// Moved on every path that reaches here.
    Definite,
    /// Moved on at least one path that reaches here.
    Maybe,
}

impl Certainty {
    fn join(self, other: Certainty) -> Certainty {
        if self == Certainty::Definite && other == Certainty::Definite {
            Certainty::Definite
        } else {
            Certainty::Maybe
        }
    }
}

/// One place that has been moved out of.
#[derive(Clone, Debug)]
pub(crate) struct MoveRecord {
    pub(crate) place: Place,
    pub(crate) at: Span,
    pub(crate) certainty: Certainty,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BorrowKind {
    Shared,
    Mutable,
}

/// One borrow that is live at a program point.
#[derive(Clone, Debug)]
pub(crate) struct BorrowRecord {
    pub(crate) place: Place,
    pub(crate) kind: BorrowKind,
    pub(crate) at: Span,
    /// The scope depth this borrow ends with. A borrow bound to a name lives
    /// for that name's block; a temporary lives for its statement.
    pub(crate) region: Region,
}

/// Where a borrow stops being live.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Region {
    /// Ends when the enclosing statement finishes.
    Statement,
    /// Ends when the block at this depth is left.
    Block(usize),
}

/// Ownership and borrow facts at one program point.
#[derive(Clone, Debug, Default)]
pub(crate) struct State {
    pub(crate) moves: Vec<MoveRecord>,
    pub(crate) borrows: Vec<BorrowRecord>,
}

/// How control left a statement or block.
///
/// Each channel holds the state of the paths that leave that way, or `None`
/// when no path does. A loop consumes the `break` and `continue` channels of
/// its own body; a return scope consumes `returns`.
#[derive(Clone, Debug, Default)]
pub(crate) struct Flow {
    pub(crate) normal: Option<State>,
    pub(crate) breaks: Option<State>,
    pub(crate) continues: Option<State>,
    pub(crate) returns: Option<State>,
}

impl Flow {
    pub(crate) fn normal(state: State) -> Flow {
        Flow {
            normal: Some(state),
            ..Flow::default()
        }
    }

    pub(crate) fn breaking(state: State) -> Flow {
        Flow {
            breaks: Some(state),
            ..Flow::default()
        }
    }

    pub(crate) fn continuing(state: State) -> Flow {
        Flow {
            continues: Some(state),
            ..Flow::default()
        }
    }

    pub(crate) fn returning(state: State) -> Flow {
        Flow {
            returns: Some(state),
            ..Flow::default()
        }
    }

    /// Joins two alternative flows channel by channel.
    pub(crate) fn join(one: Flow, other: Flow) -> Flow {
        Flow {
            normal: join_option(one.normal, other.normal),
            breaks: join_option(one.breaks, other.breaks),
            continues: join_option(one.continues, other.continues),
            returns: join_option(one.returns, other.returns),
        }
    }
}

pub(crate) fn join_option(one: Option<State>, other: Option<State>) -> Option<State> {
    match (one, other) {
        (Some(left), Some(right)) => Some(State::join(left, right)),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

impl State {
    pub(crate) fn entry() -> State {
        State {
            moves: Vec::new(),
            borrows: Vec::new(),
        }
    }

    /// Whether two states hold the same facts, for deciding loop stability.
    pub(crate) fn same_facts(&self, other: &State) -> bool {
        if self.moves.len() != other.moves.len() || self.borrows.len() != other.borrows.len() {
            return false;
        }
        self.moves.iter().all(|record| {
            other
                .moves
                .iter()
                .any(|mine| mine.place == record.place && mine.certainty == record.certainty)
        }) && self.borrows.iter().all(|record| {
            other
                .borrows
                .iter()
                .any(|mine| mine.place == record.place && mine.kind == record.kind)
        })
    }

    /// The first recorded move that makes using `place` invalid.
    ///
    /// A use is invalid when the moved place contains it — the value is gone —
    /// or when it contains the moved place, because docs/40 section 5 allows a
    /// partially moved aggregate to be used only to move or drop its untouched
    /// fields, never as a whole.
    pub(crate) fn blocking_move(&self, place: &Place) -> Option<&MoveRecord> {
        self.moves
            .iter()
            .find(|record| record.place.overlaps(place))
    }

    /// Records a move, keeping the earliest one for a place.
    pub(crate) fn record_move(&mut self, place: Place, at: Span) {
        if self
            .moves
            .iter()
            .any(|record| record.place == place && record.certainty == Certainty::Definite)
        {
            return;
        }
        self.moves.push(MoveRecord {
            place,
            at,
            certainty: Certainty::Definite,
        });
    }

    /// Live borrows that conflict with taking `kind` of `place`.
    pub(crate) fn conflicting_borrow(
        &self,
        place: &Place,
        kind: BorrowKind,
    ) -> Option<&BorrowRecord> {
        self.borrows.iter().find(|record| {
            record.place.overlaps(place)
                && (kind == BorrowKind::Mutable || record.kind == BorrowKind::Mutable)
        })
    }

    /// A live shared borrow that a write to `place` would invalidate.
    pub(crate) fn shared_borrow_of(&self, place: &Place) -> Option<&BorrowRecord> {
        self.borrows
            .iter()
            .find(|record| record.kind == BorrowKind::Shared && record.place.overlaps(place))
    }

    pub(crate) fn record_borrow(&mut self, record: BorrowRecord) {
        self.borrows.push(record);
    }

    /// Ends every borrow whose region has closed.
    pub(crate) fn end_statement_borrows(&mut self) {
        self.borrows
            .retain(|record| record.region != Region::Statement);
    }

    pub(crate) fn end_block_borrows(&mut self, depth: usize) {
        self.borrows
            .retain(|record| record.region != Region::Block(depth));
    }

    /// Drops every fact about bindings that have gone out of scope.
    pub(crate) fn forget(&mut self, bindings: &[BindingId]) {
        self.moves
            .retain(|record| !bindings.contains(&record.place.binding()));
        self.borrows
            .retain(|record| !bindings.contains(&record.place.binding()));
    }

    /// Joins two alternative paths.
    ///
    /// A move present on one side becomes `Maybe`, which still blocks a later
    /// use; a borrow live on either side stays live, because the checker must
    /// not lose a borrow that some path leaves open. Unreachability is carried
    /// by [`Flow`], so a path that cannot arrive never reaches this function.
    pub(crate) fn join(one: State, other: State) -> State {
        let mut moves: Vec<MoveRecord> = Vec::new();
        for record in one.moves.iter().chain(other.moves.iter()) {
            if let Some(existing) = moves
                .iter_mut()
                .find(|existing| existing.place == record.place)
            {
                existing.certainty = existing.certainty.join(record.certainty);
                continue;
            }
            let both = one.moves.iter().any(|left| left.place == record.place)
                && other.moves.iter().any(|right| right.place == record.place);
            moves.push(MoveRecord {
                place: record.place.clone(),
                at: record.at,
                certainty: if both {
                    record.certainty
                } else {
                    Certainty::Maybe
                },
            });
        }
        let mut borrows = one.borrows;
        for record in other.borrows {
            if borrows
                .iter()
                .any(|existing| existing.place == record.place && existing.kind == record.kind)
            {
                continue;
            }
            borrows.push(record);
        }
        State { moves, borrows }
    }
}
