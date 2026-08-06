// SPDX-License-Identifier: GPL-3.0-or-later
//! TOS boot ABI v1 — versioned loader-to-nucleus handoff.
//!
//! The byte layout is normative (see `interfaces/boot/BOOT_ABI_V1.md`); the
//! structs below use `repr(C)` with explicit little-endian widths and are never
//! treated as a stable ABI on their own. Validation is total over arbitrary
//! bytes: every function returns a structured error instead of panicking.

#![no_std]

/// Magic constant: `u64::from_le_bytes(*b"TOSBOOT1")`.
pub const MAGIC: u64 = 0x3154_4F4F_4253_4F54;
/// Protocol identity UUID (RFC 4112 order).
pub const PROTOCOL_UUID: [u8; 16] = [
    0xe2, 0xe8, 0xc1, 0x5a, 0x6c, 0x4b, 0x4d, 0x11, 0x9a, 0x2c, 0x8f, 0x3b, 0x1a, 0x2c, 0x4d, 0x5e,
];
pub const MAJOR: u16 = 1;
pub const MINOR: u16 = 0;
/// Size of the v1 structure.
pub const STRUCT_SIZE: u32 = 224;
pub const ARCH_X86_64: u32 = 1;
pub const BOOT_MODE_NORMAL: u32 = 0;
pub const FB_FORMAT_NONE: u32 = 0;
/// Memory-range descriptor size (bytes).
pub const MEM_DESC_SIZE: u64 = 24;

// TOS memory-range types (BOOT_ABI_V1.md §5).
pub const MEM_USABLE: u32 = 1;
pub const MEM_RESERVED: u32 = 2;
pub const MEM_ACPI_RECLAIM: u32 = 3;
pub const MEM_ACPI_NVS: u32 = 4;
pub const MEM_MMIO: u32 = 5;

// Capsule source-identity kinds (mirrors capsule crate; kept local so the boot
// ABI crate has no dependency on the capsule format crate).
pub const SRC_KIND_GIT: u8 = 1;
pub const SRC_KIND_DETACHED: u8 = 2;

/// Identity OID algorithms mirrored from the capsule header (v1): the raw
/// `capsule_source_identity` is a git object id when kind is GIT.
pub const OID_ALG_NONE: u8 = 0;
pub const OID_ALG_SHA1: u8 = 1;
pub const OID_ALG_SHA256: u8 = 2;
pub const OID_LEN_SHA1: u8 = 20;
pub const OID_LEN_SHA256: u8 = 32;

/// QEMU `isa-debug-exit` I/O port; a written u8 makes QEMU exit with
/// `(value << 1) | 1`.
pub const RESULT_PORT: u16 = 0x501;
pub const RESULT_HALT_OK: u8 = 0x10;
pub const RESULT_PANIC: u8 = 0x20;
pub const RESULT_CAPSULE_INVALID: u8 = 0x21;
pub const RESULT_ABI_INVALID: u8 = 0x22;
pub const RESULT_MEMORY_INVALID: u8 = 0x23;

/// Memory-range descriptor (24 bytes, little-endian).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryRange {
    pub phys_start: u64,
    pub phys_length: u64,
    pub ty: u32,
    pub flags: u32,
}

impl MemoryRange {
    /// Exclusive end of the range, or `None` when `phys_start + phys_length`
    /// wraps past `u64::MAX`.
    ///
    /// A wrapping descriptor is not a representable physical range: BOOT_ABI_V1
    /// §8 rule 6 rejects a memory map region outside addressable bounds. The
    /// checked form is mandatory here — an unchecked `+` panics in a debug
    /// build (the validator must be total, AGENTS.md §8) and silently wraps in
    /// a release build, which previously let an overlapping map pass
    /// [`BootInfo::check_memory_map`].
    pub fn checked_end(&self) -> Option<u64> {
        self.phys_start.checked_add(self.phys_length)
    }
}

/// BootInfo v1 (224 bytes, little-endian, 8-aligned).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct BootInfo {
    pub magic: u64,
    pub protocol_uuid: [u8; 16],
    pub major: u16,
    pub minor: u16,
    pub total_size: u32,
    pub architecture_id: u32,
    pub boot_mode: u32,
    pub memory_map_phys: u64,
    pub memory_map_length: u64,
    pub memory_desc_size: u64,
    pub framebuffer_phys: u64,
    pub framebuffer_width: u32,
    pub framebuffer_height: u32,
    pub framebuffer_pitch: u32,
    pub framebuffer_format: u32,
    pub capsule_phys: u64,
    pub capsule_length: u64,
    pub capsule_digest: [u8; 32],
    pub capsule_identity_kind: u8,
    pub capsule_oid_alg: u8,
    pub capsule_oid_length: u8,
    pub reserved: [u8; 5],
    pub capsule_source_identity: [u8; 32],
    pub acpi_rsdp: u64,
    pub smbios: u64,
    pub next: u64,
    pub reserved2: [u8; 24],
}

impl BootInfo {
    pub const fn new() -> Self {
        Self {
            magic: MAGIC,
            protocol_uuid: PROTOCOL_UUID,
            major: MAJOR,
            minor: MINOR,
            total_size: STRUCT_SIZE,
            architecture_id: ARCH_X86_64,
            boot_mode: BOOT_MODE_NORMAL,
            memory_map_phys: 0,
            memory_map_length: 0,
            memory_desc_size: MEM_DESC_SIZE,
            framebuffer_phys: 0,
            framebuffer_width: 0,
            framebuffer_height: 0,
            framebuffer_pitch: 0,
            framebuffer_format: FB_FORMAT_NONE,
            capsule_phys: 0,
            capsule_length: 0,
            capsule_digest: [0; 32],
            // Detached identity is the conservative default for a capsule
            // without a git provenance record (SRC_KIND_GIT=1, DETACHED=2;
            // 0 is rejected by validate_bytes).
            capsule_identity_kind: SRC_KIND_DETACHED,
            capsule_oid_alg: OID_ALG_NONE,
            capsule_oid_length: 0,
            reserved: [0; 5],
            capsule_source_identity: [0; 32],
            acpi_rsdp: 0,
            smbios: 0,
            next: 0,
            reserved2: [0; 24],
        }
    }
}

impl Default for BootInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Structured validation error for the boot ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootInfoError {
    /// `magic` mismatch.
    BadMagic,
    /// protocol UUID mismatch.
    BadUuid,
    /// major version unsupported.
    UnsupportedMajor,
    /// minor version unknown for this major.
    UnsupportedMinor,
    /// `total_size < STRUCT_SIZE`.
    ShortTotalSize,
    /// reserved trailing bytes (beyond `STRUCT_SIZE` within `total_size`)
    /// non-zero.
    NonZeroReserved,
    /// in-struct reserved blocks `reserved`/`reserved2` non-zero.
    NonZeroReservedFields,
    /// `architecture_id` not `ARCH_X86_64`.
    BadArchitecture,
    /// `boot_mode` not `BOOT_MODE_NORMAL`.
    BadBootMode,
    /// `memory_desc_size` not `MEM_DESC_SIZE`.
    BadDescriptorSize,
    /// memory map is empty.
    EmptyMemoryMap,
    /// descriptor length zero.
    ZeroLengthRange,
    /// descriptors not sorted by `phys_start`.
    UnsortedMemoryMap,
    /// descriptors overlap.
    OverlappingMemoryMap,
    /// framebuffer fields are inconsistent (present vs absent).
    BadFramebuffer,
    /// capsule range arithmetic overflow.
    CapsuleRangeOverflow,
    /// capsule absent (phys == 0 or length == 0).
    ZeroCapsule,
    /// capsule range not contained in any memory-map descriptor.
    CapsuleOutOfMemoryMap,
    /// a descriptor's `phys_start + phys_length` wraps past `u64::MAX`, so the
    /// range lies outside addressable bounds (BOOT_ABI_V1 §8 rule 6).
    MemoryRangeOverflow,
    /// `capsule_identity_kind` not SRC_KIND_GIT or SRC_KIND_DETACHED.
    UnsupportedCapsuleIdentityKind,
    /// `next` non-zero (reserved extension pointer must be 0).
    NonZeroNext,
}

impl core::fmt::Display for BootInfoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl BootInfo {
    /// Validate a raw byte slice as a BootInfo v1 image. The slice length must
    /// be at least `total_size`; bytes beyond `STRUCT_SIZE` within
    /// `total_size` must be zero. Total over arbitrary bytes.
    pub fn validate_bytes(bytes: &[u8]) -> Result<(), BootInfoError> {
        if bytes.len() < STRUCT_SIZE as usize {
            return Err(BootInfoError::ShortTotalSize);
        }
        if u64::from_le_bytes(bytes[0..8].try_into().unwrap()) != MAGIC {
            return Err(BootInfoError::BadMagic);
        }
        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&bytes[8..24]);
        if uuid != PROTOCOL_UUID {
            return Err(BootInfoError::BadUuid);
        }
        let major = u16::from_le_bytes(bytes[24..26].try_into().unwrap());
        let minor = u16::from_le_bytes(bytes[26..28].try_into().unwrap());
        if major != MAJOR {
            return Err(BootInfoError::UnsupportedMajor);
        }
        if minor != MINOR {
            return Err(BootInfoError::UnsupportedMinor);
        }
        let total_size = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        if (total_size as usize) < STRUCT_SIZE as usize {
            return Err(BootInfoError::ShortTotalSize);
        }
        if (total_size as usize) > bytes.len() {
            return Err(BootInfoError::ShortTotalSize);
        }
        if bytes[STRUCT_SIZE as usize..total_size as usize]
            .iter()
            .any(|&b| b != 0)
        {
            return Err(BootInfoError::NonZeroReserved);
        }
        let arch = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
        if arch != ARCH_X86_64 {
            return Err(BootInfoError::BadArchitecture);
        }
        let boot_mode = u32::from_le_bytes(bytes[36..40].try_into().unwrap());
        if boot_mode != BOOT_MODE_NORMAL {
            return Err(BootInfoError::BadBootMode);
        }
        let desc_size = u64::from_le_bytes(bytes[56..64].try_into().unwrap());
        if desc_size != MEM_DESC_SIZE {
            return Err(BootInfoError::BadDescriptorSize);
        }

        // --- framebuffer consistency ---
        let fb_phys = u64::from_le_bytes(bytes[64..72].try_into().unwrap());
        let fb_w = u32::from_le_bytes(bytes[72..76].try_into().unwrap());
        let fb_h = u32::from_le_bytes(bytes[76..80].try_into().unwrap());
        let fb_pitch = u32::from_le_bytes(bytes[80..84].try_into().unwrap());
        let fb_fmt = u32::from_le_bytes(bytes[84..88].try_into().unwrap());
        let fb_present = fb_phys != 0;
        if fb_present {
            if fb_w == 0 || fb_h == 0 || fb_pitch == 0 || fb_fmt == FB_FORMAT_NONE {
                return Err(BootInfoError::BadFramebuffer);
            }
        } else if fb_w != 0 || fb_h != 0 || fb_pitch != 0 || fb_fmt != FB_FORMAT_NONE {
            return Err(BootInfoError::BadFramebuffer);
        }

        // --- capsule identity kind + oid algorithm ---
        let id_kind = bytes[136];
        if id_kind != SRC_KIND_GIT && id_kind != SRC_KIND_DETACHED {
            return Err(BootInfoError::UnsupportedCapsuleIdentityKind);
        }
        let oid_alg = bytes[137];
        let oid_len = bytes[138];
        match id_kind {
            SRC_KIND_GIT => {
                let ok = match (oid_alg, oid_len) {
                    (OID_ALG_SHA1, OID_LEN_SHA1) => true,
                    (OID_ALG_SHA256, OID_LEN_SHA256) => true,
                    _ => false,
                };
                if !ok {
                    return Err(BootInfoError::UnsupportedCapsuleIdentityKind);
                }
            }
            _ => {
                if oid_alg != OID_ALG_NONE || oid_len != 0 {
                    return Err(BootInfoError::UnsupportedCapsuleIdentityKind);
                }
            }
        }

        // --- in-struct reserved blocks and extension fields ---
        if bytes[139..144].iter().any(|&b| b != 0) {
            return Err(BootInfoError::NonZeroReservedFields);
        }
        let next = u64::from_le_bytes(bytes[192..200].try_into().unwrap());
        if next != 0 {
            return Err(BootInfoError::NonZeroNext);
        }
        if bytes[200..224].iter().any(|&b| b != 0) {
            return Err(BootInfoError::NonZeroReservedFields);
        }
        Ok(())
    }

    /// Validate the memory map (descriptor array at `memory_map_phys` of
    /// `memory_map_length` bytes) and the capsule range.
    pub fn validate_memory_and_capsule(&self) -> Result<(), BootInfoError> {
        if self.memory_map_length % MEM_DESC_SIZE != 0 {
            return Err(BootInfoError::BadDescriptorSize);
        }
        let desc_count = self.memory_map_length / MEM_DESC_SIZE;
        if desc_count == 0 {
            return Err(BootInfoError::EmptyMemoryMap);
        }
        if self.capsule_phys == 0 || self.capsule_length == 0 {
            return Err(BootInfoError::ZeroCapsule);
        }
        self.capsule_phys
            .checked_add(self.capsule_length)
            .ok_or(BootInfoError::CapsuleRangeOverflow)?;
        // Ordering/overlap checks run on a caller-provided slice
        // (check_memory_map / check_capsule_in_memory). Nothing is dereferenced
        // here: this crate has no address space of its own yet.
        Ok(())
    }

    /// Check a decoded memory-map slice: sorted, non-overlapping,
    /// non-zero-length. `descs` must cover `memory_map_length` bytes exactly.
    pub fn check_memory_map(&self, descs: &[MemoryRange]) -> Result<(), BootInfoError> {
        if descs.is_empty() {
            return Err(BootInfoError::EmptyMemoryMap);
        }
        if descs[0].phys_length == 0 {
            return Err(BootInfoError::ZeroLengthRange);
        }
        let mut prev_start = descs[0].phys_start;
        // A wrapping range is rejected before it can be compared: with an
        // unchecked end, `prev_end` wrapped below `prev_start` and every later
        // overlap check passed vacuously.
        let mut prev_end = descs[0]
            .checked_end()
            .ok_or(BootInfoError::MemoryRangeOverflow)?;
        for d in &descs[1..] {
            if d.phys_length == 0 {
                return Err(BootInfoError::ZeroLengthRange);
            }
            let end = d
                .checked_end()
                .ok_or(BootInfoError::MemoryRangeOverflow)?;
            // Before the previous region's start => strictly out of order.
            if d.phys_start < prev_start {
                return Err(BootInfoError::UnsortedMemoryMap);
            }
            // Within or overlapping the previous region.
            if d.phys_start < prev_end {
                return Err(BootInfoError::OverlappingMemoryMap);
            }
            prev_start = d.phys_start;
            prev_end = end;
        }
        Ok(())
    }

    /// Verify the capsule range lies inside one declared memory range.
    pub fn check_capsule_in_memory(&self, descs: &[MemoryRange]) -> Result<(), BootInfoError> {
        let cap_end = self
            .capsule_phys
            .checked_add(self.capsule_length)
            .ok_or(BootInfoError::CapsuleRangeOverflow)?;
        // Fail closed on a malformed descriptor instead of comparing against a
        // wrapped end: `phys_start + phys_length` panicked in debug and wrapped
        // in release, so a range ending past `u64::MAX` produced an arbitrary
        // containment verdict.
        let mut inside = false;
        for d in descs {
            let d_end = d
                .checked_end()
                .ok_or(BootInfoError::MemoryRangeOverflow)?;
            if d.phys_start <= self.capsule_phys && cap_end <= d_end {
                inside = true;
            }
        }
        if !inside {
            return Err(BootInfoError::CapsuleOutOfMemoryMap);
        }
        Ok(())
    }
}

/// Fix-up: internal constant alias used for capsule identity kind matching.
/// (Kept as a private helper so the public `SRC_KIND_*` names can use the
/// capsule-compatible values.)
pub const SRC_GID: u8 = boot_protocol_src_kind();
const fn boot_protocol_src_kind() -> u8 {
    // SRC_KIND_GIT == 1 per capsule format v1.
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_size_is_224() {
        assert_eq!(core::mem::size_of::<BootInfo>(), 224);
        assert_eq!(core::mem::size_of::<MemoryRange>(), 24);
    }

    #[test]
    fn default_validates() {
        let bi = BootInfo::new();
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(BootInfo::validate_bytes(bytes), Ok(()));
    }

    #[test]
    fn bad_magic_rejected() {
        let mut bi = BootInfo::new();
        bi.magic = 0;
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(BootInfo::validate_bytes(bytes), Err(BootInfoError::BadMagic));
    }

    #[test]
    fn truncated_rejected() {
        let bi = BootInfo::new();
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 223)
        };
        assert_eq!(
            BootInfo::validate_bytes(bytes),
            Err(BootInfoError::ShortTotalSize)
        );
    }

    #[test]
    fn bad_boot_mode_rejected() {
        let mut bi = BootInfo::new();
        bi.boot_mode = 1;
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(BootInfo::validate_bytes(bytes), Err(BootInfoError::BadBootMode));
    }

    #[test]
    fn next_must_be_zero() {
        let mut bi = BootInfo::new();
        bi.next = 0x1234;
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(BootInfo::validate_bytes(bytes), Err(BootInfoError::NonZeroNext));
    }

    #[test]
    fn framebuffer_absent_consistency() {
        let bi = BootInfo::new(); // all fb fields zero
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(BootInfo::validate_bytes(bytes), Ok(()));
    }

    #[test]
    fn framebuffer_partial_rejected() {
        let mut bi = BootInfo::new();
        bi.framebuffer_phys = 0;
        bi.framebuffer_width = 800; // width without phys -> inconsistent
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(BootInfo::validate_bytes(bytes), Err(BootInfoError::BadFramebuffer));
    }

    #[test]
    fn framebuffer_present_requires_format_and_dims() {
        let mut bi = BootInfo::new();
        bi.framebuffer_phys = 0x1000;
        bi.framebuffer_width = 800;
        bi.framebuffer_height = 600;
        bi.framebuffer_pitch = 3200;
        bi.framebuffer_format = 1; // not FB_FORMAT_NONE
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(BootInfo::validate_bytes(bytes), Ok(()));

        bi.framebuffer_format = FB_FORMAT_NONE; // present but format none -> reject
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(BootInfo::validate_bytes(bytes), Err(BootInfoError::BadFramebuffer));
    }

    #[test]
    fn identity_kind_must_be_git_or_detached() {
        let mut bi = BootInfo::new();
        bi.capsule_identity_kind = 0; // SRC_KIND_NONE not allowed
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(
            BootInfo::validate_bytes(bytes),
            Err(BootInfoError::UnsupportedCapsuleIdentityKind)
        );

        let mut bi = BootInfo::new();
        bi.capsule_identity_kind = 99;
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(
            BootInfo::validate_bytes(bytes),
            Err(BootInfoError::UnsupportedCapsuleIdentityKind)
        );
    }

    #[test]
    fn in_struct_reserved_rejected() {
        let mut bi = BootInfo::new();
        bi.reserved[0] = 1;
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(BootInfo::validate_bytes(bytes), Err(BootInfoError::NonZeroReservedFields));

        let mut bi = BootInfo::new();
        bi.reserved2[0] = 1;
        let bytes = unsafe {
            core::slice::from_raw_parts(&bi as *const BootInfo as *const u8, 224)
        };
        assert_eq!(BootInfo::validate_bytes(bytes), Err(BootInfoError::NonZeroReservedFields));
    }

    #[test]
    fn memory_map_order_and_overlap() {
        let descs = [
            MemoryRange { phys_start: 0x1000, phys_length: 0x1000, ty: 1, flags: 1 },
            MemoryRange { phys_start: 0x2000, phys_length: 0x1000, ty: 1, flags: 1 },
        ];
        let bi = BootInfo::new();
        assert_eq!(bi.check_memory_map(&descs), Ok(()));

        let unsorted = [
            MemoryRange { phys_start: 0x2000, phys_length: 0x1000, ty: 1, flags: 1 },
            MemoryRange { phys_start: 0x1000, phys_length: 0x1000, ty: 1, flags: 1 },
        ];
        assert_eq!(
            bi.check_memory_map(&unsorted),
            Err(BootInfoError::UnsortedMemoryMap)
        );

        let overlap = [
            MemoryRange { phys_start: 0x1000, phys_length: 0x2000, ty: 1, flags: 1 },
            MemoryRange { phys_start: 0x2000, phys_length: 0x1000, ty: 1, flags: 1 },
        ];
        assert_eq!(
            bi.check_memory_map(&overlap),
            Err(BootInfoError::OverlappingMemoryMap)
        );

        let zero = [MemoryRange { phys_start: 0x1000, phys_length: 0, ty: 1, flags: 1 }];
        assert_eq!(bi.check_memory_map(&zero), Err(BootInfoError::ZeroLengthRange));
    }

    // --- regression: unchecked `phys_start + phys_length` (Stage 1 hardening) ---
    //
    // These run in a debug profile under plain `cargo test`, where the previous
    // unchecked addition panicked with "attempt to add with overflow", and in a
    // release profile under `cargo test --release`, where it wrapped silently.
    // Both profiles must now produce the same structured error.

    #[test]
    fn checked_end_reports_overflow() {
        let ok = MemoryRange { phys_start: 0x1000, phys_length: 0x1000, ty: 1, flags: 0 };
        assert_eq!(ok.checked_end(), Some(0x2000));
        let wraps = MemoryRange {
            phys_start: u64::MAX - 0xfff,
            phys_length: 0x2000,
            ty: 1,
            flags: 0,
        };
        assert_eq!(wraps.checked_end(), None);
    }

    #[test]
    fn memory_map_wrapping_range_rejected() {
        let bi = BootInfo::new();
        let wraps = [MemoryRange {
            phys_start: u64::MAX - 0xfff,
            phys_length: 0x2000,
            ty: 1,
            flags: 0,
        }];
        assert_eq!(
            bi.check_memory_map(&wraps),
            Err(BootInfoError::MemoryRangeOverflow)
        );
    }

    #[test]
    fn wrapping_range_no_longer_hides_overlap() {
        // Before the fix this map was ACCEPTED in a release build: the first
        // descriptor's end wrapped to 0xfff, so `0x2000 < prev_end` was false
        // and the overlap went unnoticed (in debug it panicked instead).
        let bi = BootInfo::new();
        let overlapping = [
            MemoryRange { phys_start: 0x1000, phys_length: u64::MAX, ty: 1, flags: 0 },
            MemoryRange { phys_start: 0x2000, phys_length: 0x1000, ty: 1, flags: 0 },
        ];
        assert_eq!(
            bi.check_memory_map(&overlapping),
            Err(BootInfoError::MemoryRangeOverflow)
        );
    }

    #[test]
    fn plain_overlap_still_reported_as_overlap() {
        // The overflow guard must not swallow the ordinary overlap diagnosis.
        let bi = BootInfo::new();
        let overlapping = [
            MemoryRange { phys_start: 0x1000, phys_length: 0x2000, ty: 1, flags: 0 },
            MemoryRange { phys_start: 0x2000, phys_length: 0x1000, ty: 1, flags: 0 },
        ];
        assert_eq!(
            bi.check_memory_map(&overlapping),
            Err(BootInfoError::OverlappingMemoryMap)
        );
    }

    #[test]
    fn capsule_containment_wrapping_descriptor_rejected() {
        // Before the fix: debug panicked, release wrapped the descriptor end to
        // 0x1000 and answered the containment question from a bogus range.
        let descs = [MemoryRange {
            phys_start: 0xffff_ffff_ffff_f000,
            phys_length: 0x2000,
            ty: 1,
            flags: 0,
        }];
        let mut bi = BootInfo::new();
        bi.capsule_phys = 0x100;
        bi.capsule_length = 0x100;
        assert_eq!(
            bi.check_capsule_in_memory(&descs),
            Err(BootInfoError::MemoryRangeOverflow)
        );
    }

    #[test]
    fn capsule_containment() {
        let descs = [
            MemoryRange { phys_start: 0x1000, phys_length: 0x10000, ty: 1, flags: 1 },
        ];
        let mut bi = BootInfo::new();
        bi.capsule_phys = 0x2000;
        bi.capsule_length = 0x100;
        assert_eq!(bi.check_capsule_in_memory(&descs), Ok(()));
        bi.capsule_length = 0x10000; // spills past 0x11000 end
        assert_eq!(
            bi.check_capsule_in_memory(&descs),
            Err(BootInfoError::CapsuleOutOfMemoryMap)
        );
        bi.capsule_length = 0x100;
        bi.capsule_phys = u64::MAX;
        assert_eq!(
            bi.check_capsule_in_memory(&descs),
            Err(BootInfoError::CapsuleRangeOverflow)
        );
    }
}