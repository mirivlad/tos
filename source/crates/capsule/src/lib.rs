// SPDX-License-Identifier: GPL-3.0-or-later
//! TOS boot capsule format v1 — bounded, total, allocation-free parser.
//!
//! See `interfaces/boot/CAPSULE_FORMAT_V1.md`. The parser never panics on
//! malformed input: every violation maps to a structured [`CapsError`].
//! The core is `no_std`; the deterministic builder is behind the `host`
//! feature (see [`self::build`]).

#![no_std]

use tos_hash::Sha256;

#[cfg(feature = "host")]
extern crate std;

#[cfg(feature = "host")]
pub mod build;

// ---------------------------------------------------------------------------
// Format constants (normative in `interfaces/boot/CAPSULE_FORMAT_V1.md`)
// ---------------------------------------------------------------------------

pub const MAGIC: [u8; 8] = *b"TOSCAPSU";
pub const FORMAT_UUID: [u8; 16] = [
    0x2c, 0x4f, 0x78, 0xb3, 0x9d, 0x1e, 0x4b, 0x0a, 0x9f, 0x2c, 0x1a, 0x5c, 0x8e, 0x0d, 0x6f, 0x71,
];
pub const FORMAT_VERSION: u16 = 1;
pub const HEADER_SIZE: usize = 184;
pub const ALIGNMENT: u16 = 8;
pub const PATH_ENTRY_SIZE: u32 = 16;
pub const FILE_ENTRY_SIZE: u32 = 64;
pub const DIGEST_BYTES: usize = 32;
// Grouped by component: major_minor_patch (spec §2: ARCH_SPEC_VERSION 0x000201).
pub const ARCH_SPEC_VERSION: u32 = 0x00_02_01;
pub const BUILDER_VERSION: u32 = 1;

pub const SRC_KIND_NONE: u8 = 0;
pub const SRC_KIND_GIT: u8 = 1;
pub const SRC_KIND_DETACHED: u8 = 2;

/// Identity algorithms for `source_oid_alg` (git kind): the algorithm of the
/// raw object id stored in `source_identity_value`. 0 = no OID (detached
/// kind carries a plain source-set digest instead).
pub const OID_ALG_NONE: u8 = 0;
pub const OID_ALG_SHA1: u8 = 1;
pub const OID_ALG_SHA256: u8 = 2;

/// OID byte lengths for the supported git algorithms.
pub const OID_LEN_SHA1: u8 = 20;
pub const OID_LEN_SHA256: u8 = 32;

/// Fixed domain separator for ADR-0018 detached source-set identities.
pub(crate) const DETACHED_IDENTITY_DOMAIN: &[u8] = b"TOS.DSI.v1\0";

pub const FLAG_BOOT_CANONICAL: u32 = 1 << 0;
pub const FLAG_LICENCE_NOTICE: u32 = 1 << 1;
/// Flag bits defined for v1 path entries: only boot-canonical is valid on a
/// path (a licence-notice bit on a path is a reserved-bit violation).
pub const PATH_KNOWN_FLAGS: u32 = FLAG_BOOT_CANONICAL;
/// Flag bits defined for v1 file entries.
pub const FILE_KNOWN_FLAGS: u32 = FLAG_BOOT_CANONICAL | FLAG_LICENCE_NOTICE;

/// Canonical boot text path required by Stage 1.
pub const BOOT_PATH: &[u8] = b"/system/boot/init.tos";

/// Append one validated canonical path/content-digest pair to a detached
/// source-set identity. The host builder and `no_std` parser share this exact
/// encoding so their byte-level interpretations cannot drift.
pub(crate) fn update_detached_identity(
    hasher: &mut Sha256,
    path: &[u8],
    path_length: u32,
    content_digest: &[u8; DIGEST_BYTES],
) {
    hasher.update(&path_length.to_le_bytes());
    hasher.update(path);
    hasher.update(content_digest);
}

// Byte offsets within the 184-byte header.
mod off {
    pub const MAGIC: usize = 0;
    pub const UUID: usize = 8;
    pub const FORMAT_VERSION: usize = 24;
    pub const HEADER_SIZE: usize = 26;
    pub const ALIGNMENT: usize = 28;
    pub const RESERVED: usize = 30;
    pub const TOTAL_LENGTH: usize = 32;
    pub const PATH_TABLE_OFFSET: usize = 40;
    pub const PATH_TABLE_COUNT: usize = 48;
    pub const PATH_ENTRY_SIZE: usize = 52;
    pub const FILE_TABLE_OFFSET: usize = 56;
    pub const FILE_COUNT: usize = 64;
    pub const FILE_ENTRY_SIZE: usize = 68;
    pub const PAYLOAD_OFFSET: usize = 72;
    pub const PAYLOAD_LENGTH: usize = 80;
    pub const ARCH_SPEC_VERSION: usize = 88;
    pub const BUILDER_VERSION: usize = 92;
    pub const SRC_KIND: usize = 96;
    pub const SRC_ALG: usize = 97;
    pub const SRC_OID_LEN: usize = 98;
    pub const SRC_RESERVED: usize = 99;
    pub const SRC_VALUE: usize = 100;
    pub const SRC_TAIL_RESERVED: usize = 132;
    pub const LICENCE_OFFSET: usize = 136;
    pub const LICENCE_LENGTH: usize = 144;
    pub const WHOLE_DIGEST: usize = 152;
}

/// Decoded capsule header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 8],
    pub format_uuid: [u8; 16],
    pub format_version: u16,
    pub header_size: u16,
    pub alignment: u16,
    pub total_length: u64,
    pub path_table_offset: u64,
    pub path_table_count: u32,
    pub path_entry_size: u32,
    pub file_table_offset: u64,
    pub file_count: u32,
    pub file_entry_size: u32,
    pub payload_offset: u64,
    pub payload_length: u64,
    pub arch_spec_version: u32,
    pub builder_version: u32,
    pub source_identity_kind: u8,
    pub source_oid_alg: u8,
    pub source_oid_length: u8,
    pub source_identity_value: [u8; 32],
    pub licence_notice_offset: u64,
    pub licence_notice_length: u64,
    pub whole_capsule_digest: [u8; DIGEST_BYTES],
}

/// Structured validation error. Variant names are symbolic and stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapsError {
    /// Input shorter than the header, or `total_length` exceeds the slice.
    InputTooShort,
    BadMagic,
    BadUuid,
    BadFormatVersion,
    BadHeaderSize,
    BadAlignment,
    NonZeroReservedHeader,
    TotalLengthMismatch,
    BadArchVersion,
    BadBuilderVersion,
    BadPathEntrySize,
    BadFileEntrySize,
    RegionOverflow,
    LayoutMismatch,
    BadUtf8,
    NulInPath,
    ControlInPath,
    NonAbsolutePath,
    TraversalInPath,
    EmptyComponent,
    DuplicatePath,
    UnsortedPathTable,
    BadPathFlags,
    PathFileIndexOutOfRange,
    /// A file entry is never referenced by any path entry.
    UnreferencedFile,
    /// `path_entry[i].file_index != i`. The bijection required by §4.1 is
    /// realised canonically (spec rule 26, ADR-0017), so any other permutation
    /// — even one that is itself a valid bijection — is a non-canonical
    /// encoding of the same file set and is rejected.
    NonCanonicalFileIndex,
    NameOutOfArena,
    /// A gap between the header and the path table: `path_table_offset` must
    /// equal `HEADER_SIZE` (spec §4 rule 24, ADR-0017).
    PathTableNotAfterHeader,
    /// The name arena is not packed: the first name does not start at offset 0,
    /// a name does not begin where the previous one ends, or the last name does
    /// not end exactly at `file_table_offset` (spec §4.1 rule 25, ADR-0017).
    /// Undescribed bytes in the arena are how arbitrary data used to travel
    /// inside an otherwise valid capsule.
    UnpackedNameArena,
    BadFileFlags,
    NonZeroReservedEntry,
    UnsortedFileTable,
    PayloadOverlap,
    PayloadGap,
    ZeroFileCount,
    BadDigest,
    BadWholeDigest,
    UnsupportedIdentityKind,
    /// A SHA-1 Git OID uses only the first 20 bytes of the 32-byte identity
    /// value; ADR-0016 requires its unused tail to be zero.
    NonZeroOidPadding,
    /// ADR-0018 detached identity does not match the canonical path/digest
    /// sequence after every path and file digest has validated.
    DetachedIdentityMismatch,
    LicenceOutOfBounds,
    MissingBootCanonical,
    DuplicateBootCanonical,
    BadBootCanonicalName,
    /// The boot-canonical flag is set on the canonical path entry but not on
    /// the file entry it references (or vice versa); the two must agree.
    BootCanonicalFlagMismatch,
    /// A file entry carries the boot-canonical flag but is not the canonical
    /// path's target (exactly one file may carry it, and it must be the
    /// canonical file).
    BootCanonicalOnWrongFile,
    /// licence notice arithmetic: offset/length must cover the exact tail
    /// `[payload_end, total_length)` when present, and both be zero when
    /// absent.
    LicenceTailMismatch,
}

impl core::fmt::Display for CapsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[cfg(feature = "host")]
impl core::error::Error for CapsError {}

// ---------------------------------------------------------------------------
// Low-level little-endian readers (checked slices)
// ---------------------------------------------------------------------------

#[inline]
fn rd_u16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
#[inline]
fn rd_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
#[inline]
fn rd_u64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([
        b[at],
        b[at + 1],
        b[at + 2],
        b[at + 3],
        b[at + 4],
        b[at + 5],
        b[at + 6],
        b[at + 7],
    ])
}

/// Decoded path entry (16 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathEntry {
    pub name_offset: u32,
    pub name_length: u32,
    pub file_index: u32,
    pub flags: u32,
}

/// Decoded file entry (64 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub content_offset: u64,
    pub content_length: u64,
    pub content_digest: [u8; DIGEST_BYTES],
    pub file_flags: u32,
    /// 12 reserved bytes at +52..+64; must be zero.
    pub reserved: [u8; 12],
}

fn decode_path_entry(b: &[u8], at: usize) -> PathEntry {
    PathEntry {
        name_offset: rd_u32(b, at),
        name_length: rd_u32(b, at + 4),
        file_index: rd_u32(b, at + 8),
        flags: rd_u32(b, at + 12),
    }
}

fn decode_file_entry(b: &[u8], at: usize) -> FileEntry {
    let mut dg = [0u8; DIGEST_BYTES];
    dg.copy_from_slice(&b[at + 16..at + 16 + DIGEST_BYTES]);
    let mut reserved = [0u8; 12];
    reserved.copy_from_slice(&b[at + 52..at + 64]);
    FileEntry {
        content_offset: rd_u64(b, at),
        content_length: rd_u64(b, at + 8),
        content_digest: dg,
        file_flags: rd_u32(b, at + 48),
        reserved,
    }
}

/// Decode the header region (caller guarantees `len >= HEADER_SIZE`).
fn decode_header(b: &[u8]) -> Header {
    let mut magic = [0u8; 8];
    magic.copy_from_slice(&b[off::MAGIC..off::MAGIC + 8]);
    let mut format_uuid = [0u8; 16];
    format_uuid.copy_from_slice(&b[off::UUID..off::UUID + 16]);
    let mut sd = [0u8; 32];
    sd.copy_from_slice(&b[off::SRC_VALUE..off::SRC_VALUE + 32]);
    let mut wd = [0u8; DIGEST_BYTES];
    wd.copy_from_slice(&b[off::WHOLE_DIGEST..off::WHOLE_DIGEST + DIGEST_BYTES]);
    Header {
        magic,
        format_uuid,
        format_version: rd_u16(b, off::FORMAT_VERSION),
        header_size: rd_u16(b, off::HEADER_SIZE),
        alignment: rd_u16(b, off::ALIGNMENT),
        total_length: rd_u64(b, off::TOTAL_LENGTH),
        path_table_offset: rd_u64(b, off::PATH_TABLE_OFFSET),
        path_table_count: rd_u32(b, off::PATH_TABLE_COUNT),
        path_entry_size: rd_u32(b, off::PATH_ENTRY_SIZE),
        file_table_offset: rd_u64(b, off::FILE_TABLE_OFFSET),
        file_count: rd_u32(b, off::FILE_COUNT),
        file_entry_size: rd_u32(b, off::FILE_ENTRY_SIZE),
        payload_offset: rd_u64(b, off::PAYLOAD_OFFSET),
        payload_length: rd_u64(b, off::PAYLOAD_LENGTH),
        arch_spec_version: rd_u32(b, off::ARCH_SPEC_VERSION),
        builder_version: rd_u32(b, off::BUILDER_VERSION),
        source_identity_kind: b[off::SRC_KIND],
        source_oid_alg: b[off::SRC_ALG],
        source_oid_length: b[off::SRC_OID_LEN],
        source_identity_value: sd,
        licence_notice_offset: rd_u64(b, off::LICENCE_OFFSET),
        licence_notice_length: rd_u64(b, off::LICENCE_LENGTH),
        whole_capsule_digest: wd,
    }
}

// ---------------------------------------------------------------------------
// Canonical-path validation
// ---------------------------------------------------------------------------

fn is_control(b: u8) -> bool {
    b < 0x20 || b == 0x7f
}

/// Validate a canonical absolute path.
fn check_path(name: &[u8]) -> Result<(), CapsError> {
    core::str::from_utf8(name).map_err(|_| CapsError::BadUtf8)?;
    if !name.starts_with(b"/") {
        return Err(CapsError::NonAbsolutePath);
    }
    if name.contains(&0) {
        return Err(CapsError::NulInPath);
    }
    if name.iter().any(|&b| is_control(b)) {
        return Err(CapsError::ControlInPath);
    }
    if name.len() == 1 {
        return Err(CapsError::EmptyComponent);
    }
    for comp in name[1..].split(|&b| b == b'/') {
        if comp.is_empty() {
            return Err(CapsError::EmptyComponent);
        }
        if comp == b"." || comp == b".." {
            return Err(CapsError::TraversalInPath);
        }
    }
    Ok(())
}

fn name_cmp(a: &[u8], b: &[u8]) -> core::cmp::Ordering {
    a.cmp(b)
}

// ---------------------------------------------------------------------------
// Parsed capsule (zero-alloc: borrows the input)
// ---------------------------------------------------------------------------

/// A resolved file: name and content slices live inside the capsule bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct File<'a> {
    pub name: &'a [u8],
    pub content: &'a [u8],
    pub digest: [u8; DIGEST_BYTES],
    pub flags: u32,
}

/// A validated capsule. Holds a borrowed view over the input bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capsule<'a> {
    bytes: &'a [u8],
    header: Header,
    name_start: usize,
    file_tbl_start: usize,
    payload_start: usize,
}

impl<'a> Capsule<'a> {
    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn file_count(&self) -> u32 {
        self.header.file_count
    }

    pub fn path_table_count(&self) -> u32 {
        self.header.path_table_count
    }

    /// The file with the given canonical path (binary search; table is sorted).
    pub fn find(&self, path: &[u8]) -> Option<File<'a>> {
        let count = self.header.path_table_count as usize;
        let mut lo = 0usize;
        let mut hi = count;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let pe = self.path_entry_at(mid);
            match name_cmp(self.name_bytes(&pe), path) {
                core::cmp::Ordering::Less => lo = mid + 1,
                core::cmp::Ordering::Greater => hi = mid,
                core::cmp::Ordering::Equal => return self.file_for(&pe),
            }
        }
        None
    }

    /// The boot-canonical file `/system/boot/init.tos`.
    pub fn boot_file(&self) -> Option<File<'a>> {
        self.find(BOOT_PATH)
    }

    /// Iterator over (name, content) for all files, in path-table order.
    pub fn files(&self) -> FileIter<'a> {
        FileIter { cap: *self, idx: 0 }
    }

    // --- internal decoding helpers ---

    fn path_entry_at(&self, i: usize) -> PathEntry {
        let at = self.header.path_table_offset as usize + i * PATH_ENTRY_SIZE as usize;
        decode_path_entry(self.bytes, at)
    }

    fn name_bytes(&self, pe: &PathEntry) -> &'a [u8] {
        let start = self.name_start + pe.name_offset as usize;
        &self.bytes[start..start + pe.name_length as usize]
    }

    fn file_for(&self, pe: &PathEntry) -> Option<File<'a>> {
        let idx = pe.file_index as usize;
        if idx >= self.header.file_count as usize {
            return None;
        }
        let at = self.file_tbl_start + idx * FILE_ENTRY_SIZE as usize;
        let fe = decode_file_entry(self.bytes, at);
        let start = self.payload_start + fe.content_offset as usize;
        let end = start + fe.content_length as usize;
        Some(File {
            name: self.name_bytes(pe),
            content: &self.bytes[start..end],
            digest: fe.content_digest,
            flags: fe.file_flags,
        })
    }
}

/// Iterator over files (used by tests and inspection).
pub struct FileIter<'a> {
    cap: Capsule<'a>,
    idx: usize,
}

impl<'a> Iterator for FileIter<'a> {
    type Item = File<'a>;
    fn next(&mut self) -> Option<File<'a>> {
        if self.idx >= self.cap.path_table_count() as usize {
            return None;
        }
        let pe = self.cap.path_entry_at(self.idx);
        self.idx += 1;
        self.cap.file_for(&pe)
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse and fully validate a capsule over `bytes`. Total and bounded:
/// returns a structured [`CapsError`] for any violation, never panics.
pub fn parse(bytes: &[u8]) -> Result<Capsule<'_>, CapsError> {
    if bytes.len() < HEADER_SIZE {
        return Err(CapsError::InputTooShort);
    }
    let h = decode_header(bytes);

    // --- identity and framing ---
    if h.magic != MAGIC {
        return Err(CapsError::BadMagic);
    }
    if h.format_uuid != FORMAT_UUID {
        return Err(CapsError::BadUuid);
    }
    if h.format_version != FORMAT_VERSION {
        return Err(CapsError::BadFormatVersion);
    }
    if h.header_size != HEADER_SIZE as u16 {
        return Err(CapsError::BadHeaderSize);
    }
    if h.alignment != ALIGNMENT {
        return Err(CapsError::BadAlignment);
    }
    if bytes[off::RESERVED..off::RESERVED + 2]
        .iter()
        .any(|&b| b != 0)
    {
        return Err(CapsError::NonZeroReservedHeader);
    }
    if bytes[off::SRC_RESERVED..off::SRC_VALUE]
        .iter()
        .any(|&b| b != 0)
    {
        return Err(CapsError::NonZeroReservedHeader);
    }
    // 4-byte reserved gap between the identity value and the licence offset.
    if bytes[off::SRC_TAIL_RESERVED..off::LICENCE_OFFSET]
        .iter()
        .any(|&b| b != 0)
    {
        return Err(CapsError::NonZeroReservedHeader);
    }
    // Identity kind/algorithm consistency: git requires an explicit OID
    // algorithm and length (SHA-1 20B or SHA-256 32B); detached carries a
    // plain source-set digest (no OID algorithm).
    match h.source_identity_kind {
        SRC_KIND_GIT => {
            let ok = matches!(
                (h.source_oid_alg, h.source_oid_length),
                (OID_ALG_SHA1, OID_LEN_SHA1) | (OID_ALG_SHA256, OID_LEN_SHA256)
            );
            if !ok {
                return Err(CapsError::UnsupportedIdentityKind);
            }
            if h.source_oid_alg == OID_ALG_SHA1
                && h.source_identity_value[OID_LEN_SHA1 as usize..]
                    .iter()
                    .any(|&b| b != 0)
            {
                return Err(CapsError::NonZeroOidPadding);
            }
        }
        SRC_KIND_DETACHED => {
            if h.source_oid_alg != OID_ALG_NONE || h.source_oid_length != 0 {
                return Err(CapsError::UnsupportedIdentityKind);
            }
        }
        _ => return Err(CapsError::UnsupportedIdentityKind),
    }
    if h.total_length as usize != bytes.len() {
        return Err(CapsError::TotalLengthMismatch);
    }
    if h.arch_spec_version != ARCH_SPEC_VERSION {
        return Err(CapsError::BadArchVersion);
    }
    if h.builder_version != BUILDER_VERSION {
        return Err(CapsError::BadBuilderVersion);
    }
    if h.path_entry_size != PATH_ENTRY_SIZE {
        return Err(CapsError::BadPathEntrySize);
    }
    if h.file_entry_size != FILE_ENTRY_SIZE {
        return Err(CapsError::BadFileEntrySize);
    }
    if h.source_identity_kind != SRC_KIND_GIT && h.source_identity_kind != SRC_KIND_DETACHED {
        return Err(CapsError::UnsupportedIdentityKind);
    }

    // --- sequential-layout arithmetic (all checked) ---
    // The path table begins immediately after the header: a capsule has no
    // undescribed bytes (spec §4, ADR-0017). A lower bound alone would let
    // arbitrary data sit between the header and the first path entry.
    let path_tbl_start = h.path_table_offset as usize;
    if path_tbl_start != HEADER_SIZE {
        return Err(CapsError::PathTableNotAfterHeader);
    }
    let path_tbl_bytes = (h.path_table_count as usize)
        .checked_mul(PATH_ENTRY_SIZE as usize)
        .ok_or(CapsError::RegionOverflow)?;
    let name_start = path_tbl_start
        .checked_add(path_tbl_bytes)
        .ok_or(CapsError::RegionOverflow)?;
    let file_tbl_start = h.file_table_offset as usize;
    // name arena must be non-empty and end where the file table begins
    if file_tbl_start < name_start {
        return Err(CapsError::LayoutMismatch);
    }
    let file_tbl_bytes = (h.file_count as usize)
        .checked_mul(FILE_ENTRY_SIZE as usize)
        .ok_or(CapsError::RegionOverflow)?;
    let payload_start = file_tbl_start
        .checked_add(file_tbl_bytes)
        .ok_or(CapsError::RegionOverflow)?;
    if payload_start != h.payload_offset as usize {
        return Err(CapsError::LayoutMismatch);
    }
    let payload_end = payload_start
        .checked_add(h.payload_length as usize)
        .ok_or(CapsError::RegionOverflow)?;
    if h.file_count == 0 {
        return Err(CapsError::ZeroFileCount);
    }
    // payload ends at or before EOF; the licence block, when present, is the
    // exact tail of the capsule.
    if payload_end > bytes.len() {
        return Err(CapsError::LayoutMismatch);
    }
    let tail = bytes.len() - payload_end;
    if tail != h.licence_notice_length as usize {
        return Err(CapsError::LicenceOutOfBounds);
    }
    if h.licence_notice_length == 0 {
        // Absent notice: both fields must be zero (spec §3, §7).
        if h.licence_notice_offset != 0 {
            return Err(CapsError::LicenceTailMismatch);
        }
    } else {
        // Present notice: it must cover exactly the tail of the capsule,
        // i.e. [payload_end, total_length).
        if h.licence_notice_offset as usize != payload_end {
            return Err(CapsError::LicenceTailMismatch);
        }
        let lic_end = h
            .licence_notice_offset
            .checked_add(h.licence_notice_length)
            .ok_or(CapsError::RegionOverflow)?;
        if lic_end as usize != bytes.len() {
            return Err(CapsError::LicenceTailMismatch);
        }
        // The block itself must be valid UTF-8 text (spec §7).
        if core::str::from_utf8(&bytes[payload_end..]).is_err() {
            return Err(CapsError::LicenceTailMismatch);
        }
    }

    let name_arena_len = file_tbl_start - name_start;

    // --- path table ---
    let mut prev_name: Option<&[u8]> = None;
    let mut canonical_count = 0usize;
    let mut canonical_name_ok = false;
    // Boot-canonical cross-check: the canonical path's target file must carry
    // FLAG_BOOT_CANONICAL too (checked after the file loop).
    let mut canonical_file_index: Option<u32> = None;
    // Packed-arena cursor: names tile the arena exactly, in table order
    // (spec §4.1, ADR-0017).
    let mut arena_cursor = 0usize;
    for i in 0..h.path_table_count as usize {
        let pe = decode_path_entry(bytes, path_tbl_start + i * PATH_ENTRY_SIZE as usize);
        if pe.flags & !PATH_KNOWN_FLAGS != 0 {
            return Err(CapsError::BadPathFlags);
        }
        let name_off = pe.name_offset as usize;
        let name_len = pe.name_length as usize;
        if name_len == 0 {
            return Err(CapsError::EmptyComponent);
        }
        let end = name_off
            .checked_add(name_len)
            .ok_or(CapsError::RegionOverflow)?;
        // Each name starts exactly where the previous one ended (the first at
        // offset 0), so no arena byte lies outside a name. Checked before the
        // bounds test: a misplaced name is a packing violation, and reporting
        // it as such is more precise than the overrun it may also cause.
        if name_off != arena_cursor {
            return Err(CapsError::UnpackedNameArena);
        }
        if end > name_arena_len {
            return Err(CapsError::NameOutOfArena);
        }
        arena_cursor = end;
        let name = &bytes[name_start + name_off..name_start + end];
        check_path(name)?;
        if let Some(prev) = prev_name {
            match name_cmp(prev, name) {
                core::cmp::Ordering::Equal => return Err(CapsError::DuplicatePath),
                core::cmp::Ordering::Greater => return Err(CapsError::UnsortedPathTable),
                core::cmp::Ordering::Less => {}
            }
        }
        prev_name = Some(name);
        if pe.file_index as usize >= h.file_count as usize {
            return Err(CapsError::PathFileIndexOutOfRange);
        }
        // Canonical index mapping (spec §4.1 rule 26, ADR-0017): path entry i
        // references file i. Verifying the bijection here costs one comparison
        // per entry; the previous reference-counting form was O(n²) and took
        // 3.55 s in release for 20 001 files, against the 250 ms p95 budget of
        // docs/35_PERFORMANCE_CONTRACTS.md §Stage 1.
        if pe.file_index as usize != i {
            return Err(CapsError::NonCanonicalFileIndex);
        }
        if pe.flags & FLAG_BOOT_CANONICAL != 0 {
            canonical_count += 1;
            canonical_file_index = Some(pe.file_index);
            if name == BOOT_PATH {
                canonical_name_ok = true;
            }
        }
    }

    // The last name ends exactly at `file_table_offset`: trailing arena slack
    // is undescribed data, not padding.
    if arena_cursor != name_arena_len {
        return Err(CapsError::UnpackedNameArena);
    }

    // --- file table ---
    let mut prev_end: Option<u64> = None;
    // Bijection path<->file: every file entry must be referenced exactly once
    // (no duplicates, no orphans). O(n²) over the (small) path table; bounded
    // by the input size and exits early on the first violation.
    let mut canonical_target_ok = false;
    for i in 0..h.file_count as usize {
        let fe = decode_file_entry(bytes, file_tbl_start + i * FILE_ENTRY_SIZE as usize);
        if fe.file_flags & !FILE_KNOWN_FLAGS != 0 {
            return Err(CapsError::BadFileFlags);
        }
        if fe.reserved.iter().any(|&b| b != 0) {
            return Err(CapsError::NonZeroReservedEntry);
        }
        let end = fe
            .content_offset
            .checked_add(fe.content_length)
            .ok_or(CapsError::RegionOverflow)?;
        if end > h.payload_length {
            return Err(CapsError::PayloadGap);
        }
        match prev_end {
            None => {
                if fe.content_offset != 0 {
                    return Err(CapsError::PayloadGap);
                }
            }
            Some(prev) => {
                if fe.content_offset < prev {
                    return Err(CapsError::UnsortedFileTable);
                }
                if fe.content_offset > prev {
                    return Err(CapsError::PayloadGap);
                }
            }
        }
        prev_end = Some(end);

        // boot-canonical cross-check: the file referenced by the canonical
        // path must carry FLAG_BOOT_CANONICAL, and no other file may.
        if fe.file_flags & FLAG_BOOT_CANONICAL != 0 {
            if canonical_file_index != Some(i as u32) {
                return Err(CapsError::BootCanonicalOnWrongFile);
            }
            canonical_target_ok = true;
        }

        // digest check
        let cs = payload_start + fe.content_offset as usize;
        let content = &bytes[cs..cs + fe.content_length as usize];
        let mut hh = Sha256::new();
        hh.update(content);
        let d = hh.finalize();
        if d != fe.content_digest {
            return Err(CapsError::BadDigest);
        }
    }
    if prev_end != Some(h.payload_length) {
        return Err(CapsError::PayloadGap);
    }

    // --- bijection ---
    // With rule 26 enforced above (path_entry[i].file_index == i), equal counts
    // are sufficient: the mapping is the identity on [0, file_count), which is
    // a bijection by construction. Only the count relation remains to check.
    // A path table *longer* than the file table cannot reach this point: entry
    // `file_count` would have to carry `file_index == file_count`, which the
    // range check above already rejects. The only remaining mismatch is a file
    // table longer than the path table, i.e. files no path names.
    if h.path_table_count != h.file_count {
        return Err(CapsError::UnreferencedFile);
    }

    // --- boot-canonical rules ---
    if canonical_count == 0 {
        return Err(CapsError::MissingBootCanonical);
    }
    if canonical_count > 1 {
        return Err(CapsError::DuplicateBootCanonical);
    }
    if !canonical_name_ok {
        return Err(CapsError::BadBootCanonicalName);
    }
    // The canonical path's target file must carry FLAG_BOOT_CANONICAL (and no
    // other file may, enforced in the file loop above).
    if !canonical_target_ok {
        return Err(CapsError::BootCanonicalFlagMismatch);
    }

    // ADR-0018: a detached capsule must name exactly its validated canonical
    // path/file-table sequence. This second allocation-free O(n) pass happens
    // only after the earlier loops validated path canonicality/index mapping
    // and each content digest, so it hashes trusted parsed fields rather than
    // a caller-selected label.
    if h.source_identity_kind == SRC_KIND_DETACHED {
        let mut identity = Sha256::new();
        identity.update(DETACHED_IDENTITY_DOMAIN);
        for i in 0..h.file_count as usize {
            let pe = decode_path_entry(bytes, path_tbl_start + i * PATH_ENTRY_SIZE as usize);
            let name_start_at = name_start + pe.name_offset as usize;
            let name_end_at = name_start_at + pe.name_length as usize;
            let fe = decode_file_entry(bytes, file_tbl_start + i * FILE_ENTRY_SIZE as usize);
            update_detached_identity(
                &mut identity,
                &bytes[name_start_at..name_end_at],
                pe.name_length,
                &fe.content_digest,
            );
        }
        if identity.finalize() != h.source_identity_value {
            return Err(CapsError::DetachedIdentityMismatch);
        }
    }

    // --- whole-capsule digest: bytes[0..152] || zeros[32] || bytes[184..] ---
    let mut hh = Sha256::new();
    hh.update(&bytes[0..off::WHOLE_DIGEST]);
    hh.update(&[0u8; DIGEST_BYTES]);
    hh.update(&bytes[HEADER_SIZE..]);
    let d = hh.finalize();
    if d != h.whole_capsule_digest {
        return Err(CapsError::BadWholeDigest);
    }

    Ok(Capsule {
        bytes,
        header: h,
        name_start,
        file_tbl_start,
        payload_start,
    })
}

// ---------------------------------------------------------------------------
// Unit tests (host build)
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "host"))]
mod tests {
    use super::*;
    use crate::build::{Builder, FileSpec};
    use std::format;

    fn sample_builder() -> Builder {
        let mut b = Builder::new();
        b.source_identity_kind = SRC_KIND_DETACHED;
        b.source_identity_value = [0x11; DIGEST_BYTES];
        b.add(FileSpec::new(
            "/system/boot/init.tos",
            b"# TOS boot text\nprint(\"hello from boot\")\n",
        ));
        b.add(FileSpec::new("/system/version", b"0.2.1\n"));
        b
    }

    fn sha1_capsule() -> std::vec::Vec<u8> {
        let mut b = Builder::new();
        b.source_identity_kind = SRC_KIND_GIT;
        b.source_oid_alg = OID_ALG_SHA1;
        b.source_oid_length = OID_LEN_SHA1;
        for (i, byte) in b.source_identity_value[..OID_LEN_SHA1 as usize]
            .iter_mut()
            .enumerate()
        {
            *byte = i as u8;
        }
        b.add(FileSpec::new("/system/boot/init.tos", b"# boot\n"));
        b.build().expect("build SHA-1 capsule")
    }

    fn expected_detached_identity(entries: &[(&[u8], &[u8])]) -> [u8; DIGEST_BYTES] {
        let mut entries = entries.to_vec();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        let mut identity = Sha256::new();
        identity.update(b"TOS.DSI.v1\0");
        for (path, content) in entries {
            identity.update(&(path.len() as u32).to_le_bytes());
            identity.update(path);
            let mut content_hash = Sha256::new();
            content_hash.update(content);
            identity.update(&content_hash.finalize());
        }
        identity.finalize()
    }

    #[test]
    fn round_trip() {
        let b = sample_builder();
        let bytes = b.build().expect("build");
        let cap = parse(&bytes).expect("parse");
        assert_eq!(cap.file_count(), 2);
        let boot = cap.boot_file().expect("boot file");
        assert_eq!(boot.name, BOOT_PATH);
        assert!(boot.flags & FLAG_BOOT_CANONICAL != 0);
        let v = cap.find(b"/system/version").expect("version");
        assert_eq!(v.content, b"0.2.1\n");
        assert_eq!(cap.find(b"/nope"), None);
    }

    #[test]
    fn licence_notice_round_trip() {
        let mut b = sample_builder();
        b.set_licence_notice(b"SPDX-License-Identifier: GPL-3.0-or-later\n".to_vec());
        let bytes = b.build().expect("build");
        let cap = parse(&bytes).expect("parse with licence tail");
        let h = cap.header();
        assert_eq!(
            h.licence_notice_length as usize,
            bytes.len() - h.payload_offset as usize - h.payload_length as usize
        );
        assert_eq!(
            h.licence_notice_offset as usize,
            h.payload_offset as usize + h.payload_length as usize
        );
        let boot = cap.boot_file().expect("boot file");
        assert!(boot.flags & FLAG_BOOT_CANONICAL != 0);
    }

    #[test]
    fn deterministic_build() {
        let a = sample_builder().build().expect("build a");
        let b2 = sample_builder().build().expect("build b");
        assert_eq!(a, b2, "builder must be byte-for-byte deterministic");
    }

    #[test]
    fn whole_digest_detects_tamper() {
        let mut bytes = sample_builder().build().expect("build");
        // flip one payload byte
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        assert_eq!(parse(&bytes), Err(CapsError::BadDigest));
    }

    #[test]
    fn header_tamper_detected() {
        let mut bytes = sample_builder().build().expect("build");
        bytes[0] ^= 0xff; // magic
        assert_eq!(parse(&bytes), Err(CapsError::BadMagic));
    }

    #[test]
    fn truncation_rejected() {
        let bytes = sample_builder().build().expect("build");
        let truncated = &bytes[..bytes.len() - 1];
        assert_eq!(parse(truncated), Err(CapsError::TotalLengthMismatch));
        let tiny = &bytes[..HEADER_SIZE - 1];
        assert_eq!(parse(tiny), Err(CapsError::InputTooShort));
    }

    #[test]
    fn identity_kind_none_rejected() {
        let mut b = sample_builder();
        b.source_identity_kind = SRC_KIND_NONE;
        let bytes = b.build().expect("build");
        assert_eq!(parse(&bytes), Err(CapsError::UnsupportedIdentityKind));
    }

    #[test]
    fn sha1_oid_nonzero_padding_is_rejected() {
        let mut bytes = sha1_capsule();
        bytes[off::SRC_VALUE + OID_LEN_SHA1 as usize] = 0x01;
        refix_whole_digest(&mut bytes);
        assert_eq!(parse(&bytes), Err(CapsError::NonZeroOidPadding));
    }

    #[test]
    fn detached_builder_computes_canonical_path_digest_identity() {
        let bytes = sample_builder().build().expect("build");
        let expected = expected_detached_identity(&[
            (
                b"/system/boot/init.tos",
                b"# TOS boot text\nprint(\"hello from boot\")\n",
            ),
            (b"/system/version", b"0.2.1\n"),
        ]);
        assert_eq!(
            parse(&bytes).expect("parse").header().source_identity_value,
            expected,
            "builder must not retain a caller-supplied detached identity"
        );
    }

    #[test]
    fn detached_identity_binds_canonical_paths_not_only_contents() {
        let mut left = Builder::new();
        left.source_identity_value = [0x55; DIGEST_BYTES];
        left.add(FileSpec::new("/system/boot/init.tos", b"same\n"));
        left.add(FileSpec::new("/system/a.tos", b"same\n"));
        let mut right = Builder::new();
        right.source_identity_value = [0x55; DIGEST_BYTES];
        right.add(FileSpec::new("/system/boot/init.tos", b"same\n"));
        right.add(FileSpec::new("/system/b.tos", b"same\n"));
        let left = parse(&left.build().expect("build left"))
            .expect("parse left")
            .header()
            .source_identity_value;
        let right = parse(&right.build().expect("build right"))
            .expect("parse right")
            .header()
            .source_identity_value;
        assert_ne!(left, right, "canonical paths must bind detached identity");
    }

    #[test]
    fn digest_consistent_corrupt_detached_identity_is_rejected() {
        let mut bytes = sample_builder().build().expect("build");
        bytes[off::SRC_VALUE] ^= 0x01;
        refix_whole_digest(&mut bytes);
        assert_eq!(parse(&bytes), Err(CapsError::DetachedIdentityMismatch));
    }

    #[test]
    fn traversal_rejected() {
        let mut b = sample_builder();
        b.add(FileSpec::new("/system/../etc/passwd", b"x"));
        let bytes = b.build().expect("build");
        assert_eq!(parse(&bytes), Err(CapsError::TraversalInPath));
    }

    #[test]
    fn duplicate_path_rejected() {
        let mut b = sample_builder();
        b.add(FileSpec::new("/system/version", b"dup"));
        let bytes = b.build().expect("build");
        assert_eq!(parse(&bytes), Err(CapsError::DuplicatePath));
    }

    #[test]
    fn missing_boot_canonical_rejected() {
        let mut b = Builder::new();
        b.source_identity_kind = SRC_KIND_DETACHED;
        b.source_identity_value = [0x11; DIGEST_BYTES];
        b.add(FileSpec::new("/system/version", b"0.2.1\n"));
        let bytes = b.build().expect("build");
        assert_eq!(parse(&bytes), Err(CapsError::MissingBootCanonical));
    }

    // --- no undescribed bytes (spec §4/§4.1 rules 24-25, ADR-0017) ---

    /// Recompute `whole_capsule_digest` after a structural edit, so the test
    /// exercises the layout rules instead of stopping at the digest check.
    fn refix_whole_digest(v: &mut [u8]) {
        let mut h = Sha256::new();
        h.update(&v[0..off::WHOLE_DIGEST]);
        h.update(&[0u8; DIGEST_BYTES]);
        h.update(&v[HEADER_SIZE..]);
        let d = h.finalize();
        v[off::WHOLE_DIGEST..HEADER_SIZE].copy_from_slice(&d);
    }

    fn rd64(v: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(v[at..at + 8].try_into().unwrap())
    }

    #[test]
    fn hidden_bytes_in_name_arena_rejected() {
        // Regression: 64 arbitrary bytes appended to the name arena, with every
        // downstream offset and the whole-capsule digest fixed up, used to
        // parse as a fully valid capsule. One file set then had unboundedly
        // many "valid" encodings and the extra bytes travelled to the nucleus.
        let base = sample_builder().build().expect("build");
        let file_tbl = rd64(&base, off::FILE_TABLE_OFFSET) as usize;
        let mut v = std::vec::Vec::with_capacity(base.len() + 64);
        v.extend_from_slice(&base[..file_tbl]);
        v.extend_from_slice(&[0xee; 64]); // undescribed arena bytes
        v.extend_from_slice(&base[file_tbl..]);
        for at in [
            off::TOTAL_LENGTH,
            off::FILE_TABLE_OFFSET,
            off::PAYLOAD_OFFSET,
        ] {
            let old = rd64(&v, at);
            v[at..at + 8].copy_from_slice(&(old + 64).to_le_bytes());
        }
        refix_whole_digest(&mut v);
        assert_eq!(parse(&v), Err(CapsError::UnpackedNameArena));
    }

    #[test]
    fn gap_between_header_and_path_table_rejected() {
        // The same trick one region earlier: pad between the header and the
        // path table and move every later offset.
        let base = sample_builder().build().expect("build");
        let mut v = std::vec::Vec::with_capacity(base.len() + 8);
        v.extend_from_slice(&base[..HEADER_SIZE]);
        v.extend_from_slice(&[0x5a; 8]);
        v.extend_from_slice(&base[HEADER_SIZE..]);
        for at in [
            off::TOTAL_LENGTH,
            off::PATH_TABLE_OFFSET,
            off::FILE_TABLE_OFFSET,
            off::PAYLOAD_OFFSET,
        ] {
            let old = rd64(&v, at);
            v[at..at + 8].copy_from_slice(&(old + 8).to_le_bytes());
        }
        let lic = rd64(&v, off::LICENCE_OFFSET);
        if lic != 0 {
            v[off::LICENCE_OFFSET..off::LICENCE_OFFSET + 8]
                .copy_from_slice(&(lic + 8).to_le_bytes());
        }
        refix_whole_digest(&mut v);
        assert_eq!(parse(&v), Err(CapsError::PathTableNotAfterHeader));
    }

    #[test]
    fn arena_slack_between_names_rejected() {
        // Same total length, but the second name is shifted forward by one
        // byte, leaving one undescribed byte behind it.
        let mut v = sample_builder().build().expect("build");
        let path_tbl = rd64(&v, off::PATH_TABLE_OFFSET) as usize;
        let second = path_tbl + PATH_ENTRY_SIZE as usize; // name_offset field
        let old = u32::from_le_bytes(v[second..second + 4].try_into().unwrap());
        v[second..second + 4].copy_from_slice(&(old + 1).to_le_bytes());
        refix_whole_digest(&mut v);
        assert_eq!(parse(&v), Err(CapsError::UnpackedNameArena));
    }

    #[test]
    fn non_canonical_file_index_rejected() {
        // Path entry 1 is repointed at file 0. Under the pre-ADR-0017 rule this
        // was a duplicate reference found by an O(n²) reference count; it is now
        // a non-canonical mapping found in the same pass that reads the entry.
        let mut v = sample_builder().build().expect("build");
        let path_tbl = rd64(&v, off::PATH_TABLE_OFFSET) as usize;
        let idx_field = path_tbl + PATH_ENTRY_SIZE as usize + 8; // entry 1, file_index
        v[idx_field..idx_field + 4].copy_from_slice(&0u32.to_le_bytes());
        refix_whole_digest(&mut v);
        assert_eq!(parse(&v), Err(CapsError::NonCanonicalFileIndex));
    }

    #[test]
    fn swapped_file_indices_rejected() {
        // A genuine bijection that is not the identity: 0<->1 swapped. The old
        // reference count accepted this; ADR-0017 rejects it so that a file set
        // has exactly one valid encoding.
        let mut v = sample_builder().build().expect("build");
        let path_tbl = rd64(&v, off::PATH_TABLE_OFFSET) as usize;
        let e0 = path_tbl + 8;
        let e1 = path_tbl + PATH_ENTRY_SIZE as usize + 8;
        v[e0..e0 + 4].copy_from_slice(&1u32.to_le_bytes());
        v[e1..e1 + 4].copy_from_slice(&0u32.to_le_bytes());
        refix_whole_digest(&mut v);
        assert_eq!(parse(&v), Err(CapsError::NonCanonicalFileIndex));
    }

    #[test]
    fn packed_arena_accepted() {
        // The reference builder already emits a packed arena, so the rule must
        // not reject anything it produces.
        let bytes = sample_builder().build().expect("build");
        assert!(parse(&bytes).is_ok());
    }

    #[test]
    fn many_files_round_trip() {
        let mut b = Builder::new();
        b.source_identity_kind = SRC_KIND_DETACHED;
        b.source_identity_value = [0x22; DIGEST_BYTES];
        b.add(FileSpec::new("/system/boot/init.tos", b"# boot\n"));
        for i in 0..64 {
            b.add(FileSpec::new(
                &format!("/system/lib/file{i:03}.tos"),
                &format!("content {i}\n").into_bytes(),
            ));
        }
        let bytes = b.build().expect("build");
        let cap = parse(&bytes).expect("parse");
        assert_eq!(cap.file_count(), 65);
        assert_eq!(
            cap.find(b"/system/lib/file042.tos").unwrap().content,
            b"content 42\n"
        );
    }
}
