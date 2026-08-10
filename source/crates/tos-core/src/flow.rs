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
//! The lattice is small and monotone: a move, once recorded, is never removed,
//! and a borrow live on either path is live after the join. A use is rejected
//! when a move reaches it on *any* path, so `Definite` and `Maybe` differ only
//! in what the diagnostic says, never in whether it fires.
//!
//! **Layering.** This is TOS Core frontend semantic state, not a
//! language-neutral executable representation. Ownership, borrows and
//! `Transferable` are rules of the safe TOS Core language; they are proof the
//! frontend produces, not a precondition for a program to be representable at
//! all. A later frontend for another language may not satisfy them, and the
//! isolation TOS guarantees any process — address space, capabilities, granted
//! regions, verifier and runtime contract — is a separate layer that does not
//! depend on these types. Nothing here may migrate into a shared IR or
//! executable boundary as a mandatory condition.

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
    /// Whether control can actually arrive here. A `return`, `break` or
    /// `continue` makes the rest of its block unreachable, and an unreachable
    /// path contributes nothing to a join.
    pub(crate) reachable: bool,
    pub(crate) moves: Vec<MoveRecord>,
    pub(crate) borrows: Vec<BorrowRecord>,
}

impl State {
    pub(crate) fn entry() -> State {
        State {
            reachable: true,
            moves: Vec::new(),
            borrows: Vec::new(),
        }
    }

    pub(crate) fn unreachable() -> State {
        State {
            reachable: false,
            moves: Vec::new(),
            borrows: Vec::new(),
        }
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
    /// An unreachable path contributes nothing. A move present on one side
    /// becomes `Maybe`, which still blocks a later use; a borrow live on either
    /// side stays live, because the checker must not lose a borrow that some
    /// path leaves open.
    pub(crate) fn join(one: State, other: State) -> State {
        if !one.reachable {
            return other;
        }
        if !other.reachable {
            return one;
        }
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
        State {
            reachable: true,
            moves,
            borrows,
        }
    }
}
