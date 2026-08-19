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

/// The state of whatever the timer interrupted.
///
/// Laid out to match what `timer_stub` pushes, in that order, followed by the
/// five words the processor pushed itself. A handler that only counts ticks
/// does not need it; a handler that returns to a *different* process does,
/// because everything `iretq` will read is in here. It exists now, with one
/// reader, so that the step to a scheduler is a change in what the handler
/// writes rather than in what it can see.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrapFrame {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl TrapFrame {
    /// A frame of zeros: what an unused process slot holds.
    pub const ZERO: TrapFrame = TrapFrame {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rip: 0,
        cs: 0,
        rflags: 0,
        rsp: 0,
        ss: 0,
    };

    /// Whether the interrupt was taken in a process rather than in the nucleus.
    fn interrupted_a_process(&self) -> bool {
        self.cs & 3 == 3
    }
}

/// Counts one timer interrupt, acknowledges it, and hands the frame to the
/// scheduler when it interrupted a process. Called only by the stub.
///
/// Everything it does is bounded: one add, one store to a device register, and
/// — at most — two frame copies and a `CR3` load. It allocates nothing and
/// takes no lock, which is ADR-0023's discipline extended to the only handler
/// that resumes (ADR-0049 §5).
///
/// **A tick taken in the nucleus preempts nobody.** The frame the stub built
/// then describes the nucleus's own interrupted work, and returning from it
/// into a process would abandon that work on a stack nothing would ever pop.
/// The privilege level in the interrupted `CS` is the whole of the test.
#[no_mangle]
extern "C" fn timer_interrupt(frame: &mut TrapFrame) {
    // SAFETY: the handler cannot be re-entered — it runs with interrupts masked
    // and nested interrupts are not enabled — so this is the only writer.
    let tick = unsafe {
        TICKS = TICKS.wrapping_add(1);
        TICKS
    };
    // Where the machine's time went is the nucleus's to say, and it says it per
    // process: the interrupted `CS` decides whether this tick belongs to a
    // process at all, and the scheduler charges it to the one that was running.
    let in_process = frame.interrupted_a_process();
    // SAFETY: the APIC page is mapped for as long as interrupts are enabled,
    // and a zero to the EOI register is how an interrupt is acknowledged. It is
    // written before the switch below so that the acknowledgement belongs to
    // this interrupt rather than to whatever the next context does first.
    unsafe { write(EOI, 0) };
    if in_process {
        // SAFETY: the frame is the one the stub built on the nucleus's own
        // stack, `iretq` will read exactly what this leaves in it, and the
        // interrupted `CS` says a process was on the processor — which is what
        // makes there be a current process to charge the tick to and to
        // preempt.
        unsafe { crate::process::preempt(frame, tick) };
    }
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
