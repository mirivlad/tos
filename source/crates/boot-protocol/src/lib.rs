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
    pub fn end(&self) -> u64 {
        self.phys_start + self.phys_length
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
    pub reserved: [u8; 7],
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
            capsule_identity_kind: 0,
            reserved: [0; 7],
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
    /// reserved trailing bytes non-zero.
    NonZeroReserved,
    /// `architecture_id` not `ARCH_X86_64`.
    BadArchitecture,
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
    /// capsule range arithmetic overflow.
    CapsuleRangeOverflow,
    /// capsule absent (phys == 0 or length == 0).
    ZeroCapsule,
    /// capsule range not contained in any memory-map descriptor.
    CapsuleOutOfMemoryMap,
}

impl core::fmt::Display for BootInfoError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl BootInfo {
    /// Validate a raw byte slice as a BootInfo v1 image. The slice length must
    /// be at least `total_size`; bytes beyond `STRUCT_SIZE` within
    /// `total_size` must be zero.
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
        let desc_size = u64::from_le_bytes(bytes[56..64].try_into().unwrap());
        if desc_size != MEM_DESC_SIZE {
            return Err(BootInfoError::BadDescriptorSize);
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
        let mut prev_start = descs[0].phys_start;
        let mut prev_end = descs[0].end();
        if descs[0].phys_length == 0 {
            return Err(BootInfoError::ZeroLengthRange);
        }
        for d in &descs[1..] {
            if d.phys_length == 0 {
                return Err(BootInfoError::ZeroLengthRange);
            }
            // Before the previous region's start => strictly out of order.
            if d.phys_start < prev_start {
                return Err(BootInfoError::UnsortedMemoryMap);
            }
            // Within or overlapping the previous region.
            if d.phys_start < prev_end {
                return Err(BootInfoError::OverlappingMemoryMap);
            }
            prev_start = d.phys_start;
            prev_end = d.end();
        }
        Ok(())
    }

    /// Verify the capsule range lies inside one declared memory range.
    pub fn check_capsule_in_memory(&self, descs: &[MemoryRange]) -> Result<(), BootInfoError> {
        let cap_end = self
            .capsule_phys
            .checked_add(self.capsule_length)
            .ok_or(BootInfoError::CapsuleRangeOverflow)?;
        let inside = descs.iter().any(|d| {
            d.phys_start <= self.capsule_phys && cap_end <= d.phys_start + d.phys_length
        });
        if !inside {
            return Err(BootInfoError::CapsuleOutOfMemoryMap);
        }
        Ok(())
    }
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