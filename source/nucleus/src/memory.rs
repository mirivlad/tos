// SPDX-License-Identifier: GPL-3.0-or-later
//! What this machine's memory is, and what the nucleus may hand out of it.
//!
//! The nucleus is the only component that reads the memory map, and after
//! ADR-0050 section 1 it is also the only owner of physical frames. This module
//! is where the two meet: it turns a validated map into the pool
//! [`tos_frames::Frames`] owns, subtracting everything that is already spoken
//! for.
//!
//! **The subtraction is the whole of the safety argument.** The loader reserves
//! what it handed over, but it allocates the nucleus image by file length while
//! `.bss` is `NOLOAD` — so the memory the nucleus's own statics occupy is still
//! reported usable. The rest are subtracted although the map already marks them
//! reserved: memory a process could write over is not protected by one
//! component's bookkeeping being correct.

use tos_boot_protocol::{BootInfo, MemoryRange, MEM_USABLE};
use tos_frames::{Admission, Frames};
use tos_runtime::region::Span;

use tos_runtime::stack;

extern "C" {
    static __tos_image_start: u8;
    static __tos_image_load_end: u8;
    static __tos_image_end: u8;
}

/// The span the nucleus occupies in memory, `.bss` included.
fn image() -> Span {
    // Only the addresses of these symbols are taken; no byte of the image is
    // read through them, which is why no unsafe block is needed here.
    Span::new(
        core::ptr::addr_of!(__tos_image_start) as u64,
        core::ptr::addr_of!(__tos_image_end) as u64,
    )
}

/// A digest of the loaded nucleus image, truncated to name this build.
///
/// `--oformat=binary` makes `[__tos_image_start, __tos_image_load_end)` exactly
/// the bytes of `nucleus.bin`, so this identity can be recomputed from the
/// artifact rather than taken on the running image's word.
pub fn identity() -> u64 {
    // SAFETY: the linker script places both symbols in the loaded image, in
    // this order, and the range between them is mapped and readable.
    let bytes = unsafe {
        let start = core::ptr::addr_of!(__tos_image_start);
        let end = core::ptr::addr_of!(__tos_image_load_end);
        core::slice::from_raw_parts(start, end as usize - start as usize)
    };
    let digest = tos_hash::sha256(bytes);
    u64::from_le_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ])
}

/// The map entry holding the stack this nucleus is running on.
pub fn running_stack(descs: &[MemoryRange]) -> Option<Span> {
    stack::containing(all_spans(descs), stack::pointer())
}

/// Every span the pool must not admit.
fn occupied(bi: &BootInfo, bi_address: u64, stack: Option<Span>) -> ([Span; 6], usize) {
    let mut spans = [Span::new(0, 0); 6];
    let mut count = 0;
    let mut push = |span: Option<Span>| {
        if let Some(span) = span {
            if span.length() > 0 && count < spans.len() {
                spans[count] = span;
                count += 1;
            }
        }
    };
    push(Some(image()));
    push(Span::sized(bi.capsule_phys, bi.capsule_length));
    push(Span::sized(
        bi_address,
        tos_boot_protocol::STRUCT_SIZE as u64,
    ));
    push(Span::sized(bi.memory_map_phys, bi.memory_map_length));
    push(Span::sized(
        bi.framebuffer_phys,
        u64::from(bi.framebuffer_pitch) * u64::from(bi.framebuffer_height),
    ));
    push(stack);
    (spans, count)
}

/// Free spans, as the pool wants them.
fn free_spans(descs: &[MemoryRange]) -> impl Iterator<Item = Span> + '_ {
    descs
        .iter()
        .filter(|descriptor| descriptor.ty == MEM_USABLE)
        .filter_map(|descriptor| Span::sized(descriptor.phys_start, descriptor.phys_length))
}

/// Every span in the map, for locating the stack the nucleus is running on.
fn all_spans(descs: &[MemoryRange]) -> impl Iterator<Item = Span> + '_ {
    descs
        .iter()
        .filter_map(|descriptor| Span::sized(descriptor.phys_start, descriptor.phys_length))
}

/// The pool of physical frames this nucleus owns.
///
/// **Nucleus state, not a caller's local.** ADR-0050 makes the nucleus the
/// owner of this machine's memory, and an owner whose property lives on
/// somebody's stack is a bookkeeping arrangement rather than an owner. It is
/// also what makes the pool reachable from the system-call edge, where a
/// process holding process authority asks for a process to be built out of it.
static mut FRAMES: Frames = Frames::new();

/// The pool.
///
/// **No `&mut Frames` is ever held across an instruction that leaves the
/// nucleus.** That is the rule this accessor exists to make checkable: the
/// scheduler used to carry the pool through `iretq` as a parameter, and a
/// system call taken by the process it entered would then have produced a
/// second borrow of the same memory while the first was still alive. Every
/// caller below takes the pool, finishes with it, and drops it before anything
/// else runs.
///
/// # Safety
///
/// The nucleus is single-context: it runs with interrupts masked except while a
/// process is on the processor, and a process is not the nucleus. Callers
/// observe the rule above, so no second borrow can exist.
// SAFETY: the caller is nucleus code observing the no-borrow-across-`iretq`
// rule, which is why the single-context argument covers every access.
pub unsafe fn frames() -> &'static mut Frames {
    // SAFETY: the static is initialized at link time and lives for the whole
    // boot; this is the only way it is ever named.
    unsafe { &mut *core::ptr::addr_of_mut!(FRAMES) }
}

/// Gives this machine's free memory to the nucleus's pool, once.
///
/// # Safety
///
/// The caller states that `bi` and `descs` have passed the Boot ABI v1
/// validation the nucleus performs at entry — so the map describes this
/// machine's real memory — and that `stack` names the map entry the running
/// stack is in. This is what makes every admitted frame real, exclusively
/// owned, identity-mapped memory, which is what [`Frames::admit`] requires.
// SAFETY: the caller's promise that the Boot ABI validation already accepted this record and map is what makes the admitted memory real.
pub unsafe fn admit_memory(
    bi: &BootInfo,
    bi_address: u64,
    descs: &[MemoryRange],
    stack: Option<Span>,
) -> Admission {
    let (spans, count) = occupied(bi, bi_address, stack);
    // SAFETY: called once at nucleus entry, before any process exists.
    let frames = unsafe { frames() };
    // SAFETY: the caller's contract makes `free_spans(descs)` the usable memory
    // of a validated map, and `spans[..count]` names everything already spoken
    // for: the nucleus image with its `.bss`, the capsule, the handoff record,
    // the converted map, the framebuffer and the running stack.
    unsafe { frames.admit(free_spans(descs), &spans[..count]) }
}
