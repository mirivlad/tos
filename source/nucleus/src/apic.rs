// SPDX-License-Identifier: GPL-3.0-or-later
//! The one interrupt this system routes (ADR-0049).
//!
//! ADR-0023 left maskable interrupts disabled and Stage 1 and Stage 2 were
//! measured that way. ADR-0049 enables them once, after the process substrate
//! exists and before the first process is entered, and admits exactly one
//! source: a timer, for preemption and for a monotonic tick.
//!
//! **The legacy controller is masked, not used.** The 8259 pair is masked
//! entirely rather than reprogrammed, because a controller nobody routes
//! anything through has nothing to configure and every line it could deliver
//! would arrive on a vector this system has not claimed.
//!
//! **A tick is a tick.** It counts timer interrupts and nothing else. Stage 3
//! claims no wall-clock time, no calibration against a reference and no trusted
//! time source; docs/34 assigns time threats to Stage 7, and a number presented
//! as seconds would be a claim this nucleus cannot support.

use crate::msr;

/// Where the local APIC's registers live by default. The address is fixed by
/// the architecture, and this nucleus does not relocate it: moving a device's
/// registers is a decision with no benefit here and one more thing to get
/// wrong.
pub const LOCAL_APIC: u64 = 0xfee0_0000;

/// `IA32_APIC_BASE`, and the bit that makes the local APIC exist at all.
const IA32_APIC_BASE: u32 = 0x1b;
const APIC_GLOBAL_ENABLE: u64 = 1 << 11;

/// Register offsets used here, from `LOCAL_APIC`.
const SPURIOUS: u64 = 0xf0;
const EOI: u64 = 0xb0;
const LVT_TIMER: u64 = 0x320;
const TIMER_INITIAL: u64 = 0x380;
const TIMER_DIVIDE: u64 = 0x3e0;

/// The vectors this stage claims above 31, and nothing else.
pub const TIMER_VECTOR: u8 = 32;
pub const SPURIOUS_VECTOR: u8 = 255;

/// `APIC software enable` in the spurious-interrupt register.
const SOFTWARE_ENABLE: u32 = 1 << 8;
/// Periodic mode in the timer's local vector table entry.
const PERIODIC: u32 = 1 << 17;
/// Divide the bus clock by 16.
const DIVIDE_BY_16: u32 = 0b0011;

/// How many bus ticks between interrupts.
///
/// Not calibrated against anything, and not presented as a duration: it is the
/// count that makes a tick frequent enough to preempt and rare enough not to
/// spend the machine on interrupt entry. ADR-0049 leaves the concrete source
/// and mode to the implementation; what it fixes is that there is one timer and
/// that its purpose is preemption and timekeeping.
const QUANTUM: u32 = 100_000;

/// Timer interrupts taken since the timer was started.
///
/// Written only by the handler, which cannot be re-entered: the handler runs
/// with interrupts masked and nested interrupts are not enabled (ADR-0049).
static mut TICKS: u64 = 0;

/// The monotonic tick, as `time_monotonic` reports it.
pub fn ticks() -> u64 {
    // SAFETY: single processor, and the only writer is a handler that cannot
    // run while this reads: either this is the handler's own interrupt-masked
    // context, or the read is in nucleus code the handler interrupts and
    // returns from without leaving the value half-written — it is one aligned
    // `u64` store.
    unsafe { TICKS }
}

/// Writes one local APIC register.
///
/// SAFETY: as [`read`], and `value` is legal for that register.
// SAFETY: the caller names an architected offset and a legal value.
unsafe fn write(offset: u64, value: u32) {
    // SAFETY: per the caller's contract.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<u32>((LOCAL_APIC + offset) as usize)
            .write_volatile(value)
    };
}

/// Masks the legacy 8259 pair.
///
/// SAFETY: called once, before interrupts are enabled.
// SAFETY: the two OUTs address the architected PIC data ports and mask every
// line; nothing in this system routes an interrupt through them.
unsafe fn mask_legacy_controller() {
    // SAFETY: single-byte OUTs to the two fixed PIC data ports, with no memory
    // operands.
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") 0xa1u16,
            in("al") 0xffu8,
            options(nomem, nostack, preserves_flags)
        );
        core::arch::asm!(
            "out dx, al",
            in("dx") 0x21u16,
            in("al") 0xffu8,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Starts the timer and enables maskable interrupts.
///
/// # Safety
///
/// The caller states that the IDT has gates for [`TIMER_VECTOR`] and
/// [`SPURIOUS_VECTOR`], that the local APIC page is mapped uncacheable in the
/// live address space, and that the process substrate is initialized — after
/// this call an interrupt can arrive at any instruction.
// SAFETY: the caller's promise that the gates and the mapping exist is what
// makes the first interrupt land somewhere defined.
pub unsafe fn start() {
    // SAFETY: called once here, before interrupts are enabled.
    unsafe { mask_legacy_controller() };
    // SAFETY: `IA32_APIC_BASE` is architected; setting the enable bit leaves
    // the base address field as the firmware left it, which is the default.
    unsafe {
        msr::write(
            IA32_APIC_BASE,
            msr::read(IA32_APIC_BASE) | APIC_GLOBAL_ENABLE,
        )
    };
    // SAFETY: the caller states the APIC page is mapped; these four writes are
    // the documented order — software-enable, divide, vector and mode, then the
    // count that starts it.
    unsafe {
        write(SPURIOUS, SOFTWARE_ENABLE | u32::from(SPURIOUS_VECTOR));
        write(TIMER_DIVIDE, DIVIDE_BY_16);
        write(LVT_TIMER, PERIODIC | u32::from(TIMER_VECTOR));
        write(TIMER_INITIAL, QUANTUM);
    }
    // SAFETY: the gates exist by the caller's contract, so the first interrupt
    // has somewhere to land.
    unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
}

/// Counts one timer interrupt and acknowledges it. Called only by the stub.
///
/// This is the first handler in this system that returns, and everything it
/// does is bounded: one add, one store to a device register. It allocates
/// nothing and takes no lock, which is ADR-0023's discipline extended to the
/// only handler that resumes.
#[no_mangle]
extern "C" fn timer_interrupt() {
    // SAFETY: the handler cannot be re-entered — it runs with interrupts masked
    // and nested interrupts are not enabled — so this is the only writer.
    unsafe { TICKS = TICKS.wrapping_add(1) };
    // SAFETY: the APIC page is mapped for as long as interrupts are enabled,
    // and a zero to the EOI register is how an interrupt is acknowledged.
    unsafe { write(EOI, 0) };
}

/// Acknowledges a spurious interrupt and counts nothing.
///
/// A spurious interrupt is the controller saying it changed its mind; counting
/// it as a tick would put time forward for a reason that did not happen.
#[no_mangle]
extern "C" fn spurious_interrupt() {
    // The architecture does not want an EOI for the spurious vector, so this
    // handler does exactly nothing but return.
}
