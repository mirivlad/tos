// SPDX-License-Identifier: GPL-3.0-or-later
//! What happens to the system when a process ends.
//!
//! ADR-0049 section 3 draws the line this module implements: **a fault in the
//! nucleus is still the end of the boot; a fault in a process is not.** Before
//! it, every exception was fatal, because there was exactly one thing running
//! and it was the nucleus. Now there can be something else running, and the
//! difference between the two is a single bit of the interrupted frame — the
//! privilege level the fault was taken at.
//!
//! **The nucleus records where to continue before it stops being what runs.**
//! A process is left by `iretq` and re-entered by a fault or by the system-call
//! edge; nothing returns from it in the ordinary sense. So the launch captures
//! a context first, and the death of a process is a jump back into it. That one
//! mechanism is what a fault-kill needs today and what a process reporting its
//! own completion would need (ADR-0054, undecided): the same door, opened from
//! two sides.

use core::ptr::addr_of_mut;

use tos_frames::{Frames, FRAME_SIZE};
use tos_launch::{ImageHeader, Launch, LaunchUnit, ReportHeader, IMAGE_MAGIC, LAUNCH_VERSION};
use tos_runtime::region::Span;

use crate::paging::{self, AddressSpace, PagingRefused};

core::arch::global_asm!(include_str!("process.S"));

/// The callee-saved registers, the stack pointer and the return address — what
/// it takes to continue a call that was interrupted by leaving ring 0.
#[repr(C)]
pub struct Context {
    saved: [u64; 8],
}

impl Context {
    const EMPTY: Context = Context { saved: [0; 8] };
}

extern "C" {
    fn process_enter(entry: u64, stack: u64, code: u64, data: u64, record: u64) -> !;
    fn process_capture(context: *mut Context) -> u64;
    fn process_resume(context: *mut Context, value: u64) -> !;
}

/// Where the nucleus continues when the process running now ends.
static mut RETURN: Context = Context::EMPTY;
/// Whether anything is running at CPL 3 at this instant.
///
/// The nucleus is single-context and runs with interrupts masked, so this is
/// read and written from one place in one order. It is the fault handler's only
/// question: a fault at CPL 3 with nothing running would mean the processor
/// reported a privilege level no one in this system had, and that is not a
/// process to kill — it is a nucleus that has lost track of itself.
static mut RUNNING: bool = false;
/// How the process running now ended, written once by whichever path ended it.
static mut ENDED: Ended = Ended::Fault(0);

/// Why a process ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ended {
    /// It took a fault at CPL 3. The vector is the architecture's.
    Fault(u64),
    /// It said so itself (`process_exit`, ADR-0054). The status is the
    /// process's own claim about its work, never the nucleus's assertion about
    /// it — what the nucleus asserts is that the process exited, and when.
    Exited(u64),
}

/// Where a process's parts live in its own address space.
///
/// Fixed addresses rather than a layout the launcher invents per process: the
/// runtime image is linked at [`IMAGE`] and enters at its first byte, so that
/// address is part of the launch boundary and moving it is a versioned change.
pub const IMAGE: u64 = 0x1000_0000;
pub const RECORD: u64 = 0x2000_0000;
pub const SOURCE: u64 = 0x2100_0000;
pub const GRANT: u64 = 0x3000_0000;
pub const STACK: u64 = 0x5000_0000;
pub const REPORT: u64 = 0x6000_0000;

/// The most a launch record may occupy. A fixed nucleus bound, not a number
/// from the capsule: what it limits is how much of the nucleus's memory one
/// process's description may cost.
const MAX_RECORD_BYTES: u64 = 256 * 1024;

/// Frames of stack a process is given, and frames its report region holds.
const STACK_FRAMES: u64 = 512;
const REPORT_FRAMES: u64 = 16;

/// Page-table flags, from the process's side of the boundary.
const PRESENT_USER: u64 = 1 | (1 << 2);
const WRITABLE: u64 = 1 << 1;
const NO_EXECUTE: u64 = 1 << 63;

/// The report region of the process running now, in *physical* addresses.
///
/// The nucleus reads it from its own identity map rather than through the
/// process's, because the process's mapping is a thing the process could in
/// principle change and this is the log.
static mut REPORT_PHYS: u64 = 0;
static mut REPORT_LENGTH: u64 = 0;

/// Why a process could not be started. Never a statement about the program.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unlaunchable {
    /// The handoff record declared no runtime image.
    NoRuntimeImage,
    /// The pool could not back the process's memory.
    OutOfFrames,
    /// A mapping could not be made.
    Paging(PagingRefused),
    /// More source units than the launch record can carry.
    TooManyUnits,
    /// The image's first bytes are not a runtime image header of a version this
    /// nucleus knows, or the sections it declares do not fit the bytes it came
    /// with.
    NotARuntimeImage,
}

impl From<PagingRefused> for Unlaunchable {
    fn from(refused: PagingRefused) -> Self {
        Unlaunchable::Paging(refused)
    }
}

/// Empties the running process's report region onto the serial log.
///
/// Called whenever the process enters the nucleus, which is the only time the
/// region is stable and the nucleus is running. That is what keeps the Stage 2
/// property true across the boundary: a line written before a call is on the
/// log before the call returns, so a stage that never returns is still named by
/// the last event.
pub fn drain_report() {
    // SAFETY: single-context nucleus with interrupts masked.
    let (base, length) = unsafe { (REPORT_PHYS, REPORT_LENGTH) };
    if base == 0 {
        return;
    }
    // SAFETY: `base` is a frame-aligned physical range the launcher allocated
    // for this process and mapped into the nucleus's identity map; the header
    // is written by the process and read here, and only `drained` is written
    // back, which the process never reads.
    let header =
        unsafe { &mut *core::ptr::with_exposed_provenance_mut::<ReportHeader>(base as usize) };
    let written = header
        .written
        .min(length - size_of::<ReportHeader>() as u64);
    let mut at = header.drained;
    let text = base + size_of::<ReportHeader>() as u64;
    while at < written {
        // SAFETY: `at` is inside the region by the bound above.
        let byte = unsafe { core::ptr::with_exposed_provenance::<u8>((text + at) as usize).read() };
        at += 1;
        if byte == b'\n' {
            tos_serial::puts(b"\r\n");
        } else {
            tos_serial::putc(byte);
        }
    }
    header.drained = at;
}

/// Runs `entry` at CPL 3 on `stack`, and returns when the process has ended.
///
/// # Safety
///
/// Both addresses are mapped user-accessible in the live address space — code
/// executable, stack writable — the GDT, TSS and `syscall` MSRs are installed,
/// and no other process is running.
// SAFETY: the caller's promise that the two mappings and the edge exist is what
// makes the process reachable; the capture below makes its end recoverable.
pub unsafe fn run(entry: u64, stack: u64, record: u64) -> Ended {
    use crate::exception::{USER_CODE_SELECTOR, USER_DATA_SELECTOR};

    // SAFETY: single-context nucleus with interrupts masked; this is the only
    // writer, and the values are read only by the paths that end a process.
    unsafe {
        if process_capture(addr_of_mut!(RETURN)) == 0 {
            RUNNING = true;
            process_enter(
                entry,
                stack,
                u64::from(USER_CODE_SELECTOR),
                u64::from(USER_DATA_SELECTOR),
                record,
            )
        }
    };
    // Reaching here means something resumed the captured context, which only
    // the end of a process does.
    // SAFETY: as above.
    unsafe {
        RUNNING = false;
        drain_report();
        ENDED
    }
}

/// Ends the running process because it said so (`process_exit`, ADR-0054).
///
/// Returns `false` when nothing is running at CPL 3, which is a call that
/// cannot have come from a process and therefore is not one.
pub fn exited(status: u64) -> bool {
    // SAFETY: single-context nucleus with interrupts masked.
    if !unsafe { RUNNING } {
        return false;
    }
    // What the nucleus asserts is that the process exited; `status` is the
    // process's own claim, and the event says which is which.
    tos_serial::puts(b"TOS.RUN.PROCESS_EXIT asserted_by=nucleus self_reported_status=");
    tos_serial::put_u32_decimal(status as u32);
    // How much of the machine's time this process was on the processor, counted
    // by the nucleus. A process cannot observe how long it was *off* it, so this
    // is not a number it could have reported.
    tos_serial::puts(b" ticks=");
    tos_serial::put_u32_decimal(crate::apic::process_ticks() as u32);
    tos_serial::puts(b"\r\n");
    // SAFETY: `RETURN` was recorded before this process started and `RUNNING`
    // being true is what says so.
    unsafe {
        ENDED = Ended::Exited(status);
        process_resume(addr_of_mut!(RETURN), 1)
    }
}

/// Reports a fault taken at CPL 3 and ends the process that took it.
///
/// Returns `false` when there was no process — in which case the caller has a
/// fault at CPL 3 in a system that is not running one, and the honest response
/// is the fatal path, not a guess about whose fault it was.
pub fn fault(vector: u64, error: u64, rip: u64, cr2: Option<u64>) -> bool {
    // SAFETY: single-context nucleus with interrupts masked.
    if !unsafe { RUNNING } {
        return false;
    }
    tos_serial::puts(b"TOS.RUN.PROCESS_FAULT vector=");
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
    tos_serial::puts(b" cpl=3\r\n");
    // SAFETY: `RETURN` was recorded by `run` before the process started, and
    // `RUNNING` being true is what says so. Nothing has run on that stack
    // since: the process ran on its own, and this handler on the TSS's.
    unsafe {
        ENDED = Ended::Fault(vector);
        process_resume(addr_of_mut!(RETURN), 1)
    }
}

/// Builds the first process and runs it.
///
/// Everything the process is made of comes from the pool the nucleus owns and
/// is mapped into an address space the nucleus builds. The process discovers
/// nothing: it is entered at a fixed address with one pointer, and every other
/// address it will ever use is in the record that pointer names.
///
/// # Safety
///
/// `image` names the verified runtime image bytes and `capsule` the capsule
/// bytes, both physically contiguous and identity-mapped for the nucleus;
/// `descs` is the validated memory map; and no other process is running.
// SAFETY: the caller's promise that the image and capsule ranges are what they
// say makes the mappings below name the bytes the identity record claims.
#[allow(clippy::too_many_arguments)]
pub unsafe fn launch(
    nucleus: &AddressSpace,
    frames: &mut Frames,
    descs: &[tos_boot_protocol::MemoryRange],
    bi: &tos_boot_protocol::BootInfo,
    image: Span,
    capsule: Span,
    units: &[(&[u8], &[u8])],
    entry_index: usize,
    identity: u64,
    source_set: &[u8],
) -> Result<Ended, Unlaunchable> {
    if image.length() == 0 {
        return Err(Unlaunchable::NoRuntimeImage);
    }
    // The nucleus's own mappings are in every address space, supervisor-only:
    // a syscall and a fault both change privilege without changing CR3, so the
    // nucleus has to be reachable from where the process runs.
    let mut space = paging::build(bi, descs, frames)?;

    // The runtime image, split the way its own header says it is split. A
    // process gets its text read-only and executable and its data writable and
    // not executable, for the same reason the nucleus does: a mapping that is
    // both is a defect. The header is read from the image rather than assumed,
    // and every boundary in it is checked against the bytes that arrived.
    // SAFETY: `image` is the range the loader reserved and the nucleus digested,
    // identity-mapped and at least one frame long.
    let header =
        unsafe { core::ptr::with_exposed_provenance::<ImageHeader>(image.start as usize).read() };
    if header.magic != IMAGE_MAGIC
        || header.entry >= header.text
        || header.text > header.file
        || header.file > header.memory
        || header.text % FRAME_SIZE != 0
        || header.memory % FRAME_SIZE != 0
        || header.text > image.length()
    {
        return Err(Unlaunchable::NotARuntimeImage);
    }
    map_range(
        &mut space,
        frames,
        IMAGE,
        Span::new(image.start, image.start + header.text),
        PRESENT_USER,
    )?;
    // Data and `.bss` are fresh frames, not the loader's copy: two processes
    // will share one image and must not share one writable page, and the file
    // carries no `.bss` at all. What the file does carry is copied in; the rest
    // is what a frame from the pool already is, which is zero.
    let mut offset = header.text;
    while offset < header.memory {
        let frame = frames.allocate_frame().ok_or(Unlaunchable::OutOfFrames)?;
        let carried = image.length().saturating_sub(offset).min(FRAME_SIZE);
        if carried > 0 {
            // SAFETY: `offset + carried` is inside the image range the caller
            // named, and `frame` is a cleared frame this pool just handed out
            // that nothing else references.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    core::ptr::with_exposed_provenance::<u8>((image.start + offset) as usize),
                    core::ptr::with_exposed_provenance_mut::<u8>(frame as usize),
                    carried as usize,
                )
            };
        }
        space.map_page(
            frames,
            IMAGE + offset,
            frame,
            PRESENT_USER | WRITABLE | NO_EXECUTE,
        )?;
        offset += FRAME_SIZE;
    }
    // The source set: read-only, not executable. A process that could write the
    // text it was verified against could execute something else.
    //
    // Mapped from the frame the capsule *starts in*, not from its first byte: a
    // page table addresses frames, so a range beginning mid-frame would be
    // mapped with its low bits silently dropped and every unit inside it would
    // be offset by however far into the frame the capsule began. The skew is
    // added back when each unit's address is computed, below.
    let capsule_frame = capsule.start & !(FRAME_SIZE - 1);
    let skew = capsule.start - capsule_frame;
    map_range(
        &mut space,
        frames,
        SOURCE,
        Span::new(capsule_frame, capsule.end),
        PRESENT_USER | NO_EXECUTE,
    )?;

    // The grant. One region, contiguous, mapped writable and not executable —
    // ADR-0041's property, one address space further out.
    let grant = frames
        .grant(identity)
        .map_err(|_| Unlaunchable::OutOfFrames)?;
    let grant_span = Span::new(grant.base as u64, (grant.base + grant.length) as u64);
    map_range(
        &mut space,
        frames,
        GRANT,
        grant_span,
        PRESENT_USER | WRITABLE | NO_EXECUTE,
    )?;

    map_fresh(&mut space, frames, STACK, STACK_FRAMES)?;
    let report = map_fresh(&mut space, frames, REPORT, REPORT_FRAMES)?;
    let table_bytes = units.len() * size_of::<LaunchUnit>();
    let paths_bytes: usize = units.iter().map(|(path, _)| relative(path).len()).sum();
    let record_bytes = (size_of::<Launch>() + table_bytes + paths_bytes) as u64;
    if record_bytes > MAX_RECORD_BYTES {
        return Err(Unlaunchable::TooManyUnits);
    }
    // Sized by the set it carries, not by a frame: a capsule may hold a
    // thousand source files, and a record that fitted only what one frame holds
    // would refuse to launch a machine whose capsule is merely large. Carved
    // contiguously because the nucleus writes it through its own identity map,
    // where one frame at a time and one struct across two frames are different
    // things.
    let record_span = frames
        .carve(record_bytes, FRAME_SIZE)
        .ok_or(Unlaunchable::OutOfFrames)?;
    let record = record_span.start;
    // A carve is not cleared (ADR-0050 section 3 clears on release), and this
    // one becomes a process's memory, so it is cleared here.
    // SAFETY: the run was just carved from the pool, is identity-mapped for the
    // nucleus, and nothing else references it.
    unsafe {
        core::ptr::write_bytes(
            core::ptr::with_exposed_provenance_mut::<u8>(record as usize),
            0,
            record_span.length() as usize,
        )
    };
    let mut mapped = 0;
    while mapped < record_span.length() {
        space.map_page(
            frames,
            RECORD + mapped,
            record + mapped,
            PRESENT_USER | NO_EXECUTE,
        )?;
        mapped += FRAME_SIZE;
    }

    // The record itself, written through the nucleus's identity map and read by
    // the process through its own. Everything it carries lives in one frame:
    // the record, then the unit table, then the paths the units name. The one
    // arithmetic that matters is the last: the tail begins after *the units
    // there are*, not after the most that would fit, and the whole of it is
    // checked against the frame before a single byte is written. Sized against
    // capacity instead, as the first version of this was, the tail begins 16
    // bytes from the end of the frame and the first path walks straight out of
    // it — which it did, into a page table, and the fault it produced named a
    // missing mapping rather than the write that removed it.
    let unit_table = RECORD + size_of::<Launch>() as u64;
    let tail = size_of::<Launch>() as u64 + table_bytes as u64;
    let mut launch = Launch {
        version: LAUNCH_VERSION,
        unit_count: units.len() as u32,
        entry_index: entry_index as u32,
        grant_version: grant.version,
        grant_base: GRANT,
        grant_length: grant.length as u64,
        grant_identity: grant.identity,
        units: unit_table,
        report_base: REPORT,
        report_length: REPORT_FRAMES * FRAME_SIZE,
        stack_base: STACK,
        stack_length: STACK_FRAMES * FRAME_SIZE,
        source_set: [0; 96],
    };
    let named = source_set.len().min(launch.source_set.len());
    launch.source_set[..named].copy_from_slice(&source_set[..named]);
    // SAFETY: `record` is a cleared frame this pool just handed out; nothing
    // else references it, and it is identity-mapped for the nucleus.
    unsafe { core::ptr::with_exposed_provenance_mut::<Launch>(record as usize).write(launch) };

    for (index, (path, bytes)) in units.iter().enumerate() {
        // Every unit is inside the capsule, so its address in the process is
        // its offset from the capsule's base. Nothing is copied: the process
        // reads the same bytes whose digest the boot already accounted for.
        let unit = LaunchUnit {
            path: RECORD + tail + path_offset(units, index),
            path_length: relative(path).len() as u64,
            bytes: SOURCE + skew + (bytes.as_ptr() as u64 - capsule.start),
            bytes_length: bytes.len() as u64,
        };
        // SAFETY: the bound checked above covers the whole unit table, so this
        // entry is inside the record's frame, which the nucleus owns and has
        // cleared.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<LaunchUnit>(
                (record + size_of::<Launch>() as u64) as usize,
            )
            .add(index)
            .write(unit)
        };
        // The paths are the capsule's names, which live in the capsule's name
        // arena rather than beside their content, so they are copied into the
        // record's own tail — module-root relative, which is what docs/42
        // section 1 derives a module name from, so the capsule's leading slash
        // is not part of the name.
        let at = record + tail + path_offset(units, index);
        for (offset, byte) in relative(path).iter().enumerate() {
            // SAFETY: the bound checked above covers the record, the whole unit
            // table and every path, so this byte is inside the record's frame.
            unsafe {
                core::ptr::with_exposed_provenance_mut::<u8>(at as usize)
                    .add(offset)
                    .write(*byte)
            };
        }
    }

    // SAFETY: single-context nucleus; this is the only writer.
    unsafe {
        REPORT_PHYS = report;
        REPORT_LENGTH = REPORT_FRAMES * FRAME_SIZE;
    }

    // From here the process's address space is the live one. The nucleus is
    // mapped into it supervisor-only, which is what makes the switch survivable.
    // SAFETY: `space` maps this nucleus at the same addresses it is running at,
    // plus the process's own pages; interrupts are masked and nothing else runs.
    unsafe { space.activate() };
    // SAFETY: the image is mapped executable at `IMAGE`, the stack writable
    // below its top, and the record readable at `RECORD`; the edge was
    // installed at nucleus entry.
    let ended = unsafe {
        run(
            IMAGE + header.entry,
            STACK + STACK_FRAMES * FRAME_SIZE,
            RECORD,
        )
    };

    // The process is over. Everything below is the nucleus taking back what it
    // gave, and it happens in the nucleus's own address space: the pool writes
    // to a frame when it clears it, and doing that through the dead process's
    // mappings would mean trusting tables the process could have been running
    // in when it died.
    // SAFETY: the nucleus's space maps this nucleus at the addresses it is
    // running at — it is the space this call arrived in — and nothing else runs.
    unsafe { nucleus.activate() };
    // SAFETY: the process is over, so its report region stops being one.
    unsafe {
        REPORT_PHYS = 0;
        REPORT_LENGTH = 0;
    }

    // What the process held goes back, cleared on the way (ADR-0050 section 3),
    // and what it holds is read out of its own page tables rather than
    // remembered here: one record of what a process had, and it is the one the
    // processor used.
    //
    // Three ranges are deliberately **not** returned. The image's text and the
    // capsule's source are not the pool's — the loader reserved them — and
    // releasing memory that was never allocated would hand the same frames out
    // twice. The page tables of the dead space are the pool's and are not
    // returned yet: freeing an interior table means proving nothing else under
    // it is mapped, and the nucleus's own mappings live in that same tree.
    // About fifty frames per process, named here rather than left to be found.
    let held = frames.in_use();
    // SAFETY: every frame below was handed out by this pool for this process,
    // its address space is no longer the live one, and the process that could
    // reach it does not exist.
    unsafe {
        release_mapped(
            &mut space,
            frames,
            IMAGE + header.text,
            header.memory - header.text,
        );
        release_mapped(&mut space, frames, RECORD, record_span.length());
        release_mapped(&mut space, frames, STACK, STACK_FRAMES * FRAME_SIZE);
        release_mapped(&mut space, frames, REPORT, REPORT_FRAMES * FRAME_SIZE);
        frames.release(grant_span);
    }
    // Measured, not asserted: the pool says how many frames came back and how
    // many it holds now. A reclamation nobody counts is a claim, and this is
    // the number a second process would be built out of.
    tos_serial::puts(b"TOS.RUN.PROCESS_RECLAIMED frames=");
    tos_serial::put_u32_decimal((held - frames.in_use()) as u32);
    tos_serial::puts(b" available=");
    tos_serial::put_u32_decimal(frames.available() as u32);
    tos_serial::puts(b"\r\n");
    Ok(ended)
}

/// Returns every frame a range of a process's space is mapped to.
///
/// # Safety
///
/// The range is one this pool allocated for that process, the space is no
/// longer live, and nothing else references any of it.
// SAFETY: the caller's promise that these frames are the pool's and unreferenced
// is the whole contract; the translation says which frames those are.
unsafe fn release_mapped(space: &mut AddressSpace, frames: &mut Frames, at: u64, length: u64) {
    let mut offset = 0;
    while offset < length {
        if let Some(frame) = space.translate(at + offset) {
            // Unmapped before it is released, and not only for tidiness: a
            // frame back in the pool with a mapping to it still standing is a
            // frame two owners can reach, and the next owner is a process.
            space.unmap_page(at + offset);
            // SAFETY: per the caller's contract, and the mapping is now gone.
            unsafe { frames.release_frame(frame) };
        }
        offset += FRAME_SIZE;
    }
}

/// A capsule path as a module-root-relative one: what docs/42 section 1
/// derives a module name from, so the capsule's leading slash is not part of it.
fn relative(path: &[u8]) -> &[u8] {
    match path.split_first() {
        Some((b'/', rest)) => rest,
        _ => path,
    }
}

/// Where unit `index`'s path goes in the record's tail.
fn path_offset(units: &[(&[u8], &[u8])], index: usize) -> u64 {
    units[..index]
        .iter()
        .map(|(path, _)| path.len() as u64)
        .sum()
}

/// Maps a physically contiguous range into a process, frame by frame.
fn map_range(
    space: &mut AddressSpace,
    frames: &mut Frames,
    at: u64,
    range: Span,
    flags: u64,
) -> Result<(), Unlaunchable> {
    let mut offset = 0;
    while offset < range.length() {
        space.map_page(frames, at + offset, range.start + offset, flags)?;
        offset += FRAME_SIZE;
    }
    Ok(())
}

/// Takes `count` fresh frames and maps them writable at `at`.
fn map_fresh(
    space: &mut AddressSpace,
    frames: &mut Frames,
    at: u64,
    count: u64,
) -> Result<u64, Unlaunchable> {
    let mut first = 0;
    for index in 0..count {
        let frame = frames.allocate_frame().ok_or(Unlaunchable::OutOfFrames)?;
        if index == 0 {
            first = frame;
        }
        space.map_page(
            frames,
            at + index * FRAME_SIZE,
            frame,
            PRESENT_USER | WRITABLE | NO_EXECUTE,
        )?;
    }
    Ok(first)
}
