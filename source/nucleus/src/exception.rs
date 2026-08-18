// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal x86_64 exception containment for the Stage 1 nucleus.
//!
//! This deliberately contains only architected exceptions (vectors 0..31).
//! External interrupts remain disabled; IRQ/APIC policy is outside Stage 1.

use core::arch::{asm, global_asm};
use core::mem::size_of;
use core::ptr::{addr_of, addr_of_mut, write_unaligned};

use tos_boot_protocol::RESULT_EXCEPTION;

const EXCEPTION_VECTOR_COUNT: usize = 32;
const DF_IST_INDEX: u8 = 1;
const DF_IST_STACK_BYTES: usize = 16 * 1024;
/// The stack a fault taken at CPL 3 lands on.
///
/// It must be nucleus memory and it must be idle when the fault arrives, so it
/// is neither the stack the nucleus is running on nor the one the system-call
/// edge re-enters on. A process cannot influence it: the processor loads it
/// from the TSS, not from anything the process holds.
const RING0_STACK_BYTES: usize = 16 * 1024;
const CODE_SELECTOR: u16 = 0x08;
const DATA_SELECTOR: u16 = 0x10;
const TSS_SELECTOR: u16 = 0x18;
const KERNEL_CODE_DESCRIPTOR: u64 = 0x00af_9a00_0000_ffff;
const KERNEL_DATA_DESCRIPTOR: u64 = 0x00cf_9200_0000_ffff;
/// The same two descriptors at DPL 3.
///
/// Their **order and position** are not free. `sysret` loads `SS` from
/// `IA32_STAR[63:48] + 8` and `CS` from `+ 16`, so the data descriptor must sit
/// immediately before the code one, and the base recorded in `IA32_STAR` must be
/// the slot before both. That base is the TSS's high half, which is never loaded
/// as a selector — the architecture reads it only as an arithmetic origin.
/// Kernel selectors keep the values Stage 1 evidence describes.
const USER_DATA_DESCRIPTOR: u64 = 0x00cf_f200_0000_ffff;
const USER_CODE_DESCRIPTOR: u64 = 0x00af_fa00_0000_ffff;
/// `IA32_STAR[63:48]`: user descriptors are at 0x28 and 0x30, and `sysret`
/// forces RPL 3 on what it loads.
pub const USER_SELECTOR_BASE: u16 = 0x20 | 3;
/// What `iretq` needs to reach CPL 3, which is the same pair by another route.
pub const USER_CODE_SELECTOR: u16 = 0x30 | 3;
pub const USER_DATA_SELECTOR: u16 = 0x28 | 3;
/// `IA32_STAR[47:32]`: `syscall` loads `CS` from it and `SS` from it plus 8.
pub const KERNEL_SELECTOR_BASE: u16 = CODE_SELECTOR;

// The relationship `sysret` computes from is arithmetic, not convention: it
// loads SS from the base plus 8 and CS from the base plus 16. Asserting it here
// means that moving a descriptor stops the build, rather than producing a boot
// that returns to CPL 3 through a descriptor which is not the one intended.
const _: () = assert!(USER_DATA_SELECTOR == USER_SELECTOR_BASE + 8);
const _: () = assert!(USER_CODE_SELECTOR == USER_SELECTOR_BASE + 16);

global_asm!(include_str!("exception.S"));

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
struct TaskStateSegment {
    reserved0: u32,
    rsp: [u64; 3],
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    io_map_base: u16,
}

impl TaskStateSegment {
    const EMPTY: Self = Self {
        reserved0: 0,
        rsp: [0; 3],
        reserved1: 0,
        ist: [0; 7],
        reserved2: 0,
        reserved3: 0,
        io_map_base: size_of::<Self>() as u16,
    };
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attributes: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const EMPTY: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        type_attributes: 0,
        offset_mid: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn set(&mut self, handler: u64, ist: u8) {
        self.offset_low = handler as u16;
        self.selector = CODE_SELECTOR;
        self.ist = ist;
        self.type_attributes = 0x8e; // present, DPL 0, 64-bit interrupt gate
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.reserved = 0;
    }
}

#[repr(align(16))]
#[allow(dead_code)]
struct AlignedStack([u8; DF_IST_STACK_BYTES]);

#[repr(align(16))]
#[allow(dead_code)]
struct AlignedRing0Stack([u8; RING0_STACK_BYTES]);

// The stack is deliberately initialized so it occupies bytes in the flat
// nucleus image: the loader sizes its allocation from that image. It is a
// fixed 16 KiB, nucleus-owned emergency stack; no untrusted field controls its
// address or size. The normal exception stack remains the loader-provided one.
static mut DF_IST_STACK: AlignedStack = AlignedStack([0xa5; DF_IST_STACK_BYTES]);
static mut RING0_STACK: AlignedRing0Stack = AlignedRing0Stack([0xa5; RING0_STACK_BYTES]);
static mut TSS: TaskStateSegment = TaskStateSegment::EMPTY;
static mut GDT: [u64; 7] = [0; 7];
static mut IDT: [IdtEntry; EXCEPTION_VECTOR_COUNT] = [IdtEntry::EMPTY; EXCEPTION_VECTOR_COUNT];

// SAFETY: exception.S defines this exact 32-entry, 8-byte-aligned table in the
// same nucleus image, with one non-returning stub address per vector 0..31.
unsafe extern "C" {
    static exception_stub_table: [u64; EXCEPTION_VECTOR_COUNT];
}

/// Establish the nucleus-owned GDT/TSS and all Stage 1 exception gates.
///
/// SAFETY: called exactly once at nucleus entry while maskable interrupts are
/// disabled by the loader. The static tables are then immutable for Stage 1.
pub unsafe fn install() {
    // The fixed static image is linked at 0x0200_0000 and this bounded stack
    // is 16 KiB, so this addition cannot overflow the x86_64 address space.
    let stack_top = addr_of_mut!(DF_IST_STACK) as *mut u8 as u64 + DF_IST_STACK_BYTES as u64;
    write_unaligned(addr_of_mut!(TSS.ist[0]), stack_top);
    // Where a fault taken at CPL 3 continues. Set here rather than when the
    // first process starts: the processor reads it on every privilege change,
    // and a TSS with `rsp0` still zero would turn the first such fault into a
    // triple fault, which reports nothing to anyone.
    let ring0_top = addr_of_mut!(RING0_STACK) as *mut u8 as u64 + RING0_STACK_BYTES as u64;
    write_unaligned(addr_of_mut!(TSS.rsp[0]), ring0_top);

    let tss_base = addr_of!(TSS) as u64;
    let tss_limit = (size_of::<TaskStateSegment>() - 1) as u64;
    GDT[0] = 0;
    GDT[1] = KERNEL_CODE_DESCRIPTOR;
    GDT[2] = KERNEL_DATA_DESCRIPTOR;
    GDT[3] = (tss_limit & 0xffff)
        | ((tss_base & 0x00ff_ffff) << 16)
        | (0x89 << 40)
        | (((tss_limit >> 16) & 0x0f) << 48)
        | (((tss_base >> 24) & 0xff) << 56);
    GDT[4] = tss_base >> 32;
    GDT[5] = USER_DATA_DESCRIPTOR;
    GDT[6] = USER_CODE_DESCRIPTOR;

    for vector in 0..EXCEPTION_VECTOR_COUNT {
        let handler = exception_stub_table[vector];
        let entry = &mut IDT[vector];
        entry.set(handler, if vector == 8 { DF_IST_INDEX } else { 0 });
    }

    let gdt = DescriptorTablePointer {
        limit: (size_of::<[u64; 7]>() - 1) as u16,
        base: addr_of!(GDT) as u64,
    };
    let idt = DescriptorTablePointer {
        limit: (size_of::<[IdtEntry; EXCEPTION_VECTOR_COUNT]>() - 1) as u16,
        base: addr_of!(IDT) as u64,
    };
    load_gdt_and_segments(&gdt);
    load_task_register(TSS_SELECTOR);
    asm!("lidt [{0}]", in(reg) &idt, options(readonly, nostack, preserves_flags));
}

/// SAFETY: `gdt` points to the initialized nucleus-owned five-entry GDT; the
/// selectors and far-return target are the constants installed immediately
/// before this call, while external interrupts remain disabled.
unsafe fn load_gdt_and_segments(gdt: &DescriptorTablePointer) {
    asm!(
        "lgdt [{gdt}]",
        "push {code}",
        "lea rax, [rip + 2f]",
        "push rax",
        "retfq",
        "2:",
        "mov ax, {data}",
        "mov ds, ax",
        "mov es, ax",
        "mov ss, ax",
        gdt = in(reg) gdt,
        code = const CODE_SELECTOR as u64,
        data = const DATA_SELECTOR,
        out("rax") _,
        options(preserves_flags),
    );
}

/// SAFETY: `selector` is TSS_SELECTOR for the initialized available TSS
/// descriptor in the active nucleus-owned GDT.
unsafe fn load_task_register(selector: u16) {
    asm!("ltr ax", in("ax") selector, options(nostack, preserves_flags));
}

/// Called only by the assembly stubs.
///
/// It returns for exactly one reason: a fault taken at CPL 3 ends the process
/// that took it and leaves the system running (ADR-0049 section 3), and
/// `process::fault` reaches the nucleus's recorded context rather than this
/// frame. Every other path from here is terminal.
#[no_mangle]
extern "C" fn exception_fatal(vector: u64, error: u64, rip: u64, cs: u64) -> ! {
    let cr2 = if vector == 14 {
        let cr2: u64;
        // SAFETY: reading CR2 is a privileged x86_64 register read performed
        // only in a fault handler; it has no memory operands.
        unsafe {
            asm!("mov {0}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
        }
        Some(cr2)
    } else {
        None
    };
    if cs & 3 == 3 && crate::process::fault(vector, error, rip, cr2) {
        // Unreachable: `process::fault` returning true means it did not return.
        unreachable!()
    }
    tos_serial::puts(b"TOS.EXCEPTION vector=");
    tos_serial::put_u32_decimal(vector as u32);
    tos_serial::puts(b" error=0x");
    tos_serial::put_hex64(error);
    tos_serial::puts(b" rip=0x");
    tos_serial::put_hex64(rip);
    tos_serial::puts(b" cr2=");
    match cr2 {
        Some(address) => {
            tos_serial::puts(b"0x");
            tos_serial::put_hex64(address);
        }
        None => tos_serial::puts(b"none"),
    }
    tos_serial::puts(b"\r\n");
    crate::result_port(RESULT_EXCEPTION)
}

#[cfg(feature = "test-exception-ud2")]
#[inline(never)]
pub fn test_injection() {
    // SAFETY: this isolated test-only artifact deliberately executes UD2 only
    // after install() loaded the Stage 1 IDT; the handler never returns.
    unsafe { asm!("ud2", options(nostack, preserves_flags)) }
}

#[cfg(feature = "test-exception-gp")]
#[inline(never)]
pub fn test_injection() {
    // Vector 0x80 has no gate in the Stage 1 IDT. INT therefore generates #GP
    // with the hardware-supplied IDT selector error code 0x402.
    // SAFETY: this isolated test-only INT runs after install() loaded the IDT;
    // the #GP handler is fatal and never resumes the instruction stream.
    unsafe { asm!("int 0x80", options(nostack, preserves_flags)) }
}
