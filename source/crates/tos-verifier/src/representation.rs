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

use tos_ir::TypeDef;

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

/// How a capability of this interface is represented.
///
/// Absence means the default, which is exactly `SYSTEM_INTERFACE_V1` §4.3 rule
/// 5: an interface with no non-default row has the version 1 semantics.
pub fn representation_of(interface: &str) -> Representation {
    match REPRESENTED.iter().find(|row| row.interface == interface) {
        Some(row) => row.representation,
        None => Representation::AsInterface,
    }
}

/// Whether an `import capability` could produce a value of this
/// representation (ADR-0085 §4a).
///
/// An import is typed `TypeDef::Capability(interface)`. `AsInterface` is
/// importable because an import is exactly that type; `DmaRegionFamily` is not,
/// because no import can be. Derived rather than declared, so there is no
/// second flag to keep in step.
pub fn startup_importable(representation: Representation) -> bool {
    matches!(representation, Representation::AsInterface)
}

/// The interface a value of this type represents, if the type is a member of a
/// non-default representation family.
///
/// **This is the half of the derivation the artifact cannot supply.** Given the
/// operand type from the module's own type table, it answers which interface —
/// and only ever one, because a family belongs to at most one interface. A
/// `TypeDef::Capability` is not answered here: that case is the default
/// representation and the derivation handles it directly.
pub fn interface_of(ty: &TypeDef) -> Option<&'static str> {
    let family = match ty {
        TypeDef::DmaRegion(_) | TypeDef::DmaRegionMut(_) => Representation::DmaRegionFamily,
        _ => return None,
    };
    REPRESENTED
        .iter()
        .find(|row| row.representation == family)
        .map(|row| row.interface)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rule 1, over this table: a family belongs to at most one interface.
    ///
    /// It is what makes `interface_of` a function rather than a search with
    /// more than one answer, and the derivation in `lib.rs` relies on that being
    /// true — so it is proved here rather than assumed from the table being
    /// short today.
    #[test]
    fn a_family_belongs_to_at_most_one_interface() {
        for (at, row) in REPRESENTED.iter().enumerate() {
            assert!(
                !REPRESENTED[..at]
                    .iter()
                    .any(|earlier| earlier.representation == row.representation),
                "{} shares a representation family with an earlier interface",
                row.interface
            );
        }
    }

    /// And the default is what absence means (rule 5), which is the whole of
    /// why this table carries the non-default rows only.
    #[test]
    fn an_interface_with_no_row_is_as_interface() {
        assert_eq!(
            representation_of("system.ipc.Endpoint"),
            Representation::AsInterface
        );
        assert_eq!(
            representation_of("platform.pci.FunctionConfig"),
            Representation::AsInterface
        );
        // Including a path no accepted schema declares. The caller has already
        // refused those; answering the default here means this table never
        // decides whether an interface exists.
        assert_eq!(
            representation_of("nothing.declares.This"),
            Representation::AsInterface
        );
        assert_eq!(
            representation_of("platform.dma.Region"),
            Representation::DmaRegionFamily
        );
    }

    /// Importability follows from the representation and from nothing else
    /// (ADR-0085 §4a): there is no per-interface flag to disagree with it.
    #[test]
    fn only_the_default_representation_is_startup_importable() {
        assert!(startup_importable(Representation::AsInterface));
        assert!(!startup_importable(Representation::DmaRegionFamily));
    }

    /// The two arms of the derivation ADR-0085 §7 adds, and everything that is
    /// not one of them.
    #[test]
    fn the_region_family_derives_its_one_interface_and_nothing_else_does() {
        assert_eq!(
            interface_of(&TypeDef::DmaRegion(0)),
            Some("platform.dma.Region")
        );
        assert_eq!(
            interface_of(&TypeDef::DmaRegionMut(0)),
            Some("platform.dma.Region")
        );
        // The element type is not part of the family membership: `any T`.
        assert_eq!(
            interface_of(&TypeDef::DmaRegion(7)),
            Some("platform.dma.Region")
        );
        // And nothing else is a member of any family — an ordinary region, a
        // device window, a scalar, or a capability of the very interface this
        // family represents. The last is the one that matters: the interface's
        // own path is **not** a member of the family that represents it.
        for outside in [
            TypeDef::Region(0),
            TypeDef::RegionMut(0),
            TypeDef::MmioRegion,
            TypeDef::MmioRegionMut,
            TypeDef::Unit,
            TypeDef::Capability(alloc::string::String::from("platform.dma.Region")),
        ] {
            assert_eq!(interface_of(&outside), None, "{outside:?} is not a family");
        }
    }
}
