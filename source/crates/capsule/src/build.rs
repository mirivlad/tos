// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic capsule builder (host-only feature).
//!
//! The builder is intentionally a dumb byte-layout tool: it does not validate
//! paths, flags or canonicality — the parser does. This asymmetry lets tests
//! construct deliberately invalid capsules (traversal paths, duplicate names,
//! missing boot-canonical files) and prove the parser rejects them. Output is
//! byte-for-byte deterministic for identical inputs: no timestamps, no
//! randomness, stable sort by path bytes.

use std::vec::Vec;

use tos_hash::Sha256;

use crate::{
    update_detached_identity, ALIGNMENT, ARCH_SPEC_VERSION, BOOT_PATH, BUILDER_VERSION,
    DETACHED_IDENTITY_DOMAIN, DIGEST_BYTES, FILE_ENTRY_SIZE, FILE_KNOWN_FLAGS, FLAG_BOOT_CANONICAL,
    FORMAT_UUID, FORMAT_VERSION, HEADER_SIZE, MAGIC, MAX_CAPSULE_BYTES, MAX_FILE_COUNT,
    MAX_LICENCE_NOTICE_BYTES, MAX_NAME_ARENA_BYTES, MAX_PATH_BYTES, OID_ALG_NONE, PATH_ENTRY_SIZE,
    SRC_KIND_DETACHED,
};

/// One file to place in the capsule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSpec {
    pub path: Vec<u8>,
    pub content: Vec<u8>,
    pub flags: u32,
}

impl FileSpec {
    /// New spec; the boot-canonical flag is set automatically for the canonical
    /// boot path (Stage 1 semantics), otherwise the spec carries no flags.
    pub fn new(path: &str, content: &[u8]) -> Self {
        let mut flags = 0u32;
        if path.as_bytes() == BOOT_PATH {
            flags |= FLAG_BOOT_CANONICAL;
        }
        Self {
            path: path.as_bytes().to_vec(),
            content: content.to_vec(),
            flags,
        }
    }
}

/// Builder failure: only arithmetic overflow is possible (absurd inputs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    Overflow,
    CapsuleTooLarge,
    FileCountTooLarge,
    PathTooLong,
    NameArenaTooLarge,
    LicenceNoticeTooLarge,
}

/// Capsule builder.
#[derive(Clone, Debug)]
pub struct Builder {
    pub source_identity_kind: u8,
    pub source_oid_alg: u8,
    pub source_oid_length: u8,
    /// Explicit raw Git OID input for `SRC_KIND_GIT`. For `SRC_KIND_DETACHED`,
    /// `build()` ignores this compatibility field and computes the accepted
    /// ADR-0018 source-set identity from the canonical file set.
    pub source_identity_value: [u8; 32],
    pub builder_version: u32,
    files: Vec<FileSpec>,
    licence_notice: Vec<u8>,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            source_identity_kind: SRC_KIND_DETACHED,
            source_oid_alg: OID_ALG_NONE,
            source_oid_length: 0,
            source_identity_value: [0u8; 32],
            builder_version: BUILDER_VERSION,
            files: Vec::new(),
            licence_notice: Vec::new(),
        }
    }

    pub fn add(&mut self, spec: FileSpec) {
        // Reserved bits are stripped silently; the parser is the authority.
        let mut spec = spec;
        spec.flags &= FILE_KNOWN_FLAGS;
        self.files.push(spec);
    }

    pub fn set_licence_notice(&mut self, bytes: Vec<u8>) {
        self.licence_notice = bytes;
    }

    /// Build the capsule. Deterministic for identical inputs.
    pub fn build(&self) -> Result<Vec<u8>, BuildError> {
        if self.files.is_empty() {
            // an empty capsule can never validate; reject at build time
            return Err(BuildError::Overflow);
        }
        if self.files.len() > MAX_FILE_COUNT as usize {
            return Err(BuildError::FileCountTooLarge);
        }
        if self.licence_notice.len() > MAX_LICENCE_NOTICE_BYTES {
            return Err(BuildError::LicenceNoticeTooLarge);
        }
        if self
            .files
            .iter()
            .any(|spec| spec.path.len() > MAX_PATH_BYTES)
        {
            return Err(BuildError::PathTooLong);
        }
        // Stable sort by path bytes (byte order, as the spec requires).
        let mut sorted: Vec<usize> = (0..self.files.len()).collect();
        sorted.sort_by(|&a, &b| self.files[a].path.cmp(&self.files[b].path));

        let count = self.files.len();
        let path_tbl_bytes = count
            .checked_mul(PATH_ENTRY_SIZE as usize)
            .ok_or(BuildError::Overflow)?;
        let name_total: usize = sorted
            .iter()
            .map(|&i| self.files[i].path.len())
            .try_fold(0usize, |acc, n| acc.checked_add(n))
            .ok_or(BuildError::Overflow)?;
        if name_total > MAX_NAME_ARENA_BYTES {
            return Err(BuildError::NameArenaTooLarge);
        }
        let file_tbl_bytes = count
            .checked_mul(FILE_ENTRY_SIZE as usize)
            .ok_or(BuildError::Overflow)?;
        let payload_bytes: usize = sorted
            .iter()
            .map(|&i| self.files[i].content.len())
            .try_fold(0usize, |acc, n| acc.checked_add(n))
            .ok_or(BuildError::Overflow)?;

        let source_identity_value = if self.source_identity_kind == SRC_KIND_DETACHED {
            let mut identity = Sha256::new();
            identity.update(DETACHED_IDENTITY_DOMAIN);
            for &fi in &sorted {
                let spec = &self.files[fi];
                let path_length =
                    u32::try_from(spec.path.len()).map_err(|_| BuildError::Overflow)?;
                let mut content_hash = Sha256::new();
                content_hash.update(&spec.content);
                update_detached_identity(
                    &mut identity,
                    &spec.path,
                    path_length,
                    &content_hash.finalize(),
                );
            }
            identity.finalize()
        } else {
            self.source_identity_value
        };

        let path_tbl_offset = HEADER_SIZE;
        let name_start = path_tbl_offset
            .checked_add(path_tbl_bytes)
            .ok_or(BuildError::Overflow)?;
        let file_tbl_offset = name_start
            .checked_add(name_total)
            .ok_or(BuildError::Overflow)?;
        let payload_offset = file_tbl_offset
            .checked_add(file_tbl_bytes)
            .ok_or(BuildError::Overflow)?;
        let payload_end = payload_offset
            .checked_add(payload_bytes)
            .ok_or(BuildError::Overflow)?;
        let total_length = payload_end
            .checked_add(self.licence_notice.len())
            .ok_or(BuildError::Overflow)?;
        if total_length > MAX_CAPSULE_BYTES {
            return Err(BuildError::CapsuleTooLarge);
        }
        let count_u32 = u32::try_from(count).map_err(|_| BuildError::FileCountTooLarge)?;

        let mut out = std::vec![0u8; total_length];

        // --- header ---
        w_bytes(&mut out, 0, &MAGIC);
        w_bytes(&mut out, 8, &FORMAT_UUID);
        w_u16(&mut out, 24, FORMAT_VERSION);
        w_u16(&mut out, 26, HEADER_SIZE as u16);
        w_u16(&mut out, 28, ALIGNMENT);
        w_u64(&mut out, 32, total_length as u64);
        w_u64(&mut out, 40, path_tbl_offset as u64);
        w_u32(&mut out, 48, count_u32);
        w_u32(&mut out, 52, PATH_ENTRY_SIZE);
        w_u64(&mut out, 56, file_tbl_offset as u64);
        w_u32(&mut out, 64, count_u32);
        w_u32(&mut out, 68, FILE_ENTRY_SIZE);
        w_u64(&mut out, 72, payload_offset as u64);
        w_u64(&mut out, 80, payload_bytes as u64);
        w_u32(&mut out, 88, ARCH_SPEC_VERSION);
        w_u32(&mut out, 92, self.builder_version);
        out[96] = self.source_identity_kind;
        out[97] = self.source_oid_alg;
        out[98] = self.source_oid_length;
        w_bytes(&mut out, 100, &source_identity_value);
        if !self.licence_notice.is_empty() {
            w_u64(&mut out, 136, payload_end as u64);
            w_u64(&mut out, 144, self.licence_notice.len() as u64);
        }

        // --- path table + name arena ---
        let mut name_cursor = 0usize;
        for (idx, &fi) in sorted.iter().enumerate() {
            let spec = &self.files[fi];
            let at = path_tbl_offset + idx * PATH_ENTRY_SIZE as usize;
            w_u32(&mut out, at, name_cursor as u32);
            let path_length = u32::try_from(spec.path.len()).map_err(|_| BuildError::Overflow)?;
            w_u32(&mut out, at + 4, path_length);
            w_u32(&mut out, at + 8, idx as u32);
            w_u32(&mut out, at + 12, spec.flags);
            out[name_start + name_cursor..name_start + name_cursor + spec.path.len()]
                .copy_from_slice(&spec.path);
            name_cursor += spec.path.len();
        }

        // --- file table + payload ---
        let mut content_cursor = 0usize;
        for (idx, &fi) in sorted.iter().enumerate() {
            let spec = &self.files[fi];
            let at = file_tbl_offset + idx * FILE_ENTRY_SIZE as usize;
            w_u64(&mut out, at, content_cursor as u64);
            w_u64(&mut out, at + 8, spec.content.len() as u64);
            let mut h = Sha256::new();
            h.update(&spec.content);
            let d = h.finalize();
            w_bytes(&mut out, at + 16, &d);
            w_u32(&mut out, at + 48, spec.flags);
            // reserved at +52 stays zero
            out[payload_offset + content_cursor
                ..payload_offset + content_cursor + spec.content.len()]
                .copy_from_slice(&spec.content);
            content_cursor += spec.content.len();
        }
        if !self.licence_notice.is_empty() {
            out[payload_end..].copy_from_slice(&self.licence_notice);
        }

        // --- whole-capsule digest: bytes[0..152] || zeros[32] || bytes[184..] ---
        let mut h = Sha256::new();
        h.update(&out[0..152]);
        h.update(&[0u8; DIGEST_BYTES]);
        h.update(&out[HEADER_SIZE..]);
        let d = h.finalize();
        w_bytes(&mut out, 152, &d);

        Ok(out)
    }
}

fn w_u16(out: &mut [u8], at: usize, v: u16) {
    out[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn w_u32(out: &mut [u8], at: usize, v: u32) {
    out[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn w_u64(out: &mut [u8], at: usize, v: u64) {
    out[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn w_bytes(out: &mut [u8], at: usize, v: &[u8]) {
    out[at..at + v.len()].copy_from_slice(v);
}
