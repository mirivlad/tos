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
use tos_frames::{Admission, Frames, FRAME_SIZE};
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

/// The frames the nucleus builds page tables out of, and nothing else.
///
/// **Why page tables cannot keep coming out of the pool.** ADR-0076 §2 rule 3
/// lets kernel-only overhead live outside the authority tree *only if it is
/// bounded and reserved before the tree exists*, and rule 4 says nothing
/// allocates user memory by asking the pool once the root authority is endowed.
/// Page tables are allocated on demand — one per absent level, at every
/// `map_page` — so a tree built after the endowment would quietly spend frames
/// the root authority has already promised to somebody. Two counters again,
/// with the second one hidden inside the pager.
///
/// So the frames are taken out of the pool **before** the root authority is
/// endowed, into a run this type owns, and the pager is given this instead of
/// the pool. The guarantee is structural rather than a rule anybody has to
/// remember: after `paging` stops taking `&mut Frames`, it cannot spend user
/// memory, because it has no way to name any.
///
/// A reserve that runs out refuses. That is the honest failure — a process
/// that cannot be given an address space is not started — and it is bounded
/// by [`crate::process::table_reserve`], which is computed from this machine's
/// map and this nucleus's own layout constants rather than measured once and
/// hoped for.
///
/// **Held as a free list of ordinary frames, not as a contiguous run.** A page
/// table is one frame, reached through the identity map and pointed at by a
/// physical address in the entry above it; nothing about it wants its
/// neighbours. Demanding a contiguous span would have invented a refusal the
/// hardware does not have — a machine with the frames but not one unbroken
/// piece of them would fail to boot — so the reserve takes its frames wherever
/// the pool has them and links them through their own first words.
pub struct Tables {
    /// The head of the free list, threaded through the frames themselves.
    /// Zero is the empty list: physical page zero is never in the pool.
    free: u64,
    available: u64,
}

impl Tables {
    /// A reserve holding nothing, before the map has been read.
    pub const fn empty() -> Tables {
        Tables {
            free: 0,
            available: 0,
        }
    }

    /// One cleared frame, or nothing when the reserve is spent.
    ///
    /// Cleared here because a carve is not: [`Frames::carve`] hands back a run
    /// as it lies, and a page table read before it is written is a table full
    /// of whatever the last owner left.
    pub fn allocate_frame(&mut self) -> Option<u64> {
        #[cfg(feature = "test-creation-rollback")]
        if crate::injection::tables_refuse() {
            return None;
        }
        if self.free == 0 {
            return None;
        }
        let frame = self.free;
        // SAFETY: the frame is one this reserve holds, identity-mapped, and its
        // first word is the link this reserve wrote when it took it in.
        self.free = unsafe { core::ptr::with_exposed_provenance::<u64>(frame as usize).read() };
        self.available -= 1;
        // SAFETY: the frame is one this reserve took out of the pool, which is
        // identity-mapped for the nucleus and which nothing else can reach —
        // the pool gave it up before the reserve began handing it out.
        unsafe {
            core::ptr::write_bytes(
                core::ptr::with_exposed_provenance_mut::<u8>(frame as usize),
                0,
                FRAME_SIZE as usize,
            )
        };
        Some(frame)
    }

    /// Takes one frame back.
    ///
    /// **Page tables have to come back or the reserve is not a bound.** A
    /// process's address space is its own tree, and when the process is gone
    /// nothing reaches any of it — so the tables it was made of return here and
    /// the next process is built out of them. Without this the reserve would be
    /// a bound on how many address spaces a boot may ever build rather than on
    /// how many may exist, which is not a bound at all: the old code leaked
    /// about fifty frames per process into the pool and only the pool's size
    /// hid it.
    ///
    /// # Safety
    ///
    /// The frame came from this reserve, nothing maps it, and no page table
    /// entry anywhere still points at it.
    // SAFETY: the caller's promise that the frame is unreachable is what makes
    // writing the free-list link into it sound.
    pub unsafe fn release_frame(&mut self, frame: u64) {
        // SAFETY: per the contract; the frame is in this reserve's run, which
        // is identity-mapped for the nucleus.
        unsafe { core::ptr::with_exposed_provenance_mut::<u64>(frame as usize).write(self.free) };
        self.free = frame;
        self.available += 1;
    }

    /// How many frames of the reserve are still there.
    pub fn remaining(&self) -> u64 {
        self.available
    }
}

/// The page-table reserve.
static mut TABLES: Tables = Tables::empty();

/// That reserve.
///
/// # Safety
///
/// As [`frames`]: the nucleus is single-context, and no caller holds a borrow
/// across an instruction that leaves it.
// SAFETY: the caller is nucleus code observing the same no-borrow-across-`iretq`
// rule the pool is accessed under.
pub unsafe fn tables() -> &'static mut Tables {
    // SAFETY: the static is initialized at link time and lives for the whole
    // boot; this is the only way it is ever named.
    unsafe { &mut *core::ptr::addr_of_mut!(TABLES) }
}

/// Takes the page-table reserve out of the pool, once, before anything else
/// spends.
///
/// Frame by frame, from wherever the pool has them: a page table has no use for
/// its neighbours, and demanding a contiguous run would refuse a machine that
/// has the memory but not one unbroken piece of it.
///
/// **A partial reserve is not a reserve.** If the pool runs out part-way, every
/// frame already taken goes back and the boot refuses — a nucleus that started
/// with room for three address spaces where its bound says five would fail
/// later, on a process creation, with nothing to point at.
///
/// # Safety
///
/// Called once, at boot, after [`admit_memory`] and before any address space or
/// process exists.
// SAFETY: the caller's promise that this is boot, before any space exists, is
// what makes every frame taken here unreferenced.
pub unsafe fn reserve_tables(bound: u64) -> Option<u64> {
    // SAFETY: nucleus code at boot; nothing else holds the pool.
    let frames = unsafe { frames() };
    // SAFETY: as above; nothing else holds the reserve.
    let reserve = unsafe { tables() };
    for _ in 0..bound {
        let Some(frame) = frames.allocate_frame() else {
            while let Some(taken) = reserve.allocate_frame() {
                // SAFETY: the frame came from this pool moments ago, nothing
                // ever mapped it, and the reserve has just given it up.
                unsafe { frames.release_frame(taken) };
            }
            return None;
        };
        // SAFETY: the pool just handed this frame over and nothing else names
        // it; from here it is the reserve's.
        unsafe { reserve.release_frame(frame) };
    }
    Some(reserve.remaining())
}

/// The authority tree, and the root of it.
///
/// One instance, nucleus-owned, for the whole boot: ADR-0076 §2 says one pool
/// and one tree, and a tree with two instances is two trees.
static mut AUTHORITY: crate::region::Regions = crate::region::Regions::new();

/// The root authority, once the boot has endowed it. `None` before that, which
/// is the only window in which nothing can be funded.
static mut ROOT: Option<crate::region::AuthorityId> = None;

/// The tree.
///
/// # Safety
///
/// As [`frames`]: single-context nucleus, no borrow held across an instruction
/// that leaves it.
// SAFETY: the caller is nucleus code observing the same no-borrow-across-`iretq`
// rule the pool is accessed under.
pub unsafe fn authority() -> &'static mut crate::region::Regions {
    // SAFETY: the static is initialized at link time and lives for the whole
    // boot; this is the only way it is ever named.
    unsafe { &mut *core::ptr::addr_of_mut!(AUTHORITY) }
}

/// Latched when the pool and the tree have been observed to disagree.
static mut DIVERGED: bool = false;

/// The root authority, for whoever needs to fund something out of it.
///
/// `None` before the boot has endowed it, and `None` for good after the
/// accounting has been seen to diverge — see [`accounting_diverged`].
pub fn root() -> Option<crate::region::AuthorityId> {
    // SAFETY: single-context nucleus; written once at boot and only read after.
    if unsafe { DIVERGED } {
        return None;
    }
    // SAFETY: as above.
    unsafe { ROOT }
}

/// Whether the pool and the tree have been seen to disagree.
///
/// A question only the evidence builds ask out loud: production reads the same
/// fact through [`root`], which stops answering, and the boot log says it once
/// in [`note_divergence`].
#[cfg_attr(not(feature = "test-creation-rollback"), allow(dead_code))]
pub fn accounting_diverged() -> bool {
    // SAFETY: single-context nucleus.
    unsafe { DIVERGED }
}

/// Records that an internal accounting step failed, and stops funding.
///
/// **A refund that refuses is not a user's refusal.** A `GrantCharge` names one
/// outstanding entry in the ledger, so the only way `refund_grant` can fail on
/// an internal lifecycle path is that the nucleus lost track of what it funded
/// — and from that instant the tree's idea of free memory is larger than the
/// pool's. Continuing to fund out of a number that is known to be wrong is how
/// an accounting defect becomes two owners of one frame.
///
/// So this latches, and [`root`] answers `None` from here on: nothing new is
/// built, and what is already running is left alone rather than taken down. The
/// nucleus does not panic over it — a process that exists is not made safer by
/// stopping the machine — but it does not pretend either, and the boot log says
/// so in the vocabulary the operator already reads faults in.
pub fn note_divergence(reason: &[u8]) {
    // SAFETY: single-context nucleus.
    unsafe { DIVERGED = true };
    tos_serial::puts(b"TOS.NUCLEUS.INVARIANT reason=");
    tos_serial::puts(reason);
    tos_serial::puts(b" effect=funding-stopped asserted_by=nucleus\r\n");
}

/// Endows the root authority over what the pool has left, once.
///
/// **Everything the pool still holds, and no second subtraction.** The
/// page-table reserve has already physically left the pool
/// ([`reserve_tables`]), so `available()` is exactly the memory that is free to
/// be promised; taking the reserve off again would endow a root over less than
/// the machine has and quietly strand the difference.
///
/// After this returns there is one number for free user memory, and it is the
/// tree's. Nothing may allocate user memory by asking the pool directly
/// (ADR-0076 §2 rule 4) — the pool still hands out the physical frames, but
/// only behind a charge that was made first.
///
/// # Safety
///
/// Called once, at boot, after [`reserve_tables`] and before any process
/// exists.
// SAFETY: the caller's promise that no process exists yet is what makes the
// pool's remainder unpromised.
pub unsafe fn endow_root() -> Option<usize> {
    // SAFETY: nucleus code at boot; nothing else holds the pool.
    let bytes = unsafe { frames() }.available() as usize * FRAME_SIZE as usize;
    // SAFETY: as above; nothing else holds the tree.
    let tree = unsafe { authority() };
    let root = tree.endow_root(bytes).ok()?;
    // SAFETY: single-context nucleus at boot; this is the only write.
    unsafe { ROOT = Some(root) };
    Some(bytes)
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
