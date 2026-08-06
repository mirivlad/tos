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
pub const ARCH_SPEC_VERSION: u32 = 0x0002_01; // packed '0.2.1'
pub const BUILDER_VERSION: u32 = 1;

pub const SRC_KIND_NONE: u8 = 0;
pub const SRC_KIND_GIT: u8 = 1;
pub const SRC_KIND_DETACHED: u8 = 2;

pub const FLAG_BOOT_CANONICAL: u32 = 1 << 0;
pub const FLAG_LICENCE_NOTICE: u32 = 1 << 1;
/// All flag bits defined for v1.
pub const ALL_KNOWN_FLAGS: u32 = FLAG_BOOT_CANONICAL | FLAG_LICENCE_NOTICE;

/// Canonical boot text path required by Stage 1.
pub const BOOT_PATH: &[u8] = b"/system/boot/init.tos";

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
    pub const SRC_RESERVED: usize = 97;
    pub const SRC_DIGEST: usize = 104;
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
    pub source_identity_digest: [u8; DIGEST_BYTES],
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
    NameOutOfArena,
    BadFileFlags,
    NonZeroReservedEntry,
    UnsortedFileTable,
    PayloadOverlap,
    PayloadGap,
    ZeroFileCount,
    BadDigest,
    BadWholeDigest,
    UnsupportedIdentityKind,
    LicenceOutOfBounds,
    MissingBootCanonical,
    DuplicateBootCanonical,
    BadBootCanonicalName,
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
        b[at], b[at + 1], b[at + 2], b[at + 3], b[at + 4], b[at + 5], b[at + 6], b[at + 7],
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
    pub reserved: u32,
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
    FileEntry {
        content_offset: rd_u64(b, at),
        content_length: rd_u64(b, at + 8),
        content_digest: dg,
        file_flags: rd_u32(b, at + 48),
        reserved: rd_u32(b, at + 52),
    }
}

/// Decode the header region (caller guarantees `len >= HEADER_SIZE`).
fn decode_header(b: &[u8]) -> Header {
    let mut magic = [0u8; 8];
    magic.copy_from_slice(&b[off::MAGIC..off::MAGIC + 8]);
    let mut format_uuid = [0u8; 16];
    format_uuid.copy_from_slice(&b[off::UUID..off::UUID + 16]);
    let mut sd = [0u8; DIGEST_BYTES];
    sd.copy_from_slice(&b[off::SRC_DIGEST..off::SRC_DIGEST + DIGEST_BYTES]);
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
        source_identity_digest: sd,
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
        FileIter {
            cap: *self,
            idx: 0,
        }
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
    if bytes[off::RESERVED..off::RESERVED + 2].iter().any(|&b| b != 0) {
        return Err(CapsError::NonZeroReservedHeader);
    }
    if bytes[off::SRC_RESERVED..off::SRC_DIGEST].iter().any(|&b| b != 0) {
        return Err(CapsError::NonZeroReservedHeader);
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
    let path_tbl_start = h.path_table_offset as usize;
    if path_tbl_start < HEADER_SIZE {
        return Err(CapsError::LayoutMismatch);
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
    let payload_end = payload_start
        .checked_add(h.payload_length as usize)
        .ok_or(CapsError::RegionOverflow)?;
    if payload_end > bytes.len() {
        return Err(CapsError::LayoutMismatch);
    }
    let tail = bytes.len() - payload_end;
    if tail != h.licence_notice_length as usize {
        return Err(CapsError::LicenceOutOfBounds);
    }
    if h.licence_notice_length != 0 && h.licence_notice_offset as usize != payload_end {
        return Err(CapsError::LicenceOutOfBounds);
    }

    let name_arena_len = file_tbl_start - name_start;

    // --- path table ---
    let mut prev_name: Option<&[u8]> = None;
    let mut canonical_count = 0usize;
    let mut canonical_name_ok = false;
    for i in 0..h.path_table_count as usize {
        let pe = decode_path_entry(bytes, path_tbl_start + i * PATH_ENTRY_SIZE as usize);
        if pe.flags & !ALL_KNOWN_FLAGS != 0 {
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
        if end > name_arena_len {
            return Err(CapsError::NameOutOfArena);
        }
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
        if pe.flags & FLAG_BOOT_CANONICAL != 0 {
            canonical_count += 1;
            if name == BOOT_PATH {
                canonical_name_ok = true;
            }
        }
    }

    // --- file table ---
    let mut prev_end: Option<u64> = None;
    for i in 0..h.file_count as usize {
        let fe = decode_file_entry(bytes, file_tbl_start + i * FILE_ENTRY_SIZE as usize);
        if fe.file_flags & !ALL_KNOWN_FLAGS != 0 {
            return Err(CapsError::BadFileFlags);
        }
        if fe.reserved != 0 {
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
        b.source_identity_digest = [0x11; DIGEST_BYTES];
        b.add(FileSpec::new(
            "/system/boot/init.tos",
            b"# TOS boot text\nprint(\"hello from boot\")\n",
        ));
        b.add(FileSpec::new("/system/version", b"0.2.1\n"));
        b
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
        assert_eq!(h.licence_notice_length as usize, bytes.len() - h.payload_offset as usize - h.payload_length as usize);
        assert_eq!(h.licence_notice_offset as usize, h.payload_offset as usize + h.payload_length as usize);
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
        b.source_identity_digest = [0x11; DIGEST_BYTES];
        b.add(FileSpec::new("/system/version", b"0.2.1\n"));
        let bytes = b.build().expect("build");
        assert_eq!(parse(&bytes), Err(CapsError::MissingBootCanonical));
    }

    #[test]
    fn many_files_round_trip() {
        let mut b = Builder::new();
        b.source_identity_kind = SRC_KIND_DETACHED;
        b.source_identity_digest = [0x22; DIGEST_BYTES];
        b.add(FileSpec::new(
            "/system/boot/init.tos",
            b"# boot\n",
        ));
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