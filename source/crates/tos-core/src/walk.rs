// SPDX-License-Identifier: GPL-3.0-or-later
//! Walking an expression tree without spending stack on its length.
//!
//! A left-associative chain is as **deep** as it is **long**. `a + b + c + d`
//! parses as `(((a + b) + c) + d)`, so a walk that recurses into `left` recurses
//! once per operand — and a source unit inside every published limit can hold
//! tens of thousands of them.
//!
//! That is not a depth the source declares. docs/44 section 2 bounds delimiter
//! nesting at 256 and states that the published limits exist to "prevent
//! attacker-controlled recursion"; a flat operator chain nests nothing, so it
//! must not consume stack in proportion to its length. Measured before this
//! module existed: a conforming 256 KiB unit whose body is one chain of
//! additions aborted the process with a stack overflow in the typing, ownership,
//! mutability and guard slices and in lowering. An abort is not a diagnostic,
//! and a frontend that aborts has not rejected anything.
//!
//! So the rule this module exists to keep is: **recursion may follow syntax
//! that genuinely nests, and nothing else.** Blocks nest inside braces, groups
//! inside parentheses, closure bodies inside their delimiters — all bounded by
//! the delimiter-nesting limit, and all still walked recursively here. The
//! operand chain of an operator run is walked with an explicit worklist.
//!
//! [`walk_expression`] serves the visitors that only look. The walks that
//! compute a value from an operand chain — typing, lowering — unroll the chain
//! themselves, because what they carry up out of it is not a visit.

use alloc::vec::Vec;

use crate::parser::{Block, Expression};

/// Whether a visitor handled an expression itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Descend {
    /// Walk this expression's subexpressions, in source order.
    Children,
    /// Do not: the visitor dealt with the whole subtree. This is what an early
    /// `return` meant when these walks were recursive.
    Skip,
}

/// A visitor over an expression tree.
pub(crate) trait ExpressionWalk<'source> {
    /// Acts on one expression, and says whether to descend into it.
    fn expression(&mut self, expression: &'source Expression) -> Descend;

    /// Walks a block body reached from an expression — a closure or a spawn.
    ///
    /// Blocks nest only where the source nests, so this is the one place the
    /// walk is still allowed to recurse.
    fn block(&mut self, block: &'source Block);

    /// The block body to walk with this expression, if any.
    ///
    /// Overridden by visitors that handle particular bodies themselves.
    fn body_of(&mut self, expression: &'source Expression) -> Option<&'source Block> {
        expression.body()
    }

    /// Whether a body is walked before this expression's subexpressions rather
    /// than after. Two slices did it in that order and the order is theirs to
    /// keep — this walk replaces recursion, not behaviour.
    fn body_first(&self) -> bool {
        false
    }
}

/// One node of an expression tree, for the closure form of the walk.
pub(crate) enum Node<'source> {
    Expression(&'source Expression),
    Block(&'source Block),
}

/// The same walk, for visitors that are free functions rather than types.
///
/// One closure rather than two, because a visitor that borrows its diagnostic
/// list mutably cannot lend it to two.
pub(crate) fn walk_tree<'source, F>(root: &'source Expression, body_first: bool, visit: F)
where
    F: FnMut(Node<'source>) -> Descend,
{
    let mut adapter = Adapter { visit, body_first };
    walk_expression(&mut adapter, root);
}

struct Adapter<F> {
    visit: F,
    body_first: bool,
}

impl<'source, F> ExpressionWalk<'source> for Adapter<F>
where
    F: FnMut(Node<'source>) -> Descend,
{
    fn expression(&mut self, expression: &'source Expression) -> Descend {
        (self.visit)(Node::Expression(expression))
    }

    fn block(&mut self, block: &'source Block) {
        (self.visit)(Node::Block(block));
    }

    fn body_first(&self) -> bool {
        self.body_first
    }
}

/// One thing left to do. A block is deferred rather than walked on sight, so
/// that the order matches what the recursive form produced exactly: a node's
/// body came after its subexpressions and before its next sibling.
enum Step<'source> {
    Expression(&'source Expression),
    Block(&'source Block),
}

/// Visits `root` and every subexpression, in the order the recursive walk used.
///
/// The worklist is last-in first-out, so children are pushed in reverse: `left`
/// goes on last and therefore comes off first. A node's whole subtree is
/// processed before its right sibling, which is what makes this identical to the
/// recursion it replaces rather than merely similar to it.
pub(crate) fn walk_expression<'source, W>(walker: &mut W, root: &'source Expression)
where
    W: ExpressionWalk<'source>,
{
    let mut work: Vec<Step<'source>> = Vec::new();
    work.push(Step::Expression(root));
    while let Some(step) = work.pop() {
        let expression = match step {
            Step::Block(block) => {
                walker.block(block);
                continue;
            }
            Step::Expression(expression) => expression,
        };
        if walker.expression(expression) == Descend::Skip {
            continue;
        }
        let body = walker.body_of(expression);
        let body_first = walker.body_first();
        // Last pushed is first popped, so a body that must be walked *after* the
        // subexpressions goes on before them, and one that must be walked first
        // goes on after.
        if !body_first {
            if let Some(body) = body {
                work.push(Step::Block(body));
            }
        }
        for element in expression.elements().iter().rev() {
            work.push(Step::Expression(element));
        }
        for argument in expression.arguments().iter().rev() {
            work.push(Step::Expression(argument.value()));
        }
        for child in [
            expression.callee(),
            expression.inner(),
            expression.right(),
            expression.left(),
        ]
        .into_iter()
        .flatten()
        {
            work.push(Step::Expression(child));
        }
        if body_first {
            if let Some(body) = body {
                work.push(Step::Block(body));
            }
        }
    }
}

/// The operand chain of a binary run, outermost first.
///
/// `(((a + b) + c) + d)` yields the three `Binary` nodes and leaves the walk
/// pointing at `a`. Returns the chain and its innermost left operand, which is
/// `None` when the deepest node has no left side — a shape only a malformed
/// tree produces, and one every caller already had to answer for.
///
/// Callers fold **from the end of the chain forwards**, which is the order the
/// recursion evaluated in: innermost operand first, then each right operand
/// outwards.
pub(crate) fn binary_chain<'source>(
    expression: &'source Expression,
    is_chained: impl Fn(&'source Expression) -> bool,
) -> (Vec<&'source Expression>, Option<&'source Expression>) {
    operator_chain(expression, is_chained, Expression::left)
}

/// The operand chain of a prefix operator run, outermost first.
///
/// `!!!!b` is four `Unary` nodes deep and nests nothing a delimiter bounds, so
/// it is the same defect as an operator chain wearing different syntax.
pub(crate) fn prefix_chain<'source>(
    expression: &'source Expression,
    is_chained: impl Fn(&'source Expression) -> bool,
) -> (Vec<&'source Expression>, Option<&'source Expression>) {
    operator_chain(expression, is_chained, Expression::inner)
}

fn operator_chain<'source>(
    expression: &'source Expression,
    is_chained: impl Fn(&'source Expression) -> bool,
    next: impl Fn(&'source Expression) -> Option<&'source Expression>,
) -> (Vec<&'source Expression>, Option<&'source Expression>) {
    let mut chain = Vec::new();
    let mut node = expression;
    loop {
        if !is_chained(node) {
            return (chain, Some(node));
        }
        chain.push(node);
        match next(node) {
            Some(operand) => node = operand,
            None => return (chain, None),
        }
    }
}
