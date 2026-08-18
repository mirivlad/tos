// SPDX-License-Identifier: GPL-3.0-or-later
//! The model-specific registers the nucleus writes, in one place.
//!
//! Two subsystems configure the processor before anything else runs — the
//! address space (`NX`) and the system-call edge (`SCE`, and the three
//! registers that describe the entry). They share `IA32_EFER`, so they share
//! this module rather than each keeping a private copy of the same two
//! instructions and the same register number.

/// `IA32_EFER`, and the two bits the nucleus sets in it.
pub const IA32_EFER: u32 = 0xc000_0080;
/// Without it `syscall` is an undefined instruction.
pub const EFER_SCE: u64 = 1;
/// Without it the `NX` bit of a page-table entry is reserved rather than
/// meaningful, and using it faults.
pub const EFER_NXE: u64 = 1 << 11;

/// The three registers that describe the `syscall` entry.
pub const IA32_STAR: u32 = 0xc000_0081;
pub const IA32_LSTAR: u32 = 0xc000_0082;
pub const IA32_FMASK: u32 = 0xc000_0084;

/// SAFETY: `msr` is an architected model-specific register of this processor.
// SAFETY: the caller names an architected MSR; the read has no memory operands.
pub unsafe fn read(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    // SAFETY: `rdmsr` on an architected register, per the caller's contract.
    unsafe {
        core::arch::asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high,
            options(nomem, nostack, preserves_flags))
    };
    u64::from(low) | (u64::from(high) << 32)
}

/// SAFETY: `msr` is an architected model-specific register and `value` is a
/// legal content for it.
// SAFETY: the caller names an architected MSR and a legal value for it.
pub unsafe fn write(msr: u32, value: u64) {
    // SAFETY: `wrmsr` on an architected register with a value the caller
    // declares legal; it has no memory operands.
    unsafe {
        core::arch::asm!("wrmsr", in("ecx") msr, in("eax") value as u32,
            in("edx") (value >> 32) as u32, options(nomem, nostack, preserves_flags))
    };
}
