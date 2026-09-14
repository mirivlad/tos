// SPDX-License-Identifier: GPL-3.0-or-later
//! The authoritative TOS Core V1 predeclared-call contract (ADR-0088).
//!
//! `docs/39` §2 fixes a closed namespace of predeclared functions, and every
//! component that meets one of those calls needs the same five facts about it:
//! its name, the language minor it exists in, how many arguments it takes, what
//! those arguments must be, and what it produces. Before this crate each
//! component knew some of them and none knew all: the checker typed `to_u8`'s
//! result and never looked at its argument, the lowerer kept a second list of
//! the same names for the same purpose, the minor gate kept a third, and the
//! independent verifier held none at all — `CallTarget::Predeclared` was an
//! empty branch, so a forged artifact could call `to_u8` with three operands of
//! any types and claim any result.
//!
//! This crate is **data and nothing else**. It says that `share` takes one
//! operand satisfying the Shareable rule and produces `Shared<T>`; it does not
//! decide whether a particular type is Shareable, does not perform a
//! conversion, does not touch a device and does not know what a diagnostic is.
//! Each consumer evaluates the rules against its own representation of types —
//! the frontend against source types, the verifier against `tos-ir` type
//! definitions — and neither takes the other's word for the answer.
//!
//! **`docs/43` §5 is what permits the verifier to depend on it**:
//!
//! > A shared declarative type/interface table may be used only if its content
//! > digest is input to both components; no frontend callback participates in
//! > verifier acceptance.
//!
//! So the table has a canonical [`Contract::digest`], and each consumer states
//! the digest it was built against as its own constant and proves the equality
//! in its own test. A table that changed under either component would fail that
//! component's build rather than silently give two implementations two
//! contracts.
//!
//! **Operations that lower to their own IR operation are in the table too**,
//! and are marked as such by [`Form`]. Their specialised verifier obligations —
//! ADR-0081 §8 for device memory, ADR-0086 for the DMA ordering points,
//! ADR-0037 for `share` — belong to those operation families and are not
//! replaced here. What the table owns about them is the *source call*: the
//! name, the minor, the arity, the argument rules and the result.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

use tos_hash::Sha256;
use tos_ir::{DmaSyncDirection, IntKind};

/// What a predeclared operation becomes in `tos-ir/v1`.
///
/// A call form is not a free choice: `docs/43` §3 forbids hiding a shared-memory
/// access behind an opaque call, ADR-0081 §8 requires a device access to be its
/// own verifier-visible operation because an ordinary call may be eliminated or
/// duplicated and a device access may not, and ADR-0086 §4 requires the same of
/// an ordering point for the narrower reason that a `Call` is exactly the thing
/// a backend is free to inline away. So the representation is part of the
/// contract, and an artifact that writes one of these as a `Call` is malformed
/// rather than merely unusual.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Form {
    /// `Op::Call` with `CallTarget::Predeclared`.
    Call,
    /// `Op::Share` (ADR-0037 §4).
    Share,
    /// `Op::MmioRead` or `Op::MmioWrite` of this width and byte order
    /// (ADR-0081 §7).
    ///
    /// **Both fields are here so the table describes the whole IR form.** The
    /// independent verifier has to decide whether an instruction's
    /// `width`/`little_endian` pair is one an accepted operation produces, and
    /// a second width table kept in the verifier would be the duplication this
    /// crate exists to remove.
    Mmio {
        /// Bytes moved, which is also the alignment the offset must satisfy.
        width: u8,
        /// Whether this is a write, which requires the mutable region form.
        writes: bool,
        /// The byte order of the transaction. A single byte has no byte order;
        /// the flag is recorded as little-endian there too, because that is the
        /// form every accepted artifact carries and a verifier that admitted
        /// both would be admitting an artifact no operation produces.
        little_endian: bool,
    },
    /// `Op::DmaSync` in this direction (ADR-0086 §3).
    DmaSync(DmaSyncDirection),
}

/// What one argument position must be.
///
/// Declarative on purpose: the rule names a class of types, and the component
/// that holds a type decides whether it is in the class. A rule that named one
/// concrete type identifier could not describe `wrapping_add`, whose second
/// operand's type is decided by its first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterRule {
    /// Any exact fixed-width integer type. `size` is not one: `docs/40` §3 gives
    /// the wrapping contracts "exact fixed-width type arguments", and `size` is
    /// a target-ABI width that portable source may not assume.
    Integer,
    /// Any exact fixed-width integer type, or `size`. The checked conversions
    /// accept both (`docs/40` §3: "any fixed-width integer or `size`").
    IntegerOrSize,
    /// Exactly `size`, as every bounded offset and index in this language is.
    Size,
    /// Exactly this integer type.
    Exactly(IntKind),
    /// The same exact integer type as the operand at this position, which is
    /// always an earlier one.
    SameAs(usize),
    /// A readable device-memory mapping: `MmioRegion` or `MmioRegionMut`.
    MmioRegion,
    /// A writable device-memory mapping: `MmioRegionMut` alone. A read-only
    /// grant is read-only in the type as well as in the page table.
    MmioRegionMut,
    /// Either DMA region constructor, `DmaRegion<T>` or `DmaRegion<mut T>`
    /// (ADR-0086 §3). The rule is nominal and over the closed family.
    DmaRegion,
    /// A value satisfying the accepted Shareable rule: transitively immutable,
    /// and of a region kind that admits sharing (ADR-0037 §4). The rule is
    /// named here and evaluated by each component over its own types.
    Shareable,
}

/// What a predeclared operation produces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultRule {
    /// `unit`.
    Unit,
    /// Exactly this integer type.
    Integer(IntKind),
    /// `Result<D, ConversionError>` for the spelled destination `D`.
    Conversion(IntKind),
    /// Exactly the type of the operand at this position.
    SameAsOperand(usize),
    /// `Shared<T>` where `T` is the type of the operand at this position.
    SharedOfOperand(usize),
}

/// One predeclared callable, stated once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Operation {
    /// The name `docs/39` §2 spells. It is not a reserved word: a module may
    /// declare a function of the same name, and that declaration wins, so a
    /// consumer consults this table only for a name its own module does not
    /// declare.
    pub name: &'static str,
    /// The TOS Core minor in which this operation exists (`docs/42` §1). A
    /// module receives the language its header claims, so a call of an
    /// operation above the declared minor is refused however capable the
    /// implementation that meets it.
    pub minimum_minor: u32,
    /// The feature this operation belongs to, as the frontend's
    /// `E1608_FEATURE_REQUIRES_LANGUAGE_MINOR` names it. Read only where
    /// `minimum_minor` is above the module's.
    pub feature: &'static str,
    /// The exact ordered argument list. Its length is the exact arity.
    pub parameters: &'static [ParameterRule],
    /// The result, which the frontend gives the lowered instruction and the
    /// verifier rechecks against the artifact.
    pub result: ResultRule,
    /// What the operation becomes in the IR.
    pub form: Form,
}

impl Operation {
    /// The exact arity, which is the length of the parameter list and never
    /// anything else.
    pub fn arity(&self) -> usize {
        self.parameters.len()
    }
}

const CONVERSION: &str = "checked conversion";
const WRAPPING: &str = "wrapping arithmetic";
const SHARING: &str = "sharing";
const DEVICE_MEMORY: &str = "device memory";
const DMA_ORDERING: &str = "DMA ordering";

/// The minor in which device memory became part of the language (ADR-0081 §6).
const DEVICE_MEMORY_MINOR: u32 = 2;
/// The minor in which the DMA ordering points became part of it (ADR-0086 §13).
const DMA_ORDERING_MINOR: u32 = 4;

/// A checked conversion: one operand of any exact integer type or `size`,
/// answering `Result<D, ConversionError>` (`docs/40` §3).
const fn conversion(name: &'static str, destination: IntKind) -> Operation {
    Operation {
        name,
        minimum_minor: 0,
        feature: CONVERSION,
        parameters: &[ParameterRule::IntegerOrSize],
        result: ResultRule::Conversion(destination),
        form: Form::Call,
    }
}

/// A wrapping contract: two operands of one exact fixed-width integer type,
/// answering that type (`docs/40` §3).
const fn wrapping(name: &'static str) -> Operation {
    Operation {
        name,
        minimum_minor: 0,
        feature: WRAPPING,
        parameters: &[ParameterRule::Integer, ParameterRule::SameAs(0)],
        result: ResultRule::SameAsOperand(0),
        form: Form::Call,
    }
}

/// A device-memory read: the region and a byte offset, answering `u64`
/// (ADR-0081 §7). The result is `u64` at every width because the width belongs
/// to the transaction rather than to the value's type.
const fn mmio_read(name: &'static str, width: u8) -> Operation {
    Operation {
        name,
        minimum_minor: DEVICE_MEMORY_MINOR,
        feature: DEVICE_MEMORY,
        parameters: &[ParameterRule::MmioRegion, ParameterRule::Size],
        result: ResultRule::Integer(IntKind::U64),
        form: Form::Mmio {
            width,
            writes: false,
            little_endian: true,
        },
    }
}

/// A device-memory write: the mutable region, a byte offset and the value
/// (ADR-0081 §7, amended).
///
/// **The value is exact `u64` at every width**, which is the read's rule read
/// backwards and is now the accepted carrier rule rather than an inference:
/// ADR-0081 §7's amendment states that every V1 MMIO read returns exact `u64`
/// and every write takes exact `u64`, and that the hardware transaction width
/// is decided by the operation and its verifier-visible IR width and never by
/// the integer type of the carrier value.
const fn mmio_write(name: &'static str, width: u8) -> Operation {
    Operation {
        name,
        minimum_minor: DEVICE_MEMORY_MINOR,
        feature: DEVICE_MEMORY,
        parameters: &[
            ParameterRule::MmioRegionMut,
            ParameterRule::Size,
            ParameterRule::Exactly(IntKind::U64),
        ],
        result: ResultRule::Unit,
        form: Form::Mmio {
            width,
            writes: true,
            little_endian: true,
        },
    }
}

/// A DMA ordering point: one region, no value (ADR-0086 §3).
const fn dma(name: &'static str, direction: DmaSyncDirection) -> Operation {
    Operation {
        name,
        minimum_minor: DMA_ORDERING_MINOR,
        feature: DMA_ORDERING,
        parameters: &[ParameterRule::DmaRegion],
        result: ResultRule::Unit,
        form: Form::DmaSync(direction),
    }
}

/// The complete predeclared-function namespace of `docs/39` §2, in the order
/// that document lists it.
///
/// The order is part of the canonical content: the digest is taken over the
/// table as written, so reordering it is a content change and says so.
pub const OPERATIONS: &[Operation] = &[
    conversion("to_i8", IntKind::I8),
    conversion("to_i16", IntKind::I16),
    conversion("to_i32", IntKind::I32),
    conversion("to_i64", IntKind::I64),
    conversion("to_u8", IntKind::U8),
    conversion("to_u16", IntKind::U16),
    conversion("to_u32", IntKind::U32),
    conversion("to_u64", IntKind::U64),
    wrapping("wrapping_add"),
    wrapping("wrapping_sub"),
    wrapping("wrapping_mul"),
    Operation {
        name: "share",
        minimum_minor: 0,
        feature: SHARING,
        parameters: &[ParameterRule::Shareable],
        result: ResultRule::SharedOfOperand(0),
        form: Form::Share,
    },
    mmio_read("mmio_read_u8", 1),
    mmio_read("mmio_read_le_u16", 2),
    mmio_read("mmio_read_le_u32", 4),
    mmio_read("mmio_read_le_u64", 8),
    mmio_write("mmio_write_u8", 1),
    mmio_write("mmio_write_le_u16", 2),
    mmio_write("mmio_write_le_u32", 4),
    mmio_write("mmio_write_le_u64", 8),
    dma("dma_publish", DmaSyncDirection::Publish),
    dma("dma_consume", DmaSyncDirection::Consume),
];

/// What a name means to a module declaring a given minor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lookup {
    /// No predeclared operation has this name, at any minor.
    Unknown,
    /// The operation exists but not in this module's language
    /// (`E1608_FEATURE_REQUIRES_LANGUAGE_MINOR`, `docs/42` §1).
    RequiresMinor(&'static Operation),
    /// The operation is available here.
    Available(&'static Operation),
}

/// The predeclared contract a module declaring TOS Core 1.`minor` receives.
///
/// A module receives the language its header claims (`docs/42` §1), so the
/// contract is selected by the artifact's or source's own declared version and
/// never by what the implementation happens to support.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Contract {
    minor: u32,
}

impl Contract {
    /// The contract at one declared minor.
    pub const fn at_minor(minor: u32) -> Contract {
        Contract { minor }
    }

    /// The declared minor this contract was selected by.
    pub const fn minor(&self) -> u32 {
        self.minor
    }

    /// What a name means here.
    pub fn lookup(&self, name: &str) -> Lookup {
        match operation(name) {
            None => Lookup::Unknown,
            Some(operation) if operation.minimum_minor > self.minor => {
                Lookup::RequiresMinor(operation)
            }
            Some(operation) => Lookup::Available(operation),
        }
    }

    /// The operations this minor has, in table order.
    pub fn operations(&self) -> impl Iterator<Item = &'static Operation> + '_ {
        OPERATIONS
            .iter()
            .filter(move |operation| operation.minimum_minor <= self.minor)
    }

    /// The canonical content digest of this contract (`docs/43` §5).
    ///
    /// Over the selected minor and every operation it admits, each field
    /// length-prefixed so no two different tables can encode the same bytes.
    /// A consumer states the digest it was built against and proves the
    /// equality in its own test; that is what makes this table shared rather
    /// than duplicated.
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"tos-predeclared/v1\n");
        hasher.update(&self.minor.to_le_bytes());
        for operation in self.operations() {
            text(&mut hasher, operation.name);
            hasher.update(&operation.minimum_minor.to_le_bytes());
            text(&mut hasher, operation.feature);
            form(&mut hasher, operation.form);
            hasher.update(&(operation.parameters.len() as u32).to_le_bytes());
            for rule in operation.parameters {
                parameter_rule(&mut hasher, *rule);
            }
            result_rule(&mut hasher, operation.result);
        }
        hasher.finalize()
    }
}

/// The operation of this name, whatever minor it belongs to.
pub fn operation(name: &str) -> Option<&'static Operation> {
    OPERATIONS.iter().find(|operation| operation.name == name)
}

/// The device access an IR `width`/`writes`/`little_endian` triple names, if
/// any accepted operation produces it (ADR-0081 §7).
///
/// **The question an independent verifier actually has.** It meets an
/// `Op::MmioRead` or `Op::MmioWrite` carrying a width and a byte order and must
/// decide whether that combination is one the language has an operation for —
/// a 3-byte access, a big-endian one, or a 16-byte one is not a device access
/// the contract admits, whatever else is right about the instruction.
pub fn mmio_operation(width: u8, writes: bool, little_endian: bool) -> Option<&'static Operation> {
    OPERATIONS.iter().find(|operation| {
        operation.form
            == Form::Mmio {
                width,
                writes,
                little_endian,
            }
    })
}

fn text(hasher: &mut Sha256, value: &str) {
    hasher.update(&(value.len() as u32).to_le_bytes());
    hasher.update(value.as_bytes());
}

/// An integer type by its **spelling**, so the digest describes the contract
/// rather than the host enum's discriminant order.
fn int(hasher: &mut Sha256, kind: IntKind) {
    text(hasher, kind.spelled());
}

fn form(hasher: &mut Sha256, form: Form) {
    match form {
        Form::Call => hasher.update(&[0]),
        Form::Share => hasher.update(&[1]),
        Form::Mmio {
            width,
            writes,
            little_endian,
        } => {
            hasher.update(&[2, width, u8::from(writes), u8::from(little_endian)]);
        }
        Form::DmaSync(direction) => {
            hasher.update(&[3]);
            text(hasher, direction.spelled());
        }
    }
}

fn parameter_rule(hasher: &mut Sha256, rule: ParameterRule) {
    match rule {
        ParameterRule::Integer => hasher.update(&[0]),
        ParameterRule::IntegerOrSize => hasher.update(&[1]),
        ParameterRule::Size => hasher.update(&[2]),
        ParameterRule::Exactly(kind) => {
            hasher.update(&[3]);
            int(hasher, kind);
        }
        ParameterRule::SameAs(position) => {
            hasher.update(&[4]);
            hasher.update(&(position as u32).to_le_bytes());
        }
        ParameterRule::MmioRegion => hasher.update(&[5]),
        ParameterRule::MmioRegionMut => hasher.update(&[6]),
        ParameterRule::DmaRegion => hasher.update(&[7]),
        ParameterRule::Shareable => hasher.update(&[8]),
    }
}

fn result_rule(hasher: &mut Sha256, rule: ResultRule) {
    match rule {
        ResultRule::Unit => hasher.update(&[0]),
        ResultRule::Integer(kind) => {
            hasher.update(&[1]);
            int(hasher, kind);
        }
        ResultRule::Conversion(kind) => {
            hasher.update(&[2]);
            int(hasher, kind);
        }
        ResultRule::SameAsOperand(position) => {
            hasher.update(&[3]);
            hasher.update(&(position as u32).to_le_bytes());
        }
        ResultRule::SharedOfOperand(position) => {
            hasher.update(&[4]);
            hasher.update(&(position as u32).to_le_bytes());
        }
    }
}

/// The canonical digest of the contract at each accepted minor, as hexadecimal.
///
/// Held here so the table's own crate states its content identity; each
/// consumer states the same values independently and proves them against
/// [`Contract::digest`] in its own test, which is what `docs/43` §5 asks for.
pub const ACCEPTED_DIGESTS: [(u32, &str); 5] = [
    (
        0,
        "849bd9de6a7c0c485ffbcc972791a5768c724a4165d67087fd1961e863f7f555",
    ),
    (
        1,
        "26848cb863d7727c9b6787e141da156df418ea57af0471863a5767fe9f170bb3",
    ),
    (
        2,
        "c5e8ffbb3467768e15a235b57c56bdde1a359cb707ad5f33ee45f721b357c2a5",
    ),
    (
        3,
        "a4bad47aac6cdb361ced38352fde05acfedb47d4a6c331b8cd4762fd3daec02c",
    ),
    (
        4,
        "9d496d76b8e8b25157547fe6a5d81fb3c32c0dc7f331d3e19779ee1e2271a17c",
    ),
];

/// The digest of a contract, as lowercase hexadecimal.
pub fn hex_digest(contract: &Contract) -> [u8; 64] {
    let digest = contract.digest();
    let mut out = [0u8; 64];
    tos_hash::hex(&digest, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(minor: u32) -> std::string::String {
        let contract = Contract::at_minor(minor);
        let bytes = hex_digest(&contract);
        std::string::String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// The whole of `docs/39` §2's predeclared-function inventory, and nothing
    /// else. The list is repeated here rather than derived from the table, so
    /// that a name added to one has to be added to the other deliberately.
    #[test]
    fn the_table_is_the_documented_namespace() {
        let expected = [
            "to_i8",
            "to_i16",
            "to_i32",
            "to_i64",
            "to_u8",
            "to_u16",
            "to_u32",
            "to_u64",
            "wrapping_add",
            "wrapping_sub",
            "wrapping_mul",
            "share",
            "mmio_read_u8",
            "mmio_read_le_u16",
            "mmio_read_le_u32",
            "mmio_read_le_u64",
            "mmio_write_u8",
            "mmio_write_le_u16",
            "mmio_write_le_u32",
            "mmio_write_le_u64",
            "dma_publish",
            "dma_consume",
        ];
        let actual: std::vec::Vec<&str> = OPERATIONS.iter().map(|o| o.name).collect();
        assert_eq!(actual, expected);
    }

    /// Every `SameAs` and every result rule names a position the operation has,
    /// and a `SameAs` names an **earlier** one — otherwise a consumer resolving
    /// the rules left to right would have to guess.
    #[test]
    fn every_rule_names_a_position_that_exists() {
        for operation in OPERATIONS {
            for (position, rule) in operation.parameters.iter().enumerate() {
                if let ParameterRule::SameAs(other) = rule {
                    assert!(*other < position, "{} rule {position}", operation.name);
                }
            }
            match operation.result {
                ResultRule::SameAsOperand(position) | ResultRule::SharedOfOperand(position) => {
                    assert!(position < operation.arity(), "{}", operation.name);
                }
                _ => {}
            }
        }
    }

    /// A later minor admits everything an earlier one did.
    #[test]
    fn minors_are_cumulative() {
        for minor in 1..=4 {
            let earlier: std::vec::Vec<&str> = Contract::at_minor(minor - 1)
                .operations()
                .map(|o| o.name)
                .collect();
            let later: std::vec::Vec<&str> = Contract::at_minor(minor)
                .operations()
                .map(|o| o.name)
                .collect();
            for name in earlier {
                assert!(later.contains(&name), "1.{minor} lost {name}");
            }
        }
    }

    /// 1.2 is where device memory arrives and 1.4 where the ordering points do.
    #[test]
    fn a_minor_admits_exactly_what_its_decision_added() {
        assert_eq!(Contract::at_minor(0).operations().count(), 12);
        assert_eq!(Contract::at_minor(1).operations().count(), 12);
        assert_eq!(Contract::at_minor(2).operations().count(), 20);
        assert_eq!(Contract::at_minor(3).operations().count(), 20);
        assert_eq!(Contract::at_minor(4).operations().count(), 22);
        assert!(matches!(
            Contract::at_minor(3).lookup("dma_publish"),
            Lookup::RequiresMinor(_)
        ));
        assert!(matches!(
            Contract::at_minor(4).lookup("dma_publish"),
            Lookup::Available(_)
        ));
        assert!(matches!(
            Contract::at_minor(1).lookup("mmio_read_u8"),
            Lookup::RequiresMinor(_)
        ));
        assert!(matches!(
            Contract::at_minor(4).lookup("nope"),
            Lookup::Unknown
        ));
    }

    /// The digest is the contract's identity, so it is pinned here and read
    /// independently by every consumer.
    #[test]
    fn the_digest_is_the_recorded_one() {
        for (minor, expected) in ACCEPTED_DIGESTS {
            assert_eq!(hex(minor), expected, "1.{minor}");
        }
    }

    /// And it is a digest of the *content*: a table with one rule changed has a
    /// different one. Proved by digesting a deliberately different minor's
    /// selection rather than by mutating the constant table.
    #[test]
    fn different_content_digests_differently() {
        assert_ne!(hex(0), hex(2));
        assert_ne!(hex(2), hex(4));
    }
}
