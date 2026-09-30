// SPDX-License-Identifier: GPL-3.0-or-later
//! The ADR-0103 Rust VirtIO-block reference: an **oracle**, not a driver.
//!
//! A minimal, separately isolated benchmark that drives the Stage 4 reference
//! device directly and performs exactly the workload the TOS measurement boot
//! performs, under the same external observer. It is not linked into anything,
//! it is not a TOS driver or runtime dependency, and it cannot satisfy the
//! Stage 4 identity gate: it exists so that R1–R3 have a denominator.
//!
//! **What "the same" means here, decided rather than assumed.** ADR-0103 fixes
//! the machine, the device configuration, the queue depth, the image and the
//! workload, and leaves the oracle's completion mechanism unstated. This one
//! waits for a completion the way the TOS service does — one MSI-X vector, the
//! processor halted until it arrives — and uses the service's request shape: a
//! three-descriptor chain (header, 512-byte data, status) in one page with the
//! rings, a queue of at most 128 entries, `VIRTIO_F_VERSION_1` and nothing else.
//! A polling oracle would be faster in wall time and dearer in CPU time, so it
//! would move R1/R2 one way and R3 the other; matching the mechanism keeps all
//! three ratios about the software above the device.
//!
//! **Where it runs.** A UEFI application booted by the same OVMF on the same
//! QEMU command as the TOS boot, with its own ESP entry. After
//! `ExitBootServices` it owns the machine: interrupts are masked except while it
//! waits in `hlt`, the firmware's timer is masked at the local APIC, and the only
//! vector it expects is its own.
//!
//! **The plan** is ADR-0103's, and the TOS measurement client implements the same
//! one in canonical text; the harness compares the two sides' sector digests.
//!
//! ```text
//! warm-up   128 sequential READs of sectors 0..127, each checked against the
//!           image pattern (le64 sector number in the first eight bytes)
//! READY     0xff on COM1, then wait for GO (0xc0) from the observer
//! floor     21 empty OPEN/CLOSE pairs                  tags 0x00 | (i mod 16)
//! R1        3 windows of 2048 sequential READs,        tags 0x10 | w
//!           window w starting at sector 128 + 2048 w
//! R2        303 random 4 KiB operations of 8 consecutive READs, 3 warm-up and
//!           300 retained, block k from the Park–Miller sequence below,
//!                                                      tags 0x10 | (k mod 16)
//! STOP      wait for 0xe0: the observer has disarmed its event
//! ```

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::ffi::c_void;
use core::ptr::{addr_of, addr_of_mut, read_volatile, write_volatile};
use core::sync::atomic::{fence, Ordering};

// --- the plan (shared with the TOS measurement client, and checked) ----------

const SECTOR_BYTES: usize = 512;
const WARMUP_SECTORS: u64 = 128;
const FLOOR_PAIRS: u64 = 21;
const R1_WINDOWS: u64 = 3;
const R1_WINDOW_READS: u64 = 2048;
const R1_FIRST_SECTOR: u64 = 128;
const R2_OPERATIONS: u64 = 303;
const R2_READS_PER_OPERATION: u64 = 8;
/// 4 KiB blocks in the 16 MiB reference image.
const R2_BLOCKS: u64 = 4096;
/// Park–Miller minimal standard: `x' = 48271 x mod (2^31 - 1)`. Every product
/// stays below 2^47, so the TOS side computes it in `u64` without overflow.
const R2_SEED: u64 = 20_260_926;
const R2_MULTIPLIER: u64 = 48_271;
const R2_MODULUS: u64 = 2_147_483_647;
/// The digest both sides fold every measured sector into, in order.
const DIGEST_MULTIPLIER: u64 = 31;
const DIGEST_MODULUS: u64 = 1_000_000_007;

// --- the ADR-0066 marker protocol ---------------------------------------------

const OPEN: u8 = 0x80;
const CLOSE: u8 = 0xa0;
const WORK: u8 = 0x10;
const SEQUENCE: u8 = 0x0f;
const GO: u8 = 0xc0;
const STOP: u8 = 0xe0;
const READY: u8 = 0xff;

// --- the machine --------------------------------------------------------------

const COM1: u16 = 0x3f8;
/// QEMU's `isa-debug-exit` at its default port, the one Boot ABI v1's
/// `RESULT_PORT` names: the machine exits with `(value << 1) | 1`.
const DEBUG_EXIT: u16 = 0x501;
/// `(16 << 1) | 1 = 33`, the value every passing TOS boot also exits with.
const EXIT_PASS: u8 = 16;
/// `(0x40 << 1) | 1 = 129`; a processor exception exits with `0x41`, 131.
const EXIT_FAIL: u8 = 0x40;
const MSIX_VECTOR: u8 = 0x40;
const IA32_APIC_BASE: u32 = 0x1b;

const VIRTIO_VENDOR: u16 = 0x1af4;
const VIRTIO_BLOCK_MODERN: u16 = 0x1042;

// VirtIO 1.x common configuration (§4.1.4.3), by byte offset.
const DEVICE_FEATURE_SELECT: usize = 0;
const DEVICE_FEATURE: usize = 4;
const DRIVER_FEATURE_SELECT: usize = 8;
const DRIVER_FEATURE: usize = 12;
const MSIX_CONFIG: usize = 16;
const DEVICE_STATUS: usize = 20;
const QUEUE_SELECT: usize = 22;
const QUEUE_SIZE: usize = 24;
const QUEUE_MSIX_VECTOR: usize = 26;
const QUEUE_ENABLE: usize = 28;
const QUEUE_NOTIFY_OFF: usize = 30;
const QUEUE_DESC: usize = 32;
const QUEUE_DRIVER: usize = 40;
const QUEUE_DEVICE: usize = 48;

const STATUS_ACKNOWLEDGE: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FEATURES_OK: u8 = 8;
const VERSION_1_IN_HIGH_DWORD: u32 = 1;
const NO_VECTOR: u16 = 0xffff;
const QUEUE_CAP: u16 = 128;

const DESC_F_NEXT: u16 = 1;
const DESC_F_WRITE: u16 = 2;
const BLK_T_IN: u32 = 0;
const BLK_S_OK: u8 = 0;
const READ_USED_LEN: u32 = 513;

// --- the minimum of UEFI ------------------------------------------------------

type Status = usize;
const EFI_SUCCESS: Status = 0;
const ALLOCATE_MAX_ADDRESS: u32 = 1;
const LOADER_DATA: u32 = 2;

#[repr(C)]
struct BootServices {
    _header: [u8; 24],
    _tpl: [usize; 2],
    allocate_pages: extern "efiapi" fn(u32, u32, usize, *mut u64) -> Status,
    _free_pages: usize,
    get_memory_map:
        extern "efiapi" fn(*mut usize, *mut u8, *mut usize, *mut usize, *mut u32) -> Status,
    _between: [usize; 21],
    exit_boot_services: extern "efiapi" fn(*mut c_void, usize) -> Status,
}

#[repr(C)]
pub struct SystemTable {
    _before: [u8; 0x60],
    boot_services: *mut BootServices,
}

const _: () = {
    use core::mem::offset_of;
    // UEFI 2.10 §4.4: AllocatePages 0x28, GetMemoryMap 0x38, ExitBootServices 0xE8.
    assert!(offset_of!(BootServices, allocate_pages) == 0x28);
    assert!(offset_of!(BootServices, get_memory_map) == 0x38);
    assert!(offset_of!(BootServices, exit_boot_services) == 0xe8);
    // UEFI 2.10 §4.3: BootServices at 0x60.
    assert!(offset_of!(SystemTable, boot_services) == 0x60);
};

// --- port and memory access ---------------------------------------------------

fn outb(port: u16, value: u8) {
    // SAFETY: after ExitBootServices this application owns the machine; the
    // ports it names are COM1, the PCI configuration mechanism, the legacy
    // interrupt controllers and QEMU's debug-exit device.
    unsafe { asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack)) };
}

fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: as `outb`.
    unsafe { asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack)) };
    value
}

fn outl(port: u16, value: u32) {
    // SAFETY: as `outb`.
    unsafe { asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack)) };
}

fn inl(port: u16) -> u32 {
    let value: u32;
    // SAFETY: as `outb`.
    unsafe { asm!("in eax, dx", out("eax") value, in("dx") port, options(nomem, nostack)) };
    value
}

fn peek<T: Copy>(address: u64) -> T {
    // SAFETY: every address passed here is inside the device's BAR window, the
    // local APIC page or this application's own allocated page, all
    // identity-mapped by the firmware's page tables and aligned for `T`.
    unsafe { read_volatile(address as usize as *const T) }
}

fn poke<T: Copy>(address: u64, value: T) {
    // SAFETY: as `peek`.
    unsafe { write_volatile(address as usize as *mut T, value) }
}

// --- COM1: the marker wire and the report ---------------------------------------

fn wire_write(byte: u8) {
    while inb(COM1 + 5) & 0x20 == 0 {
        core::hint::spin_loop();
    }
    outb(COM1, byte);
}

fn wire_read() -> u8 {
    while inb(COM1 + 5) & 0x01 == 0 {
        core::hint::spin_loop();
    }
    inb(COM1)
}

fn wire_drain() {
    while inb(COM1 + 5) & 0x01 != 0 {
        let _ = inb(COM1);
    }
}

fn say(text: &str) {
    for byte in text.bytes() {
        wire_write(byte);
    }
}

fn say_number(mut value: u64) {
    let mut digits = [0u8; 20];
    let mut count = 0;
    loop {
        digits[count] = b'0' + (value % 10) as u8;
        count += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    while count > 0 {
        count -= 1;
        wire_write(digits[count]);
    }
}

fn exit(value: u8) -> ! {
    outb(DEBUG_EXIT, value);
    loop {
        // SAFETY: nothing is left to run; halting with interrupts masked is the
        // end of this application.
        unsafe { asm!("cli", "hlt", options(nomem, nostack)) };
    }
}

fn fail(step: &str, detail: u64) -> ! {
    say("\r\nORACLE.FAIL step=");
    say(step);
    say(" detail=");
    say_number(detail);
    say("\r\n");
    exit(EXIT_FAIL);
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    fail("panic", 0)
}

// --- PCI configuration mechanism #1 ---------------------------------------------

#[derive(Clone, Copy)]
struct Function {
    bus: u8,
    device: u8,
    function: u8,
}

impl Function {
    fn address(self, offset: u8) -> u32 {
        0x8000_0000
            | (u32::from(self.bus) << 16)
            | (u32::from(self.device) << 11)
            | (u32::from(self.function) << 8)
            | u32::from(offset & 0xfc)
    }

    fn read32(self, offset: u8) -> u32 {
        outl(0xcf8, self.address(offset));
        inl(0xcfc)
    }

    fn write32(self, offset: u8, value: u32) {
        outl(0xcf8, self.address(offset));
        outl(0xcfc, value);
    }

    fn read8(self, offset: u8) -> u8 {
        (self.read32(offset) >> ((offset & 3) * 8)) as u8
    }

    fn read16(self, offset: u8) -> u16 {
        (self.read32(offset) >> ((offset & 2) * 8)) as u16
    }

    fn write16(self, offset: u8, value: u16) {
        let shift = (offset & 2) * 8;
        let old = self.read32(offset) & !(0xffff << shift);
        self.write32(offset, old | (u32::from(value) << shift));
    }

    /// The memory address a BAR decodes, 64-bit BARs included.
    fn bar(self, index: u8) -> u64 {
        let low = self.read32(0x10 + 4 * index);
        if low & 1 != 0 {
            fail("bar-is-io", u64::from(index));
        }
        let mut address = u64::from(low & !0xf);
        if (low >> 1) & 3 == 2 {
            address |= u64::from(self.read32(0x14 + 4 * index)) << 32;
        }
        if address == 0 {
            fail("bar-unassigned", u64::from(index));
        }
        address
    }

    /// Every capability with this ID, walked from the list head.
    fn capabilities(self, id: u8, mut each: impl FnMut(u8)) {
        if self.read16(0x06) & 0x10 == 0 {
            return;
        }
        let mut at = self.read8(0x34) & 0xfc;
        let mut steps = 0;
        while at != 0 && steps < 48 {
            if self.read8(at) == id {
                each(at);
            }
            at = self.read8(at + 1) & 0xfc;
            steps += 1;
        }
    }
}

fn find_device() -> Function {
    let mut found = None;
    let mut count = 0u64;
    for bus in 0..=255u8 {
        for device in 0..32u8 {
            let candidate = Function {
                bus,
                device,
                function: 0,
            };
            let id = candidate.read32(0x00);
            if id as u16 == VIRTIO_VENDOR && (id >> 16) as u16 == VIRTIO_BLOCK_MODERN {
                found = Some(candidate);
                count += 1;
            }
        }
    }
    match (found, count) {
        (Some(function), 1) => function,
        _ => fail("device-count", count),
    }
}

/// A VirtIO structure's absolute address, from its vendor capability.
fn virtio_structure(function: Function, cfg_type: u8) -> (u64, u8) {
    let mut result = None;
    function.capabilities(0x09, |at| {
        if result.is_none() && function.read8(at + 3) == cfg_type {
            let bar = function.read8(at + 4);
            let offset = function.read32(at + 8);
            result = Some((function.bar(bar) + u64::from(offset), at));
        }
    });
    result.unwrap_or_else(|| fail("virtio-capability", u64::from(cfg_type)))
}

// --- interrupts -----------------------------------------------------------------

#[no_mangle]
static mut ORACLE_DELIVERIES: u64 = 0;
#[no_mangle]
static mut ORACLE_UNEXPECTED: u64 = 0;
#[no_mangle]
static mut ORACLE_LAPIC_EOI: u64 = 0;

global_asm!(
    ".global oracle_msix_stub",
    "oracle_msix_stub:",
    "    push rax",
    "    lock inc qword ptr [rip + ORACLE_DELIVERIES]",
    "    mov rax, qword ptr [rip + ORACLE_LAPIC_EOI]",
    "    mov dword ptr [rax], 0",
    "    pop rax",
    "    iretq",
    ".global oracle_unexpected_stub",
    "oracle_unexpected_stub:",
    "    push rax",
    "    lock inc qword ptr [rip + ORACLE_UNEXPECTED]",
    "    mov rax, qword ptr [rip + ORACLE_LAPIC_EOI]",
    "    mov dword ptr [rax], 0",
    "    pop rax",
    "    iretq",
    ".global oracle_fault_stub",
    "oracle_fault_stub:",
    "    mov dx, 0x501",
    "    mov al, 0x41",
    "    out dx, al",
    "2:  cli",
    "    hlt",
    "    jmp 2b",
);

extern "C" {
    fn oracle_msix_stub();
    fn oracle_unexpected_stub();
    fn oracle_fault_stub();
}

fn gate(idt: u64, vector: usize, handler: u64, selector: u16) {
    let entry = idt + 16 * vector as u64;
    poke::<u16>(entry, handler as u16);
    poke::<u16>(entry + 2, selector);
    poke::<u8>(entry + 4, 0);
    poke::<u8>(entry + 5, 0x8e);
    poke::<u16>(entry + 6, (handler >> 16) as u16);
    poke::<u32>(entry + 8, (handler >> 32) as u32);
    poke::<u32>(entry + 12, 0);
}

/// Returns the local APIC's base and this processor's APIC ID.
fn take_interrupts(idt: u64) -> (u64, u32) {
    // The legacy controllers stay silent: every line masked.
    outb(0x21, 0xff);
    outb(0xa1, 0xff);

    let selector: u16;
    // SAFETY: reads the code segment selector the firmware left running.
    unsafe { asm!("mov {0:x}, cs", out(reg) selector, options(nomem, nostack)) };
    for vector in 0..256 {
        let handler = match vector {
            0..=31 => oracle_fault_stub as *const () as u64,
            v if v == usize::from(MSIX_VECTOR) => oracle_msix_stub as *const () as u64,
            _ => oracle_unexpected_stub as *const () as u64,
        };
        gate(idt, vector, handler, selector);
    }
    let descriptor: [u16; 5] = [
        (256 * 16 - 1) as u16,
        idt as u16,
        (idt >> 16) as u16,
        (idt >> 32) as u16,
        (idt >> 48) as u16,
    ];
    // SAFETY: the table is 256 complete gates in a page this application owns.
    unsafe { asm!("lidt [{0}]", in(reg) descriptor.as_ptr(), options(nostack)) };

    let (low, high): (u32, u32);
    // SAFETY: IA32_APIC_BASE is architectural on every x86-64 processor.
    unsafe {
        asm!("rdmsr", in("ecx") IA32_APIC_BASE, out("eax") low, out("edx") high,
             options(nomem, nostack))
    };
    let lapic = ((u64::from(high) << 32) | u64::from(low)) & !0xfff;
    // SAFETY: written once, before any vector that reads it can be delivered.
    unsafe { ORACLE_LAPIC_EOI = lapic + 0xb0 };
    // Software-enable the APIC with spurious vector 0xff, accept every priority,
    // and mask the firmware's timer and every local line: the only interrupt this
    // application expects is its own completion.
    poke::<u32>(lapic + 0xf0, 0x1ff);
    poke::<u32>(lapic + 0x80, 0);
    for lvt in [0x320u64, 0x350, 0x360, 0x370] {
        poke::<u32>(lapic + lvt, peek::<u32>(lapic + lvt) | 1 << 16);
    }
    let apic_id = peek::<u32>(lapic + 0x20) >> 24;
    (lapic, apic_id)
}

// --- the device -----------------------------------------------------------------

struct Queue {
    notify: u64,
    size: u16,
    page: u64,
    avail: u64,
    used: u64,
    header: u64,
    data: u64,
    status: u64,
    published: u16,
    completed: u16,
}

fn align(value: u64, to: u64) -> u64 {
    (value + to - 1) & !(to - 1)
}

fn bring_up(function: Function, page: u64, apic_id: u32) -> Queue {
    // Memory decoding and bus mastering, whatever the firmware left.
    function.write16(0x04, function.read16(0x04) | 0x6);

    let (common, _) = virtio_structure(function, 1);
    let (notify_base, notify_cap) = virtio_structure(function, 2);
    let multiplier = u64::from(function.read32(notify_cap + 16));

    poke::<u8>(common + DEVICE_STATUS as u64, 0);
    while peek::<u8>(common + DEVICE_STATUS as u64) != 0 {
        core::hint::spin_loop();
    }
    poke::<u8>(common + DEVICE_STATUS as u64, STATUS_ACKNOWLEDGE);
    poke::<u8>(
        common + DEVICE_STATUS as u64,
        STATUS_ACKNOWLEDGE | STATUS_DRIVER,
    );
    poke::<u32>(common + DEVICE_FEATURE_SELECT as u64, 1);
    let offered = peek::<u32>(common + DEVICE_FEATURE as u64);
    if offered & VERSION_1_IN_HIGH_DWORD == 0 {
        fail("no-version-1", u64::from(offered));
    }
    poke::<u32>(common + DRIVER_FEATURE_SELECT as u64, 0);
    poke::<u32>(common + DRIVER_FEATURE as u64, 0);
    poke::<u32>(common + DRIVER_FEATURE_SELECT as u64, 1);
    poke::<u32>(common + DRIVER_FEATURE as u64, VERSION_1_IN_HIGH_DWORD);
    let status = STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK;
    poke::<u8>(common + DEVICE_STATUS as u64, status);
    if peek::<u8>(common + DEVICE_STATUS as u64) & STATUS_FEATURES_OK == 0 {
        fail("features-refused", 0);
    }

    // MSI-X: table entry 0 aims vector 0x40 at this processor, unmasked.
    let mut table = None;
    function.capabilities(0x11, |at| table = Some(at));
    let msix = table.unwrap_or_else(|| fail("no-msix", 0));
    let bir_offset = function.read32(msix + 4);
    let entry = function.bar((bir_offset & 7) as u8) + u64::from(bir_offset & !7);
    poke::<u32>(entry, 0xfee0_0000 | (apic_id << 12));
    poke::<u32>(entry + 4, 0);
    poke::<u32>(entry + 8, u32::from(MSIX_VECTOR));
    poke::<u32>(entry + 12, 0);
    let control = function.read16(msix + 2);
    function.write16(msix + 2, (control | 0x8000) & !0x4000);

    poke::<u16>(common + MSIX_CONFIG as u64, NO_VECTOR);
    poke::<u16>(common + QUEUE_SELECT as u64, 0);
    let maximum = peek::<u16>(common + QUEUE_SIZE as u64);
    if maximum < 4 {
        fail("queue-too-small", u64::from(maximum));
    }
    let size = maximum.min(QUEUE_CAP);
    poke::<u16>(common + QUEUE_SIZE as u64, size);
    poke::<u16>(common + QUEUE_MSIX_VECTOR as u64, 0);
    if peek::<u16>(common + QUEUE_MSIX_VECTOR as u64) != 0 {
        fail("queue-vector-refused", 0);
    }
    let q = u64::from(size);
    let avail = page + 16 * q;
    let used = align(avail + 6 + 2 * q, 4);
    let header = align(used + 6 + 8 * q, 16);
    let data = header + 16;
    let status_byte = data + SECTOR_BYTES as u64;
    if status_byte + 1 > page + 4096 {
        fail("layout", status_byte - page);
    }
    // The one chain every request uses: header, data (device-written), status.
    let chain: [(u64, u32, u16, u16); 3] = [
        (header, 16, DESC_F_NEXT, 1),
        (data, SECTOR_BYTES as u32, DESC_F_WRITE | DESC_F_NEXT, 2),
        (status_byte, 1, DESC_F_WRITE, 0),
    ];
    for (index, (address, length, flags, next)) in chain.iter().enumerate() {
        let at = page + 16 * index as u64;
        poke::<u64>(at, *address);
        poke::<u32>(at + 8, *length);
        poke::<u16>(at + 12, *flags);
        poke::<u16>(at + 14, *next);
    }
    poke::<u64>(common + QUEUE_DESC as u64, page);
    poke::<u64>(common + QUEUE_DRIVER as u64, avail);
    poke::<u64>(common + QUEUE_DEVICE as u64, used);
    let notify_off = u64::from(peek::<u16>(common + QUEUE_NOTIFY_OFF as u64));
    poke::<u16>(common + QUEUE_ENABLE as u64, 1);
    poke::<u8>(common + DEVICE_STATUS as u64, status | STATUS_DRIVER_OK);

    Queue {
        notify: notify_base + notify_off * multiplier,
        size,
        page,
        avail,
        used,
        header,
        data,
        status: status_byte,
        published: 0,
        completed: 0,
    }
}

impl Queue {
    /// One READ of one sector, queue depth one, waited for by its interrupt.
    fn read(&mut self, sector: u64) {
        poke::<u32>(self.header, BLK_T_IN);
        poke::<u32>(self.header + 4, 0);
        poke::<u64>(self.header + 8, sector);
        poke::<u8>(self.status, 0xff);
        let slot = u64::from(self.published % self.size);
        poke::<u16>(self.avail + 4 + 2 * slot, 0);
        fence(Ordering::SeqCst);
        self.published = self.published.wrapping_add(1);
        poke::<u16>(self.avail + 2, self.published);
        fence(Ordering::SeqCst);
        poke::<u16>(self.notify, 0);
        loop {
            if peek::<u16>(self.used + 2) != self.completed {
                break;
            }
            // `sti` takes effect after the next instruction, so a completion
            // arriving between the check above and the halt wakes the halt.
            // SAFETY: the IDT routes every vector this machine can deliver.
            unsafe { asm!("sti", "hlt", "cli", options(nomem, nostack)) };
        }
        fence(Ordering::SeqCst);
        let slot = u64::from(self.completed % self.size);
        let id = peek::<u32>(self.used + 4 + 8 * slot);
        let length = peek::<u32>(self.used + 8 + 8 * slot);
        self.completed = self.completed.wrapping_add(1);
        let status = peek::<u8>(self.status);
        if id != 0 || length != READ_USED_LEN || status != BLK_S_OK {
            fail("completion", sector);
        }
    }

    fn first_word(&self) -> u64 {
        peek::<u64>(self.data)
    }
}

// --- the run --------------------------------------------------------------------

struct Run {
    queue: Queue,
    digest: u64,
    measured: u64,
}

impl Run {
    fn measured_read(&mut self, sector: u64) {
        self.queue.read(sector);
        self.digest = (self.digest * DIGEST_MULTIPLIER + sector) % DIGEST_MODULUS;
        self.measured += 1;
    }
}

/// A static map buffer, because nothing may be allocated between reading the
/// map and handing the machine over.
static mut MEMORY_MAP: [u8; 32768] = [0; 32768];

fn allocate_page(boot: &BootServices) -> u64 {
    let mut address: u64 = 0xffff_f000;
    if (boot.allocate_pages)(ALLOCATE_MAX_ADDRESS, LOADER_DATA, 1, &mut address) != EFI_SUCCESS {
        fail("allocate", 0);
    }
    for offset in (0..4096).step_by(8) {
        poke::<u64>(address + offset, 0);
    }
    address
}

fn leave_firmware(boot: &BootServices, image: *mut c_void) {
    for _ in 0..2 {
        let mut size = 32768usize;
        let (mut key, mut descriptor_size, mut version) = (0usize, 0usize, 0u32);
        // The buffer is this application's own static, used by nothing else.
        let buffer = addr_of_mut!(MEMORY_MAP).cast::<u8>();
        if (boot.get_memory_map)(
            &mut size,
            buffer,
            &mut key,
            &mut descriptor_size,
            &mut version,
        ) != EFI_SUCCESS
        {
            fail("memory-map", size as u64);
        }
        if (boot.exit_boot_services)(image, key) == EFI_SUCCESS {
            return;
        }
    }
    fail("exit-boot-services", 0);
}

/// The UEFI entry point.
///
/// # Safety
///
/// Called once by the firmware, with this image's handle and a live system
/// table, as the UEFI image entry contract states.
#[no_mangle]
// SAFETY: the firmware is the only caller, and it keeps the entry contract above.
pub unsafe extern "efiapi" fn efi_main(image: *mut c_void, system: *mut SystemTable) -> Status {
    // SAFETY: the firmware entry contract supplies a live system table.
    let boot = unsafe { &*(*system).boot_services };
    let page = allocate_page(boot);
    let idt = allocate_page(boot);
    leave_firmware(boot, image);
    // SAFETY: the machine is this application's from here; nothing may interrupt
    // it until the table below routes every vector.
    unsafe { asm!("cli", options(nomem, nostack)) };
    let (_lapic, apic_id) = take_interrupts(idt);
    let function = find_device();
    let queue = bring_up(function, page, apic_id);
    let queue_size = queue.size;
    let mut run = Run {
        queue,
        digest: 0,
        measured: 0,
    };

    say("\r\nORACLE.BEGIN bus=");
    say_number(u64::from(function.bus));
    say(" device=");
    say_number(u64::from(function.device));
    say(" queue_size=");
    say_number(u64::from(queue_size));
    say(" apic_id=");
    say_number(u64::from(apic_id));
    say(" page=");
    say_number(run.queue.page);
    say("\r\n");

    // Warm-up: every READ checked against the image, none measured.
    for sector in 0..WARMUP_SECTORS {
        run.queue.read(sector);
        if run.queue.first_word() != sector {
            fail("warmup-pattern", sector);
        }
    }

    wire_drain();
    wire_write(READY);
    while wire_read() != GO {}

    for pair in 0..FLOOR_PAIRS {
        let tag = pair as u8 & SEQUENCE;
        wire_write(OPEN | tag);
        wire_write(CLOSE | tag);
    }
    for window in 0..R1_WINDOWS {
        let tag = WORK | (window as u8 & SEQUENCE);
        let first = R1_FIRST_SECTOR + window * R1_WINDOW_READS;
        wire_write(OPEN | tag);
        for sector in first..first + R1_WINDOW_READS {
            run.measured_read(sector);
        }
        wire_write(CLOSE | tag);
    }
    let mut state = R2_SEED;
    for operation in 0..R2_OPERATIONS {
        state = state * R2_MULTIPLIER % R2_MODULUS;
        let first = (state % R2_BLOCKS) * R2_READS_PER_OPERATION;
        let tag = WORK | (operation as u8 & SEQUENCE);
        wire_write(OPEN | tag);
        for sector in first..first + R2_READS_PER_OPERATION {
            run.measured_read(sector);
        }
        wire_write(CLOSE | tag);
    }

    // The observer disarms its window event and then says STOP; ending the
    // machine before that would race the disarm.
    while wire_read() != STOP {}

    // SAFETY: interrupts are masked again; nothing writes these any more.
    let (deliveries, unexpected) = unsafe {
        (
            read_volatile(addr_of!(ORACLE_DELIVERIES)),
            read_volatile(addr_of!(ORACLE_UNEXPECTED)),
        )
    };
    say("\r\nORACLE.RESULT warmup_verified=");
    say_number(WARMUP_SECTORS);
    say(" measured_reads=");
    say_number(run.measured);
    say(" digest=");
    say_number(run.digest);
    say(" deliveries=");
    say_number(deliveries);
    say(" unexpected=");
    say_number(unexpected);
    say("\r\n");
    let total = WARMUP_SECTORS + run.measured;
    if deliveries != total || unexpected != 0 {
        fail("interrupt-account", deliveries);
    }
    exit(EXIT_PASS)
}
