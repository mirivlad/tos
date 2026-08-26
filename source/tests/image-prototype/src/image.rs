// SPDX-License-Identifier: GPL-3.0-or-later
//! An experimental, measurement-only container for one `tos-ir/v1` module.
//!
//! **This is not a production format and the engine never executes it.** The
//! magic ends in `x0` and the encoding version is `0` precisely so that nothing
//! here can be mistaken for, or later silently promoted into, the format
//! ADR-0070 decides. It exists to answer the seven questions ADR-0070 section 6
//! asks before that decision is taken, and for nothing else.
//!
//! Two halves, with different completeness obligations:
//!
//! - **The container and its security surface are complete.** Magic, an
//!   encoding version independent of the semantic version, canonical varints,
//!   explicit section and table lengths, every bound checked *before* the
//!   allocation sized from it, an artifact digest over the framed bytes, and
//!   fail-closed behaviour on an unknown version or an unknown tag. The parser
//!   is total over arbitrary bytes: it returns an error for every input it does
//!   not accept and panics on none.
//! - **The semantic payload covers the surface the ceiling fixture exercises**,
//!   which is the sequential core of `tos-ir/v1`. Every tag outside that surface
//!   fails closed, on both sides: the encoder refuses to write what it cannot
//!   round-trip, and the parser refuses to read a tag it does not know. The
//!   exact coverage is recorded by [`coverage`] and published in the evidence.
//!   A production encoder must cover the whole schema; this one must not be
//!   read as having done so.
//!
//! The parser belongs to the verifier path. It consumes untrusted bytes and
//! materializes an internal [`Module`] which the existing semantic verifier
//! then checks — so the verifier still reaches its own conclusion by traversing
//! a module value, exactly as `docs/43` section 5 requires. A production
//! zero-copy or bounded-view reader is deliberately not designed here; what is
//! measured is what this materializing reader actually costs.
//!
//! The division of labour between parser and verifier is deliberate. The parser
//! validates only what is *its own*: frame integrity, canonical form, UTF-8, and
//! references into the tables the container itself introduces (the string and
//! source-map identity tables). It does **not** check that a `TypeId` names a
//! type or that a `BlockId` names a block — those are semantic references, the
//! verifier's to check, and a parser that quietly pre-checked them would be a
//! second verifier nobody reviewed.
//!
//! ## The frame
//!
//! ```text
//! 0  .. 8    magic                    "TOSIMGx0"
//! 8  .. 12   encoding version         u32, big endian, independent of tos-ir/v1
//! 12 .. 20   payload length           u64, big endian
//! 20 .. 20+n payload                  canonical varint encoded sections
//! 20+n ..    artifact digest          sha256 over bytes 0 .. 20+n
//! ```
//!
//! ## Canonical rules
//!
//! One encoding per value, so two encoders that agree on the meaning agree on
//! the bytes:
//!
//! - every integer is a minimal-length varint; a non-minimal encoding is
//!   refused rather than accepted and normalized;
//! - the string table is sorted by byte value and free of duplicates;
//! - the source-map identity table is sorted by its encoded tuple and free of
//!   duplicates;
//! - the payload length is exact: trailing bytes after the digest are refused.

use std::collections::{BTreeMap, BTreeSet};

use tos_ir::{
    Block, BorrowKind, CallTarget, CleanupCall, Constant, Function, FunctionOrigin, Header, Import,
    Instruction, IntKind, Module, NominalKind, Op, Operand, Parameter, PassMode, Place, PlaceStep,
    Profile, ResourceEnvelope, ResourceKind, Signature, SourceMapEntry, Terminator, TypeDef,
    UnaryOp, Variant, Visibility,
};
use tos_verifier::Limits;

/// The experimental magic. `x0` is part of it: an image written by this
/// prototype must never be readable as a production artifact.
pub const MAGIC: [u8; 8] = *b"TOSIMGx0";

/// The container's own version, independent of `tos-ir/v1`'s semantic version.
/// A reader knows how to interpret before it knows what it holds.
pub const ENCODING_VERSION: u32 = 0;

/// Magic, version and payload length.
pub const FRAME_HEADER: usize = 8 + 4 + 8;

/// The artifact digest, sha-256 over everything before it.
pub const DIGEST_BYTES: usize = 32;

/// The largest image this reader will consider. Declared before any allocation
/// is sized from a number the bytes supplied.
pub const MAX_IMAGE_BYTES: usize = 512 * 1024 * 1024;

/// The largest string table this reader will consider. `tos-ir/v1` publishes no
/// limit on distinct strings, so the prototype declares one rather than
/// inheriting a bound it does not have.
pub const MAX_STRINGS: usize = 4 * 1024 * 1024;

/// The longest place path and operand list this reader will consider, for the
/// same reason.
pub const MAX_OPERANDS: usize = 65_536;

/// 128 bits at seven bits a byte.
pub const MAX_VARINT_BYTES: usize = 19;

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
    UnknownVersion(u32),
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
    /// The encoder met a semantic variant outside its declared coverage.
    Unsupported(&'static str),
}

// ---------------------------------------------------------------- the frame

/// Wraps an arbitrary payload in the container frame, sealing it with a correct
/// artifact digest.
///
/// Public because the negative tests need to build hostile payloads that are
/// otherwise well framed — an attacker who controls the bytes controls the
/// digest too, so a digest is integrity, never authenticity, and the payload
/// parser must stand on its own.
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(FRAME_HEADER + payload.len() + DIGEST_BYTES);
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&ENCODING_VERSION.to_be_bytes());
    bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&tos_hash::sha256(&bytes));
    bytes
}

/// Recomputes and replaces the artifact digest of an otherwise intact image.
///
/// The hostile case the mutation sweep needs: bytes changed *and* resealed, so
/// what is being tested is the payload parser rather than the digest.
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
    let version = u32::from_be_bytes([image[8], image[9], image[10], image[11]]);
    if version != ENCODING_VERSION {
        return Err(ImageError::UnknownVersion(version));
    }
    let mut length = [0u8; 8];
    length.copy_from_slice(&image[12..20]);
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

/// The artifact digest of a framed image, as `sha256:<hex>`.
pub fn artifact_digest(image: &[u8]) -> String {
    let digest = tos_hash::sha256(image);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    std::format!(
        "sha256:{}",
        std::str::from_utf8(&hex).expect("hex output is ASCII")
    )
}

// -------------------------------------------------------------- the encoder

/// Where the bytes of one image went, section by section.
///
/// Reported rather than summed, because "the image is smaller" and "the source
/// map stopped repeating itself" are different claims and a total cannot tell
/// them apart.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Layout {
    pub strings: usize,
    pub header: usize,
    pub types: usize,
    pub imports: usize,
    pub capability_imports: usize,
    pub exports: usize,
    pub constants: usize,
    pub functions: usize,
    pub source_map_identities: usize,
    pub source_map_entries: usize,
    /// What the source map would have cost with each entry's seven identity
    /// fields written inline, in this same encoding.
    pub source_map_inline_equivalent: usize,
    /// Distinct source-map identities in the module.
    pub identity_count: usize,
    pub string_count: usize,
    pub payload: usize,
    pub image: usize,
}

/// Encodes one module. Refuses rather than approximating.
pub fn encode(module: &Module) -> Result<(Vec<u8>, Layout), ImageError> {
    let table = collect_strings(module)?;
    let index: BTreeMap<&str, u32> = table
        .iter()
        .enumerate()
        .map(|(at, text)| (text.as_str(), at as u32))
        .collect();

    let mut out = Out {
        bytes: Vec::new(),
        index,
    };
    let mut layout = Layout {
        string_count: table.len(),
        ..Layout::default()
    };

    out.count(table.len());
    for text in &table {
        out.blob(text.as_bytes());
    }
    layout.strings = out.bytes.len();

    let mark = out.bytes.len();
    write_header(&mut out, &module.header)?;
    layout.header = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.types.len());
    for definition in &module.types {
        write_type(&mut out, definition)?;
    }
    layout.types = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.imports.len());
    for import in &module.imports {
        out.strref(&import.module_name)?;
        out.strref(&import.module_content_id)?;
        out.strref(&import.binding)?;
    }
    layout.imports = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.capability_imports.len());
    for import in &module.capability_imports {
        out.strref(&import.interface)?;
        out.strref(&import.binding)?;
        out.count(import.ty);
    }
    layout.capability_imports = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.exports.len());
    for signature in &module.exports {
        write_signature(&mut out, signature)?;
    }
    layout.exports = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.constants.len());
    for constant in &module.constants {
        write_constant(&mut out, constant)?;
    }
    layout.constants = out.bytes.len() - mark;

    let mark = out.bytes.len();
    out.count(module.functions.len());
    for function in &module.functions {
        write_function(&mut out, function)?;
    }
    layout.functions = out.bytes.len() - mark;

    // The source map, with module-level identity referenced rather than
    // repeated. This is ADR-0044's v2 proposal, measured: logically every entry
    // still carries the docs/43 fields, physically the identical ones name a
    // shared record.
    let identities = collect_identities(module, &out.index)?;
    let mark = out.bytes.len();
    out.count(identities.len());
    for identity in &identities {
        for reference in [
            identity.source_set,
            identity.path,
            identity.content_id,
            identity.frontend_identity,
            identity.language_version,
            identity.unicode_normalization_baseline,
        ] {
            out.varint(reference as u128);
        }
        out.tag(identity.profile);
    }
    layout.identity_count = identities.len();
    layout.source_map_identities = out.bytes.len() - mark;

    let placement: BTreeMap<Identity, u32> = identities
        .iter()
        .enumerate()
        .map(|(at, identity)| (*identity, at as u32))
        .collect();
    let mark = out.bytes.len();
    out.count(module.source_map.len());
    for entry in &module.source_map {
        let identity = identity_of(entry, &out.index)?;
        let at = placement
            .get(&identity)
            .ok_or(ImageError::OutOfRange { what: "identity" })?;
        out.varint(*at as u128);
        out.count(entry.byte_start);
        out.count(entry.byte_end);
        match entry.derived_from {
            Some(parent) => {
                out.tag(1);
                out.count(parent);
            }
            None => out.tag(0),
        }
    }
    layout.source_map_entries = out.bytes.len() - mark;
    layout.source_map_inline_equivalent = inline_source_map_bytes(module);

    layout.payload = out.bytes.len();
    let image = frame(&out.bytes);
    layout.image = image.len();
    Ok((image, layout))
}

/// What the source map would cost if every entry wrote its own identity, in
/// this same varint encoding, with no table and no sharing.
///
/// The comparison the interning claim needs. Measuring it against
/// `canonical_stream` instead would confound two changes — interning and the
/// move from sixteen-byte lengths to varints — into one number.
fn inline_source_map_bytes(module: &Module) -> usize {
    let mut total = varint_len(module.source_map.len() as u128);
    for entry in &module.source_map {
        for text in [
            &entry.source_set,
            &entry.path,
            &entry.content_id,
            &entry.frontend_identity,
            &entry.language_version,
            &entry.unicode_normalization_baseline,
        ] {
            total += varint_len(text.len() as u128) + text.len();
        }
        total += 1; // profile
        total += varint_len(entry.byte_start as u128);
        total += varint_len(entry.byte_end as u128);
        total += 1; // presence tag
        if let Some(parent) = entry.derived_from {
            total += varint_len(parent as u128);
        }
    }
    total
}

fn varint_len(mut value: u128) -> usize {
    let mut bytes = 1;
    while value >= 0x80 {
        value >>= 7;
        bytes += 1;
    }
    bytes
}

/// One source-map identity, as table references.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Identity {
    source_set: u32,
    path: u32,
    content_id: u32,
    frontend_identity: u32,
    language_version: u32,
    unicode_normalization_baseline: u32,
    profile: u8,
}

fn identity_of(
    entry: &SourceMapEntry,
    index: &BTreeMap<&str, u32>,
) -> Result<Identity, ImageError> {
    let at = |text: &str| {
        index
            .get(text)
            .copied()
            .ok_or(ImageError::OutOfRange { what: "string" })
    };
    Ok(Identity {
        source_set: at(&entry.source_set)?,
        path: at(&entry.path)?,
        content_id: at(&entry.content_id)?,
        frontend_identity: at(&entry.frontend_identity)?,
        language_version: at(&entry.language_version)?,
        unicode_normalization_baseline: at(&entry.unicode_normalization_baseline)?,
        profile: profile_tag(entry.profile),
    })
}

fn collect_identities(
    module: &Module,
    index: &BTreeMap<&str, u32>,
) -> Result<Vec<Identity>, ImageError> {
    let mut set = BTreeSet::new();
    for entry in &module.source_map {
        set.insert(identity_of(entry, index)?);
    }
    Ok(set.into_iter().collect())
}

fn profile_tag(profile: Profile) -> u8 {
    match profile {
        Profile::Bootstrap => 0,
        Profile::Full => 1,
    }
}

struct Out<'a> {
    bytes: Vec<u8>,
    index: BTreeMap<&'a str, u32>,
}

impl Out<'_> {
    fn tag(&mut self, tag: u8) {
        self.bytes.push(tag);
    }

    fn flag(&mut self, value: bool) {
        self.bytes.push(u8::from(value));
    }

    fn varint(&mut self, mut value: u128) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                self.bytes.push(byte);
                return;
            }
            self.bytes.push(byte | 0x80);
        }
    }

    fn signed(&mut self, value: i128) {
        // Zigzag, so a small negative number is a small varint.
        self.varint(((value << 1) ^ (value >> 127)) as u128);
    }

    fn count(&mut self, value: usize) {
        self.varint(value as u128);
    }

    fn blob(&mut self, value: &[u8]) {
        self.varint(value.len() as u128);
        self.bytes.extend_from_slice(value);
    }

    fn strref(&mut self, text: &str) -> Result<(), ImageError> {
        let at = self
            .index
            .get(text)
            .copied()
            .ok_or(ImageError::OutOfRange { what: "string" })?;
        self.varint(at as u128);
        Ok(())
    }

    fn opt_strref(&mut self, text: Option<&str>) -> Result<(), ImageError> {
        match text {
            Some(text) => {
                self.tag(1);
                self.strref(text)
            }
            None => {
                self.tag(0);
                Ok(())
            }
        }
    }
}

fn write_header(out: &mut Out<'_>, header: &Header) -> Result<(), ImageError> {
    out.strref(&header.schema_id)?;
    out.strref(&header.language_version)?;
    out.strref(&header.unicode_normalization_baseline)?;
    out.tag(profile_tag(header.profile));
    out.strref(&header.module_name)?;
    out.strref(&header.source_set)?;
    out.strref(&header.path)?;
    out.strref(&header.content_id)?;
    out.strref(&header.dependency_digest)?;
    out.strref(&header.frontend_identity)?;
    out.strref(&header.source_map_revision)?;
    let envelope = &header.resource_envelope;
    for limit in [
        envelope.fuel,
        envelope.stack,
        envelope.allocation,
        envelope.tasks,
        envelope.workers,
        envelope.sync,
        envelope.shared,
        envelope.cleanup,
        envelope.recursion,
        envelope.imports,
    ] {
        out.varint(limit);
    }
    out.strref(&header.capability_interface_digest)?;
    Ok(())
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

/// The tag space is the digest scheme's tag space, deliberately.
///
/// A tag is part of a module's identity, and having the image number its
/// constructors differently would be the second canonical form ADR-0070
/// section 3 refuses. What the image changes is how a value is *spelled*, not
/// which constructor a number names.
fn write_type(out: &mut Out<'_>, definition: &TypeDef) -> Result<(), ImageError> {
    match definition {
        TypeDef::Unit => out.tag(0),
        TypeDef::Bool => out.tag(1),
        TypeDef::Int(kind) => {
            out.tag(2);
            out.tag(int_tag(*kind));
        }
        TypeDef::Size => out.tag(3),
        TypeDef::Duration => out.tag(4),
        TypeDef::Text => out.tag(5),
        TypeDef::Bytes => out.tag(6),
        TypeDef::Option(inner) => {
            out.tag(15);
            out.count(*inner);
        }
        TypeDef::Slice(inner) => {
            out.tag(24);
            out.count(*inner);
        }
        TypeDef::Result(ok, error) => {
            out.tag(25);
            out.count(*ok);
            out.count(*error);
        }
        TypeDef::Array(element, length) => {
            out.tag(26);
            out.count(*element);
            out.varint(*length as u128);
        }
        TypeDef::Tuple(elements) => {
            out.tag(27);
            out.count(elements.len());
            for element in elements {
                out.count(*element);
            }
        }
        TypeDef::Function(parameters, result) => {
            out.tag(28);
            out.count(parameters.len());
            for parameter in parameters {
                out.count(*parameter);
            }
            out.count(*result);
        }
        TypeDef::Nominal {
            module_content_id,
            export_name,
            kind,
            fields,
            variants,
        } => {
            out.tag(30);
            out.strref(module_content_id)?;
            out.strref(export_name)?;
            out.tag(match kind {
                NominalKind::Record => 0,
                NominalKind::Enum => 1,
            });
            out.count(fields.len());
            for field in fields {
                out.count(*field);
            }
            out.count(variants.len());
            for variant in variants {
                out.strref(&variant.name)?;
                out.count(variant.payload.len());
                for payload in &variant.payload {
                    out.count(*payload);
                }
            }
        }
        TypeDef::ConversionError => {
            return Err(ImageError::Unsupported("TypeDef::ConversionError"))
        }
        TypeDef::Event => return Err(ImageError::Unsupported("TypeDef::Event")),
        TypeDef::Semaphore => return Err(ImageError::Unsupported("TypeDef::Semaphore")),
        TypeDef::Barrier => return Err(ImageError::Unsupported("TypeDef::Barrier")),
        TypeDef::Latch => return Err(ImageError::Unsupported("TypeDef::Latch")),
        TypeDef::AtomicBool => return Err(ImageError::Unsupported("TypeDef::AtomicBool")),
        TypeDef::AtomicU32 => return Err(ImageError::Unsupported("TypeDef::AtomicU32")),
        TypeDef::AtomicU64 => return Err(ImageError::Unsupported("TypeDef::AtomicU64")),
        TypeDef::Task(_) => return Err(ImageError::Unsupported("TypeDef::Task")),
        TypeDef::TaskResult(_) => return Err(ImageError::Unsupported("TypeDef::TaskResult")),
        TypeDef::Shared(_) => return Err(ImageError::Unsupported("TypeDef::Shared")),
        TypeDef::Region(_) => return Err(ImageError::Unsupported("TypeDef::Region")),
        TypeDef::DmaRegion(_) => return Err(ImageError::Unsupported("TypeDef::DmaRegion")),
        TypeDef::RegionMut(_) => return Err(ImageError::Unsupported("TypeDef::RegionMut")),
        TypeDef::DmaRegionMut(_) => return Err(ImageError::Unsupported("TypeDef::DmaRegionMut")),
        TypeDef::Mutex(_) => return Err(ImageError::Unsupported("TypeDef::Mutex")),
        TypeDef::RwLock(_) => return Err(ImageError::Unsupported("TypeDef::RwLock")),
        TypeDef::MutexGuard(_) => return Err(ImageError::Unsupported("TypeDef::MutexGuard")),
        TypeDef::ReadGuard(_) => return Err(ImageError::Unsupported("TypeDef::ReadGuard")),
        TypeDef::WriteGuard(_) => return Err(ImageError::Unsupported("TypeDef::WriteGuard")),
        TypeDef::Channel(_) => return Err(ImageError::Unsupported("TypeDef::Channel")),
        TypeDef::Capability(_) => return Err(ImageError::Unsupported("TypeDef::Capability")),
    }
    Ok(())
}

fn write_signature(out: &mut Out<'_>, signature: &Signature) -> Result<(), ImageError> {
    out.strref(&signature.name)?;
    out.tag(match signature.visibility {
        Visibility::Private => 0,
        Visibility::Public => 1,
    });
    out.flag(signature.is_async);
    out.count(signature.parameters.len());
    for parameter in &signature.parameters {
        out.strref(&parameter.name)?;
        out.count(parameter.ty);
        out.tag(match parameter.mode {
            PassMode::Owned => 0,
            PassMode::SharedBorrow => 1,
            PassMode::MutableBorrow => 2,
        });
    }
    out.count(signature.result);
    out.count(signature.effects.len());
    for effect in &signature.effects {
        out.strref(effect)?;
    }
    Ok(())
}

fn write_constant(out: &mut Out<'_>, constant: &Constant) -> Result<(), ImageError> {
    match constant {
        Constant::Unit => out.tag(0),
        Constant::Bool(value) => {
            out.tag(1);
            out.flag(*value);
        }
        Constant::Int(kind, value) => {
            out.tag(2);
            out.tag(int_tag(*kind));
            out.signed(*value);
        }
        Constant::Size(value) => {
            out.tag(3);
            out.varint(*value);
        }
        Constant::Duration(value) => {
            out.tag(4);
            out.varint(*value);
        }
        Constant::Text(value) => {
            out.tag(5);
            out.strref(value)?;
        }
        Constant::Bytes(value) => {
            out.tag(6);
            out.blob(value);
        }
    }
    Ok(())
}

fn write_function(out: &mut Out<'_>, function: &Function) -> Result<(), ImageError> {
    write_signature(out, &function.signature)?;
    out.tag(match function.origin {
        FunctionOrigin::Declared => 0,
        FunctionOrigin::LoweredBody => 1,
    });
    out.count(function.source);
    out.varint(function.stack_contribution);
    out.varint(function.fuel_contribution);
    out.varint(function.cleanup_contribution);
    out.count(function.values.len());
    for ty in &function.values {
        out.count(*ty);
    }
    out.count(function.blocks.len());
    for block in &function.blocks {
        write_block(out, block)?;
    }
    Ok(())
}

fn write_block(out: &mut Out<'_>, block: &Block) -> Result<(), ImageError> {
    out.count(block.parameters.len());
    for parameter in &block.parameters {
        out.count(*parameter);
    }
    out.count(block.instructions.len());
    for instruction in &block.instructions {
        write_instruction(out, instruction)?;
    }
    write_terminator(out, &block.terminator)?;
    out.count(block.source);
    Ok(())
}

fn write_instruction(out: &mut Out<'_>, instruction: &Instruction) -> Result<(), ImageError> {
    match instruction.result {
        Some(value) => {
            out.tag(1);
            out.count(value);
        }
        None => out.tag(0),
    }
    out.count(instruction.ty);
    write_op(out, &instruction.op)?;
    out.count(instruction.source);
    out.flag(instruction.unsafe_block);
    out.opt_strref(instruction.runtime_contract.as_deref())?;
    out.opt_strref(instruction.unsafe_interface.as_deref())?;
    Ok(())
}

fn write_operand(out: &mut Out<'_>, operand: &Operand) {
    match operand {
        Operand::Value(value) => {
            out.tag(0);
            out.count(*value);
        }
        Operand::Constant(constant) => {
            out.tag(1);
            out.count(*constant);
        }
    }
}

fn write_operands(out: &mut Out<'_>, operands: &[Operand]) {
    out.count(operands.len());
    for operand in operands {
        write_operand(out, operand);
    }
}

fn write_place(out: &mut Out<'_>, place: &Place) {
    out.count(place.root);
    out.count(place.path.len());
    for step in &place.path {
        match step {
            PlaceStep::Field(index) => {
                out.tag(0);
                out.count(*index);
            }
            PlaceStep::Index(Some(index)) => {
                out.tag(1);
                out.varint(*index as u128);
            }
            PlaceStep::Index(None) => out.tag(2),
            PlaceStep::DynamicIndex(value) => {
                out.tag(3);
                out.count(*value);
            }
        }
    }
}

fn binary_tag(op: tos_ir::BinaryOp) -> u8 {
    use tos_ir::BinaryOp;
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

fn binary_op(tag: u8) -> Result<tos_ir::BinaryOp, ImageError> {
    use tos_ir::BinaryOp;
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

fn write_op(out: &mut Out<'_>, op: &Op) -> Result<(), ImageError> {
    match op {
        Op::Const(constant) => {
            out.tag(0);
            out.count(*constant);
        }
        Op::Aggregate { ty, operands } => {
            out.tag(1);
            out.count(*ty);
            write_operands(out, operands);
        }
        Op::Variant {
            ty,
            index,
            operands,
        } => {
            out.tag(2);
            out.count(*ty);
            out.count(*index);
            write_operands(out, operands);
        }
        Op::Read { place } => {
            out.tag(3);
            write_place(out, place);
        }
        Op::Move { place } => {
            out.tag(4);
            write_place(out, place);
        }
        Op::Write { place, value } => {
            out.tag(5);
            write_place(out, place);
            write_operand(out, value);
        }
        Op::Borrow { place, kind } => {
            out.tag(6);
            write_place(out, place);
            out.tag(match kind {
                BorrowKind::Shared => 0,
                BorrowKind::Mutable => 1,
            });
        }
        Op::Drop { place } => {
            out.tag(7);
            write_place(out, place);
        }
        Op::Binary { op, left, right } => {
            out.tag(8);
            out.tag(binary_tag(*op));
            write_operand(out, left);
            write_operand(out, right);
        }
        Op::Unary { op, operand } => {
            out.tag(9);
            out.tag(match op {
                UnaryOp::Negate => 0,
                UnaryOp::Not => 1,
            });
            write_operand(out, operand);
        }
        Op::Widen { operand, to } => {
            out.tag(10);
            write_operand(out, operand);
            out.tag(int_tag(*to));
        }
        Op::Call { target, operands } => {
            out.tag(11);
            match target {
                CallTarget::Local(index) => {
                    out.tag(0);
                    out.count(*index);
                }
                CallTarget::Imported { import, name } => {
                    out.tag(1);
                    out.count(*import);
                    out.strref(name)?;
                }
                CallTarget::Predeclared(name) => {
                    out.tag(2);
                    out.strref(name)?;
                }
            }
            write_operands(out, operands);
        }
        Op::Resource {
            kind,
            amount,
            release,
        } => {
            out.tag(18);
            out.tag(resource_tag(*kind));
            write_operand(out, amount);
            out.flag(*release);
        }
        Op::RegisterCleanup { body } => {
            out.tag(19);
            out.count(*body);
        }
        Op::RunCleanups { calls } => {
            out.tag(20);
            out.count(calls.len());
            for call in calls {
                out.count(call.body);
                write_operands(out, &call.captures);
            }
        }
        Op::Closure { body, captures } => {
            out.tag(21);
            out.count(*body);
            write_operands(out, captures);
        }
        Op::CallValue { callee, operands } => {
            out.tag(22);
            write_operand(out, callee);
            write_operands(out, operands);
        }
        Op::Spawn { .. } => return Err(ImageError::Unsupported("Op::Spawn")),
        Op::Lock { .. } => return Err(ImageError::Unsupported("Op::Lock")),
        Op::Share { .. } => return Err(ImageError::Unsupported("Op::Share")),
        Op::Join { .. } => return Err(ImageError::Unsupported("Op::Join")),
        Op::Await { .. } => return Err(ImageError::Unsupported("Op::Await")),
        Op::Cancel { .. } => return Err(ImageError::Unsupported("Op::Cancel")),
        Op::Atomic { .. } => return Err(ImageError::Unsupported("Op::Atomic")),
        Op::Capability { .. } => return Err(ImageError::Unsupported("Op::Capability")),
    }
    Ok(())
}

fn write_terminator(out: &mut Out<'_>, terminator: &Terminator) -> Result<(), ImageError> {
    match terminator {
        Terminator::Return(value) => {
            out.tag(0);
            match value {
                Some(operand) => {
                    out.tag(1);
                    write_operand(out, operand);
                }
                None => out.tag(0),
            }
        }
        Terminator::Branch { target, arguments } => {
            out.tag(1);
            out.count(*target);
            write_operands(out, arguments);
        }
        Terminator::BranchIf {
            condition,
            true_target,
            true_arguments,
            false_target,
            false_arguments,
        } => {
            out.tag(2);
            write_operand(out, condition);
            out.count(*true_target);
            write_operands(out, true_arguments);
            out.count(*false_target);
            write_operands(out, false_arguments);
        }
        Terminator::MatchEnum { subject, arms } => {
            out.tag(3);
            write_operand(out, subject);
            out.count(arms.len());
            for (variant, target) in arms {
                out.count(*variant);
                out.count(*target);
            }
        }
        Terminator::PropagateError { result, ok_target } => {
            out.tag(4);
            write_operand(out, result);
            out.count(*ok_target);
        }
        Terminator::Trap(code) => {
            out.tag(5);
            out.strref(code)?;
        }
    }
    Ok(())
}

/// Every string the encoder will reference, gathered before anything is written.
///
/// A separate traversal rather than interning as it goes, so that the table can
/// be sorted: a canonical order is one a reader can check, and first-occurrence
/// order is not.
fn collect_strings(module: &Module) -> Result<BTreeSet<String>, ImageError> {
    let mut strings = BTreeSet::new();
    let keep = |text: &str, set: &mut BTreeSet<String>| {
        if !set.contains(text) {
            set.insert(String::from(text));
        }
    };
    let header = &module.header;
    for text in [
        &header.schema_id,
        &header.language_version,
        &header.unicode_normalization_baseline,
        &header.module_name,
        &header.source_set,
        &header.path,
        &header.content_id,
        &header.dependency_digest,
        &header.frontend_identity,
        &header.source_map_revision,
        &header.capability_interface_digest,
    ] {
        keep(text, &mut strings);
    }
    for definition in &module.types {
        if let TypeDef::Nominal {
            module_content_id,
            export_name,
            variants,
            ..
        } = definition
        {
            keep(module_content_id, &mut strings);
            keep(export_name, &mut strings);
            for variant in variants {
                keep(&variant.name, &mut strings);
            }
        }
    }
    for import in &module.imports {
        keep(&import.module_name, &mut strings);
        keep(&import.module_content_id, &mut strings);
        keep(&import.binding, &mut strings);
    }
    for import in &module.capability_imports {
        keep(&import.interface, &mut strings);
        keep(&import.binding, &mut strings);
    }
    for signature in module
        .exports
        .iter()
        .chain(module.functions.iter().map(|function| &function.signature))
    {
        keep(&signature.name, &mut strings);
        for parameter in &signature.parameters {
            keep(&parameter.name, &mut strings);
        }
        for effect in &signature.effects {
            keep(effect, &mut strings);
        }
    }
    for constant in &module.constants {
        if let Constant::Text(value) = constant {
            keep(value, &mut strings);
        }
    }
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Some(text) = &instruction.runtime_contract {
                    keep(text, &mut strings);
                }
                if let Some(text) = &instruction.unsafe_interface {
                    keep(text, &mut strings);
                }
                match &instruction.op {
                    Op::Call {
                        target: CallTarget::Imported { name, .. },
                        ..
                    }
                    | Op::Call {
                        target: CallTarget::Predeclared(name),
                        ..
                    } => keep(name, &mut strings),
                    _ => {}
                }
            }
            if let Terminator::Trap(code) = &block.terminator {
                keep(code, &mut strings);
            }
        }
    }
    for entry in &module.source_map {
        for text in [
            &entry.source_set,
            &entry.path,
            &entry.content_id,
            &entry.frontend_identity,
            &entry.language_version,
            &entry.unicode_normalization_baseline,
        ] {
            keep(text, &mut strings);
        }
    }
    Ok(strings)
}

// --------------------------------------------------------------- the parser

/// Reads untrusted bytes into a module value the semantic verifier can check.
///
/// Total over arbitrary input: every path either returns a module or an
/// [`ImageError`], and no input reaches a panic, an unbounded allocation or a
/// read past the slice.
pub fn parse(image: &[u8], limits: &Limits) -> Result<Module, ImageError> {
    let payload = unframe(image)?;
    let mut input = In {
        bytes: payload,
        at: 0,
        limits: *limits,
        strings: Vec::new(),
    };
    let module = input.module()?;
    if input.at != input.bytes.len() {
        return Err(ImageError::TrailingBytes(input.bytes.len() - input.at));
    }
    Ok(module)
}

struct In<'a> {
    bytes: &'a [u8],
    at: usize,
    limits: Limits,
    strings: Vec<String>,
}

impl In<'_> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }

    fn byte(&mut self, what: &'static str) -> Result<u8, ImageError> {
        let byte = *self.bytes.get(self.at).ok_or(ImageError::Truncated(what))?;
        self.at += 1;
        Ok(byte)
    }

    fn flag(&mut self) -> Result<bool, ImageError> {
        match self.byte("flag")? {
            0 => Ok(false),
            1 => Ok(true),
            tag => Err(ImageError::UnknownTag {
                family: "flag",
                tag,
            }),
        }
    }

    /// A canonical, bounded varint. Non-minimal encodings are refused rather
    /// than accepted and normalized: accepting two spellings of one value is
    /// how a canonical form stops being one.
    fn varint(&mut self) -> Result<u128, ImageError> {
        let mut value: u128 = 0;
        let mut shift: u32 = 0;
        let mut taken = 0usize;
        loop {
            let byte = self.byte("varint")?;
            taken += 1;
            let payload = u128::from(byte & 0x7f);
            if taken == MAX_VARINT_BYTES && payload > 0b11 {
                return Err(ImageError::VarintOverflow);
            }
            value |= payload << shift;
            shift += 7;
            if byte & 0x80 == 0 {
                if taken > 1 && byte == 0 {
                    return Err(ImageError::NonCanonicalVarint);
                }
                return Ok(value);
            }
            if taken == MAX_VARINT_BYTES {
                return Err(ImageError::VarintOverflow);
            }
        }
    }

    /// A table count: bounded by its declared limit **and** by the bytes that
    /// remain, before anything is allocated from it.
    ///
    /// The second bound is what makes a forged count harmless. Every entry of
    /// every table costs at least one byte, so a count larger than the bytes
    /// left cannot be honoured whatever the limit says, and the reader learns
    /// that before it reserves anything.
    fn count(&mut self, what: &'static str, limit: usize) -> Result<usize, ImageError> {
        let count = self.varint()?;
        if count > limit as u128 {
            return Err(ImageError::CountExceedsLimit { what, count, limit });
        }
        let count = count as usize;
        if count > self.remaining() {
            return Err(ImageError::Truncated(what));
        }
        Ok(count)
    }

    /// An index into a semantic table. Bounded to `usize` and nothing more:
    /// whether it names anything is the verifier's question.
    fn index(&mut self) -> Result<usize, ImageError> {
        let value = self.varint()?;
        if value > usize::MAX as u128 {
            return Err(ImageError::IndexOverflow);
        }
        Ok(value as usize)
    }

    fn wide(&mut self) -> Result<u128, ImageError> {
        self.varint()
    }

    fn signed(&mut self) -> Result<i128, ImageError> {
        let zigzag = self.varint()?;
        Ok(((zigzag >> 1) as i128) ^ -((zigzag & 1) as i128))
    }

    fn blob(&mut self, what: &'static str) -> Result<&[u8], ImageError> {
        let length = self.varint()?;
        if length > self.remaining() as u128 {
            return Err(ImageError::Truncated(what));
        }
        let length = length as usize;
        let bytes = &self.bytes[self.at..self.at + length];
        self.at += length;
        Ok(bytes)
    }

    fn strref(&mut self) -> Result<String, ImageError> {
        let at = self.varint()?;
        if at >= self.strings.len() as u128 {
            return Err(ImageError::OutOfRange {
                what: "string table",
            });
        }
        Ok(self.strings[at as usize].clone())
    }

    fn opt_strref(&mut self) -> Result<Option<String>, ImageError> {
        match self.byte("optional string")? {
            0 => Ok(None),
            1 => Ok(Some(self.strref()?)),
            tag => Err(ImageError::UnknownTag {
                family: "optional string",
                tag,
            }),
        }
    }

    fn profile(&mut self) -> Result<Profile, ImageError> {
        match self.byte("Profile")? {
            0 => Ok(Profile::Bootstrap),
            1 => Ok(Profile::Full),
            tag => Err(ImageError::UnknownTag {
                family: "Profile",
                tag,
            }),
        }
    }

    fn module(&mut self) -> Result<Module, ImageError> {
        self.string_table()?;
        let header = self.header()?;

        let count = self.count("types", self.limits.table_entries)?;
        let mut types = Vec::with_capacity(count);
        for _ in 0..count {
            types.push(self.type_definition()?);
        }

        let count = self.count("imports", self.limits.modules)?;
        let mut imports = Vec::with_capacity(count);
        for _ in 0..count {
            imports.push(Import {
                module_name: self.strref()?,
                module_content_id: self.strref()?,
                binding: self.strref()?,
            });
        }

        let count = self.count("capability imports", self.limits.table_entries)?;
        let mut capability_imports = Vec::with_capacity(count);
        for _ in 0..count {
            capability_imports.push(tos_ir::CapabilityImport {
                interface: self.strref()?,
                binding: self.strref()?,
                ty: self.index()?,
            });
        }

        let count = self.count("exports", self.limits.table_entries)?;
        let mut exports = Vec::with_capacity(count);
        for _ in 0..count {
            exports.push(self.signature()?);
        }

        let count = self.count("constants", self.limits.table_entries)?;
        let mut constants = Vec::with_capacity(count);
        for _ in 0..count {
            constants.push(self.constant()?);
        }

        let count = self.count("functions", self.limits.table_entries)?;
        let mut functions = Vec::with_capacity(count);
        for _ in 0..count {
            functions.push(self.function()?);
        }

        let source_map = self.source_map()?;

        Ok(Module {
            header,
            types,
            imports,
            capability_imports,
            exports,
            constants,
            functions,
            source_map,
        })
    }

    /// The string table, checked for canonical order as it is read.
    fn string_table(&mut self) -> Result<(), ImageError> {
        let count = self.count("string table", MAX_STRINGS)?;
        let mut strings = Vec::with_capacity(count);
        let mut previous: Option<String> = None;
        for _ in 0..count {
            let bytes = self.blob("string")?;
            let text = std::str::from_utf8(bytes).map_err(|_| ImageError::BadUtf8)?;
            if let Some(previous) = &previous {
                if previous.as_str() >= text {
                    return Err(ImageError::NonCanonicalTable("string table"));
                }
            }
            let owned = String::from(text);
            previous = Some(owned.clone());
            strings.push(owned);
        }
        self.strings = strings;
        Ok(())
    }

    fn header(&mut self) -> Result<Header, ImageError> {
        let schema_id = self.strref()?;
        let language_version = self.strref()?;
        let unicode_normalization_baseline = self.strref()?;
        let profile = self.profile()?;
        let module_name = self.strref()?;
        let source_set = self.strref()?;
        let path = self.strref()?;
        let content_id = self.strref()?;
        let dependency_digest = self.strref()?;
        let frontend_identity = self.strref()?;
        let source_map_revision = self.strref()?;
        let resource_envelope = ResourceEnvelope {
            fuel: self.wide()?,
            stack: self.wide()?,
            allocation: self.wide()?,
            tasks: self.wide()?,
            workers: self.wide()?,
            sync: self.wide()?,
            shared: self.wide()?,
            cleanup: self.wide()?,
            recursion: self.wide()?,
            imports: self.wide()?,
        };
        let capability_interface_digest = self.strref()?;
        Ok(Header {
            schema_id,
            language_version,
            unicode_normalization_baseline,
            profile,
            module_name,
            source_set,
            path,
            content_id,
            dependency_digest,
            frontend_identity,
            source_map_revision,
            resource_envelope,
            capability_interface_digest,
        })
    }

    fn type_definition(&mut self) -> Result<TypeDef, ImageError> {
        Ok(match self.byte("TypeDef")? {
            0 => TypeDef::Unit,
            1 => TypeDef::Bool,
            2 => TypeDef::Int(int_kind(self.byte("IntKind")?)?),
            3 => TypeDef::Size,
            4 => TypeDef::Duration,
            5 => TypeDef::Text,
            6 => TypeDef::Bytes,
            15 => TypeDef::Option(self.index()?),
            24 => TypeDef::Slice(self.index()?),
            25 => TypeDef::Result(self.index()?, self.index()?),
            26 => {
                let element = self.index()?;
                let length = self.varint()?;
                if length > u128::from(u64::MAX) {
                    return Err(ImageError::IndexOverflow);
                }
                TypeDef::Array(element, length as u64)
            }
            27 => {
                let count = self.count("tuple elements", self.limits.fields)?;
                let mut elements = Vec::with_capacity(count);
                for _ in 0..count {
                    elements.push(self.index()?);
                }
                TypeDef::Tuple(elements)
            }
            28 => {
                let count = self.count("function parameters", self.limits.parameters)?;
                let mut parameters = Vec::with_capacity(count);
                for _ in 0..count {
                    parameters.push(self.index()?);
                }
                TypeDef::Function(parameters, self.index()?)
            }
            30 => {
                let module_content_id = self.strref()?;
                let export_name = self.strref()?;
                let kind = match self.byte("NominalKind")? {
                    0 => NominalKind::Record,
                    1 => NominalKind::Enum,
                    tag => {
                        return Err(ImageError::UnknownTag {
                            family: "NominalKind",
                            tag,
                        })
                    }
                };
                let count = self.count("fields", self.limits.fields)?;
                let mut fields = Vec::with_capacity(count);
                for _ in 0..count {
                    fields.push(self.index()?);
                }
                let count = self.count("variants", self.limits.fields)?;
                let mut variants = Vec::with_capacity(count);
                for _ in 0..count {
                    let name = self.strref()?;
                    let payload_count = self.count("variant payload", self.limits.fields)?;
                    let mut payload = Vec::with_capacity(payload_count);
                    for _ in 0..payload_count {
                        payload.push(self.index()?);
                    }
                    variants.push(Variant { name, payload });
                }
                TypeDef::Nominal {
                    module_content_id,
                    export_name,
                    kind,
                    fields,
                    variants,
                }
            }
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "TypeDef",
                    tag,
                })
            }
        })
    }

    fn signature(&mut self) -> Result<Signature, ImageError> {
        let name = self.strref()?;
        let visibility = match self.byte("Visibility")? {
            0 => Visibility::Private,
            1 => Visibility::Public,
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "Visibility",
                    tag,
                })
            }
        };
        let is_async = self.flag()?;
        let count = self.count("parameters", self.limits.parameters)?;
        let mut parameters = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.strref()?;
            let ty = self.index()?;
            let mode = match self.byte("PassMode")? {
                0 => PassMode::Owned,
                1 => PassMode::SharedBorrow,
                2 => PassMode::MutableBorrow,
                tag => {
                    return Err(ImageError::UnknownTag {
                        family: "PassMode",
                        tag,
                    })
                }
            };
            parameters.push(Parameter { name, ty, mode });
        }
        let result = self.index()?;
        let count = self.count("effects", self.limits.table_entries)?;
        let mut effects = Vec::with_capacity(count);
        for _ in 0..count {
            effects.push(self.strref()?);
        }
        Ok(Signature {
            name,
            visibility,
            is_async,
            parameters,
            result,
            effects,
        })
    }

    fn constant(&mut self) -> Result<Constant, ImageError> {
        Ok(match self.byte("Constant")? {
            0 => Constant::Unit,
            1 => Constant::Bool(self.flag()?),
            2 => {
                let kind = int_kind(self.byte("IntKind")?)?;
                Constant::Int(kind, self.signed()?)
            }
            3 => Constant::Size(self.wide()?),
            4 => Constant::Duration(self.wide()?),
            5 => Constant::Text(self.strref()?),
            6 => Constant::Bytes(self.blob("constant bytes")?.to_vec()),
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "Constant",
                    tag,
                })
            }
        })
    }

    fn function(&mut self) -> Result<Function, ImageError> {
        let signature = self.signature()?;
        let origin = match self.byte("FunctionOrigin")? {
            0 => FunctionOrigin::Declared,
            1 => FunctionOrigin::LoweredBody,
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "FunctionOrigin",
                    tag,
                })
            }
        };
        let source = self.index()?;
        let stack_contribution = self.wide()?;
        let fuel_contribution = self.wide()?;
        let cleanup_contribution = self.wide()?;
        let count = self.count("ssa values", self.limits.table_entries)?;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.index()?);
        }
        let count = self.count("blocks", self.limits.blocks_per_function)?;
        let mut blocks = Vec::with_capacity(count);
        for _ in 0..count {
            blocks.push(self.block()?);
        }
        Ok(Function {
            signature,
            origin,
            source,
            stack_contribution,
            fuel_contribution,
            cleanup_contribution,
            values,
            blocks,
        })
    }

    fn block(&mut self) -> Result<Block, ImageError> {
        let count = self.count("block parameters", self.limits.parameters)?;
        let mut parameters = Vec::with_capacity(count);
        for _ in 0..count {
            parameters.push(self.index()?);
        }
        let count = self.count("instructions", self.limits.instructions_per_block)?;
        let mut instructions = Vec::with_capacity(count);
        for _ in 0..count {
            instructions.push(self.instruction()?);
        }
        let terminator = self.terminator()?;
        let source = self.index()?;
        Ok(Block {
            parameters,
            instructions,
            terminator,
            source,
        })
    }

    fn instruction(&mut self) -> Result<Instruction, ImageError> {
        let result = match self.byte("instruction result")? {
            0 => None,
            1 => Some(self.index()?),
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "instruction result",
                    tag,
                })
            }
        };
        let ty = self.index()?;
        let op = self.op()?;
        let source = self.index()?;
        let unsafe_block = self.flag()?;
        let runtime_contract = self.opt_strref()?;
        let unsafe_interface = self.opt_strref()?;
        Ok(Instruction {
            result,
            ty,
            op,
            source,
            runtime_contract,
            unsafe_block,
            unsafe_interface,
        })
    }

    fn operand(&mut self) -> Result<Operand, ImageError> {
        Ok(match self.byte("Operand")? {
            0 => Operand::Value(self.index()?),
            1 => Operand::Constant(self.index()?),
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "Operand",
                    tag,
                })
            }
        })
    }

    fn operands(&mut self) -> Result<Vec<Operand>, ImageError> {
        let count = self.count("operands", MAX_OPERANDS)?;
        let mut operands = Vec::with_capacity(count);
        for _ in 0..count {
            operands.push(self.operand()?);
        }
        Ok(operands)
    }

    fn place(&mut self) -> Result<Place, ImageError> {
        let root = self.index()?;
        let count = self.count("place path", MAX_OPERANDS)?;
        let mut path = Vec::with_capacity(count);
        for _ in 0..count {
            path.push(match self.byte("PlaceStep")? {
                0 => PlaceStep::Field(self.index()?),
                1 => {
                    let value = self.varint()?;
                    if value > u128::from(u64::MAX) {
                        return Err(ImageError::IndexOverflow);
                    }
                    PlaceStep::Index(Some(value as u64))
                }
                2 => PlaceStep::Index(None),
                3 => PlaceStep::DynamicIndex(self.index()?),
                tag => {
                    return Err(ImageError::UnknownTag {
                        family: "PlaceStep",
                        tag,
                    })
                }
            });
        }
        Ok(Place { root, path })
    }

    fn op(&mut self) -> Result<Op, ImageError> {
        Ok(match self.byte("Op")? {
            0 => Op::Const(self.index()?),
            1 => Op::Aggregate {
                ty: self.index()?,
                operands: self.operands()?,
            },
            2 => Op::Variant {
                ty: self.index()?,
                index: self.index()?,
                operands: self.operands()?,
            },
            3 => Op::Read {
                place: self.place()?,
            },
            4 => Op::Move {
                place: self.place()?,
            },
            5 => Op::Write {
                place: self.place()?,
                value: self.operand()?,
            },
            6 => Op::Borrow {
                place: self.place()?,
                kind: match self.byte("BorrowKind")? {
                    0 => BorrowKind::Shared,
                    1 => BorrowKind::Mutable,
                    tag => {
                        return Err(ImageError::UnknownTag {
                            family: "BorrowKind",
                            tag,
                        })
                    }
                },
            },
            7 => Op::Drop {
                place: self.place()?,
            },
            8 => Op::Binary {
                op: binary_op(self.byte("BinaryOp")?)?,
                left: self.operand()?,
                right: self.operand()?,
            },
            9 => Op::Unary {
                op: match self.byte("UnaryOp")? {
                    0 => UnaryOp::Negate,
                    1 => UnaryOp::Not,
                    tag => {
                        return Err(ImageError::UnknownTag {
                            family: "UnaryOp",
                            tag,
                        })
                    }
                },
                operand: self.operand()?,
            },
            10 => Op::Widen {
                operand: self.operand()?,
                to: int_kind(self.byte("IntKind")?)?,
            },
            11 => {
                let target = match self.byte("CallTarget")? {
                    0 => CallTarget::Local(self.index()?),
                    1 => CallTarget::Imported {
                        import: self.index()?,
                        name: self.strref()?,
                    },
                    2 => CallTarget::Predeclared(self.strref()?),
                    tag => {
                        return Err(ImageError::UnknownTag {
                            family: "CallTarget",
                            tag,
                        })
                    }
                };
                Op::Call {
                    target,
                    operands: self.operands()?,
                }
            }
            18 => Op::Resource {
                kind: resource_kind(self.byte("ResourceKind")?)?,
                amount: self.operand()?,
                release: self.flag()?,
            },
            19 => Op::RegisterCleanup {
                body: self.index()?,
            },
            20 => {
                let count = self.count("cleanup calls", MAX_OPERANDS)?;
                let mut calls = Vec::with_capacity(count);
                for _ in 0..count {
                    calls.push(CleanupCall {
                        body: self.index()?,
                        captures: self.operands()?,
                    });
                }
                Op::RunCleanups { calls }
            }
            21 => Op::Closure {
                body: self.index()?,
                captures: self.operands()?,
            },
            22 => Op::CallValue {
                callee: self.operand()?,
                operands: self.operands()?,
            },
            tag => return Err(ImageError::UnknownTag { family: "Op", tag }),
        })
    }

    fn terminator(&mut self) -> Result<Terminator, ImageError> {
        Ok(match self.byte("Terminator")? {
            0 => Terminator::Return(match self.byte("return value")? {
                0 => None,
                1 => Some(self.operand()?),
                tag => {
                    return Err(ImageError::UnknownTag {
                        family: "return value",
                        tag,
                    })
                }
            }),
            1 => Terminator::Branch {
                target: self.index()?,
                arguments: self.operands()?,
            },
            2 => Terminator::BranchIf {
                condition: self.operand()?,
                true_target: self.index()?,
                true_arguments: self.operands()?,
                false_target: self.index()?,
                false_arguments: self.operands()?,
            },
            3 => {
                let subject = self.operand()?;
                let count = self.count("match arms", MAX_OPERANDS)?;
                let mut arms = Vec::with_capacity(count);
                for _ in 0..count {
                    arms.push((self.index()?, self.index()?));
                }
                Terminator::MatchEnum { subject, arms }
            }
            4 => Terminator::PropagateError {
                result: self.operand()?,
                ok_target: self.index()?,
            },
            5 => Terminator::Trap(self.strref()?),
            tag => {
                return Err(ImageError::UnknownTag {
                    family: "Terminator",
                    tag,
                })
            }
        })
    }

    /// The source map: an identity table, then entries that reference it.
    ///
    /// De-interning here is deliberate and is part of what is being measured. A
    /// production reader would keep the reference; this one materializes the
    /// same `Module` the frontend produces, so that the digest comparison is
    /// over identical values and the memory figure is honest about what a
    /// materializing reader costs.
    fn source_map(&mut self) -> Result<Vec<SourceMapEntry>, ImageError> {
        let count = self.count("identity table", self.limits.source_map_entries)?;
        let mut identities = Vec::with_capacity(count);
        let mut previous: Option<[u128; 7]> = None;
        for _ in 0..count {
            let mut references = [0u128; 7];
            for slot in references.iter_mut().take(6) {
                *slot = self.varint()?;
            }
            let profile = self.byte("Profile")?;
            references[6] = u128::from(profile);
            if let Some(previous) = previous {
                if previous >= references {
                    return Err(ImageError::NonCanonicalTable("identity table"));
                }
            }
            previous = Some(references);
            let mut resolved = Vec::with_capacity(6);
            for reference in references.iter().take(6) {
                if *reference >= self.strings.len() as u128 {
                    return Err(ImageError::OutOfRange {
                        what: "string table",
                    });
                }
                resolved.push(*reference as usize);
            }
            let profile = match profile {
                0 => Profile::Bootstrap,
                1 => Profile::Full,
                tag => {
                    return Err(ImageError::UnknownTag {
                        family: "Profile",
                        tag,
                    })
                }
            };
            identities.push((resolved, profile));
        }

        let count = self.count("source map", self.limits.source_map_entries)?;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let at = self.varint()?;
            if at >= identities.len() as u128 {
                return Err(ImageError::OutOfRange {
                    what: "identity table",
                });
            }
            let (references, profile) = &identities[at as usize];
            let byte_start = self.index()?;
            let byte_end = self.index()?;
            let derived_from = match self.byte("derived_from")? {
                0 => None,
                1 => Some(self.index()?),
                tag => {
                    return Err(ImageError::UnknownTag {
                        family: "derived_from",
                        tag,
                    })
                }
            };
            entries.push(SourceMapEntry {
                source_set: self.strings[references[0]].clone(),
                path: self.strings[references[1]].clone(),
                content_id: self.strings[references[2]].clone(),
                frontend_identity: self.strings[references[3]].clone(),
                language_version: self.strings[references[4]].clone(),
                profile: *profile,
                unicode_normalization_baseline: self.strings[references[5]].clone(),
                byte_start,
                byte_end,
                derived_from,
            });
        }
        Ok(entries)
    }
}

// -------------------------------------------------------------- the coverage

/// How many times each tagged variant occurs in a module.
///
/// The evidence has to say what the prototype's payload actually covers, and
/// the honest way to say it is to count what the fixture contains rather than
/// to assert what the encoder implements.
pub fn coverage(module: &Module) -> BTreeMap<&'static str, usize> {
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let seen = |name: &'static str, counts: &mut BTreeMap<&'static str, usize>| {
        *counts.entry(name).or_insert(0) += 1;
    };
    for definition in &module.types {
        seen(type_name(definition), &mut counts);
    }
    for constant in &module.constants {
        seen(constant_name(constant), &mut counts);
    }
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                seen(op_name(&instruction.op), &mut counts);
                if let Op::Call { target, .. } = &instruction.op {
                    seen(
                        match target {
                            CallTarget::Local(_) => "CallTarget::Local",
                            CallTarget::Imported { .. } => "CallTarget::Imported",
                            CallTarget::Predeclared(_) => "CallTarget::Predeclared",
                        },
                        &mut counts,
                    );
                }
                for place in places_of(&instruction.op) {
                    for step in &place.path {
                        seen(
                            match step {
                                PlaceStep::Field(_) => "PlaceStep::Field",
                                PlaceStep::Index(Some(_)) => "PlaceStep::Index(const)",
                                PlaceStep::Index(None) => "PlaceStep::Index(unknown)",
                                PlaceStep::DynamicIndex(_) => "PlaceStep::DynamicIndex",
                            },
                            &mut counts,
                        );
                    }
                }
            }
            seen(terminator_name(&block.terminator), &mut counts);
        }
    }
    counts
}

fn places_of(op: &Op) -> Vec<&Place> {
    match op {
        Op::Read { place } | Op::Move { place } | Op::Drop { place } => std::vec![place],
        Op::Write { place, .. } | Op::Borrow { place, .. } => std::vec![place],
        _ => Vec::new(),
    }
}

fn type_name(definition: &TypeDef) -> &'static str {
    match definition {
        TypeDef::Unit => "TypeDef::Unit",
        TypeDef::Bool => "TypeDef::Bool",
        TypeDef::Int(_) => "TypeDef::Int",
        TypeDef::Size => "TypeDef::Size",
        TypeDef::Duration => "TypeDef::Duration",
        TypeDef::Text => "TypeDef::Text",
        TypeDef::Bytes => "TypeDef::Bytes",
        TypeDef::ConversionError => "TypeDef::ConversionError",
        TypeDef::Event => "TypeDef::Event",
        TypeDef::Semaphore => "TypeDef::Semaphore",
        TypeDef::Barrier => "TypeDef::Barrier",
        TypeDef::Latch => "TypeDef::Latch",
        TypeDef::AtomicBool => "TypeDef::AtomicBool",
        TypeDef::AtomicU32 => "TypeDef::AtomicU32",
        TypeDef::AtomicU64 => "TypeDef::AtomicU64",
        TypeDef::Option(_) => "TypeDef::Option",
        TypeDef::Task(_) => "TypeDef::Task",
        TypeDef::TaskResult(_) => "TypeDef::TaskResult",
        TypeDef::Shared(_) => "TypeDef::Shared",
        TypeDef::Region(_) => "TypeDef::Region",
        TypeDef::DmaRegion(_) => "TypeDef::DmaRegion",
        TypeDef::RegionMut(_) => "TypeDef::RegionMut",
        TypeDef::DmaRegionMut(_) => "TypeDef::DmaRegionMut",
        TypeDef::Mutex(_) => "TypeDef::Mutex",
        TypeDef::RwLock(_) => "TypeDef::RwLock",
        TypeDef::MutexGuard(_) => "TypeDef::MutexGuard",
        TypeDef::ReadGuard(_) => "TypeDef::ReadGuard",
        TypeDef::WriteGuard(_) => "TypeDef::WriteGuard",
        TypeDef::Channel(_) => "TypeDef::Channel",
        TypeDef::Slice(_) => "TypeDef::Slice",
        TypeDef::Result(_, _) => "TypeDef::Result",
        TypeDef::Array(_, _) => "TypeDef::Array",
        TypeDef::Tuple(_) => "TypeDef::Tuple",
        TypeDef::Function(_, _) => "TypeDef::Function",
        TypeDef::Capability(_) => "TypeDef::Capability",
        TypeDef::Nominal { .. } => "TypeDef::Nominal",
    }
}

fn constant_name(constant: &Constant) -> &'static str {
    match constant {
        Constant::Unit => "Constant::Unit",
        Constant::Bool(_) => "Constant::Bool",
        Constant::Int(_, _) => "Constant::Int",
        Constant::Size(_) => "Constant::Size",
        Constant::Duration(_) => "Constant::Duration",
        Constant::Text(_) => "Constant::Text",
        Constant::Bytes(_) => "Constant::Bytes",
    }
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Const(_) => "Op::Const",
        Op::Aggregate { .. } => "Op::Aggregate",
        Op::Variant { .. } => "Op::Variant",
        Op::Read { .. } => "Op::Read",
        Op::Move { .. } => "Op::Move",
        Op::Write { .. } => "Op::Write",
        Op::Borrow { .. } => "Op::Borrow",
        Op::Drop { .. } => "Op::Drop",
        Op::Binary { .. } => "Op::Binary",
        Op::Unary { .. } => "Op::Unary",
        Op::Widen { .. } => "Op::Widen",
        Op::Call { .. } => "Op::Call",
        Op::Spawn { .. } => "Op::Spawn",
        Op::Closure { .. } => "Op::Closure",
        Op::CallValue { .. } => "Op::CallValue",
        Op::Lock { .. } => "Op::Lock",
        Op::Share { .. } => "Op::Share",
        Op::Join { .. } => "Op::Join",
        Op::Await { .. } => "Op::Await",
        Op::Cancel { .. } => "Op::Cancel",
        Op::Atomic { .. } => "Op::Atomic",
        Op::Capability { .. } => "Op::Capability",
        Op::Resource { .. } => "Op::Resource",
        Op::RegisterCleanup { .. } => "Op::RegisterCleanup",
        Op::RunCleanups { .. } => "Op::RunCleanups",
    }
}

fn terminator_name(terminator: &Terminator) -> &'static str {
    match terminator {
        Terminator::Return(_) => "Terminator::Return",
        Terminator::Branch { .. } => "Terminator::Branch",
        Terminator::BranchIf { .. } => "Terminator::BranchIf",
        Terminator::MatchEnum { .. } => "Terminator::MatchEnum",
        Terminator::PropagateError { .. } => "Terminator::PropagateError",
        Terminator::Trap(_) => "Terminator::Trap",
    }
}

/// Every tagged variant this prototype's payload encoder implements.
pub const SUPPORTED: &[&str] = &[
    "TypeDef::Unit",
    "TypeDef::Bool",
    "TypeDef::Int",
    "TypeDef::Size",
    "TypeDef::Duration",
    "TypeDef::Text",
    "TypeDef::Bytes",
    "TypeDef::Option",
    "TypeDef::Slice",
    "TypeDef::Result",
    "TypeDef::Array",
    "TypeDef::Tuple",
    "TypeDef::Function",
    "TypeDef::Nominal",
    "Constant::Unit",
    "Constant::Bool",
    "Constant::Int",
    "Constant::Size",
    "Constant::Duration",
    "Constant::Text",
    "Constant::Bytes",
    "Op::Const",
    "Op::Aggregate",
    "Op::Variant",
    "Op::Read",
    "Op::Move",
    "Op::Write",
    "Op::Borrow",
    "Op::Drop",
    "Op::Binary",
    "Op::Unary",
    "Op::Widen",
    "Op::Call",
    "Op::Resource",
    "Op::RegisterCleanup",
    "Op::RunCleanups",
    "Op::Closure",
    "Op::CallValue",
    "Terminator::Return",
    "Terminator::Branch",
    "Terminator::BranchIf",
    "Terminator::MatchEnum",
    "Terminator::PropagateError",
    "Terminator::Trap",
    "CallTarget::Local",
    "CallTarget::Imported",
    "CallTarget::Predeclared",
    "PlaceStep::Field",
    "PlaceStep::Index(const)",
    "PlaceStep::Index(unknown)",
    "PlaceStep::DynamicIndex",
    "Operand::Value",
    "Operand::Constant",
    "IntKind (all eight)",
    "BinaryOp (all eighteen)",
    "UnaryOp (both)",
    "BorrowKind (both)",
    "ResourceKind (all nine)",
    "NominalKind (both)",
    "Visibility (both)",
    "PassMode (all three)",
    "FunctionOrigin (both)",
    "Profile (both)",
];

/// Every tagged variant this prototype refuses, on both sides.
///
/// A production encoder must cover these. The prototype fails closed on them so
/// that a measurement can never be mistaken for full coverage.
pub const UNSUPPORTED: &[&str] = &[
    "TypeDef::ConversionError",
    "TypeDef::Event",
    "TypeDef::Semaphore",
    "TypeDef::Barrier",
    "TypeDef::Latch",
    "TypeDef::AtomicBool",
    "TypeDef::AtomicU32",
    "TypeDef::AtomicU64",
    "TypeDef::Task",
    "TypeDef::TaskResult",
    "TypeDef::Shared",
    "TypeDef::Region",
    "TypeDef::DmaRegion",
    "TypeDef::RegionMut",
    "TypeDef::DmaRegionMut",
    "TypeDef::Mutex",
    "TypeDef::RwLock",
    "TypeDef::MutexGuard",
    "TypeDef::ReadGuard",
    "TypeDef::WriteGuard",
    "TypeDef::Channel",
    "TypeDef::Capability",
    "Op::Spawn",
    "Op::Lock",
    "Op::Share",
    "Op::Join",
    "Op::Await",
    "Op::Cancel",
    "Op::Atomic",
    "Op::Capability",
    "AtomicOp (all nine)",
    "MemoryOrder (all five)",
    "LockMode (all three)",
];
