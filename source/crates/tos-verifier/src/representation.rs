// SPDX-License-Identifier: GPL-3.0-or-later
//! The verifier's own closed capability-representation table (ADR-0085 §7a).
//!
//! `SYSTEM_INTERFACE_V1` §4.3 separates an interface's identity from the class
//! of TOS Core values that represents it, and a verifier checking a capability
//! position has to know that mapping. **Where it may not come from** is the
//! whole point of this file:
//!
//! - not the frontend. This crate depends on `tos-ir` and on nothing else
//!   (docs/43 §5), and reading `tos_core::interfaces` would make the verifier's
//!   conclusion depend on the producer's own table;
//! - not a callback, for the same reason;
//! - not the artifact. A representation a producer wrote down is a claim, and
//!   the verifier's job is to check claims rather than to hold them;
//! - not `Instruction::unsafe_interface`, which is the claim being checked.
//!
//! So it is a table of this crate's own, and
//! `scripts/tests/check-interface-schema.sh` pairs it against the accepted
//! schema line for line — the same gate that already pairs the frontend's
//! operations, object kinds, ABI assignments and capability requirements
//! against it. The two tables cannot drift from the document, or from each
//! other, without that gate going red.
//!
//! **It is not a second operation table.** It carries at most one row per
//! non-default representation and nothing else: no operation names, no
//! parameter types, no rights, no ABI numbers.

/// The closed set of capability representations ADR-0085 fixes.
///
/// **Closed is the control.** An open relation between an interface and the
/// value families that satisfy it would let a schema edit widen what may occupy
/// a capability position with no decision behind it; a named member of an
/// enumeration makes each widening an ADR of ADR-0085's weight. Nothing outside
/// this type is a representation — not an arbitrary nominal type, not a
/// program-defined record, not a structural shape, not an erased handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Representation {
    /// `TypeDef::Capability(path)`, exactly. The default, and what every
    /// interface accepted before ADR-0085 has.
    AsInterface,
    /// `TypeDef::DmaRegion(_)` or `TypeDef::DmaRegionMut(_)`, any element type.
    DmaRegionFamily,
}

/// One interface whose representation is not the default, as this crate knows
/// it.
pub struct Represented {
    /// The interface path an accepted schema declares.
    pub interface: &'static str,
    /// The family that represents it.
    pub representation: Representation,
}

/// Every interface whose representation is **not** the default.
///
/// One row, and the shape of the table is the cardinality rule: a family
/// appears at most once, so `interface_of` below is a function rather than a
/// search that could find two answers. `SYSTEM_INTERFACE_V1` §4.3 rule 1 is
/// what makes that safe to rely on, and it is why deriving an interface from an
/// operand's type is possible at all.
pub const REPRESENTED: &[Represented] = &[Represented {
    interface: "platform.dma.Region",
    representation: Representation::DmaRegionFamily,
}];
