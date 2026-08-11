// SPDX-License-Identifier: GPL-3.0-or-later
//! The bounded heap under the properties ADR-0041 requires of it.
//!
//! The interesting ones are not "allocation works" but the ones that decide
//! whether this can be a permanent recovery runtime: does memory actually come
//! back, does repeated execution return the arena to where it started, and does
//! exhaustion refuse rather than corrupt.

use core::alloc::Layout;

use tos_runtime::{BoundedHeap, GrantError, RuntimeMemoryGrant, GRANT_VERSION};

const ARENA: usize = 64 * 1024;

/// Host-owned backing memory standing in for the nucleus's grant.
///
/// A test has to get the bytes from somewhere; what matters is that the heap
/// receives a base and a length and never looks for memory itself.
struct Backing {
    bytes: Vec<u8>,
}

impl Backing {
    fn new(length: usize) -> Backing {
        Backing {
            bytes: vec![0u8; length + 64],
        }
    }

    fn grant(&mut self) -> RuntimeMemoryGrant {
        let raw = self.bytes.as_mut_ptr() as usize;
        let base = raw.div_ceil(64) * 64;
        RuntimeMemoryGrant {
            version: GRANT_VERSION,
            base,
            length: self.bytes.len() - (base - raw),
            alignment: 64,
            identity: 0x_705,
        }
    }
}

fn heap_over(backing: &mut Backing) -> BoundedHeap {
    let grant = backing.grant();
    let mut heap = BoundedHeap::ungranted();
    // SAFETY: `backing` owns the bytes for the whole test and nothing else
    // holds them, which is the promise the nucleus makes for a real grant.
    safely::adopt(&mut heap, &grant).expect("a well-formed grant is adopted");
    heap
}

/// The heap's unsafe surface, with its obligations discharged once.
///
/// Every test below drives the heap through these, so the reasoning lives in
/// one place instead of being restated at each call: the heap has adopted a
/// grant whose backing outlives it, every pointer passed to `release` came from
/// `claim` on the same heap, and none is used after release.
mod safely {
    use super::*;

    pub fn claim(heap: &mut BoundedHeap, size: usize) -> Option<*mut u8> {
        // SAFETY: the heap adopted a live grant in `heap_over`.
        unsafe { heap.try_allocate(layout(size)) }
    }

    pub fn claim_aligned(heap: &mut BoundedHeap, request: Layout) -> Option<*mut u8> {
        // SAFETY: as above.
        unsafe { heap.try_allocate(request) }
    }

    pub fn release(heap: &mut BoundedHeap, pointer: *mut u8) {
        // SAFETY: `pointer` came from `claim` on this heap and is live.
        unsafe { heap.deallocate(pointer) }
    }

    pub fn fill(pointer: *mut u8, byte: u8, size: usize) {
        // SAFETY: `pointer` addresses at least `size` writable bytes, which is
        // what `try_allocate` returned it for.
        unsafe { core::ptr::write_bytes(pointer, byte, size) }
    }

    pub fn first_byte(pointer: *mut u8) -> u8 {
        // SAFETY: `pointer` is a live allocation of at least one byte.
        unsafe { *pointer }
    }

    pub fn adopt(heap: &mut BoundedHeap, grant: &RuntimeMemoryGrant) -> Result<(), GrantError> {
        // SAFETY: every grant below either fails validation before any memory
        // is touched, or names backing this test owns.
        unsafe { heap.adopt(grant) }
    }
}

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, 8).unwrap()
}

#[test]
fn a_runtime_without_a_grant_has_no_memory() {
    // Not an edge case: it is the property that keeps memory discovery in the
    // nucleus. A runtime that could allocate without a grant would have found
    // memory somewhere.
    let mut heap = BoundedHeap::ungranted();
    assert!(safely::claim(&mut heap, 16).is_none());
    assert_eq!(heap.capacity(), 0);
}

#[test]
fn a_malformed_grant_is_refused_by_reason() {
    let mut heap = BoundedHeap::ungranted();
    let mut backing = Backing::new(ARENA);
    let good = backing.grant();

    let wrong_version = RuntimeMemoryGrant {
        version: 99,
        ..good
    };
    assert_eq!(
        safely::adopt(&mut heap, &wrong_version),
        Err(GrantError::UnsupportedVersion(99))
    );

    let unaligned = RuntimeMemoryGrant {
        base: good.base + 1,
        ..good
    };
    assert_eq!(
        safely::adopt(&mut heap, &unaligned),
        Err(GrantError::Unaligned)
    );

    let null = RuntimeMemoryGrant { base: 0, ..good };
    assert_eq!(safely::adopt(&mut heap, &null), Err(GrantError::Unaligned));

    let tiny = RuntimeMemoryGrant { length: 4, ..good };
    assert!(matches!(
        safely::adopt(&mut heap, &tiny),
        Err(GrantError::TooSmall { .. })
    ));

    // Aligned on purpose: alignment is checked first, so an unaligned base
    // would hide the overflow case rather than test it.
    let wrapping = RuntimeMemoryGrant {
        base: usize::MAX - 63,
        length: 128,
        ..good
    };
    assert_eq!(
        safely::adopt(&mut heap, &wrapping),
        Err(GrantError::Overflows)
    );
}

#[test]
fn allocations_are_distinct_writable_and_within_the_arena() {
    let mut backing = Backing::new(ARENA);
    let base = backing.grant().base;
    let mut heap = heap_over(&mut backing);

    let mut handed_out = Vec::new();
    for size in [8usize, 64, 256, 1024] {
        let pointer = safely::claim(&mut heap, size).expect("fits");
        // Writing the whole request proves the capacity is real, not nominal.
        safely::fill(pointer, 0xAB, size);
        let address = pointer as usize;
        assert!(address >= base && address + size <= base + heap.capacity() + 64);
        handed_out.push((address, size));
    }
    for (i, (one, one_size)) in handed_out.iter().enumerate() {
        for (other, other_size) in handed_out.iter().skip(i + 1) {
            let disjoint = one + one_size <= *other || other + other_size <= *one;
            assert!(disjoint, "two live allocations overlap");
        }
    }
}

#[test]
fn freed_memory_actually_comes_back() {
    // The property that separates this from a bump allocator: allocate the
    // whole arena, free it, and allocate the whole arena again.
    let mut backing = Backing::new(ARENA);
    let mut heap = heap_over(&mut backing);
    let big = heap.capacity() / 2;

    let first = safely::claim(&mut heap, big).expect("half the arena fits");
    safely::release(&mut heap, first);
    let second = safely::claim(&mut heap, big).expect("it comes back");
    safely::release(&mut heap, second);
    assert_eq!(
        heap.committed(),
        0,
        "nothing is held after everything is freed"
    );
}

#[test]
fn adjacent_free_blocks_coalesce_in_both_directions() {
    let mut backing = Backing::new(ARENA);
    let mut heap = heap_over(&mut backing);

    let a = safely::claim(&mut heap, 512).unwrap();
    let b = safely::claim(&mut heap, 512).unwrap();
    let c = safely::claim(&mut heap, 512).unwrap();

    // Free the outer two first, then the middle: the middle free must merge
    // with the block before it and the block after it in one step.
    safely::release(&mut heap, a);
    safely::release(&mut heap, c);
    safely::release(&mut heap, b);

    let (blocks, free) = heap.block_census();
    assert_eq!(
        (blocks, free),
        (1, 1),
        "three freed neighbours must become one free block"
    );
    // And the arena is one piece again: a request for all of it less the
    // per-allocation prefix must fit, which it could not if the three blocks
    // had stayed separate.
    let capacity = heap.capacity();
    let whole = safely::claim(&mut heap, capacity - 64);
    assert!(whole.is_some(), "a coalesced arena allocates as one block");
}

#[test]
fn repeated_execution_returns_the_arena_to_its_starting_state() {
    // ADR-0041 refuses an allocator that leaks between ordinary operations. A
    // thousand allocate-and-free cycles must leave the arena exactly as it
    // began, or a long-running recovery runtime would degrade.
    let mut backing = Backing::new(ARENA);
    let mut heap = heap_over(&mut backing);
    let start = heap.block_census();

    for round in 0..1000 {
        let sizes = [24usize, 300, 1000, 48];
        let mut live = Vec::new();
        for (index, size) in sizes.iter().enumerate() {
            let adjusted = size + (round % 7) * (index + 1);
            if let Some(pointer) = safely::claim(&mut heap, adjusted) {
                live.push(pointer);
            }
        }
        // Free in a different order each round, so coalescing is exercised from
        // both sides rather than always tail-first.
        if round % 2 == 0 {
            live.reverse();
        }
        for pointer in live {
            safely::release(&mut heap, pointer);
        }
    }

    assert_eq!(heap.committed(), 0);
    assert_eq!(
        heap.block_census(),
        start,
        "the arena must return to its starting layout, not merely to zero in use"
    );
}

#[test]
fn exhaustion_refuses_rather_than_corrupting() {
    let mut backing = Backing::new(ARENA);
    let mut heap = heap_over(&mut backing);

    let mut live = Vec::new();
    while let Some(pointer) = safely::claim(&mut heap, 1024) {
        safely::fill(pointer, 0x5A, 1024);
        live.push(pointer);
    }
    assert!(!live.is_empty(), "the arena held at least one allocation");
    // A refused request is a refusal, not a corrupted heap: everything still
    // live must still be intact and freeable.
    assert!(safely::claim(&mut heap, 1024).is_none());
    for pointer in &live {
        let byte = safely::first_byte(*pointer);
        assert_eq!(byte, 0x5A, "a live allocation was damaged by a refusal");
    }
    for pointer in live {
        safely::release(&mut heap, pointer);
    }
    assert_eq!(heap.committed(), 0);
    assert!(safely::claim(&mut heap, 1024).is_some());
}

#[test]
fn the_arena_bound_survives_frees() {
    let mut backing = Backing::new(ARENA);
    let mut heap = heap_over(&mut backing);
    let a = safely::claim(&mut heap, 1000).unwrap();
    let b = safely::claim(&mut heap, 2000).unwrap();
    safely::release(&mut heap, a);
    safely::release(&mut heap, b);
    assert_eq!(heap.committed(), 0);
    assert!(
        heap.peak_extent() >= 3000,
        "a bound that shrank on free would understate what an identical later \
         run needs, so it must err upward"
    );
}

#[test]
fn freeing_an_unsplit_block_must_not_eat_another_allocations_accounting() {
    // A request whose leftover is too small to be its own block keeps the whole
    // block. If the accounting adds the requested size but subtracts the block
    // size, freeing that allocation charges the difference to whatever else is
    // live — and the arena appears emptier than it is.
    let mut backing = Backing::new(ARENA);
    let mut heap = heap_over(&mut backing);

    // Fill the arena, then free one block so exactly one hole exists.
    let mut live = Vec::new();
    while let Some(pointer) = safely::claim(&mut heap, 96) {
        live.push(pointer);
    }
    assert!(live.len() >= 3);
    let hole = live.remove(1);
    safely::release(&mut heap, hole);

    let before = heap.committed();
    // Ask for slightly less than the hole: too little left over to split, so
    // the whole hole is occupied.
    let snug = safely::claim(&mut heap, 90).expect("the hole takes a smaller request");
    let after_claim = heap.committed();
    safely::release(&mut heap, snug);
    let after_release = heap.committed();

    assert!(
        after_claim > before,
        "claiming must raise the committed total"
    );
    assert_eq!(
        after_release, before,
        "releasing must return exactly what claiming took, not the block size"
    );
}

#[test]
fn the_arena_bound_accounts_for_metadata_and_fragmentation() {
    // `peak_extent` is what ADR-0041 sizes an arena from, so it must bound the
    // real footprint — tags, rounding and holes included — not the sum of the
    // payloads asked for.
    let mut backing = Backing::new(ARENA);
    let mut heap = heap_over(&mut backing);

    let mut live = Vec::new();
    let mut requested = 0usize;
    for _ in 0..8 {
        if let Some(pointer) = safely::claim(&mut heap, 100) {
            live.push(pointer);
            requested += 100;
        }
    }
    // Free every other one: the holes stay inside the used extent.
    for (index, pointer) in live.iter().enumerate() {
        if index % 2 == 0 {
            safely::release(&mut heap, *pointer);
        }
    }
    assert!(
        heap.peak_extent() > requested,
        "the bound must exceed the requested payload, since every block carries \
         metadata and a hole below the frontier is still arena the run needed"
    );
}

#[test]
fn a_strongly_aligned_request_is_served_correctly() {
    // A `GlobalAlloc` cannot assume its dependency closure never asks for more
    // than the block grain, so the alignment has to be real.
    let mut backing = Backing::new(ARENA);
    let mut heap = heap_over(&mut backing);
    for align in [16usize, 32, 64, 256, 4096] {
        let request = Layout::from_size_align(200, align).unwrap();
        let pointer = safely::claim_aligned(&mut heap, request)
            .unwrap_or_else(|| panic!("alignment {align} must be served"));
        assert_eq!(
            pointer as usize % align,
            0,
            "alignment {align} was not honoured"
        );
        safely::fill(pointer, 0xC3, 200);
        safely::release(&mut heap, pointer);
    }
    assert_eq!(heap.committed(), 0, "every aligned block came back");
}
