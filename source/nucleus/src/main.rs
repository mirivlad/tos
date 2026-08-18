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

mod console;
mod exception;
mod framebuffer;
mod memory;
mod msr;
mod paging;
mod process;
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

/// How many source units one boot may offer the frontend.
///
/// A fixed bound, chosen by the nucleus and not derived from capsule input: the
/// nucleus must not size an array from a number an attacker chose. It bounds
/// the **set offered**, which is every `.tos` file the capsule carries — not
/// the closure the entry actually reaches, which resolution computes and which
/// docs/44 caps at 256. Those are different numbers, and an earlier version of
/// this comment confused them: a capsule may legitimately carry more source
/// than any one module imports, and the Stage 1 performance fixture carries a
/// thousand files for exactly that reason. A capsule offering more than this is
/// refused rather than truncated, because a silently shortened set would run a
/// program whose dependencies are missing.
const MAX_BOOT_MODULES: usize = 1024;

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
    let mut modules: [(&[u8], &[u8]); MAX_BOOT_MODULES] = [(&[], &[]); MAX_BOOT_MODULES];
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
    let (mut frames, admission) = unsafe { memory::pool(bi, bi_address, descs, running_on) };
    if admission.frames == 0 {
        tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-frames\r\n");
        console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"no-frames");
        mem_fail();
    }

    // The nucleus takes over its own address space here, and not earlier: the
    // tables are frames from the pool, so the pool has to exist first. Until
    // this instruction the machine ran on the firmware's identity map — a map
    // the nucleus never wrote and cannot describe, which after ADR-0048 is the
    // very thing that has to keep one process out of another's memory.
    // Mutable only in a test configuration: the ring-3 excursion maps its own
    // pages into this space, and the production path only ever reads it.
    #[allow(unused_mut)]
    let mut space = match paging::build(bi, descs, &mut frames) {
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

    #[cfg(any(
        feature = "test-ring3-abi",
        feature = "test-ring3-privileged",
        feature = "test-ring3-nucleus"
    ))]
    {
        #[cfg(feature = "test-ring3-abi")]
        let payload = ring3::Payload::Abi;
        #[cfg(feature = "test-ring3-privileged")]
        let payload = ring3::Payload::Privileged;
        #[cfg(feature = "test-ring3-nucleus")]
        let payload = ring3::Payload::Nucleus;
        // SAFETY: `space` is the address space loaded into CR3 immediately
        // above and no other context is running.
        match unsafe { ring3::run(&mut space, &mut frames, payload) } {
            Ok(ended) => {
                tos_serial::puts(b"TOS.TEST.RING3.ENDED ");
                match ended {
                    process::Ended::Fault(vector) => {
                        tos_serial::puts(b"vector=");
                        tos_serial::put_u32_decimal(vector as u32);
                    }
                    process::Ended::Exited(status) => {
                        tos_serial::puts(b"exit=");
                        tos_serial::put_u32_decimal(status as u32);
                    }
                }
                tos_serial::puts(b"\r\n");
            }
            Err(_) => {
                tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-user-mapping\r\n");
                mem_fail();
            }
        }
    }

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
    tos_serial::puts(b"TOS.RUN.PROCESS_BEGIN module=");
    tos_serial::puts(boot.name);
    tos_serial::puts(b" runtime_engine=sha256:");
    tos_serial::puts(&runtime_hex);
    tos_serial::puts(b" system_commit=absent asserted_by=launcher\r\n");

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
    // SAFETY: the image and capsule ranges were validated above — the image by
    // its digest, the capsule by digest and structure — both are physically
    // contiguous and identity-mapped for this nucleus, and nothing else is
    // running.
    let ended = unsafe {
        process::launch(
            &space,
            &mut frames,
            descs,
            bi,
            image,
            capsule_span,
            &modules[..module_count],
            entry_index,
            memory::identity(),
            &source_set[..named],
        )
    };
    match ended {
        Ok(process::Ended::Exited(0)) => {}
        Ok(process::Ended::Exited(_)) | Ok(process::Ended::Fault(_)) => {
            // The process ended without completing its work. Which way it ended
            // is already on the log, asserted by the nucleus.
            tos_serial::puts(b"TOS.BOOTMODULE.FAIL stage=process\r\n");
            result_port(RESULT_BOOT_MODULE_FAILED);
        }
        Err(_) => {
            tos_serial::puts(b"TOS.RUN.UNSTARTABLE reason=no-process\r\n");
            console_failed(&mut console, b"RUNTIME_UNSTARTABLE", b"no-process");
            mem_fail();
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

    // --- 8. halt with success code ---
    tos_serial::puts(b"TOS.HALT ok=0x10\r\n");
    result_port(RESULT_HALT_OK)
}
