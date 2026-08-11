// SPDX-License-Identifier: GPL-3.0-or-later
//! `RuntimeMemoryGrantV1` and the bounded heap of the reference runtime
//! (ADR-0041).
//!
//! The nucleus owns the memory mechanism before Stage 3. It hands this runtime
//! **one** bounded region, and that region is the only heap the runtime has.
//! Nothing here probes a memory map, walks firmware tables or acquires an
//! ambient allocator: a base and a length arrive, or the runtime does not run.
//!
//! The heap is a first-fit free list with boundary tags and immediate
//! coalescing of both neighbours. That shape is chosen deliberately over a bump
//! allocator: ADR-0041 refuses one that leaks between ordinary operations,
//! because a reference runtime that must be restarted to reclaim memory is not
//! a recovery oracle. Every free is a real free, and a block freed between two
//! free blocks becomes one block again, so repeated execution returns the arena
//! to the state it started in.
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
//! the two disciplines ADR-0041 accepts — and [`BoundedHeap::high_water`]
//! exists so that bound can be measured rather than assumed.

#![no_std]

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
/// Every block is aligned to this, so a payload satisfies any alignment up to it
/// without the allocator needing to shift the payload inside the block.
const GRAIN: usize = 16;

const fn round_up(value: usize, to: usize) -> usize {
    value.div_ceil(to) * to
}

/// A bounded heap over one granted region, with real reclaim.
pub struct BoundedHeap {
    base: usize,
    length: usize,
    /// Bytes of payload currently handed out, for the accounting a runtime
    /// needs and for measuring the bound the arena must exceed.
    in_use: usize,
    high_water: usize,
    /// Whether the region has been laid out with its initial free block.
    ready: bool,
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
            in_use: 0,
            high_water: 0,
            ready: false,
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
        self.in_use = 0;
        self.high_water = 0;
        // SAFETY: the caller's promise covers `length` bytes from `base`, and
        // `usable + OVERHEAD` is no larger than the granted length.
        unsafe { write_tags(self.base, usable, true) };
        self.ready = true;
        Ok(())
    }

    /// Bytes of payload currently handed out.
    pub fn in_use(&self) -> usize {
        self.in_use
    }

    /// The most ever handed out at one moment.
    ///
    /// This is the measurement ADR-0041's second discipline needs: an arena is
    /// sized above a measured bound rather than trusted to refuse gracefully.
    pub fn high_water(&self) -> usize {
        self.high_water
    }

    /// Total usable bytes, excluding the first block's own tags.
    pub fn capacity(&self) -> usize {
        self.length.saturating_sub(OVERHEAD)
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
        // A block is `GRAIN`-aligned, so any alignment up to `GRAIN` is
        // satisfied by the block itself. A larger one is over-allocated and the
        // payload start is advanced, which the free path recovers by reading
        // the tag that precedes the returned pointer.
        if layout.align() > GRAIN {
            return None;
        }
        let wanted = round_up(layout.size(), GRAIN);

        let mut cursor = self.base;
        let end = self.base + self.length;
        while cursor + OVERHEAD <= end {
            // SAFETY: `cursor` walks block headers laid out by `adopt` and
            // maintained by every split and merge below, so it addresses a tag.
            let tag = unsafe { read_tag(cursor) };
            if tag.size == 0 {
                break;
            }
            if tag.free && tag.size >= wanted {
                // SAFETY: the block at `cursor` is free and large enough.
                unsafe { self.occupy(cursor, tag.size, wanted) };
                self.in_use += wanted;
                self.high_water = self.high_water.max(self.in_use);
                return Some((cursor + TAG_BYTES) as *mut u8);
            }
            cursor += tag.size + OVERHEAD;
        }
        None
    }

    /// Marks a block used, splitting off the remainder when it is worth a block.
    ///
    /// # Safety
    ///
    /// `at` is a free block header whose payload is `size`, and `wanted` is a
    /// `GRAIN` multiple no larger than `size`.
    // SAFETY: `at` is a free block header of `size`, and `wanted` is a grain multiple no larger, so the split stays inside the block.
    unsafe fn occupy(&mut self, at: usize, size: usize, wanted: usize) {
        let leftover = size - wanted;
        if leftover >= OVERHEAD + GRAIN {
            // SAFETY: the split stays inside the original block.
            unsafe {
                write_tags(at, wanted, false);
                write_tags(at + wanted + OVERHEAD, leftover - OVERHEAD, true);
            }
        } else {
            // Too small to be a block of its own; it stays with this one, which
            // is what keeps every block at least `GRAIN` of payload.
            // SAFETY: `at` is a block header.
            unsafe { write_tags(at, size, false) };
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
        let header = pointer as usize - TAG_BYTES;
        if header < self.base || header >= self.base + self.length {
            return;
        }
        // SAFETY: `header` is the tag of a block this heap handed out.
        let tag = unsafe { read_tag(header) };
        if tag.free {
            return;
        }
        self.in_use = self.in_use.saturating_sub(tag.size);

        let mut start = header;
        let mut size = tag.size;

        // Merge forward while the next block is free.
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
            size += previous.size + OVERHEAD;
            start = previous_start;
        }

        // SAFETY: `start .. start + size + OVERHEAD` is inside the region.
        unsafe { write_tags(start, size, true) };
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

    /// Bytes handed out, and the most ever handed out at once.
    pub fn usage(&self) -> (usize, usize) {
        // SAFETY: single-context use; see the type's note.
        let heap = unsafe { &*self.heap.get() };
        (heap.in_use(), heap.high_water())
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
