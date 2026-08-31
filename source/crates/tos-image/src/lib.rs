// SPDX-License-Identifier: GPL-3.0-or-later
//! The bounded, versioned encoding of a `tos-ir/v1` module (ADR-0070).
//!
//! A module image is **untrusted input to the verifier**, not a verified
//! output. It is produced before verification and treated as hostile bytes
//! until the verifier has read it:
//!
//! ```text
//! source -> lower -> untrusted compact encoding -> verifier -> receipt -> engine
//! ```
//!
//! The parser belongs to the verifier path: it reconstructs semantic
//! `tos-ir/v1` from bytes and the verifier then computes the versioned semantic
//! digest **from the reconstruction**, never from the bytes it was handed and
//! never from a value the image carried. ADR-0070 §3 versions the storage
//! encoding independently of the semantic digest scheme for that reason.
//!
//! ## What this format promises (docs/43 §1)
//!
//! - a **magic** that identifies the format;
//! - an **encoding version**, independent of the semantic schema, so a reader
//!   knows how to interpret before it knows what it holds;
//! - a **schema version**, saying which semantic schema the payload claims;
//! - **explicit length and table bounds**, checked before any allocation sized
//!   from them — the parser never sizes a read from a number it has not
//!   bounded;
//! - **canonical rules**: one encoding per value, so two encoders that agree on
//!   the meaning agree on the bytes;
//! - **unknown-version and unknown-tag behaviour, failing closed** — a reader
//!   that meets something it does not know refuses rather than skipping;
//! - an **artifact digest** over the bytes themselves, distinct from the
//!   semantic digest of the module;
//! - **totality**: the parser returns for every input, allocates nothing
//!   unbounded, and reads nothing past its slice.
//!
//! ## Coverage
//!
//! **Every** `tos-ir/v1` tagged variant round-trips: all 36 `TypeDef`, all 25
//! `Op`, all six `Terminator`, both `Operand`, all four `PlaceStep`, all three
//! `CallTarget`, all seven `Constant`, and the closed families —
//! `IntKind`, `BinaryOp`, `UnaryOp`, `BorrowKind`, `AtomicOp`, `MemoryOrder`,
//! `LockMode`, `ResourceKind`, `NominalKind`, `Visibility`, `PassMode`,
//! `FunctionOrigin`, `Profile` — together with the header, imports, capability
//! imports, exports, constants, functions and source map.
//!
//! ## Canonical rules
//!
//! - every integer is a minimal-length varint; a non-minimal encoding is
//!   refused rather than accepted and normalized;
//! - the string table is sorted by byte value and free of duplicates;
//! - the source-map identity table is sorted by its encoded tuple and free of
//!   duplicates;
//! - the payload length is exact: trailing bytes after the digest are refused.
//!
//! Encoding the same module twice produces the same bytes, and every image this
//! encoder writes is one its parser accepts.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(test)]
extern crate std;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;

use tos_ir::{
    AtomicOp, BinaryOp, Block, BorrowKind, CallTarget, CapabilityImport, CleanupCall, Constant,
    Function, FunctionOrigin, Header, Import, Instruction, IntKind, LockMode, MemoryOrder, Module,
    NominalKind, Op, Operand, Parameter, PassMode, Place, PlaceStep, Profile, ResourceEnvelope,
    ResourceKind, Signature, SourceMapEntry, Terminator, TypeDef, UnaryOp, Variant, Visibility,
};

mod parse;
mod write;

pub use parse::parse;
pub use write::encode;

/// The magic. Eight bytes, and not the experimental prototype's.
pub const MAGIC: [u8; 8] = *b"TOSIMAGE";

/// The container's own version. Independent of the semantic schema: a new
/// spelling of the same modules raises this and leaves identity alone.
pub const ENCODING_VERSION: u32 = 2;

/// Which semantic schema the payload claims. `1` is `tos-ir/v1`.
pub const SCHEMA_VERSION: u32 = 1;

/// Magic, encoding version, schema version and payload length.
pub const FRAME_HEADER: usize = 8 + 4 + 4 + 8;

/// The artifact digest, sha-256 over everything before it.
pub const DIGEST_BYTES: usize = 32;

/// The largest image this reader will consider, before any allocation is sized
/// from a number the bytes supplied.
pub const MAX_IMAGE_BYTES: usize = 512 * 1024 * 1024;

/// The largest string table this reader will consider. `tos-ir/v1` publishes no
/// limit on distinct strings, so the format declares one rather than inheriting
/// a bound it does not have.
pub const MAX_STRINGS: usize = 4 * 1024 * 1024;

/// The longest operand list, place path or arm list this reader will consider,
/// for the same reason.
pub const MAX_OPERANDS: usize = 65_536;

/// 128 bits at seven bits a byte.
pub const MAX_VARINT_BYTES: usize = 19;

/// The bounds the parser checks a table count against, before allocating.
///
/// **Data, not a dependency.** The numbers are the verifier's — docs/44 §2
/// publishes them and `tos_verifier::Limits` is where they are declared — but a
/// format that reads untrusted bytes has no business depending on the verifier
/// that will read what it produces. The verifier hands these down; this crate
/// never reaches up for them.
///
/// There is deliberately no `Default`. A caller that has not said what its
/// limits are has not said what it will accept, and a parser that guessed would
/// be publishing a ceiling nobody declared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseLimits {
    /// Entries in any one table of the module.
    pub table_entries: usize,
    /// Modules in the dependency closure.
    pub modules: usize,
    /// Fields or variants a nominal type may declare.
    pub fields: usize,
    /// Parameters a function may declare.
    pub parameters: usize,
    /// Basic blocks in one function.
    pub blocks_per_function: usize,
    /// Instructions in one basic block.
    pub instructions_per_block: usize,
    /// Source-map entries in a module.
    pub source_map_entries: usize,
}

/// Why an image was refused, or why one could not be written.
///
/// Every variant is a refusal. There is no "recovered", no "skipped unknown
/// field" and no partial success: a reader that meets something it does not
/// know stops.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImageError {
    /// Fewer bytes than the structure at this position requires.
    Truncated(&'static str),
    /// The first eight bytes are not this container's.
    BadMagic,
    /// A container version this reader does not implement.
    UnknownEncodingVersion(u32),
    /// A semantic schema this reader does not implement.
    UnknownSchemaVersion(u32),
    /// A declared length past what this reader will allocate for.
    Oversized { declared: u128 },
    /// Bytes after the artifact digest.
    TrailingBytes(usize),
    /// The artifact digest does not cover these bytes.
    WrongDigest,
    /// A varint longer than the minimal encoding of its value.
    NonCanonicalVarint,
    /// A varint that does not fit in 128 bits.
    VarintOverflow,
    /// A table whose entries are not in the canonical order, or repeat.
    NonCanonicalTable(&'static str),
    /// A discriminant this reader does not know, in the family named.
    UnknownTag { family: &'static str, tag: u8 },
    /// A reference outside a table the container itself introduced.
    OutOfRange { what: &'static str },
    /// A count past the declared limit for its table.
    CountExceedsLimit {
        what: &'static str,
        count: u128,
        limit: usize,
    },
    /// A string that is not UTF-8.
    BadUtf8,
    /// A value too large for this host's `usize`.
    IndexOverflow,
}

/// The artifact digest of a framed image, as `sha256:<hex>`.
///
/// Distinct from the module's semantic digest: this one says *which bytes*, and
/// the semantic digest says *which module*. A receipt binds to both.
pub fn artifact_digest(image: &[u8]) -> String {
    let digest = tos_hash::sha256(image);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    alloc::format!(
        "sha256:{}",
        core::str::from_utf8(&hex).expect("hex output is ASCII")
    )
}

/// Wraps a payload in the container frame, sealing it with its artifact digest.
///
/// Public because negative tests need well-framed hostile payloads: an attacker
/// who controls the bytes controls the digest too, so a digest is integrity and
/// never authenticity, and the payload parser must stand on its own.
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(FRAME_HEADER + payload.len() + DIGEST_BYTES);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&ENCODING_VERSION.to_be_bytes());
    bytes.extend_from_slice(&SCHEMA_VERSION.to_be_bytes());
    bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&tos_hash::sha256(&bytes));
    bytes
}

/// Recomputes and replaces the artifact digest of an otherwise intact image.
///
/// The hostile case a mutation sweep needs: bytes changed *and* resealed, so
/// what is tested is the payload parser rather than the digest.
pub fn reseal(image: &mut [u8]) {
    if image.len() < FRAME_HEADER + DIGEST_BYTES {
        return;
    }
    let split = image.len() - DIGEST_BYTES;
    let digest = tos_hash::sha256(&image[..split]);
    image[split..].copy_from_slice(&digest);
}

/// The payload of a framed image, after the frame's own checks.
fn unframe(image: &[u8]) -> Result<&[u8], ImageError> {
    if image.len() < FRAME_HEADER + DIGEST_BYTES {
        return Err(ImageError::Truncated("frame"));
    }
    if image[..8] != MAGIC {
        return Err(ImageError::BadMagic);
    }
    let encoding = u32::from_be_bytes([image[8], image[9], image[10], image[11]]);
    if encoding != ENCODING_VERSION {
        return Err(ImageError::UnknownEncodingVersion(encoding));
    }
    let schema = u32::from_be_bytes([image[12], image[13], image[14], image[15]]);
    if schema != SCHEMA_VERSION {
        return Err(ImageError::UnknownSchemaVersion(schema));
    }
    let mut length = [0u8; 8];
    length.copy_from_slice(&image[16..24]);
    let declared = u64::from_be_bytes(length) as u128;
    // The declared length is bounded before it is used to slice anything, and
    // before the digest is computed over a range it names.
    if declared > MAX_IMAGE_BYTES as u128 {
        return Err(ImageError::Oversized { declared });
    }
    let declared = declared as usize;
    let available = image.len() - FRAME_HEADER - DIGEST_BYTES;
    if declared > available {
        return Err(ImageError::Truncated("payload"));
    }
    if declared < available {
        return Err(ImageError::TrailingBytes(available - declared));
    }
    let split = FRAME_HEADER + declared;
    if tos_hash::sha256(&image[..split]) != image[split..split + DIGEST_BYTES] {
        return Err(ImageError::WrongDigest);
    }
    Ok(&image[FRAME_HEADER..split])
}

// ------------------------------------------------------------- closed families
//
// The tag space is the digest scheme's, deliberately: a tag is part of a
// module's identity, so numbering constructors differently here would invent a
// second way to name them. What this format changes is how a value is spelled,
// not which constructor a number names.

fn profile_tag(profile: Profile) -> u8 {
    match profile {
        Profile::Bootstrap => 0,
        Profile::Full => 1,
    }
}

fn profile_of(tag: u8) -> Result<Profile, ImageError> {
    Ok(match tag {
        0 => Profile::Bootstrap,
        1 => Profile::Full,
        tag => {
            return Err(ImageError::UnknownTag {
                family: "Profile",
                tag,
            })
        }
    })
}

fn int_tag(kind: IntKind) -> u8 {
    match kind {
        IntKind::I8 => 0,
        IntKind::I16 => 1,
        IntKind::I32 => 2,
        IntKind::I64 => 3,
        IntKind::U8 => 4,
        IntKind::U16 => 5,
        IntKind::U32 => 6,
        IntKind::U64 => 7,
    }
}

fn int_kind(tag: u8) -> Result<IntKind, ImageError> {
    Ok(match tag {
        0 => IntKind::I8,
        1 => IntKind::I16,
        2 => IntKind::I32,
        3 => IntKind::I64,
        4 => IntKind::U8,
        5 => IntKind::U16,
        6 => IntKind::U32,
        7 => IntKind::U64,
        tag => {
            return Err(ImageError::UnknownTag {
                family: "IntKind",
                tag,
            })
        }
    })
}

fn binary_tag(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Add => 0,
        BinaryOp::Subtract => 1,
        BinaryOp::Multiply => 2,
        BinaryOp::Divide => 3,
        BinaryOp::Remainder => 4,
        BinaryOp::ShiftLeft => 5,
        BinaryOp::ShiftRight => 6,
        BinaryOp::BitAnd => 7,
        BinaryOp::BitOr => 8,
        BinaryOp::BitXor => 9,
        BinaryOp::Equal => 10,
        BinaryOp::NotEqual => 11,
        BinaryOp::Less => 12,
        BinaryOp::LessOrEqual => 13,
        BinaryOp::Greater => 14,
        BinaryOp::GreaterOrEqual => 15,
        BinaryOp::LogicalAnd => 16,
        BinaryOp::LogicalOr => 17,
    }
}

fn binary_op(tag: u8) -> Result<BinaryOp, ImageError> {
    Ok(match tag {
        0 => BinaryOp::Add,
        1 => BinaryOp::Subtract,
        2 => BinaryOp::Multiply,
        3 => BinaryOp::Divide,
        4 => BinaryOp::Remainder,
        5 => BinaryOp::ShiftLeft,
        6 => BinaryOp::ShiftRight,
        7 => BinaryOp::BitAnd,
        8 => BinaryOp::BitOr,
        9 => BinaryOp::BitXor,
        10 => BinaryOp::Equal,
        11 => BinaryOp::NotEqual,
        12 => BinaryOp::Less,
        13 => BinaryOp::LessOrEqual,
        14 => BinaryOp::Greater,
        15 => BinaryOp::GreaterOrEqual,
        16 => BinaryOp::LogicalAnd,
        17 => BinaryOp::LogicalOr,
        tag => {
            return Err(ImageError::UnknownTag {
                family: "BinaryOp",
                tag,
            })
        }
    })
}

fn atomic_tag(op: AtomicOp) -> u8 {
    match op {
        AtomicOp::Load => 0,
        AtomicOp::Store => 1,
        AtomicOp::Swap => 2,
        AtomicOp::FetchAdd => 3,
        AtomicOp::FetchSub => 4,
        AtomicOp::FetchAnd => 5,
        AtomicOp::FetchOr => 6,
        AtomicOp::FetchXor => 7,
        AtomicOp::CompareExchange => 8,
    }
}

fn atomic_op(tag: u8) -> Result<AtomicOp, ImageError> {
    Ok(match tag {
        0 => AtomicOp::Load,
        1 => AtomicOp::Store,
        2 => AtomicOp::Swap,
        3 => AtomicOp::FetchAdd,
        4 => AtomicOp::FetchSub,
        5 => AtomicOp::FetchAnd,
        6 => AtomicOp::FetchOr,
        7 => AtomicOp::FetchXor,
        8 => AtomicOp::CompareExchange,
        tag => {
            return Err(ImageError::UnknownTag {
                family: "AtomicOp",
                tag,
            })
        }
    })
}

fn order_tag(order: MemoryOrder) -> u8 {
    match order {
        MemoryOrder::Relaxed => 0,
        MemoryOrder::Acquire => 1,
        MemoryOrder::Release => 2,
        MemoryOrder::AcqRel => 3,
        MemoryOrder::SeqCst => 4,
    }
}

fn memory_order(tag: u8) -> Result<MemoryOrder, ImageError> {
    Ok(match tag {
        0 => MemoryOrder::Relaxed,
        1 => MemoryOrder::Acquire,
        2 => MemoryOrder::Release,
        3 => MemoryOrder::AcqRel,
        4 => MemoryOrder::SeqCst,
        tag => {
            return Err(ImageError::UnknownTag {
                family: "MemoryOrder",
                tag,
            })
        }
    })
}

fn lock_tag(mode: LockMode) -> u8 {
    match mode {
        LockMode::Mutex => 0,
        LockMode::Read => 1,
        LockMode::Write => 2,
    }
}

fn lock_mode(tag: u8) -> Result<LockMode, ImageError> {
    Ok(match tag {
        0 => LockMode::Mutex,
        1 => LockMode::Read,
        2 => LockMode::Write,
        tag => {
            return Err(ImageError::UnknownTag {
                family: "LockMode",
                tag,
            })
        }
    })
}

fn resource_tag(kind: ResourceKind) -> u8 {
    match kind {
        ResourceKind::Fuel => 0,
        ResourceKind::Stack => 1,
        ResourceKind::Allocation => 2,
        ResourceKind::Task => 3,
        ResourceKind::Worker => 4,
        ResourceKind::Sync => 5,
        ResourceKind::Shared => 6,
        ResourceKind::Cleanup => 7,
        ResourceKind::Recursion => 8,
    }
}

fn resource_kind(tag: u8) -> Result<ResourceKind, ImageError> {
    Ok(match tag {
        0 => ResourceKind::Fuel,
        1 => ResourceKind::Stack,
        2 => ResourceKind::Allocation,
        3 => ResourceKind::Task,
        4 => ResourceKind::Worker,
        5 => ResourceKind::Sync,
        6 => ResourceKind::Shared,
        7 => ResourceKind::Cleanup,
        8 => ResourceKind::Recursion,
        tag => {
            return Err(ImageError::UnknownTag {
                family: "ResourceKind",
                tag,
            })
        }
    })
}

#[cfg(test)]
mod tests;
