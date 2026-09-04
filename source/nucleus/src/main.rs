// SPDX-License-Identifier: GPL-3.0-or-later
//! TOS Stage 1 nucleus (freestanding, x86_64).
//!
//! Entry convention (boot ABI v1): `rdi` = physical address of a
//! [`BootInfo`]. The nucleus validates the ABI record and the capsule, emits
//! the serial boot-event log (BOOT_ABI_V1.md §6), runs the canonical boot
//! module, then halts via the QEMU isa-debug-exit port with a stable result
//! code.
//!
//! Once the ABI record and the memory map have been accepted, the same facts
//! are also drawn on the framebuffer for a person watching the machine start
//! (see [`console`]). That presentation is best-effort throughout: it is
//! created only if there is a framebuffer to create it on, every call into it
//! is allowed to do nothing, and no result below depends on it. The serial
//! events remain the normative record.

#![no_std]
#![no_main]

// The two launcher constants describe different systems — a pair sharing an
// endpoint, and one process able to create others — and a build asking for both
// would be asking which of two decisions the launcher made.
#[cfg(any(
    all(feature = "test-deputy", feature = "test-two-processes"),
    all(feature = "test-deputy", feature = "test-supervisor"),
    all(feature = "test-deputy", feature = "test-deadlock"),
    all(feature = "test-deputy", feature = "test-call-reply"),
    all(feature = "test-two-processes", feature = "test-supervisor"),
    all(feature = "test-two-processes", feature = "test-deadlock"),
    all(feature = "test-two-processes", feature = "test-call-reply"),
    all(feature = "test-supervisor", feature = "test-deadlock"),
    all(feature = "test-supervisor", feature = "test-call-reply"),
    all(feature = "test-deadlock", feature = "test-call-reply"),
    all(feature = "test-second-receiver", feature = "test-two-processes"),
    all(feature = "test-second-receiver", feature = "test-supervisor"),
    all(feature = "test-second-receiver", feature = "test-deadlock"),
    all(feature = "test-second-receiver", feature = "test-call-reply"),
    all(feature = "test-second-receiver", feature = "test-deputy"),
    all(feature = "test-module-operation", feature = "test-two-processes"),
    all(feature = "test-module-operation", feature = "test-supervisor"),
    all(feature = "test-module-operation", feature = "test-deadlock"),
    all(feature = "test-module-operation", feature = "test-call-reply"),
    all(feature = "test-module-operation", feature = "test-deputy"),
    all(feature = "test-module-operation", feature = "test-second-receiver"),
    all(feature = "test-wrong-kind", feature = "test-two-processes"),
    all(feature = "test-wrong-kind", feature = "test-supervisor"),
    all(feature = "test-wrong-kind", feature = "test-deadlock"),
    all(feature = "test-wrong-kind", feature = "test-call-reply"),
    all(feature = "test-wrong-kind", feature = "test-deputy"),
    all(feature = "test-wrong-kind", feature = "test-second-receiver"),
    all(feature = "test-wrong-kind", feature = "test-module-operation"),
    all(feature = "test-process-control", feature = "test-two-processes"),
    all(feature = "test-process-control", feature = "test-supervisor"),
    all(feature = "test-process-control", feature = "test-deadlock"),
    all(feature = "test-process-control", feature = "test-call-reply"),
    all(feature = "test-process-control", feature = "test-deputy"),
    all(feature = "test-process-control", feature = "test-second-receiver"),
    all(feature = "test-process-control", feature = "test-module-operation"),
    all(feature = "test-process-control", feature = "test-wrong-kind"),
    all(feature = "test-process-terminate", feature = "test-two-processes"),
    all(feature = "test-process-terminate", feature = "test-supervisor"),
    all(feature = "test-process-terminate", feature = "test-deadlock"),
    all(feature = "test-process-terminate", feature = "test-call-reply"),
    all(feature = "test-process-terminate", feature = "test-deputy"),
    all(feature = "test-process-terminate", feature = "test-second-receiver"),
    all(feature = "test-process-terminate", feature = "test-module-operation"),
    all(feature = "test-process-terminate", feature = "test-wrong-kind"),
    all(feature = "test-process-control", feature = "test-process-terminate"),
    all(feature = "test-process-launch", feature = "test-two-processes"),
    all(feature = "test-process-launch", feature = "test-supervisor"),
    all(feature = "test-process-launch", feature = "test-deadlock"),
    all(feature = "test-process-launch", feature = "test-call-reply"),
    all(feature = "test-process-launch", feature = "test-deputy"),
    all(feature = "test-process-launch", feature = "test-second-receiver"),
    all(feature = "test-process-launch", feature = "test-module-operation"),
    all(feature = "test-process-launch", feature = "test-wrong-kind"),
    all(feature = "test-process-launch", feature = "test-process-control"),
    all(feature = "test-process-launch", feature = "test-process-terminate"),
    all(feature = "test-lifecycle", feature = "test-process-launch"),
    all(feature = "test-lifecycle", feature = "test-two-processes"),
    all(feature = "test-lifecycle", feature = "test-supervisor"),
    all(feature = "test-lifecycle", feature = "test-call-reply"),
    all(feature = "test-lifecycle", feature = "test-process-control"),
    all(feature = "test-lifecycle", feature = "test-process-terminate"),
    all(feature = "test-region-transport", feature = "test-two-processes"),
    all(feature = "test-region-transport", feature = "test-supervisor"),
    all(feature = "test-region-transport", feature = "test-deadlock"),
    all(feature = "test-region-transport", feature = "test-call-reply"),
    all(feature = "test-region-transport", feature = "test-deputy"),
    all(feature = "test-region-transport", feature = "test-second-receiver"),
    all(feature = "test-region-transport", feature = "test-module-operation"),
    all(feature = "test-region-transport", feature = "test-wrong-kind"),
    all(feature = "test-region-transport", feature = "test-process-control"),
    all(feature = "test-region-transport", feature = "test-process-terminate"),
    all(feature = "test-region-transport", feature = "test-process-launch"),
    all(feature = "test-region-transport", feature = "test-lifecycle"),
    all(feature = "test-region-transport", feature = "test-memory-authority"),
    all(feature = "test-region-transport", feature = "test-creation-rollback"),
    all(feature = "test-region-faults", feature = "test-two-processes"),
    all(feature = "test-region-faults", feature = "test-supervisor"),
    all(feature = "test-region-faults", feature = "test-deadlock"),
    all(feature = "test-region-faults", feature = "test-call-reply"),
    all(feature = "test-region-faults", feature = "test-deputy"),
    all(feature = "test-region-faults", feature = "test-second-receiver"),
    all(feature = "test-region-faults", feature = "test-module-operation"),
    all(feature = "test-region-faults", feature = "test-wrong-kind"),
    all(feature = "test-region-faults", feature = "test-process-control"),
    all(feature = "test-region-faults", feature = "test-process-terminate"),
    all(feature = "test-region-faults", feature = "test-process-launch"),
    all(feature = "test-region-faults", feature = "test-lifecycle"),
    all(feature = "test-region-faults", feature = "test-memory-authority"),
    all(feature = "test-region-faults", feature = "test-creation-rollback"),
    all(feature = "test-region-faults", feature = "test-region-transport"),
    all(feature = "test-bundle-launch", feature = "test-two-processes"),
    all(feature = "test-bundle-launch", feature = "test-supervisor"),
    all(feature = "test-bundle-launch", feature = "test-deadlock"),
    all(feature = "test-bundle-launch", feature = "test-call-reply"),
    all(feature = "test-bundle-launch", feature = "test-deputy"),
    all(feature = "test-bundle-launch", feature = "test-second-receiver"),
    all(feature = "test-bundle-launch", feature = "test-module-operation"),
    all(feature = "test-bundle-launch", feature = "test-wrong-kind"),
    all(feature = "test-bundle-launch", feature = "test-process-control"),
    all(feature = "test-bundle-launch", feature = "test-process-terminate"),
    all(feature = "test-bundle-launch", feature = "test-process-launch"),
    all(feature = "test-bundle-launch", feature = "test-lifecycle"),
    all(feature = "test-bundle-launch", feature = "test-memory-authority"),
    all(feature = "test-bundle-launch", feature = "test-creation-rollback"),
    all(feature = "test-bundle-launch", feature = "test-region-transport"),
    all(feature = "test-bundle-launch", feature = "test-region-faults")
))]
compile_error!("these are different launcher constants, and a build must be one of them");

#[cfg(all(
    feature = "test-measurement-no-preemption",
    any(feature = "test-two-processes", feature = "test-call-reply")
))]
compile_error!("the IPC numerator must keep preemption active");

#[cfg_attr(feature = "test-measurement-no-preemption", allow(dead_code))]
mod apic;
mod backing;
mod capability;
mod console;
/// Mapped device memory (ADR-0081). A descendant of a PCI function
/// assignment, and not an ordinary region: nothing funds it and nothing
/// reclaims it to the pool.
mod device;
mod exception;
mod framebuffer;
#[cfg(feature = "test-creation-rollback")]
mod injection;
mod ipc;
mod launch;
mod memory;
mod msr;
mod paging;
/// The PCI configuration mechanism (ADR-0079, `SYSTEM_ABI_V1` §2.1).
///
/// Mechanism and no policy: it performs a configuration transaction against the
/// function a capability names, and it cannot enumerate, match a driver or tell
/// one device class from another.
mod pci;
mod plan;
mod process;
/// Memory authority and region objects (ADR-0075). Not reachable from ring 3
/// yet: the state machine is proved before the operations that would drive it.
#[allow(dead_code)]
mod region;
#[cfg(any(
    feature = "test-ring3-abi",
    feature = "test-ring3-privileged",
    feature = "test-ring3-nucleus"
))]
mod ring3;
mod syscall;

use core::arch::asm;
use core::panic::PanicInfo;

use console::{BootConsole, Text};

use tos_boot_protocol::{
    BootInfo, MemoryRange, RESULT_ABI_INVALID, RESULT_BOOT_MODULE_FAILED, RESULT_CAPSULE_INVALID,
    RESULT_HALT_OK, RESULT_MEMORY_INVALID, RESULT_PANIC, RESULT_PORT, SRC_KIND_DETACHED,
    SRC_KIND_GIT,
};
use tos_capsule::parse;
#[cfg(feature = "test-crypto-baseline")]
use tos_capsule::test_crypto_baseline::verify as verify_parser_crypto;
#[cfg(feature = "test-crypto-baseline")]
use tos_capsule::Capsule;
use tos_hash::{sha256, Sha256};

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    tos_serial::puts(b"TOS.PANIC nucleus\r\n");
    result_port(RESULT_PANIC)
}

/// Write an exit code to the QEMU isa-debug-exit port and stop.
fn result_port(code: u8) -> ! {
    // SAFETY: RESULT_PORT is the fixed QEMU isa-debug-exit I/O port in the
    // declared Stage 1 profile; this single-byte OUT has no memory operands.
    unsafe {
        asm!(
            "out dx, al",
            in("al") code,
            in("dx") RESULT_PORT,
            options(nomem, nostack, preserves_flags)
        );
    }
    loop {
        // SAFETY: interrupts remain disabled and this terminal path owns the
        // CPU, so HLT cannot expose shared mutable state or resume Stage 1.
        unsafe {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

fn abi_fail() -> ! {
    tos_serial::puts(b"TOS.ABI.FAIL\r\n");
    result_port(RESULT_ABI_INVALID)
}

fn mem_fail() -> ! {
    tos_serial::puts(b"TOS.MEM.FAIL\r\n");
    result_port(RESULT_MEMORY_INVALID)
}

fn cap_fail() -> ! {
    tos_serial::puts(b"TOS.CAPSULE.FAIL\r\n");
    result_port(RESULT_CAPSULE_INVALID)
}

/// Show a failure on the boot console, when there is one to show.
///
/// Deliberately returns nothing and decides nothing: the caller has already
/// reported the failure over serial and is on its way to a result code. This is
/// the picture of that decision, never the decision.
fn console_failed(console: &mut Option<BootConsole<'static>>, code: &[u8], detail: &[u8]) {
    if let Some(console) = console {
        console.fail(code, detail);
    }
}

/// Test-only baseline for the exact SHA-256 operations on a successful boot.
/// The preceding production `parse` supplies only a structural borrowed view;
/// every timed digest below starts from fresh `Sha256` state.
#[cfg(feature = "test-crypto-baseline")]
fn crypto_baseline(cap_bytes: &[u8], capsule: &Capsule<'_>) -> ! {
    let boot = match capsule.boot_file() {
        Some(file) => file,
        None => cap_fail(),
    };
    tos_serial::puts(b"TOS.TEST.CRYPTO.BASELINE.START\r\n");

    // This is the existing loader->BootInfo->nucleus mirror: its one carried
    // digest is an explicit ABI value, while both parser crypto passes remain
    // freshly recomputed below.
    let loader_capsule_digest = sha256(cap_bytes);
    let first = match verify_parser_crypto(capsule) {
        Ok(accounting) => accounting,
        Err(_) => cap_fail(),
    };
    let nucleus_capsule_digest = sha256(cap_bytes);
    if nucleus_capsule_digest != loader_capsule_digest {
        cap_fail();
    }
    let second = match verify_parser_crypto(capsule) {
        Ok(accounting) => accounting,
        Err(_) => cap_fail(),
    };
    if second != first {
        cap_fail();
    }
    // The normal nucleus emits this post-lookup digest too.
    let _boot_digest = sha256(boot.content);

    let bytes = first.bytes_hashed * 2 + (cap_bytes.len() as u64) * 2 + boot.content.len() as u64;
    let hashes = first.hash_invocations * 2 + 3;
    if bytes > u32::MAX as u64 {
        cap_fail();
    }
    tos_serial::puts(b"TOS.TEST.CRYPTO.BASELINE.DONE bytes=");
    tos_serial::put_u32_decimal(bytes as u32);
    tos_serial::puts(b" hashes=");
    tos_serial::put_u32_decimal(hashes);
    tos_serial::puts(b"\r\n");
    result_port(RESULT_HALT_OK)
}

/// The name a launcher constant binds a grant to (ADR-0061).
///
/// A name this image cannot carry is this image's own defect, not a policy
/// outcome: it would start a process holding authority under a name nothing can
/// read, so it stops instead.
///
/// Unused in a production build, and that is the policy holding rather than code
/// going spare: `system.boot.init` requests no capability, so the launcher's
/// constant grants none and there is no name to bind (ADR-0055). The allow says
/// so rather than letting the warning imply the function is surplus.
#[allow(dead_code)]
fn binding(name: &[u8]) -> capability::Binding {
    match capability::Binding::new(name) {
        Some(binding) => binding,
        None => {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=binding-too-long\r\n");
            mem_fail();
        }
    }
}

/// Writes what the boot's IPC actually cost (`IPC_V1` §8, §9.7).
///
/// Six numbers and no arithmetic: the ratios §8 bounds are computed by whoever
/// reads them, because a nucleus that reported "2 copies per message" would be
/// reporting its own opinion of a division rather than what it counted.
///
/// The crossings are split because §8's bound is **per request/reply** and a
/// boot's total is not that: `time_monotonic` in a spin loop crosses the same
/// edge and belongs to no exchange. `ipc_in` counts only the four operations an
/// exchange is made of; `returns` counts calls that came back through the edge,
/// which a blocked one does not; `resumptions` counts contexts the scheduler
/// entered, which is the other direction for exactly those. Preemption is in
/// none of them, because a tick returns through the timer stub — which is the
/// exclusion §8 states.
fn ipc_cost() {
    let (messages, copies) = ipc::cost();
    let (ipc_in, other_in, returns) = syscall::crossings();
    tos_serial::puts(b"TOS.RUN.IPC.COST messages=");
    tos_serial::put_u32_decimal(messages as u32);
    tos_serial::puts(b" payload_copies=");
    tos_serial::put_u32_decimal(copies as u32);
    tos_serial::puts(b" ipc_in=");
    tos_serial::put_u32_decimal(ipc_in as u32);
    tos_serial::puts(b" other_in=");
    tos_serial::put_u32_decimal(other_in as u32);
    tos_serial::puts(b" returns=");
    tos_serial::put_u32_decimal(returns as u32);
    tos_serial::puts(b" resumptions=");
    tos_serial::put_u32_decimal(process::entries() as u32);
    // How many request/reply exchanges there were — the unit §8 states its
    // crossing bound in — and how many outward crossings the operations of those
    // exchanges made, by whichever of the three doors each used. With `ipc_in`
    // they are the whole of §8's ratio, and the division is still the reader's.
    tos_serial::puts(b" exchanges=");
    tos_serial::put_u32_decimal(syscall::exchanges() as u32);
    tos_serial::puts(b" ipc_out=");
    tos_serial::put_u32_decimal(syscall::ipc_returns() as u32);
    tos_serial::puts(b"\r\n");
}

/// The declared identity of the capsule's source tree, written into `out`.
///
/// A detached capsule's identity is a whole-tree digest and a git one is an
/// object id. Both are named by their kind so that neither is read as the
/// other.
fn source_set_identity(kind: &[u8], value: &[u8; 32], out: &mut [u8; 96]) -> usize {
    let mut hex = [0u8; 64];
    tos_hash::hex(value, &mut hex);
    let mut at = 0;
    for byte in kind.iter().chain(b":").chain(hex.iter()) {
        if at == out.len() {
            break;
        }
        out[at] = *byte;
        at += 1;
    }
    at
}

/// First logical line of boot text: the first line that is non-empty and not
/// a comment. Returns the trimmed line.
///
/// A TOS Core line comment starts with `//` (docs/39 section 2), so the SPDX
/// header and the module's prose are skipped and the reported line is the one
/// that says what the module is. `#` is still skipped: it cannot begin a
/// logical line in TOS Core, so skipping it can never hide one.
fn first_logical_line(content: &[u8]) -> Option<&[u8]> {
    for raw in content.split(|&b| b == b'\n') {
        let mut line = raw;
        while let Some((&b, rest)) = line.split_first() {
            if b == b' ' || b == b'\t' || b == b'\r' {
                line = rest;
            } else {
                break;
            }
        }
        if line.is_empty() {
            continue;
        }
        if line[0] == b'#' || line.starts_with(b"//") {
            continue;
        }
        return Some(line);
    }
    None
}

// SAFETY (entry-point contract): `bi_raw` is the physical address of a BootInfo
// record placed in `rdi` by the loader per BOOT_ABI_V1 §3, in an identity-mapped
// region the loader marked reserved. It is validated as raw bytes
// (`BootInfo::validate_bytes`) before it is ever read as a struct. The function
// cannot be an `unsafe fn`: the loader transfers control to it with a machine
// `call`, not a Rust call, so `clippy::not_unsafe_ptr_arg_deref` does not apply.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[cfg_attr(feature = "test-crypto-baseline", allow(unreachable_code))]
#[no_mangle]
#[link_section = ".text.boot_entry"]
pub extern "C" fn boot_entry(bi_raw: *const BootInfo) -> ! {
    tos_serial::init();
    tos_serial::puts(b"TOS.NUCLEUS.ENTRY\r\n");

    // Install nucleus-owned exception containment before dereferencing or
    // trusting any loader-provided memory. The loader disabled maskable
    // interrupts before handoff; Stage 1 intentionally leaves them disabled.
    // SAFETY: boot_entry runs exactly once before any BootInfo-controlled
    // memory is read, and the loader left maskable interrupts disabled.
    unsafe { exception::install() };
    // The edge across the isolation boundary is part of the substrate, so it is
    // installed on every boot — before there is anything to call it, and
    // whether or not this boot ever reaches CPL 3.
    // SAFETY: the nucleus-owned GDT was loaded by the call immediately above,
    // which is what makes the selectors written into `IA32_STAR` name real
    // descriptors.
    unsafe { syscall::install() };

    #[cfg(any(feature = "test-exception-ud2", feature = "test-exception-gp"))]
    exception::test_injection();

    // --- 1. validate the boot ABI record over raw bytes ---
    // SAFETY: the loader's BOOT_ABI_V1 handoff places all `STRUCT_SIZE` BootInfo
    // in its reserved identity-mapped pool allocation; bytes are validated
    // before this pointer is reinterpreted as BootInfo below.
    let bi_bytes = unsafe {
        core::slice::from_raw_parts(bi_raw as *const u8, tos_boot_protocol::STRUCT_SIZE as usize)
    };
    if BootInfo::validate_bytes(bi_bytes).is_err() {
        abi_fail();
    }
    // SAFETY: the loader wrote a naturally aligned BootInfo into that pool;
    // the preceding raw-byte validation accepted its exact ABI representation.
    let bi = unsafe { &*bi_raw };
    let bi_address = bi_raw as u64;

    // --- 2. memory map ---
    if bi.memory_map_length == 0 || bi.memory_map_length % 24 != 0 {
        mem_fail();
    }
    let desc_count = (bi.memory_map_length / 24) as usize;
    // SAFETY: the trusted loader created this descriptor array in its reserved
    // identity-mapped allocation; BootInfo bounds make `desc_count` integral.
    let descs: &[MemoryRange] = unsafe {
        core::slice::from_raw_parts(bi.memory_map_phys as *const MemoryRange, desc_count)
    };
    if bi.check_memory_map(descs).is_err() {
        mem_fail();
    }
    if bi.check_capsule_in_memory(descs).is_err() {
        mem_fail();
    }

    // --- 2b. the framebuffer becomes usable here, and not before ---
    // The console is a report about this machine, so it may not be built out of
    // values the machine has not accepted yet. Everything it needs has now been
    // checked: BOOT_ABI_V1 validation accepted the framebuffer tuple over raw
    // bytes — address, geometry, byte pitch and a supported format — and the
    // memory map that describes the range has been accepted as a whole. Neither
    // check was weakened to get a picture on screen sooner.
    //
    // The two facts already established are drawn retrospectively; everything
    // after this point is drawn as it happens. With no usable framebuffer this
    // is `None` and every call below does nothing, which is the whole of the
    // best-effort contract: the boot is identical, minus the picture.
    //
    // SAFETY: the checks named above are exactly what `framebuffer::map`
    // requires of its caller, and this is the only borrow of the framebuffer
    // taken anywhere in the nucleus.
    let mut console = unsafe { framebuffer::map(bi) }.map(BootConsole::new);
    if let Some(console) = &mut console {
        console.fact(b"Boot ABI v1", None);
        console.fact(b"Memory map validated", None);
        console.begin(b"Verifying capsule", None);
    }

    // --- 3. capsule: plain digest, then full structural validation ---
    // Overflow safety: slice length must fit in usize (x86_64: u64). Range
    // containment within a declared memory map entry is enforced separately by
    // check_capsule_in_memory above (memory_map_length is unrelated to the
    // capsule size, so it is NOT compared against capsule_length).
    if bi.capsule_phys == 0 || bi.capsule_length == 0 || bi.capsule_length > usize::MAX as u64 {
        console_failed(&mut console, b"CAPSULE_ABSENT", b"");
        cap_fail();
    }
    // SAFETY: the loader reserved the capsule range in the same identity map;
    // the checked u64-to-usize conversion above bounds this borrowed slice.
    let cap_bytes = unsafe {
        core::slice::from_raw_parts(bi.capsule_phys as *const u8, bi.capsule_length as usize)
    };
    if sha256(cap_bytes) != bi.capsule_digest {
        console_failed(&mut console, b"CAPSULE_DIGEST_MISMATCH", b"");
        cap_fail();
    }
    let cap = match parse(cap_bytes) {
        Ok(c) => c,
        Err(_) => {
            console_failed(&mut console, b"CAPSULE_MALFORMED", b"");
            cap_fail()
        }
    };

    #[cfg(feature = "test-crypto-baseline")]
    crypto_baseline(cap_bytes, &cap);

    // The handoff record only mirrors the capsule's identity fields
    // (BOOT_ABI_V1 §6); it does not prove them. Verify the mirror against the
    // header of the capsule just parsed, so TOS.IDENTITY reports what the
    // artifact carries rather than what the record claimed. Fails closed with
    // RESULT_CAPSULE_INVALID, the same code already used when capsule_digest
    // disagrees with the capsule bytes.
    let ch = cap.header();
    if bi
        .check_capsule_identity(
            ch.source_identity_kind,
            ch.source_oid_alg,
            ch.source_oid_length,
            &ch.source_identity_value,
        )
        .is_err()
    {
        tos_serial::puts(b"TOS.IDENTITY.MISMATCH bootinfo-vs-capsule-header\r\n");
        console_failed(
            &mut console,
            b"IDENTITY_MISMATCH",
            b"bootinfo-vs-capsule-header",
        );
        cap_fail();
    }

    // Digest, structure and the identity mirror all held: this is the point at
    // which "the capsule is what it says it is" became true.
    if let Some(console) = &mut console {
        console.succeed();
    }

    tos_serial::puts(b"TOS.CAPSULE.OK files=");
    tos_serial::put_u32_decimal(cap.file_count());
    tos_serial::puts(b"\r\n");

    // --- 4. canonical boot text ---
    let boot = match cap.boot_file() {
        Some(f) => f,
        None => {
            console_failed(&mut console, b"BOOT_MODULE_MISSING", b"");
            cap_fail()
        }
    };
    tos_serial::puts(b"TOS.BOOTTEXT.PATH ");
    tos_serial::puts(boot.name);
    tos_serial::puts(b"\r\n");
    if let Some(line) = first_logical_line(boot.content) {
        tos_serial::puts(b"TOS.BOOTTEXT.LINE ");
        tos_serial::puts(line);
        tos_serial::puts(b"\r\n");
    }
    let mut h = Sha256::new();
    h.update(boot.content);
    let boot_digest = h.finalize();
    tos_serial::puts(b"TOS.BOOTTEXT.DIGEST ");
    tos_serial::put_hex32(&boot_digest);
    tos_serial::puts(b"\r\n");
    if let Some(console) = &mut console {
        console.fact(b"Canonical boot module", Some(boot.name));
    }

    // --- 5. identity record ---
    // Provenance is read from the parsed capsule header, never hardcoded and
    // never taken on the handoff record's word: the record was proved equal to
    // this header above, and the header belongs to bytes whose digest was
    // verified. The builder writes SRC_KIND_GIT when the capsule was built with
    // --git-commit (or SRC_KIND_DETACHED otherwise).
    let kind: &[u8] = match ch.source_identity_kind {
        SRC_KIND_GIT => b"git",
        SRC_KIND_DETACHED => b"detached",
        // parse() rejects any other value; fall back to a stable error marker
        // rather than a misleading kind.
        _ => b"unknown",
    };
    tos_serial::puts(b"TOS.IDENTITY source_kind=");
    tos_serial::puts(kind);
    tos_serial::puts(b" source_digest=");
    tos_serial::put_hex32(&ch.source_identity_value);
    tos_serial::puts(b" capsule_digest=");
    tos_serial::put_hex32(&bi.capsule_digest);
    tos_serial::puts(b" arch=0.2.1 builder=1\r\n");
    if let Some(console) = &mut console {
        // The same values the event above carries, shortened to what fits a
        // line a person reads. The full identity stays on serial.
        let mut hex = [0u8; 64];
        tos_hash::hex(&ch.source_identity_value, &mut hex);
        let mut detail = Text::<32>::new();
        detail.push(kind).push(b" ").push(&hex[..12]);
        console.fact(b"Source identity", Some(detail.as_bytes()));
    }

    // --- 6. Stage 2: run the canonical boot module ---
    // The capsule's boot text is a TOS Core module, and it goes through the
    // ordinary reference path. Nothing here is special because it is boot: the
    // nucleus calls the same pipeline a hosted test calls, and a module the
    // frontend, the verifier or the engine refuses stops the boot rather than
    // being waved through. The nucleus owns memory discovery and hands the
    // runtime one bounded region (ADR-0041); the runtime cannot name BootInfo.
    // The console goes with it: what the reference path is doing is the part of
    // this boot a person can actually watch, and the pipeline already announces
    // every stage before it runs.
    // Every `.tos` file the capsule carries is a module of the set. The rest of
    // the capsule — the version marker, the licence notice — is not source and
    // is not offered as such: a set that included them would ask the frontend
    // to parse a file that never claimed to be a module.
    let mut modules: [(&[u8], &[u8]); launch::MAX_BOOT_MODULES] =
        [(&[], &[]); launch::MAX_BOOT_MODULES];
    let mut module_count = 0usize;
    for file in cap.files() {
        if !file.name.ends_with(b".tos") {
            continue;
        }
        if module_count == modules.len() {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=too-many-modules\r\n");
            console_failed(&mut console, b"TOO_MANY_MODULES", b"");
            mem_fail();
        }
        modules[module_count] = (file.name, file.content);
        module_count += 1;
    }

    // The frames of this machine become the nucleus's, once, here. Everything
    // memory-shaped downstream — the runtime's grant today, an address space and
    // a process tomorrow — comes out of this pool and nowhere else, which is
    // what makes the nucleus the owner ADR-0050 section 1 describes rather than
    // one more component that reads the map.
    let running_on = memory::running_stack(descs);
    // SAFETY: `bi` and `descs` passed the Boot ABI v1 validation performed
    // above — the record over its raw bytes, the map for self-consistency and
    // for containing the capsule — and `running_on` is the map entry holding
    // this frame's own stack pointer.
    let admission = unsafe { memory::admit_memory(bi, bi_address, descs, running_on) };
    if admission.frames == 0 {
        tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-frames\r\n");
        console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"no-frames");
        mem_fail();
    }

    // The page-table reserve, taken out of the pool here and nowhere else.
    //
    // This is the whole of what ADR-0076 §2 rule 3 allows outside the authority
    // tree: bounded, and reserved before the tree exists. The bound is computed
    // from this machine's map and this nucleus's own layout constants — the
    // regions a process is given, each at the largest the accepted limits let it
    // be — so it is derived rather than measured, and it does not move when a
    // capsule does. Everything a process is actually *given* is charged; only
    // the tables that map it are not, because a table is the nucleus's own
    // structure and no process can reach one.
    // The nucleus takes over its own address space here, and not earlier: the
    // tables are frames from the pool, so the pool has to exist first. Until
    // this instruction the machine ran on the firmware's identity map — a map
    // the nucleus never wrote and cannot describe, which after ADR-0048 is the
    // very thing that has to keep one process out of another's memory.
    // Mutable only in a test configuration: the ring-3 excursion maps its own
    // pages into this space, and the production path only ever reads it.
    #[allow(unused_mut)]
    // SAFETY: nucleus entry; nothing else holds the pool, and no process
    // exists yet.
    // SAFETY: boot, single-context, nothing else holds the pool.
    let before_own_space = unsafe { memory::frames() }.available();
    // Mutable only in a test configuration: the ring-3 excursion maps its own
    // pages into this space, and the production path only ever reads it.
    #[allow(unused_mut)]
    // SAFETY: nucleus entry; nothing else holds the pool, and no process
    // exists yet.
    let mut space = match paging::build(bi, descs, unsafe { memory::frames() }) {
        Ok(space) => space,
        Err(_) => {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-address-space\r\n");
            console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"no-address-space");
            mem_fail();
        }
    };
    // SAFETY: `space` was built from the same validated map every mapping
    // decision above came from: every described region of memory, the loader's
    // stack and handoff record among them, plus the declared framebuffer, plus
    // this image with its own text executable. Maskable interrupts are off and
    // this is the only context running.
    unsafe { space.activate() };

    // **After the nucleus owns its own map, and not before.** Until this point
    // the machine runs on the map UEFI left behind, in which some of what the
    // memory map reports usable — memory that genuinely becomes ours when boot
    // services exit — is still mapped read-only. Writing to it faults, and
    // taking a reserve means writing to every frame of it. The nucleus's own
    // address space came from the pool a moment ago, bounded and before
    // anything could promise it; everything after this line comes from the
    // reserve.
    //
    // The region aperture has to fit this nucleus's own layout before anything
    // is sized from it: every lane above every fixed window, the last ending
    // inside the lower canonical half, and no arithmetic that wraps. A machine
    // this is not true of is refused rather than given lanes that overlap a
    // stack.
    if !process::aperture_fits() {
        tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-region-aperture\r\n");
        console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"no-region-aperture");
        mem_fail();
    }
    let bound = process::table_reserve(bi, descs, admission.frames * tos_frames::FRAME_SIZE);
    // SAFETY: boot, immediately after admission and before any address space,
    // process or memory authority exists.
    let Some(reserved) = (unsafe { memory::reserve_tables(bound) }) else {
        tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-table-reserve wanted_frames=");
        tos_serial::put_u32_decimal(bound as u32);
        tos_serial::puts(b" pool_frames=");
        // SAFETY: boot, single-context, nothing else holds the pool.
        tos_serial::put_u32_decimal(unsafe { memory::frames() }.available() as u32);
        tos_serial::puts(b"\r\n");
        console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"no-table-reserve");
        mem_fail();
    };

    // What the machine admitted, what left the pool before anything could
    // promise it, and what the pool has left for the authority tree to be
    // endowed with. Three numbers rather than one line saying memory is fine:
    // a gate can check that they add up, and that the reserve is actually
    // paying for the nucleus's own address space rather than the pool doing it
    // quietly.
    // The backing index is rooted before the tree is endowed: its one permanent
    // frame is part of the reserve, not of what the root will promise.
    // SAFETY: boot, after the reserve and before any region exists.
    if !unsafe { memory::root_backing() } {
        tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-region-backing\r\n");
        console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"no-region-backing");
        mem_fail();
    }

    // The root memory authority, over exactly what the pool has left. From here
    // there is one number for free user memory and it is the tree's: the pool
    // still hands out physical frames, but only behind a charge made first
    // (ADR-0076 §2 rule 4).
    // SAFETY: boot, after the reserve and before any process exists.
    let Some(root_bytes) = (unsafe { memory::endow_root() }) else {
        tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-root-authority\r\n");
        console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"no-root-authority");
        mem_fail();
    };

    // The reserve, decomposed, so a gate can check that no term is counted
    // twice. `nucleus_space_actual` is memory already gone from the pool before
    // the reserve existed; `process_identity` is the most *one future process's*
    // identity mappings can cost the reserve. They are related quantities and
    // not one account, and confusing them is what put a second nucleus address
    // space in the reserve.
    let admitted_bytes = admission.frames * tos_frames::FRAME_SIZE;
    tos_serial::puts(b"TOS.MEM.RESERVE process_identity_frames=");
    tos_serial::put_u32_decimal(paging::build_tables(bi, descs) as u32);
    tos_serial::puts(b" process_windows_frames=");
    tos_serial::put_u32_decimal(process::process_window_tables() as u32);
    tos_serial::puts(b" process_region_mapping_frames=");
    tos_serial::put_u32_decimal(process::region_mapping_bound(admitted_bytes) as u32);
    // Device windows are their own line, because they are their own kind
    // (ADR-0081 §5): the tables that map them come from the reserve like every
    // other mapping's, and nothing about the *memory* they reach is in this
    // account at all. A reserve whose parts do not sum to it is a reserve
    // nobody can check.
    tos_serial::puts(b" process_device_mapping_frames=");
    tos_serial::put_u32_decimal(process::device_mapping_bound() as u32);
    tos_serial::puts(b" region_backing_frames=");
    tos_serial::put_u32_decimal(process::region_backing_bound(admitted_bytes) as u32);
    tos_serial::puts(b" processes=");
    tos_serial::put_u32_decimal(process::MAX_PROCESSES as u32);
    tos_serial::puts(b" total_frames=");
    tos_serial::put_u32_decimal(reserved as u32);
    // The backing index's root is permanent: taken once and never given back,
    // so the reserve's runtime baseline is one below its size. A diagnostic
    // that expected the raw size would report a leak that is not one.
    tos_serial::puts(b" backing_root_frames=1 runtime_baseline_frames=");
    tos_serial::put_u32_decimal(reserved as u32 - 1);
    tos_serial::puts(b" asserted_by=nucleus\r\n");

    tos_serial::puts(b"TOS.MEM.ACCOUNT admitted_frames=");
    tos_serial::put_u32_decimal(admission.frames as u32);
    tos_serial::puts(b" nucleus_space_actual_frames=");
    // SAFETY: boot, single-context, nothing else holds the pool.
    let left = unsafe { memory::frames() }.available();
    tos_serial::put_u32_decimal((before_own_space - left - reserved) as u32);
    tos_serial::puts(b" table_reserve_frames=");
    tos_serial::put_u32_decimal(reserved as u32);
    tos_serial::puts(b" table_reserve_free=");
    // SAFETY: boot, single-context, nothing else holds the reserve or the pool.
    tos_serial::put_u32_decimal(unsafe { memory::tables() }.remaining() as u32);
    tos_serial::puts(b" pool_frames=");
    // SAFETY: boot, single-context, nothing else holds the pool.
    tos_serial::put_u32_decimal(unsafe { memory::frames() }.available() as u32);
    tos_serial::puts(b" root_frames=");
    tos_serial::put_u32_decimal((root_bytes / tos_frames::FRAME_SIZE as usize) as u32);
    tos_serial::puts(b" asserted_by=nucleus\r\n");

    // Interrupts are enabled here and nowhere else: after the substrate exists
    // and before the first process is entered, which is exactly where ADR-0049
    // puts it. Stage 1 and Stage 2 were measured with them off, and no number
    // taken then is relabelled by this. The one measurement-only exception is
    // the conservative ADR-0066 floor/denominator build: the IPC numerator
    // keeps preemption active, while its smaller comparator excludes timer
    // excursions and therefore cannot make the relative budget easier.
    // SAFETY: the IDT has gates for both claimed vectors, the local APIC page is
    // mapped uncacheable in the space just activated, and nothing else runs.
    #[cfg(not(feature = "test-measurement-no-preemption"))]
    unsafe {
        apic::start()
    };

    // The excursion joins the table here and is entered by the scheduler with
    // everything else, so the fault it is built to take happens while a peer
    // exists. That peer completing its own work afterwards is what ADR-0049
    // section 3 asks to see: the fault ended one process, not the system.
    #[cfg(any(
        feature = "test-ring3-abi",
        feature = "test-ring3-privileged",
        feature = "test-ring3-nucleus"
    ))]
    let excursion = {
        #[cfg(feature = "test-ring3-abi")]
        let payload = ring3::Payload::Abi;
        #[cfg(feature = "test-ring3-privileged")]
        let payload = ring3::Payload::Privileged;
        #[cfg(feature = "test-ring3-nucleus")]
        let payload = ring3::Payload::Nucleus;
        // SAFETY: `space` is the address space loaded into CR3 immediately
        // above and no other context is running.
        match unsafe { ring3::admit(&mut space, payload) } {
            Ok(index) => index,
            Err(_) => {
                tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-user-mapping\r\n");
                mem_fail();
            }
        }
    };

    #[cfg(feature = "test-paging-unmapped")]
    paging::test_injection();
    #[cfg(feature = "test-paging-readonly-text")]
    paging::test_readonly_text();

    // --- 6. Stage 3: the boot module becomes a process ---
    // The nucleus no longer runs the reference path; it launches something that
    // does. ADR-0048 makes that a replacement rather than an addition: keeping
    // the in-process call beside the launch would leave a path where TOS Core
    // executes at CPL 0, and the isolation boundary would be a thing the system
    // chose rather than a thing it has.
    if bi.runtime_phys == 0 {
        tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-runtime-image\r\n");
        console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"no-runtime-image");
        mem_fail();
    }
    // SAFETY: the handoff record was validated at entry, so this range is the
    // one the loader reserved and identity-mapped for the runtime image.
    let image_bytes = unsafe {
        core::slice::from_raw_parts(bi.runtime_phys as *const u8, bi.runtime_length as usize)
    };
    // The digest is recomputed here, not taken on the record's word: the record
    // says what the loader saw, and this says what the nucleus is about to map
    // into a process.
    if sha256(image_bytes) != bi.runtime_digest {
        tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=runtime-image-digest-mismatch\r\n");
        console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"runtime-digest");
        mem_fail();
    }
    let mut runtime_hex = [0u8; 64];
    tos_hash::hex(&bi.runtime_digest, &mut runtime_hex);

    let entry_index = modules[..module_count]
        .iter()
        .position(|(name, _)| *name == boot.name)
        .unwrap_or(0);
    let image =
        tos_runtime::region::Span::new(bi.runtime_phys, bi.runtime_phys + bi.runtime_length);
    let capsule_span =
        tos_runtime::region::Span::new(bi.capsule_phys, bi.capsule_phys + bi.capsule_length);
    let mut source_set = [0u8; 96];
    let named = source_set_identity(kind, &bi.capsule_source_identity, &mut source_set);
    // What this boot can launch, fixed once from inputs that have all been
    // validated: the image by its digest, the capsule by digest and structure,
    // the map by the Boot ABI check at entry. Everything built from here on —
    // by this launcher now, and by a process holding process authority later —
    // is built from this and from nothing a caller supplies.
    // SAFETY: every input named above passed its validation, both ranges are
    // physically contiguous and identity-mapped for this nucleus, and every
    // unit's bytes lie inside the capsule.
    unsafe {
        launch::establish(
            bi,
            descs,
            image,
            capsule_span,
            &modules[..module_count],
            memory::identity(),
            &source_set[..named],
        )
    };

    // Before the boot builds anything real, every way a creation can fail is
    // driven once and the machine is measured on both sides of it. A rollback
    // that has never been made to run is a claim; this makes each of the seven
    // failures happen and reports the pool, the reserve and the tree before and
    // after, so a gate can insist they are the same numbers.
    #[cfg(feature = "test-creation-rollback")]
    {
        use injection::Case;
        for (name, case) in [
            (&b"bad-header"[..], Case::BadHeader),
            (&b"record-too-large"[..], Case::RecordTooLarge),
            (&b"over-budget"[..], Case::OverBudget),
            (&b"data-frame"[..], Case::DataFrame),
            (&b"grant-frame"[..], Case::GrantFrame),
            (&b"grant-table"[..], Case::GrantTable),
            (&b"record-carve"[..], Case::RecordCarve),
            (&b"record-mapping"[..], Case::RecordMapping),
        ] {
            // SAFETY: boot, single-context, before any process exists.
            let (pool_before, tables_before) =
                unsafe { (memory::frames().available(), memory::tables().remaining()) };
            // SAFETY: as above.
            let tree = unsafe { memory::authority() };
            let root = memory::root().expect("the root was endowed above");
            let free_before = tree.remaining(root).unwrap_or(0);
            let committed_before = tree.committed();
            let held_before = capability::held(0);

            injection::arm(case);
            // SAFETY: the template was established above and the nucleus's own
            // address space is the live one.
            let refused =
                unsafe { process::create_bootstrap(entry_index, root, &[], 0, None) }.is_err();
            injection::disarm();
            let _ = &case;

            // SAFETY: as above.
            let (pool_after, tables_after) =
                unsafe { (memory::frames().available(), memory::tables().remaining()) };
            // SAFETY: as above.
            let tree = unsafe { memory::authority() };
            tos_serial::puts(b"TOS.RUN.CREATE_ROLLBACK case=");
            tos_serial::puts(name);
            tos_serial::puts(b" refused=");
            tos_serial::put_u32_decimal(u32::from(refused));
            tos_serial::puts(b" pool=");
            tos_serial::put_u32_decimal(pool_before as u32);
            tos_serial::puts(b"/");
            tos_serial::put_u32_decimal(pool_after as u32);
            tos_serial::puts(b" tables=");
            tos_serial::put_u32_decimal(tables_before as u32);
            tos_serial::puts(b"/");
            tos_serial::put_u32_decimal(tables_after as u32);
            tos_serial::puts(b" free=");
            tos_serial::put_u32_decimal(free_before as u32);
            tos_serial::puts(b"/");
            tos_serial::put_u32_decimal(tree.remaining(root).unwrap_or(0) as u32);
            tos_serial::puts(b" committed=");
            tos_serial::put_u32_decimal(committed_before as u32);
            tos_serial::puts(b"/");
            tos_serial::put_u32_decimal(tree.committed() as u32);
            tos_serial::puts(b" capabilities=");
            tos_serial::put_u32_decimal(held_before as u32);
            tos_serial::puts(b"/");
            tos_serial::put_u32_decimal(capability::held(0) as u32);
            tos_serial::puts(b" holds=");
            tos_serial::put_u32_decimal(u32::from(tree.accounting_holds()));
            tos_serial::puts(b" diverged=");
            tos_serial::put_u32_decimal(u32::from(memory::accounting_diverged()));
            tos_serial::puts(b" asserted_by=nucleus\r\n");
        }
    }

    // The ninth failure, and the only one that is not an injection: an
    // endowment the launcher decided on that cannot be written whole. Two valid
    // entries and a third claiming the receive right on an endpoint the second
    // already claims — `IPC_V1` §2 allows one receiver, and `grant` cannot see
    // the clash because the first of the pair is not written when the second is
    // checked. ADR-0055 makes the child invalid rather than short, so the
    // creation is refused before the process exists at all.
    #[cfg(feature = "test-creation-rollback")]
    {
        let binding = capability::Binding::new(b"a").expect("a short name");
        let endowment = [
            capability::Endowment::Existing {
                binding,
                object: capability::Object::Endpoint(0),
                rights: tos_launch::RIGHT_SEND,
                scope: 0,
            },
            capability::Endowment::Existing {
                binding,
                object: capability::Object::Endpoint(0),
                rights: tos_launch::RIGHT_RECEIVE,
                scope: 0,
            },
            capability::Endowment::Existing {
                binding,
                object: capability::Object::Endpoint(0),
                rights: tos_launch::RIGHT_RECEIVE,
                scope: 0,
            },
        ];
        // SAFETY: boot, single-context, before any process exists.
        let (pool_before, tables_before) =
            unsafe { (memory::frames().available(), memory::tables().remaining()) };
        // SAFETY: as above.
        let tree = unsafe { memory::authority() };
        let root = memory::root().expect("the root was endowed above");
        let free_before = tree.remaining(root).unwrap_or(0);
        let committed_before = tree.committed();
        let held_before = capability::held(0);

        // SAFETY: as above.
        let refused =
            unsafe { process::create_bootstrap(entry_index, root, &endowment, 0, None) }.is_err();

        // SAFETY: as above.
        let (pool_after, tables_after) =
            unsafe { (memory::frames().available(), memory::tables().remaining()) };
        // SAFETY: as above.
        let tree = unsafe { memory::authority() };
        tos_serial::puts(b"TOS.RUN.CREATE_ROLLBACK case=endowment refused=");
        tos_serial::put_u32_decimal(u32::from(refused));
        tos_serial::puts(b" pool=");
        tos_serial::put_u32_decimal(pool_before as u32);
        tos_serial::puts(b"/");
        tos_serial::put_u32_decimal(pool_after as u32);
        tos_serial::puts(b" tables=");
        tos_serial::put_u32_decimal(tables_before as u32);
        tos_serial::puts(b"/");
        tos_serial::put_u32_decimal(tables_after as u32);
        tos_serial::puts(b" free=");
        tos_serial::put_u32_decimal(free_before as u32);
        tos_serial::puts(b"/");
        tos_serial::put_u32_decimal(tree.remaining(root).unwrap_or(0) as u32);
        tos_serial::puts(b" committed=");
        tos_serial::put_u32_decimal(committed_before as u32);
        tos_serial::puts(b"/");
        tos_serial::put_u32_decimal(tree.committed() as u32);
        tos_serial::puts(b" capabilities=");
        tos_serial::put_u32_decimal(held_before as u32);
        tos_serial::puts(b"/");
        tos_serial::put_u32_decimal(capability::held(0) as u32);
        tos_serial::puts(b" holds=");
        tos_serial::put_u32_decimal(u32::from(tree.accounting_holds()));
        tos_serial::puts(b" diverged=");
        tos_serial::put_u32_decimal(u32::from(memory::accounting_diverged()));
        tos_serial::puts(b" asserted_by=nucleus\r\n");
    }

    // Building a process and entering one are separate steps, and this is where
    // that separation is visible: every process this boot has is built here,
    // out of the same pool and into its own address space, before the scheduler
    // gives the processor to any of them.
    // **The one place the boot's anchor is spent, and it is named.** Every
    // process this boot builds is built here, through `create_bootstrap`, which
    // takes the root as an argument; the creation core below it has no way to
    // reach an authority nobody handed it. After this closure there is no path
    // from ring 3 to a user frame that does not present a `MemoryAuthority`.
    let Some(boot_root) = memory::root() else {
        tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-root-authority\r\n");
        mem_fail();
    };
    let build = |endowment: &[capability::Endowment]| {
        // SAFETY: the template was established immediately above from validated
        // inputs, and the nucleus's own address space is the live one.
        // The boot process is created by the nucleus, not by a supervisor: it
        // has no parent instance, and nobody asserted a restart lineage for it.
        let built =
            unsafe { process::create_bootstrap(entry_index, boot_root, endowment, 0, None) };
        // Announced once the process exists and by the slot it occupies, so
        // that every later event about it names the same process this one does.
        // A launch announced before it succeeded would be a claim about a
        // process the system might not have.
        if let Ok(index) = built {
            tos_serial::puts(b"TOS.RUN.PROCESS_BEGIN process=");
            tos_serial::put_u32_decimal(index as u32);
            tos_serial::puts(b" module=");
            tos_serial::puts(boot.name);
            tos_serial::puts(b" runtime_engine=sha256:");
            tos_serial::puts(&runtime_hex);
            tos_serial::puts(b" system_commit=absent asserted_by=launcher\r\n");
        }
        built
    };
    // **The launcher's stated constant** (ADR-0055). Until `/system/policy/`
    // exists (ADR-0051 section 3) the endowment of a process this nucleus
    // launches is a decision written here, in one place, and put on the log as a
    // decision.
    //
    // For the canonical boot it is **empty**, and that is the policy rather than
    // the absence of one: `system.boot.init` requests no capability, and the
    // rule is to grant nothing a module did not ask for. A process endowed with
    // authority it never requested would be authority nobody decided to give.
    // Under the paired constant the two processes share one endpoint: the first
    // is given the right to *receive* on it and the second the right to *send*.
    // The rights are separate (`IPC_V1` section 2), so neither can perform the
    // other's half, and neither was given anything it could have obtained on
    // its own — no operation creates an endpoint.
    #[cfg(feature = "test-two-processes")]
    let (first_endowment, sender_endowment) = {
        let Some(endpoint) = ipc::create() else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-endpoint\r\n");
            mem_fail();
        };
        (
            [capability::Endowment::Existing {
                binding: binding(b"endpoint"),
                object: capability::Object::Endpoint(endpoint),
                rights: tos_launch::RIGHT_RECEIVE,
                scope: 0,
            }],
            [capability::Endowment::Existing {
                binding: binding(b"endpoint"),
                object: capability::Object::Endpoint(endpoint),
                rights: tos_launch::RIGHT_SEND,
                scope: 0,
            }],
        )
    };
    // Under the supervisor constant the first process is given authority over
    // **itself**, carrying the two rights a process object has. That is the
    // only capability nobody but a launcher can issue: it names a process that
    // does not exist until the instant it is granted, which is why a process
    // cannot obtain one and cannot spawn without having been given one.
    // **A creator needs two capabilities now** (ADR-0076 §4): authority over a
    // process to say it may create, and a `MemoryAuthority` to pay for what it
    // creates. Operation 19 takes both and there is no root to fall back to, so
    // a constant that granted only the first would be launching a supervisor
    // that can create nothing.
    #[cfg(feature = "test-supervisor")]
    let first_endowment = [
        capability::Endowment::Own {
            binding: binding(b"self"),
            rights: tos_launch::RIGHT_CREATE | tos_launch::RIGHT_TERMINATE,
        },
        capability::Endowment::Remainder {
            binding: binding(b"memory"),
            rights: tos_launch::RIGHT_SPEND,
        },
    ];
    // Under the deadlock constant one process is given the right to receive on
    // an endpoint **nobody can send to**: no other process exists, and no
    // operation creates an endpoint. So the wait it enters is one nothing in
    // the system can satisfy, which is the state ADR-0059's liveness rule is
    // about — and the only way to test that rule is to build a system that has
    // genuinely stopped, rather than one that looks stopped.
    #[cfg(feature = "test-deadlock")]
    let first_endowment = {
        let Some(endpoint) = ipc::create() else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-endpoint\r\n");
            mem_fail();
        };
        [capability::Endowment::Existing {
            binding: binding(b"endpoint"),
            object: capability::Object::Endpoint(endpoint),
            rights: tos_launch::RIGHT_RECEIVE,
            scope: 0,
        }]
    };
    // Under the Stage 4A constant the boot process is given the **root PCI bus
    // authority** and nothing else (ADR-0079 §5, §9).
    //
    // **This is a mint, not a derivation**, and it is the only place in the
    // system that performs one. Every other endowment names something the
    // launcher already had — an endpoint it made, the process it is about to
    // create, a remainder of the memory it was given. A bus is none of those: it
    // exists because the machine has one, so its authority cannot come from
    // attenuating anything, and `pci::endow_root` is reachable from here and
    // from no dispatcher. A process cannot ask for one.
    //
    // The scope is the whole segment the accepted mechanism reaches, and it is
    // **named in the record rather than implied**, because a root whose extent
    // nobody stated is a root nobody decided.
    //
    // What this constant deliberately does *not* grant is a function. Which
    // device is worth claiming is policy, and a launcher that picked one would
    // be the nucleus choosing a device — the arrangement ADR-0079 §5 names to be
    // rejected. The holder claims what it decides to claim, out of the scope it
    // was given.
    #[cfg(feature = "test-pci-discovery")]
    let first_endowment = {
        let Some(bus) = pci::endow_root(0, 0, 255) else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-pci-root\r\n");
            mem_fail();
        };
        tos_serial::puts(b"TOS.RUN.PCI_ROOT segment=0 first_bus=0 last_bus=255 rights=claim");
        tos_serial::puts(b" asserted_by=launcher\r\n");
        [capability::Endowment::Existing {
            binding: binding(b"bus"),
            object: capability::Object::PciBus(bus),
            rights: tos_launch::RIGHT_CLAIM,
            scope: 0,
        }]
    };
    // Under the request/reply constant the first process may **receive** on an
    // endpoint and the second may **call** on it. Neither can do the other's
    // half, and the right to answer a call is not in either endowment: it is
    // made by the nucleus when the call is made, handed to whoever receives the
    // request, and spent by answering it.
    #[cfg(feature = "test-call-reply")]
    let (first_endowment, caller_endowment) = {
        let Some(endpoint) = ipc::create() else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-endpoint\r\n");
            mem_fail();
        };
        (
            [capability::Endowment::Existing {
                binding: binding(b"endpoint"),
                object: capability::Object::Endpoint(endpoint),
                rights: tos_launch::RIGHT_RECEIVE,
                scope: 0,
            }],
            [capability::Endowment::Existing {
                binding: binding(b"endpoint"),
                object: capability::Object::Endpoint(endpoint),
                rights: tos_launch::RIGHT_CALL,
                scope: 0,
            }],
        )
    };
    // Under the deputy constant one process is **strong** — it may receive on an
    // endpoint and send on it — and the other is **weak**: it may only call. The
    // question `CAPABILITY_V1` §7.6 asks is whether the strong one's strength
    // leaks into work it does for the weak one, and the only way to ask it is to
    // build both and let them talk.
    #[cfg(feature = "test-deputy")]
    let (first_endowment, client_endowment) = {
        let Some(endpoint) = ipc::create() else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-endpoint\r\n");
            mem_fail();
        };
        (
            [capability::Endowment::Existing {
                binding: binding(b"endpoint"),
                object: capability::Object::Endpoint(endpoint),
                rights: tos_launch::RIGHT_RECEIVE | tos_launch::RIGHT_SEND,
                scope: 0,
            }],
            [capability::Endowment::Existing {
                binding: binding(b"endpoint"),
                object: capability::Object::Endpoint(endpoint),
                rights: tos_launch::RIGHT_CALL,
                scope: 0,
            }],
        )
    };
    // Under the second-receiver constant the launcher asks for something
    // `IPC_V1` §2 forbids: **both** processes are to receive on one endpoint.
    // The constant is wrong on purpose, and it is the only way to ask the
    // question §9.4 asks — a rule that is only ever obeyed is a rule nobody has
    // tested. The first process is also given `send`, so that the endpoint is
    // usable by the one holder it is allowed to have and the boot is a working
    // system with one refusal in it rather than a broken one.
    #[cfg(feature = "test-second-receiver")]
    let (first_endowment, second_endowment) = {
        let Some(endpoint) = ipc::create() else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-endpoint\r\n");
            mem_fail();
        };
        (
            [capability::Endowment::Existing {
                binding: binding(b"endpoint"),
                object: capability::Object::Endpoint(endpoint),
                rights: tos_launch::RIGHT_RECEIVE | tos_launch::RIGHT_SEND,
                scope: 0,
            }],
            [capability::Endowment::Existing {
                binding: binding(b"endpoint"),
                object: capability::Object::Endpoint(endpoint),
                rights: tos_launch::RIGHT_RECEIVE,
                scope: 0,
            }],
        )
    };
    // Under the module-operation constant one process holds `send` — and only
    // `send` — on an endpoint, under the name a TOS Core module asks for. The
    // module then performs one operation and is refused the other, and the
    // difference is the whole evidence: it could not have produced two
    // different statuses without the nucleus having judged both.
    #[cfg(feature = "test-module-operation")]
    let first_endowment = {
        let Some(endpoint) = ipc::create() else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-endpoint\r\n");
            mem_fail();
        };
        [capability::Endowment::Existing {
            binding: binding(b"endpoint"),
            object: capability::Object::Endpoint(endpoint),
            rights: tos_launch::RIGHT_SEND,
            scope: 0,
        }]
    };
    // Under the wrong-kind constant the process is given authority over
    // **itself** — a process object — under the name a module asks for an
    // *endpoint*. The name matches and the kind does not, which is the one
    // thing `SYSTEM_INTERFACE_V1` §4's object column is for. The nucleus does
    // not refuse it: it does not read modules and has nothing to compare the
    // name against. The process does, at startup, before its first instruction.
    #[cfg(feature = "test-wrong-kind")]
    let first_endowment = [capability::Endowment::Own {
        binding: binding(b"endpoint"),
        rights: tos_launch::RIGHT_TERMINATE,
    }];
    // The two process-control constants differ by one bit and nothing else, so
    // that the two boots differ by one bit and nothing else. `create` is a real
    // right of a process object and not a placeholder: the grant is genuine
    // authority over this process, and what it lacks is the one operation the
    // module calls.
    #[cfg(feature = "test-process-control")]
    let first_endowment = [
        capability::Endowment::Own {
            binding: binding(b"control"),
            rights: tos_launch::RIGHT_CREATE,
        },
        capability::Endowment::Remainder {
            binding: binding(b"memory"),
            rights: tos_launch::RIGHT_SPEND,
        },
    ];
    #[cfg(feature = "test-process-terminate")]
    let first_endowment = [
        capability::Endowment::Own {
            binding: binding(b"control"),
            rights: tos_launch::RIGHT_TERMINATE,
        },
        capability::Endowment::Remainder {
            binding: binding(b"memory"),
            rights: tos_launch::RIGHT_SPEND,
        },
    ];
    // ADR-0074's T1: a resident supervisor that may create, terminate and
    // collect, holds the root's remainder to fund a worker out of, and holds
    // one endpoint it **receives** the built artifact on. The worker it creates
    // is endowed by the supervisor and holds neither creation authority nor a
    // receiver — it can build and hand over, and nothing else.
    #[cfg(feature = "test-build-topology")]
    let first_endowment = {
        let Some(handback) = ipc::create() else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-endpoint\r\n");
            mem_fail();
        };
        [
            capability::Endowment::Own {
                binding: binding(b"process"),
                rights: tos_launch::RIGHT_CREATE
                    | tos_launch::RIGHT_TERMINATE
                    | tos_launch::RIGHT_WAIT_CHILD,
            },
            capability::Endowment::Remainder {
                binding: binding(b"memory"),
                rights: tos_launch::RIGHT_SPEND,
            },
            // One endpoint, two names. The supervisor receives on its
            // **inbox**; the worker it creates is given `send` on the same
            // object under the name **outbox**. ADR-0061 makes the binding the
            // identity of a request, so the two roles ask for different things
            // and are answered with different halves of one channel.
            capability::Endowment::Existing {
                binding: binding(b"inbox"),
                object: capability::Object::Endpoint(handback),
                rights: tos_launch::RIGHT_SEND | tos_launch::RIGHT_RECEIVE,
                scope: 0,
            },
        ]
    };
    // Section H: everything a supervisor is made of. `create` to start a
    // service, `terminate` because a supervisor that could not stop one would
    // be an observer, `wait_child` because an ending is how it learns anything,
    // the root's remainder to pay with, and one endpoint for its journal.
    //
    // The endpoint carries **both halves** deliberately: `IPC_V1` §3 bounds a
    // queue at four messages, and a journal whose sink filled would stop being
    // a journal — so the supervisor drains its own. Who consumes an operator
    // journal is not a question Stage 3 answers, and this does not invent one.
    #[cfg(feature = "test-supervision")]
    let first_endowment = {
        let Some(journal) = ipc::create() else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-endpoint\r\n");
            mem_fail();
        };
        [
            capability::Endowment::Own {
                binding: binding(b"process"),
                rights: tos_launch::RIGHT_CREATE
                    | tos_launch::RIGHT_TERMINATE
                    | tos_launch::RIGHT_WAIT_CHILD,
            },
            capability::Endowment::Remainder {
                binding: binding(b"memory"),
                rights: tos_launch::RIGHT_SPEND,
            },
            capability::Endowment::Existing {
                binding: binding(b"journal"),
                object: capability::Object::Endpoint(journal),
                rights: tos_launch::RIGHT_SEND | tos_launch::RIGHT_RECEIVE,
                scope: 0,
            },
        ]
    };
    // ADR-0078: everything a supervisor is made of, and nothing more. `create`
    // and `terminate` over itself, and the root's remainder to spend. Every
    // other capability that boot reaches is one an operation produced — the
    // scoped budget, the plan, the child — and none of them is here, because
    // none of them existed when this constant was written.
    #[cfg(feature = "test-runtime-authority")]
    let first_endowment = [
        capability::Endowment::Own {
            binding: binding(b"process"),
            rights: tos_launch::RIGHT_CREATE | tos_launch::RIGHT_TERMINATE,
        },
        capability::Endowment::Remainder {
            binding: binding(b"memory"),
            rights: tos_launch::RIGHT_SPEND,
        },
    ];
    // Under the launch constant the process holds `create` over itself, under
    // the name a module asks for process authority by. What it does with it is
    // the module's business, and here the module launches something.
    #[cfg(feature = "test-process-launch")]
    let first_endowment = [
        capability::Endowment::Own {
            binding: binding(b"control"),
            rights: tos_launch::RIGHT_CREATE,
        },
        capability::Endowment::Remainder {
            binding: binding(b"memory"),
            rights: tos_launch::RIGHT_SPEND,
        },
    ];
    // ADR-0067's supervisor: it creates children, ends them, and collects what
    // the nucleus recorded about how they ended. Three rights, because those are
    // the three operations `SYSTEM_ABI_V1` §5 names over a process — and a
    // supervisor that could not end what it started could not produce the
    // endings it then waits for.
    #[cfg(feature = "test-lifecycle")]
    let first_endowment = [
        capability::Endowment::Own {
            binding: binding(b"control"),
            rights: tos_launch::RIGHT_CREATE
                | tos_launch::RIGHT_TERMINATE
                | tos_launch::RIGHT_WAIT_CHILD,
        },
        capability::Endowment::Remainder {
            binding: binding(b"memory"),
            rights: tos_launch::RIGHT_SPEND,
        },
    ];
    // Under the region-transport constant there are two processes and three
    // capabilities between them. The **worker** is endowed a child of the root
    // memory authority — so it can make regions at all — the right to send on
    // an endpoint the peer receives on, and the right to send on a second
    // endpoint **nobody receives on**. That second one is not a convenience: a
    // queue that nothing can drain is the only way to ask what a full queue
    // does to a linear transfer, and asking it on the endpoint the peer is
    // draining would be asking a question the answer keeps changing.
    //
    // The **peer** is endowed the right to receive on the first endpoint and
    // nothing else. It holds no memory authority, so every region it comes to
    // hold arrived in a message; it cannot make one, and it cannot send.
    //
    // **The peer is built first and the worker second, and the order is not
    // cosmetic.** The worker's authority is `Remainder` — everything the root
    // has left once the creation it funds is paid for — so a worker built first
    // would leave nothing to fund the peer with. Building the peer first also
    // makes the interleaving the evidence depends on the *ordinary* one: the
    // peer runs, finds nothing to take, gives up its quantum, and the worker
    // then runs a script with no blocking call in it from end to end.
    #[cfg(feature = "test-region-transport")]
    let (first_endowment, worker_endowment) = {
        let (Some(endpoint), Some(sink)) = (ipc::create(), ipc::create()) else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-endpoint\r\n");
            mem_fail();
        };
        (
            [capability::Endowment::Existing {
                binding: binding(b"endpoint"),
                object: capability::Object::Endpoint(endpoint),
                rights: tos_launch::RIGHT_RECEIVE,
                scope: 0,
            }],
            [
                capability::Endowment::Remainder {
                    binding: binding(b"memory"),
                    rights: tos_launch::RIGHT_SPEND,
                },
                capability::Endowment::Existing {
                    binding: binding(b"endpoint"),
                    object: capability::Object::Endpoint(endpoint),
                    rights: tos_launch::RIGHT_SEND,
                    scope: 0,
                },
                capability::Endowment::Existing {
                    binding: binding(b"sink"),
                    object: capability::Object::Endpoint(sink),
                    rights: tos_launch::RIGHT_SEND,
                    scope: 0,
                },
            ],
        )
    };
    // Under the bundle-launch constant one process is a supervisor of the shape
    // ADR-0073 describes: it may create, terminate and collect endings, and it
    // holds a memory authority to fund what it creates and to make the region
    // its artifact goes in. It builds the bundle itself — that is not the
    // canonical build worker, which ADR-0074 has not settled, it is the smallest
    // thing that produces a real artifact for operation 20 to be asked about.
    #[cfg(feature = "test-bundle-launch")]
    let first_endowment = [
        capability::Endowment::Own {
            binding: binding(b"control"),
            rights: tos_launch::RIGHT_CREATE
                | tos_launch::RIGHT_TERMINATE
                | tos_launch::RIGHT_WAIT_CHILD,
        },
        capability::Endowment::Remainder {
            binding: binding(b"memory"),
            rights: tos_launch::RIGHT_SPEND,
        },
    ];
    // Under the region-faults constant there are two processes and one
    // authority between them, and each ends by touching memory it may not
    // touch — one by executing what it wrote into a region, the other by
    // reading a region after releasing the only handle to it. The **first**
    // process is the ordinary boot module and holds nothing, which is why this
    // constant does not name a `first_endowment` of its own.
    //
    // **A child of a fixed size, endowed to both, rather than a remainder.**
    // `Endowment::Remainder` hands whichever process is built first everything
    // the root has left, which would leave the second unable to make a region
    // at all. So the launcher reserves one child here and gives each process a
    // name for it: two names, one budget, which is what a memory authority is
    // (ADR-0076 §2b). The maker's own name goes once both are built, so the
    // count runs 1 → 3 → 2 and never through zero.
    #[cfg(feature = "test-region-faults")]
    let (nx_endowment, stale_endowment, allowance) = {
        let Some(root) = memory::root() else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-root-authority\r\n");
            mem_fail();
        };
        // SAFETY: single-context nucleus at boot; nothing else holds the tree.
        let tree = unsafe { memory::authority() };
        let Ok(allowance) = tree.attenuate(root, 8 * 1024 * 1024) else {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-allowance\r\n");
            mem_fail();
        };
        let object = capability::Object::MemoryAuthority {
            index: allowance.index,
            generation: allowance.generation,
        };
        (
            [capability::Endowment::Existing {
                binding: binding(b"nx"),
                object,
                rights: tos_launch::RIGHT_SPEND,
                scope: 0,
            }],
            [capability::Endowment::Existing {
                binding: binding(b"stale"),
                object,
                rights: tos_launch::RIGHT_SPEND,
                scope: 0,
            }],
            allowance,
        )
    };
    #[cfg(not(any(
        feature = "test-process-launch",
        feature = "test-two-processes",
        feature = "test-module-operation",
        feature = "test-wrong-kind",
        feature = "test-process-control",
        feature = "test-process-terminate",
        feature = "test-supervisor",
        feature = "test-deadlock",
        feature = "test-call-reply",
        feature = "test-second-receiver",
        feature = "test-deputy",
        feature = "test-runtime-authority",
        feature = "test-supervision",
        feature = "test-build-topology",
        feature = "test-pci-discovery",
        feature = "test-lifecycle"
    )))]
    // **Nothing, because the module asks for nothing.** ADR-0055 makes an
    // endowment what a launcher decided, and ADR-0061 makes each entry the
    // answer to an `import capability` the module declared. `system.boot.init`
    // declares none, so it is given none — granting it a memory authority it
    // never asked for would be the launcher answering a request that does not
    // exist, which is a different defect from ambient authority only in how it
    // is spelled.
    //
    // The bootstrap chain that ends in a held authority is real and is proved
    // by the evidence build below, where the module that receives it is one
    // that asked:
    //
    // ```text
    // Frames -> table reserve -> root MemoryAuthority
    //        -> the process's exact footprint, charged to the root
    //        -> everything the root has left, as a child, endowed explicitly
    // ```
    #[cfg(not(any(
        feature = "test-memory-authority",
        feature = "test-creation-rollback",
        feature = "test-region-transport",
        feature = "test-runtime-authority",
        feature = "test-supervision",
        feature = "test-build-topology",
        feature = "test-pci-discovery",
        feature = "test-bundle-launch"
    )))]
    let first_endowment: [capability::Endowment; 0] = [];
    // The same chain, given to a process, so operation 16 can be asked for from
    // ring 3 rather than described.
    //
    // **A child of the root and never the root itself.** The root is the boot's
    // accounting anchor: it has no parent to return to, no ring-3 handle names
    // it, and it must survive a process ending or being restarted. What this
    // process gets is an ordinary child — reserved out of the root, with an
    // ordinary reference lifecycle, returning its unspent part to the anchor
    // when its last name goes.
    //
    // All of what is left, because once there is user-space supervision the
    // policy for dynamic user memory belongs below that allowance rather than
    // to nucleus spending nobody authorised.
    #[cfg(any(feature = "test-memory-authority", feature = "test-creation-rollback"))]
    let first_endowment = [capability::Endowment::Remainder {
        binding: capability::Binding::new(b"memory").expect("a short name"),
        rights: tos_launch::RIGHT_SPEND,
    }];
    let first = match build(&first_endowment) {
        Ok(index) => index,
        Err(_) => {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-process\r\n");
            console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"no-process");
            mem_fail();
        }
    };
    // A second process over the same module, so that the scheduler has someone
    // to round-robin *to*. Test-only, and for a reason that is about honesty
    // rather than caution: nothing in an accepted contract says the canonical
    // boot has two processes, and a nucleus that decided so on its own would be
    // owning service policy, which ADR-0048 section 2 says it does not. What is
    // not test-only is everything the second process exercises — the table, the
    // round-robin, the switch — which the canonical boot runs with one process
    // in the table.
    #[cfg(feature = "test-two-processes")]
    {
        tos_serial::puts(b"TOS.TEST.SCHEDULER.SECOND\r\n");
        if build(&sender_endowment).is_err() {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-second-process\r\n");
            mem_fail();
        }
    }
    #[cfg(feature = "test-deputy")]
    {
        tos_serial::puts(b"TOS.TEST.SCHEDULER.SECOND\r\n");
        if build(&client_endowment).is_err() {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-second-process\r\n");
            mem_fail();
        }
    }
    #[cfg(feature = "test-call-reply")]
    {
        tos_serial::puts(b"TOS.TEST.SCHEDULER.SECOND\r\n");
        if build(&caller_endowment).is_err() {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-second-process\r\n");
            mem_fail();
        }
    }
    #[cfg(feature = "test-region-transport")]
    {
        tos_serial::puts(b"TOS.TEST.SCHEDULER.SECOND\r\n");
        if build(&worker_endowment).is_err() {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-second-process\r\n");
            mem_fail();
        }
    }
    // **The two faulting processes are the second and the third**, and the
    // first is the ordinary boot module. Only the first process's ending
    // decides the boot's result, and both of these end in a fault deliberately:
    // a boot that failed because its evidence worked would be a gate that could
    // only ever be red. What the first process proves beside them is the thing
    // that matters most here — a machine two of whose processes died touching
    // memory they no longer held still finished the work it was booted for.
    #[cfg(feature = "test-region-faults")]
    {
        tos_serial::puts(b"TOS.TEST.SCHEDULER.SECOND\r\n");
        if build(&nx_endowment).is_err() || build(&stale_endowment).is_err() {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-second-process\r\n");
            mem_fail();
        }
        // Both processes hold a name for it now, so the launcher's own goes.
        // Releasing it before the second build would have returned the whole
        // reservation to the root with a process still to endow out of it.
        // SAFETY: single-context nucleus at boot; nothing else holds the tree.
        if unsafe { memory::authority() }
            .release_name(allowance)
            .is_err()
        {
            memory::note_divergence(b"region-faults-allowance");
        }
    }
    // The second process starts, and starts with **nothing**: the one thing its
    // endowment asked for is the one thing it may not have. A launcher asking
    // for the impossible does not stop the boot — it is told, on the record,
    // that there is no such process.
    #[cfg(feature = "test-second-receiver")]
    {
        tos_serial::puts(b"TOS.TEST.SCHEDULER.SECOND\r\n");
        // **The refusal is the point of this boot, and it is now a refusal to
        // create.** An endowment is written whole or not at all (ADR-0055), so
        // a launcher asking for a receive right the rule cannot give does not
        // get a process holding less than it decided — it gets no process. The
        // boot goes on with the one process that was startable, because a
        // launcher asking for the impossible is told so rather than ending the
        // system.
        if build(&second_endowment).is_err() {
            tos_serial::puts(b"TOS.TEST.SECOND_NOT_CREATED reason=endowment\r\n");
        }
    }
    // SAFETY: `space` is the nucleus's own address space, it is the live one,
    // and every runnable slot was built by `process::create` above.
    unsafe { process::schedule(&space) };
    #[cfg(any(
        feature = "test-ring3-abi",
        feature = "test-ring3-privileged",
        feature = "test-ring3-nucleus"
    ))]
    {
        tos_serial::puts(b"TOS.TEST.RING3.ENDED ");
        // SAFETY: the scheduler returned, so every process it ran is over.
        match unsafe { process::ended(excursion) } {
            process::Ended::Fault(vector) => {
                tos_serial::puts(b"vector=");
                tos_serial::put_u32_decimal(vector as u32);
            }
            process::Ended::Exited(status) => {
                tos_serial::puts(b"exit=");
                tos_serial::put_u32_decimal(status as u32);
            }
            process::Ended::Terminated(by) => {
                tos_serial::puts(b"terminated_by=");
                tos_serial::put_u32_decimal(by as u32);
            }
            process::Ended::Deadlocked => tos_serial::puts(b"deadlocked"),
        }
        tos_serial::puts(b"\r\n");
        // SAFETY: the excursion is over, `space` is the live address space and
        // the one it ran in, and nothing else references either of its pages.
        unsafe { ring3::retire(&mut space) };
    }
    // SAFETY: the scheduler returned, so every process it ran is over.
    match unsafe { process::ended(first) } {
        process::Ended::Exited(0) => {}
        process::Ended::Exited(_)
        | process::Ended::Fault(_)
        | process::Ended::Terminated(_)
        | process::Ended::Deadlocked => {
            // The process ended without completing its work. Which way it ended
            // is already on the log, asserted by the nucleus.
            tos_serial::puts(b"TOS.BOOTMODULE.FAIL stage=process\r\n");
            result_port(RESULT_BOOT_MODULE_FAILED);
        }
    }

    // --- 7. the boot log has done its work ---
    // Every stage of the reference path completed and the canonical boot module
    // ran to completion, so the log is replaced by the final screen. Nothing
    // continues after this: the two lines it shows say exactly that, and no
    // rendering here can affect the result already decided above.
    if let Some(console) = &mut console {
        console.final_screen();
    }

    // --- 8. what the boot's IPC cost, counted ---------------------------------
    //
    // `IPC_V1` §9.7 asks that the boundary-crossing and copy counts be
    // *counted*, not estimated, and this is the count. It is the nucleus's own,
    // because nothing else can see either number: a process cannot observe how
    // many times it crossed, and a copy inside the nucleus is invisible from
    // outside it.
    //
    // `crossings` is both directions of the one edge — calls in, contexts
    // entered out — and excludes preemption, which returns through the timer
    // stub rather than through either. That is exactly the exclusion §8 states.
    ipc_cost();

    // --- 9. halt with success code ---
    tos_serial::puts(b"TOS.HALT ok=0x10\r\n");
    result_port(RESULT_HALT_OK)
}
