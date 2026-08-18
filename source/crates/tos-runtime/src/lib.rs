// SPDX-License-Identifier: GPL-3.0-or-later
//! `RuntimeMemoryGrantV1` and the bounded heap of the reference runtime
//! (ADR-0041).
//!
//! The nucleus owns the memory mechanism before Stage 3. It hands this runtime
//! **one** bounded region, and that region is the only heap the runtime has.
//! Nothing here probes a memory map, walks firmware tables or acquires an
//! ambient allocator: a base and a length arrive, or the runtime does not run.
//!
//! The heap is a **segregated-fit** allocator: boundary tags, immediate
//! coalescing of both neighbours, and free blocks threaded onto size-class
//! lists. That shape is chosen deliberately over a bump allocator — ADR-0041
//! refuses one that leaks between ordinary operations, because a reference
//! runtime that must be restarted to reclaim memory is not a recovery oracle.
//! Every free is a real free, and a block freed between two free blocks becomes
//! one block again, so repeated execution returns the arena to the state it
//! started in.
//!
//! **Why the free lists exist.** The first implementation searched first-fit by
//! walking every block from the base of the arena, so the cost of one
//! allocation grew with the number of blocks already in it. Measured on the
//! reference platform, a 256 KiB module — the published source-unit ceiling —
//! did not finish the frontend in 900 seconds, against a ~3 s expectation. Each
//! result was correct; the cost of reaching it was quadratic in the work done.
//!
//! The search now costs a bounded number of probes that does not depend on how
//! many blocks the arena holds, and the allocator counts its own probes so the
//! claim is checkable rather than asserted (see [`BoundedHeap::search_work`]).
//!
//! **The free lists cost no memory.** A block's list links live in the payload
//! of the block itself, which is free and therefore unused; the size-class
//! heads are a fixed array inside this struct. The allocator never allocates
//! for its own bookkeeping, which is what keeps a bounded arena bounded.
//!
//! **Two limits that are never the same thing.** [`RuntimeMemoryGrant::length`]
//! is what the implementation has. A module's `resource [allocation: ...]` is
//! the semantic budget of the program being run, enforced by the engine before
//! the effect. Exhausting the arena is not a fact about the module, and
//! exhausting a module's budget is not an implementation failure.
//!
//! **Failure discipline.** [`BoundedHeap::try_allocate`] returns `None` rather
//! than aborting. `alloc::GlobalAlloc` cannot: its contract is a null pointer,
//! which `alloc` turns into `handle_alloc_error`. So the arena is sized from a
//! measured bound rather than relied on to refuse gracefully — the second of
//! the two disciplines ADR-0041 accepts.
//!
//! **What that bound must be.** A sum of requested payloads is *not* a bound on
//! the arena a run needs. Every block carries tags, a request is rounded up to
//! the grain, a remainder too small to be its own block stays with the
//! allocation, and a hole below the highest live block is arena the run still
//! needed. [`BoundedHeap::peak_extent`] is therefore the metric: the highest
//! address the arena was ever carried to, minus the base. An arena of that size
//! serves the same sequence identically, because the search never looks past
//! the blocks the region already holds. [`BoundedHeap::committed`] is the
//! live figure, counted in whole blocks including their tags, so a free returns
//! exactly what its claim took.
//!
//! **Alignment is real.** A `GlobalAlloc` cannot assume its dependency closure
//! never asks for more than the block grain — a `repr(align(64))` type anywhere
//! would be enough — so every allocation carries a fixed prefix holding the
//! distance back to its block header, and a strongly aligned request is served
//! by placing the payload at the first suitably aligned address inside a block
//! large enough to hold it. One code path, no mode flag, and the cost is
//! measured by `peak_extent` rather than assumed away.

#![no_std]

pub mod region;
pub mod stack;

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

/// The version of the nucleus-to-runtime memory contract.
pub const GRANT_VERSION: u32 = 1;

/// One bounded region granted by the nucleus (ADR-0041 section 1).
///
/// This is a *declared input*. A runtime constructed without one has no heap
/// and runs nothing, which is the property that keeps memory discovery in the
/// nucleus where it belongs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeMemoryGrant {
    pub version: u32,
    /// Start of the granted region.
    pub base: usize,
    /// Bytes granted.
    pub length: usize,
    /// Guaranteed alignment of `base`, a power of two.
    pub alignment: usize,
    /// Which nucleus build produced the grant.
    pub identity: u64,
}

/// Why a grant cannot back a heap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrantError {
    /// The grant declares a contract version this runtime does not implement.
    UnsupportedVersion(u32),
    /// The region is too small to hold even one allocation.
    TooSmall { length: usize, minimum: usize },
    /// `base` is null, or `alignment` is not a power of two, or `base` is not
    /// aligned to it.
    Unaligned,
    /// `base + length` would wrap the address space.
    Overflows,
}

/// The header in front of every block, free or used.
///
/// `size` is the payload capacity. The same value is repeated in a footer after
/// the payload, which is what makes coalescing with the *previous* neighbour
/// possible without a search: a block can read the footer immediately before
/// its own header.
#[repr(C)]
#[derive(Clone, Copy)]
struct Tag {
    size: usize,
    free: bool,
}

const TAG_BYTES: usize = core::mem::size_of::<Tag>();
/// Header and footer, which every block carries.
const OVERHEAD: usize = 2 * TAG_BYTES;
/// Blocks and payload starts are multiples of this.
const GRAIN: usize = 16;

/// Bytes reserved before every returned pointer.
///
/// Its last `usize` holds the distance back to the block header. That is what
/// lets one `deallocate` path serve both an ordinary allocation and one whose
/// payload was pushed forward to meet a strong alignment: the pointer always
/// knows where its block is, so nothing has to be inferred from its address.
const PREFIX: usize = GRAIN;

const fn round_up(value: usize, to: usize) -> usize {
    value.div_ceil(to) * to
}

/// A bounded heap over one granted region, with real reclaim.
pub struct BoundedHeap {
    base: usize,
    length: usize,
    /// Bytes of the region currently committed to live blocks, tags included.
    ///
    /// Counted in whole blocks rather than in requested payload, so a free
    /// returns exactly what its claim took even when the block kept a remainder
    /// too small to split off.
    committed: usize,
    /// The highest address the arena was ever carried to, relative to `base`.
    frontier: usize,
    /// Whether the region has been laid out with its initial free block.
    ready: bool,
    /// The first free block of each size class, or 0 for none.
    ///
    /// A fixed array, not an allocation: an allocator that had to allocate to
    /// describe its own free space could not be given a bounded region and
    /// asked to stay inside it.
    bins: [usize; BINS],
    /// Which classes are non-empty, so the next larger one is a bit scan.
    occupied_bins: u64,
    /// Free-list nodes examined, and allocations served.
    ///
    /// Kept so the search's cost is *observable* rather than argued. A ratio
    /// that stays flat while the arena fills is the evidence that the old
    /// walk-every-block behaviour is gone.
    probes: u64,
    served: u64,
}

impl BoundedHeap {
    /// The smallest region that can hold one minimum-size block.
    pub const MINIMUM_GRANT: usize = OVERHEAD + GRAIN;

    /// Creates an unusable heap, for a runtime that has not been granted one.
    ///
    /// It refuses every allocation, which is the correct behaviour: a runtime
    /// with no grant has no memory, and pretending otherwise would be the
    /// ambient allocator ADR-0041 forbids.
    pub const fn ungranted() -> BoundedHeap {
        BoundedHeap {
            base: 0,
            length: 0,
            committed: 0,
            frontier: 0,
            ready: false,
            bins: [0; BINS],
            occupied_bins: 0,
            probes: 0,
            served: 0,
        }
    }

    /// Validates a grant and lays the region out as one free block.
    ///
    /// # Safety
    ///
    /// The caller states that `grant.base` addresses `grant.length` bytes that
    /// are readable, writable, and owned by no one else for the lifetime of
    /// this heap. That is exactly the promise the nucleus makes when it grants
    /// the region, and it is the only thing this type cannot check for itself.
    // SAFETY: the caller's grant promise is the whole contract; every other precondition is validated here before any byte is touched.
    pub unsafe fn adopt(&mut self, grant: &RuntimeMemoryGrant) -> Result<(), GrantError> {
        if grant.version != GRANT_VERSION {
            return Err(GrantError::UnsupportedVersion(grant.version));
        }
        if grant.base == 0
            || !grant.alignment.is_power_of_two()
            || grant.alignment < GRAIN
            || !grant.base.is_multiple_of(grant.alignment)
        {
            return Err(GrantError::Unaligned);
        }
        if grant.base.checked_add(grant.length).is_none() {
            return Err(GrantError::Overflows);
        }
        if grant.length < Self::MINIMUM_GRANT {
            return Err(GrantError::TooSmall {
                length: grant.length,
                minimum: Self::MINIMUM_GRANT,
            });
        }

        let usable = (grant.length - OVERHEAD) / GRAIN * GRAIN;
        self.base = grant.base;
        self.length = usable + OVERHEAD;
        self.committed = 0;
        self.frontier = 0;
        self.bins = [0; BINS];
        self.occupied_bins = 0;
        self.probes = 0;
        self.served = 0;
        // SAFETY: the caller's promise covers `length` bytes from `base`, and
        // `usable + OVERHEAD` is no larger than the granted length.
        unsafe { write_tags(self.base, usable, true) };
        self.ready = true;
        // SAFETY: the block was just written and is free, so its payload is
        // this heap's to thread onto a free list.
        unsafe { self.link_free(self.base, usable) };
        Ok(())
    }

    /// Bytes of the region committed to live blocks, tags included.
    pub fn committed(&self) -> usize {
        self.committed
    }

    /// The arena size this run would have needed.
    ///
    /// The measurement ADR-0041's second discipline requires. It is the highest
    /// address the arena was ever carried to, so it includes every block's
    /// tags, the rounding to the grain, a remainder too small to split off, and
    /// any hole below the frontier — all of which are arena the run needed even
    /// though none of them is requested payload.
    ///
    /// It never falls when a block is freed. That is deliberate: a bound that
    /// shrank would understate what a later identical run requires, and a bound
    /// must err upward.
    pub fn peak_extent(&self) -> usize {
        self.frontier
    }

    /// Total usable bytes, excluding the first block's own tags.
    pub fn capacity(&self) -> usize {
        self.length.saturating_sub(OVERHEAD)
    }

    /// Free-list nodes examined, and allocations served.
    ///
    /// The first figure divided by the second is the search work per
    /// allocation. It stays flat as the arena fills, which is the property the
    /// original walk-every-block search did not have and could not be given.
    pub fn search_work(&self) -> (u64, u64) {
        (self.probes, self.served)
    }

    /// Threads a free block onto the list for its size class.
    ///
    /// # Safety
    ///
    /// `at` is a block header this heap owns, its tag says free, and its
    /// payload is at least `GRAIN` bytes — enough for the two links.
    // SAFETY: only the payload of a free block this heap owns is written, and a free block's payload belongs to the allocator.
    unsafe fn link_free(&mut self, at: usize, size: usize) {
        let bin = bin_of(size);
        let head = self.bins[bin];
        // SAFETY: `at` is a free block with at least `GRAIN` payload bytes, so
        // the two link words lie inside it.
        unsafe {
            write_link(at, NEXT, head);
            write_link(at, PREV, 0);
            if head != 0 {
                write_link(head, PREV, at);
            }
        }
        self.bins[bin] = at;
        self.occupied_bins |= 1u64 << bin;
    }

    /// Takes a free block off its size-class list.
    ///
    /// # Safety
    ///
    /// `at` is a block this heap threaded onto the list for `size`.
    // SAFETY: only the link words of free blocks this heap owns are read and written.
    unsafe fn unlink_free(&mut self, at: usize, size: usize) {
        let bin = bin_of(size);
        // SAFETY: `at` is on this list, so its links are the ones written by
        // `link_free`, and its neighbours are blocks of the same list.
        unsafe {
            let next = read_link(at, NEXT);
            let previous = read_link(at, PREV);
            if previous == 0 {
                self.bins[bin] = next;
            } else {
                write_link(previous, NEXT, next);
            }
            if next != 0 {
                write_link(next, PREV, previous);
            }
        }
        if self.bins[bin] == 0 {
            self.occupied_bins &= !(1u64 << bin);
        }
    }

    /// Finds a free block whose payload is at least `need`, and takes it off
    /// its list.
    ///
    /// The cost is what makes this allocator usable. Only the request's own
    /// size class can hold blocks too small for it, so that class is probed a
    /// bounded number of times; every class above it holds blocks that are
    /// certainly large enough, so the next non-empty one is a bit scan.
    ///
    /// The one unbounded path is deliberate: when no larger class has anything,
    /// the rest of the exact class is walked before giving up. Refusing while a
    /// block that fits is sitting in the arena would be an allocator that lies
    /// about exhaustion, and that path is reached only when the arena is nearly
    /// full — which is where a linear walk is both rare and affordable.
    ///
    /// # Safety
    ///
    /// The heap must have adopted a valid grant.
    // SAFETY: the heap has adopted a live grant, so every address walked is inside memory the caller promised.
    unsafe fn take_fitting(&mut self, need: usize) -> Option<usize> {
        let bin = bin_of(need);
        let mut cursor = self.bins[bin];
        let mut probes = 0u32;
        while cursor != 0 && probes < PROBE_LIMIT {
            probes += 1;
            // SAFETY: `cursor` is a free block this heap threaded on.
            let size = unsafe { read_tag(cursor) }.size;
            if size >= need {
                self.probes += u64::from(probes);
                // SAFETY: `cursor` is on the list for `size`.
                unsafe { self.unlink_free(cursor, size) };
                return Some(cursor);
            }
            // SAFETY: `cursor` is a free block, so its links are valid.
            cursor = unsafe { read_link(cursor, NEXT) };
        }

        // Every class above this one holds blocks larger than any request of
        // this class, so the first non-empty one fits without being examined.
        let larger = self.occupied_bins & !((1u64 << bin) | ((1u64 << bin) - 1));
        if larger != 0 {
            let chosen = larger.trailing_zeros() as usize;
            let at = self.bins[chosen];
            self.probes += u64::from(probes) + 1;
            // SAFETY: `at` heads a non-empty class, so it is a threaded block.
            let size = unsafe { read_tag(at) }.size;
            debug_assert!(size >= need);
            // SAFETY: `at` is on the list for `size`.
            unsafe { self.unlink_free(at, size) };
            return Some(at);
        }

        // Nothing larger exists. Finish the exact class rather than refuse
        // while a block that fits is still in the arena.
        while cursor != 0 {
            probes += 1;
            // SAFETY: `cursor` is a free block this heap threaded on.
            let size = unsafe { read_tag(cursor) }.size;
            if size >= need {
                self.probes += u64::from(probes);
                // SAFETY: `cursor` is on the list for `size`.
                unsafe { self.unlink_free(cursor, size) };
                return Some(cursor);
            }
            // SAFETY: `cursor` is a free block, so its links are valid.
            cursor = unsafe { read_link(cursor, NEXT) };
        }
        self.probes += u64::from(probes);
        None
    }

    /// Allocates, returning `None` rather than aborting.
    ///
    /// This is the fallible entry point. Callers that can refuse work should
    /// use it; `GlobalAlloc` cannot, because its contract is a null pointer.
    ///
    /// # Safety
    ///
    /// The heap must have adopted a valid grant, whose promise this upholds.
    // SAFETY: the heap has adopted a live grant, so every address it walks is inside memory the caller promised.
    pub unsafe fn try_allocate(&mut self, layout: Layout) -> Option<*mut u8> {
        if !self.ready || layout.size() == 0 {
            return None;
        }
        let align = layout.align().max(GRAIN);
        // A block's payload starts `GRAIN`-aligned, so the worst case is
        // needing to skip almost a whole alignment step past the prefix.
        let need = round_up(PREFIX + layout.size() + (align - GRAIN), GRAIN);
        self.served += 1;

        // SAFETY: the heap has adopted a live grant.
        let at = unsafe { self.take_fitting(need) }?;
        // SAFETY: `take_fitting` returned a block it removed from its list.
        let size = unsafe { read_tag(at) }.size;
        // SAFETY: the block is off every list, free, and large enough.
        let kept = unsafe { self.occupy(at, size, need) };
        let payload = at + TAG_BYTES;
        let pointer = round_up(payload + PREFIX, align);
        debug_assert!(pointer + layout.size() <= payload + kept);
        // SAFETY: `pointer - WORD` lies inside this block's payload, because
        // `pointer` is at least `PREFIX` past its start.
        unsafe { write_backlink(pointer, pointer - at) };
        self.committed += kept + OVERHEAD;
        self.frontier = self.frontier.max(at + kept + OVERHEAD - self.base);
        Some(pointer as *mut u8)
    }

    /// Marks a block used, splitting off the remainder when it is worth a
    /// block, and returns the payload size the block actually kept.
    ///
    /// The return value is what the accounting must use: when the remainder is
    /// too small to stand alone it stays with this allocation, and charging
    /// only the requested amount would let the eventual free return more than
    /// the claim took.
    ///
    // SAFETY: `at` is a free block off every list whose payload is `size`, and `wanted` is a grain multiple no larger, so the split stays inside the block.
    unsafe fn occupy(&mut self, at: usize, size: usize, wanted: usize) -> usize {
        let leftover = size - wanted;
        if leftover >= OVERHEAD + GRAIN {
            let remainder = at + wanted + OVERHEAD;
            let remainder_size = leftover - OVERHEAD;
            // SAFETY: the split stays inside the original block.
            unsafe {
                write_tags(at, wanted, false);
                write_tags(remainder, remainder_size, true);
                self.link_free(remainder, remainder_size);
            }
            wanted
        } else {
            // Too small to be a block of its own; it stays with this one.
            // SAFETY: `at` is a block header.
            unsafe { write_tags(at, size, false) };
            size
        }
    }

    /// Frees a pointer this heap returned, coalescing both neighbours.
    ///
    /// # Safety
    ///
    /// `pointer` was returned by [`Self::try_allocate`] on this heap and has not
    /// been freed since.
    // SAFETY: `pointer` came from `try_allocate` on this heap, and its bounds are re-checked here before the tag is read.
    pub unsafe fn deallocate(&mut self, pointer: *mut u8) {
        if !self.ready || pointer.is_null() {
            return;
        }
        let address = pointer as usize;
        if address < self.base + TAG_BYTES + WORD || address >= self.base + self.length {
            return;
        }
        // SAFETY: every pointer this heap returned carries its distance back to
        // its own header in the word before it.
        let back = unsafe { read_backlink(address) };
        if back > address - self.base {
            return;
        }
        let header = address - back;
        if header < self.base || header >= self.base + self.length {
            return;
        }
        // SAFETY: `header` is the tag of a block this heap handed out.
        let tag = unsafe { read_tag(header) };
        if tag.free {
            return;
        }
        self.committed = self.committed.saturating_sub(tag.size + OVERHEAD);

        let mut start = header;
        let mut size = tag.size;

        // Merge forward while the next block is free. A neighbour that is
        // about to stop existing must leave its size-class list first, or the
        // list would name memory that is no longer a block.
        loop {
            let next = start + size + OVERHEAD;
            if next + OVERHEAD > self.base + self.length {
                break;
            }
            // SAFETY: `next` is inside the region and is a block header.
            let following = unsafe { read_tag(next) };
            if following.size == 0 || !following.free {
                break;
            }
            // SAFETY: a free block is always on the list for its own size.
            unsafe { self.unlink_free(next, following.size) };
            size += following.size + OVERHEAD;
        }

        // Merge backward while the previous block is free, found through its
        // footer, which is why every block carries one.
        while start > self.base {
            // SAFETY: a block always sits before `start` when `start` is past
            // the base, and its footer is the tag immediately before `start`.
            let previous = unsafe { read_tag(start - TAG_BYTES) };
            if previous.size == 0 || !previous.free {
                break;
            }
            let previous_start = start - previous.size - OVERHEAD;
            if previous_start < self.base {
                break;
            }
            // SAFETY: a free block is always on the list for its own size.
            unsafe { self.unlink_free(previous_start, previous.size) };
            size += previous.size + OVERHEAD;
            start = previous_start;
        }

        // SAFETY: `start .. start + size + OVERHEAD` is inside the region.
        unsafe { write_tags(start, size, true) };
        // SAFETY: the merged block is free and at least `GRAIN` wide.
        unsafe { self.link_free(start, size) };
    }

    /// How many blocks the region currently holds, and how many are free.
    ///
    /// Fragmentation is the property that decides whether repeated execution
    /// returns the arena to its starting state, so it is observable rather than
    /// assumed.
    pub fn block_census(&self) -> (usize, usize) {
        if !self.ready {
            return (0, 0);
        }
        let mut blocks = 0;
        let mut free = 0;
        let mut cursor = self.base;
        let end = self.base + self.length;
        while cursor + OVERHEAD <= end {
            // SAFETY: `cursor` walks maintained block headers.
            let tag = unsafe { read_tag(cursor) };
            if tag.size == 0 {
                break;
            }
            blocks += 1;
            if tag.free {
                free += 1;
            }
            cursor += tag.size + OVERHEAD;
        }
        (blocks, free)
    }
}

/// One machine word, which is what a backlink is.
const WORD: usize = core::mem::size_of::<usize>();

/// Size classes, one per power of two of a block's payload size.
///
/// Class `k` holds blocks whose payload is in `2^k .. 2^(k+1)`. That is the
/// property the search rests on: a request whose own class is `k` is satisfied
/// by *any* block from a class above `k`, so finding one costs a bit scan
/// rather than a walk.
const BINS: usize = 48;

/// How many blocks of the request's own class are examined before the search
/// gives up on an exact fit and takes a larger one.
///
/// Only this class can hold blocks too small for the request, so it is the only
/// one that needs looking at, and a fixed budget keeps the common path bounded
/// however long the list becomes.
const PROBE_LIMIT: u32 = 8;

/// The class a payload of this size belongs to.
fn bin_of(size: usize) -> usize {
    debug_assert!(size >= GRAIN);
    let highest = usize::BITS - 1 - size.leading_zeros();
    (highest as usize).min(BINS - 1)
}

/// Which of a free block's two link words is meant.
const NEXT: usize = 0;
const PREV: usize = WORD;

/// Reads one of a free block's list links.
///
// SAFETY: `at` is a free block header this heap owns, so its first two payload words are the allocator's.
unsafe fn read_link(at: usize, which: usize) -> usize {
    // SAFETY: a free block's payload is at least `GRAIN` bytes and belongs to
    // the allocator, so both link words lie inside it.
    unsafe { ptr::read_unaligned((at + TAG_BYTES + which) as *const usize) }
}

/// Writes one of a free block's list links.
///
// SAFETY: `at` is a free block header this heap owns, so its first two payload words are the allocator's.
unsafe fn write_link(at: usize, which: usize, value: usize) {
    // SAFETY: as `read_link`.
    unsafe { ptr::write_unaligned((at + TAG_BYTES + which) as *mut usize, value) }
}

/// Records how far back a returned pointer's block header lies.
///
// SAFETY: `pointer - WORD` lies inside the block's own payload.
unsafe fn write_backlink(pointer: usize, distance: usize) {
    // SAFETY: the caller guarantees the word before `pointer` belongs to this
    // block, and only this function writes it.
    unsafe { ptr::write_unaligned((pointer - WORD) as *mut usize, distance) }
}

/// Reads the distance back to a pointer's block header.
///
// SAFETY: `pointer` was returned by `try_allocate` on this heap.
unsafe fn read_backlink(pointer: usize) -> usize {
    // SAFETY: the caller guarantees `pointer` came from this heap, so the word
    // before it is the backlink `write_backlink` wrote.
    unsafe { ptr::read_unaligned((pointer - WORD) as *const usize) }
}

/// Writes a block's header and footer.
///
/// # Safety
///
/// `at .. at + size + OVERHEAD` lies inside a region this heap owns.
// SAFETY: the caller owns `at .. at + size + OVERHEAD`, which is the only memory written.
unsafe fn write_tags(at: usize, size: usize, free: bool) {
    let tag = Tag { size, free };
    // SAFETY: the caller guarantees the range is owned and large enough, and
    // `Tag` is `repr(C)` with no padding requirements beyond `usize`.
    unsafe {
        ptr::write_unaligned(at as *mut Tag, tag);
        ptr::write_unaligned((at + TAG_BYTES + size) as *mut Tag, tag);
    }
}

/// Reads a tag.
///
/// # Safety
///
/// `at` addresses a tag this heap wrote.
// SAFETY: `at` addresses a tag `write_tags` wrote; nothing else writes this memory.
unsafe fn read_tag(at: usize) -> Tag {
    // SAFETY: the caller guarantees `at` addresses a tag written by
    // `write_tags`, which is the only writer of this memory.
    unsafe { ptr::read_unaligned(at as *const Tag) }
}

/// A `GlobalAlloc` over a `BoundedHeap`, for a freestanding binary.
///
/// Interior mutability is a raw cell rather than a lock: the Stage 2 reference
/// runtime is single-threaded by construction — Bootstrap serializes, and
/// nothing here starts a host thread — so a lock would be machinery guarding
/// against a caller that cannot exist. That assumption is stated here because
/// it is the one thing that would have to change first if a Full engine ever
/// ran this allocator from more than one context.
pub struct GlobalHeap {
    heap: core::cell::UnsafeCell<BoundedHeap>,
}

// SAFETY: see the type's own note — the reference runtime is single-threaded,
// so no two contexts reach the heap at once.
unsafe impl Sync for GlobalHeap {}

impl GlobalHeap {
    pub const fn new() -> GlobalHeap {
        GlobalHeap {
            heap: core::cell::UnsafeCell::new(BoundedHeap::ungranted()),
        }
    }

    /// Adopts the nucleus's grant.
    ///
    /// # Safety
    ///
    /// The grant's promise must hold, and this must be called before any
    /// allocation and from the single context that uses the runtime.
    // SAFETY: as `BoundedHeap::adopt`, plus single-context use before any allocation.
    pub unsafe fn adopt(&self, grant: &RuntimeMemoryGrant) -> Result<(), GrantError> {
        // SAFETY: the caller guarantees single-context use before any
        // allocation, so no other reference to the heap exists.
        unsafe { (*self.heap.get()).adopt(grant) }
    }

    /// Bytes committed to live blocks, and the arena size this run needed.
    pub fn usage(&self) -> (usize, usize) {
        // SAFETY: single-context use; see the type's note.
        let heap = unsafe { &*self.heap.get() };
        (heap.committed(), heap.peak_extent())
    }

    pub fn block_census(&self) -> (usize, usize) {
        // SAFETY: single-context use; see the type's note.
        unsafe { (*self.heap.get()).block_census() }
    }
}

impl Default for GlobalHeap {
    fn default() -> GlobalHeap {
        GlobalHeap::new()
    }
}

// SAFETY: `alloc` and `dealloc` uphold the `GlobalAlloc` contract: a returned
// pointer addresses `layout.size()` writable bytes aligned to `layout.align()`,
// and `dealloc` is only ever called with such a pointer.
unsafe impl GlobalAlloc for GlobalHeap {
    // SAFETY: the `GlobalAlloc` contract; the heap returns a pointer to `layout.size()` writable bytes or null.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: single-context use; the heap upholds the grant's promise.
        match unsafe { (*self.heap.get()).try_allocate(layout) } {
            Some(pointer) => pointer,
            // The `GlobalAlloc` contract is a null pointer, which `alloc` turns
            // into `handle_alloc_error`. ADR-0041 therefore sizes the arena
            // from a measured bound rather than relying on this path.
            None => ptr::null_mut(),
        }
    }

    // SAFETY: the `GlobalAlloc` contract; `pointer` was returned by `alloc` on this allocator.
    unsafe fn dealloc(&self, pointer: *mut u8, _layout: Layout) {
        // SAFETY: the caller passes a pointer this allocator returned.
        unsafe { (*self.heap.get()).deallocate(pointer) }
    }
}
