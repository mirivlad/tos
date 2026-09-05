// SPDX-License-Identifier: GPL-3.0-or-later
//! The measurement-only mode selector for the same-artifact paired metric.
//!
//! **Why this exists.** ADR-0026's Stage 1 validation-performance ratio put its
//! numerator in the production nucleus and its denominator in a *separately
//! linked* `test-crypto-baseline` nucleus. Two images mean two layouts, and the
//! Stage 4C construct-validity investigation showed that an inert layout change
//! — one that executes nothing and does not alter the image length — can move
//! that quotient across the conformance boundary while native execution is
//! unmoved. A quotient whose two halves are translated from different binaries
//! does not cancel the emulator's layout sensitivity, which is what a ratio is
//! for.
//!
//! The repair is one artifact with two runtime-selected modes, so linker
//! layout, function placement, code addresses, static data placement and the
//! TCG translation environment are **shared** between numerator and denominator
//! and cancel in the ratio.
//!
//! **This is measurement-only and never in a production build.** It creates no
//! authority, no capability, no ABI and nothing a process can reach: it reads
//! two I/O ports that the emulator always provides, and the value it reads
//! decides only which of two things this boot measures.
//!
//! **The selector does not alter the executable image.** Both modes are the same
//! bytes with the same SHA-256; the harness refuses to compute a ratio unless
//! the two series report exactly equal image digests.

use core::arch::asm;

/// QEMU's firmware-configuration interface, which exists on every `q35`
/// machine this profile runs and needs no device added to the command line.
/// Adding a device would change the machine between the two series, which is
/// the sort of difference this repair exists to remove.
const SELECTOR_PORT: u16 = 0x510;
const DATA_PORT: u16 = 0x511;

/// The signature key answers `QEMU` when the interface is present.
const KEY_SIGNATURE: u16 = 0x0000;
/// The file directory, which maps a name to the key its bytes are read from.
const KEY_FILE_DIRECTORY: u16 = 0x0019;

/// The name the harness publishes the mode under.
const MODE_FILE: &[u8] = b"opt/tos/measurement-mode";

/// How many directory entries will be walked before giving up. The directory is
/// data the emulator supplies, and ring 0 walking emulator-supplied data without
/// a bound is how a boot hangs for a reason nobody can attribute.
const MAX_DIRECTORY_ENTRIES: u32 = 256;

/// One directory entry is a fixed 64 bytes: size, select key, reserved, name.
const ENTRY_BYTES: usize = 64;
const NAME_BYTES: usize = 56;

/// Which of the two measured series this boot is.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Mode {
    /// Everything a production boot validates, through the production
    /// implementations, up to the canonical boot-text digest.
    FullExact,
    /// Exactly the unavoidable cryptographic work of that same boot, over the
    /// same bytes, with the same hashing implementation, and with no result
    /// carried over from the other mode.
    UnavoidableCrypto,
}

impl Mode {
    /// The word this mode is recorded under, so a reader of the log can tell
    /// which series a sample belongs to without consulting the harness.
    pub fn name(self) -> &'static [u8] {
        match self {
            Mode::FullExact => b"full-exact",
            Mode::UnavoidableCrypto => b"unavoidable-crypto",
        }
    }
}

/// Reads one byte from an I/O port.
///
/// # Safety
///
/// The port is one of the two fixed firmware-configuration registers named
/// above, which are read-only to this nucleus and have no memory operand.
// SAFETY: the caller names one of this module's two fixed ports.
unsafe fn in_u8(port: u16) -> u8 {
    let value: u8;
    // SAFETY: a one-byte port read with no memory operand.
    unsafe {
        asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack, preserves_flags))
    };
    value
}

/// Writes the 16-bit selector.
///
/// # Safety
///
/// As [`in_u8`].
// SAFETY: the caller names this module's selector port.
unsafe fn out_u16(port: u16, value: u16) {
    // SAFETY: a two-byte port write with no memory operand.
    unsafe {
        asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags))
    };
}

/// Points the data register at one key and reads `into` from it.
///
/// # Safety
///
/// As [`in_u8`]; the key is one of the architected values above or one the
/// directory just reported.
// SAFETY: the caller names a key this module obtained from the interface itself.
unsafe fn read_key(key: u16, into: &mut [u8]) {
    // SAFETY: the two ports are this module's own, and the interface returns
    // the selected key's bytes sequentially from the data register.
    unsafe {
        out_u16(SELECTOR_PORT, key);
        for byte in into.iter_mut() {
            *byte = in_u8(DATA_PORT);
        }
    }
}

/// Which series this boot is, read from the emulator's firmware configuration.
///
/// **Absence means [`Mode::FullExact`]**, so this artifact boots and validates
/// exactly like a production nucleus when nothing selects a mode. A measurement
/// harness that forgot to pass the selector therefore measures the numerator
/// twice and reports a ratio of about one, which is a visible mistake rather
/// than a silent swap of the two series.
pub fn mode() -> Mode {
    // SAFETY: the ports are fixed registers of the accepted measurement
    // profile's machine, reachable only from ring 0, read in the single context
    // that uses them, with no memory operands.
    unsafe {
        let mut signature = [0u8; 4];
        read_key(KEY_SIGNATURE, &mut signature);
        if &signature != b"QEMU" {
            return Mode::FullExact;
        }
        let mut count = [0u8; 4];
        read_key(KEY_FILE_DIRECTORY, &mut count);
        // The directory's counts and keys are big-endian, unlike the selector.
        let entries = u32::from_be_bytes(count).min(MAX_DIRECTORY_ENTRIES);
        // The directory is read as one sequential stream: the count above left
        // the data register positioned at the first entry.
        for _ in 0..entries {
            let mut entry = [0u8; ENTRY_BYTES];
            for byte in entry.iter_mut() {
                *byte = in_u8(DATA_PORT);
            }
            let select = u16::from_be_bytes([entry[4], entry[5]]);
            let name = &entry[8..8 + NAME_BYTES];
            if !name.starts_with(MODE_FILE) {
                continue;
            }
            // The name must end at the file name, not merely begin with it, or
            // a longer name sharing this prefix would answer for it.
            if name[MODE_FILE.len()] != 0 {
                continue;
            }
            let mut value = [0u8; 1];
            read_key(select, &mut value);
            return match value[0] {
                b'c' => Mode::UnavoidableCrypto,
                _ => Mode::FullExact,
            };
        }
        // Nothing published a mode, so this boot is the numerator.
        Mode::FullExact
    }
}

/// Puts the selected mode on the record.
///
/// The ruling requires the selector to be retained in evidence. It is recorded
/// by the party that acted on it rather than by the harness that supplied it,
/// so a sample whose series was mislabelled by the harness is detectable from
/// the guest's own log.
pub fn report(mode: Mode) {
    tos_serial::puts(b"TOS.TEST.PAIRED.MODE mode=");
    tos_serial::puts(mode.name());
    tos_serial::puts(b" asserted_by=nucleus\r\n");
}
