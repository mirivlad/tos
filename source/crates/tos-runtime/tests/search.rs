// SPDX-License-Identifier: GPL-3.0-or-later
//! What the allocator's search costs, and that the cost stops growing.
//!
//! The heap's first implementation searched first-fit by walking every block
//! from the base of the arena. Each result was correct and the cost of reaching
//! it grew with the number of blocks already there, so an allocation-heavy
//! frontend became superlinear in its input — measured as a 256 KiB module not
//! finishing in 900 seconds on the reference platform.
//!
//! "It is faster now" is not evidence that the shape is gone. The allocator
//! counts the free-list nodes it examines, so the claim being made here is
//! about *work per allocation as a function of how many blocks exist*, which is
//! the thing that was wrong. A scaling series that stays flat cannot be
//! produced by a search that walks everything.
//!
//! The adversarial patterns below are the ones that break a naive free list:
//! many small live blocks, alternating sizes, deliberate fragmentation, frees
//! in several orders, strong alignment, and an arena driven to the edge.

use std::alloc::Layout;

use tos_runtime::{BoundedHeap, RuntimeMemoryGrant, GRANT_VERSION};

const ARENA: usize = 8 * 1024 * 1024;

/// The unsafe obligations of driving a heap over a test-owned region, taken
/// once so no individual test has to restate them.
mod safely {
    use super::*;

    pub struct Arena {
        region: Vec<u8>,
        heap: BoundedHeap,
    }

    impl Arena {
        pub fn new(bytes: usize) -> Arena {
            let region = vec![0u8; bytes];
            let base = region.as_ptr() as usize;
            let aligned = base.div_ceil(4096) * 4096;
            let grant = RuntimeMemoryGrant {
                version: GRANT_VERSION,
                base: aligned,
                length: bytes - (aligned - base),
                alignment: 4096,
                identity: 0,
            };
            let mut heap = BoundedHeap::ungranted();
            // SAFETY: `region` owns those bytes for this arena's whole life, no
            // other reference to them is used while the heap holds them, and
            // the grant names a sub-range of them.
            unsafe { heap.adopt(&grant) }.expect("a well-formed grant");
            Arena { region, heap }
        }

        pub fn allocate(&mut self, size: usize, align: usize) -> Option<*mut u8> {
            let layout = Layout::from_size_align(size, align).expect("a valid layout");
            // SAFETY: the heap adopted a live grant in `new`.
            unsafe { self.heap.try_allocate(layout) }
        }

        pub fn free(&mut self, pointer: *mut u8) {
            // SAFETY: every pointer passed here came from `allocate` on this
            // heap and is freed exactly once.
            unsafe { self.heap.deallocate(pointer) }
        }

        pub fn search_work(&self) -> (u64, u64) {
            self.heap.search_work()
        }

        /// Free-list nodes examined per allocation served.
        pub fn probes_per_allocation(&self) -> f64 {
            let (probes, served) = self.search_work();
            probes as f64 / served.max(1) as f64
        }

        pub fn census(&self) -> (usize, usize) {
            self.heap.block_census()
        }

        pub fn committed(&self) -> usize {
            self.heap.committed()
        }

        pub fn peak_extent(&self) -> usize {
            self.heap.peak_extent()
        }
    }

    impl Drop for Arena {
        fn drop(&mut self) {
            // The region outlives the heap by construction; naming it here
            // keeps that visible rather than incidental.
            let _ = &self.region;
        }
    }
}

use safely::Arena;

/// The bound the search claims: work per allocation does not grow with the
/// number of blocks in the arena.
///
/// The series is the evidence. One measurement cannot distinguish a bounded
/// search from a linear one that happened to run on a small arena.
#[test]
fn search_work_per_allocation_does_not_grow_with_the_number_of_blocks() {
    let mut measured = Vec::new();
    for count in [64usize, 256, 1024, 4096] {
        let mut arena = Arena::new(ARENA);
        let mut live = Vec::with_capacity(count);
        for index in 0..count {
            // Alternating sizes, so the classes are genuinely populated rather
            // than one list of identical blocks.
            let size = 32 + (index % 7) * 24;
            live.push(arena.allocate(size, 8).expect("the arena has room"));
        }
        measured.push((count, arena.probes_per_allocation(), arena.census().0));
        for pointer in live {
            arena.free(pointer);
        }
    }

    for (count, per, blocks) in &measured {
        assert!(
            *per <= 8.0,
            "{count} live blocks: {per:.2} probes per allocation over {blocks} blocks"
        );
    }
    // The old search walked every block, so its cost would rise with the arena.
    let smallest = measured.first().expect("a series").1;
    let largest = measured.last().expect("a series").1;
    assert!(
        largest <= smallest.max(1.0) * 2.0,
        "search work grew from {smallest:.2} to {largest:.2} as the arena filled"
    );
}

/// The same series, stated as the ratio the old implementation could not hold.
#[test]
fn sixty_four_times_the_blocks_does_not_cost_sixty_four_times_the_search() {
    let mut small = Arena::new(ARENA);
    let mut kept = Vec::new();
    for index in 0..64 {
        kept.push(small.allocate(48 + (index % 5) * 16, 8).expect("room"));
    }
    let small_cost = small.probes_per_allocation();

    let mut large = Arena::new(ARENA);
    let mut held = Vec::new();
    for index in 0..4096 {
        held.push(large.allocate(48 + (index % 5) * 16, 8).expect("room"));
    }
    let large_cost = large.probes_per_allocation();

    // A walk-every-block search would be about 64x here. Anything near that is
    // the defect returning.
    assert!(
        large_cost < small_cost * 4.0 + 4.0,
        "64x the blocks cost {large_cost:.2} probes against {small_cost:.2}"
    );
    for pointer in kept {
        small.free(pointer);
    }
    for pointer in held {
        large.free(pointer);
    }
}

#[test]
fn many_small_live_allocations_stay_correct_and_cheap() {
    let mut arena = Arena::new(ARENA);
    let mut live = Vec::new();
    for index in 0..8192 {
        let pointer = arena.allocate(24, 8).expect("the arena has room");
        // SAFETY: the pointer addresses at least 24 writable bytes.
        unsafe { pointer.write_bytes((index % 251) as u8, 24) };
        live.push((pointer, (index % 251) as u8));
    }
    for (pointer, written) in &live {
        // SAFETY: as above; the allocation is still live.
        let observed = unsafe { pointer.read() };
        assert_eq!(observed, *written, "an allocation was overwritten");
    }
    assert!(arena.probes_per_allocation() <= 8.0);
    for (pointer, _) in live {
        arena.free(pointer);
    }
}

#[test]
fn alternating_sizes_and_mixed_free_orders_reclaim_completely() {
    let mut arena = Arena::new(ARENA);
    let start = arena.census();
    for round in 0..64 {
        let mut live = Vec::new();
        for index in 0..128 {
            let size = if index % 2 == 0 { 32 } else { 512 };
            live.push(arena.allocate(size, 8).expect("room"));
        }
        // A different order every round: forward, backward, evens then odds.
        match round % 3 {
            0 => {
                for pointer in live {
                    arena.free(pointer);
                }
            }
            1 => {
                for pointer in live.into_iter().rev() {
                    arena.free(pointer);
                }
            }
            _ => {
                let (evens, odds): (Vec<_>, Vec<_>) = live
                    .into_iter()
                    .enumerate()
                    .partition(|(index, _)| index % 2 == 0);
                for (_, pointer) in evens {
                    arena.free(pointer);
                }
                for (_, pointer) in odds {
                    arena.free(pointer);
                }
            }
        }
        assert_eq!(
            arena.census(),
            start,
            "round {round} did not return the arena to one free block"
        );
        assert_eq!(arena.committed(), 0, "round {round} leaked");
    }
}

#[test]
fn heavy_fragmentation_still_coalesces_back_to_one_block() {
    let mut arena = Arena::new(ARENA);
    let start = arena.census();
    let mut live = Vec::new();
    for index in 0..2048 {
        live.push(arena.allocate(16 + (index % 13) * 32, 8).expect("room"));
    }
    // Free every other block: the worst case for coalescing, because no two
    // freed blocks touch.
    let mut remaining = Vec::new();
    for (index, pointer) in live.into_iter().enumerate() {
        if index % 2 == 0 {
            arena.free(pointer);
        } else {
            remaining.push(pointer);
        }
    }
    let fragmented = arena.census();
    assert!(
        fragmented.1 > 500,
        "the fixture must actually fragment: {fragmented:?}"
    );
    // Allocation still works while the arena is in pieces.
    let probe = arena
        .allocate(64, 8)
        .expect("a fragmented arena still serves");
    arena.free(probe);
    for pointer in remaining {
        arena.free(pointer);
    }
    assert_eq!(
        arena.census(),
        start,
        "the arena did not coalesce back after heavy fragmentation"
    );
    assert_eq!(arena.committed(), 0);
}

#[test]
fn strong_alignment_is_served_from_every_class() {
    let mut arena = Arena::new(ARENA);
    let mut live = Vec::new();
    for align in [16usize, 64, 256, 1024, 4096] {
        for size in [1usize, 17, 300, 5000] {
            let pointer = arena
                .allocate(size, align)
                .unwrap_or_else(|| panic!("size {size} align {align}"));
            assert_eq!(pointer as usize % align, 0, "size {size} align {align}");
            // SAFETY: the pointer addresses at least `size` writable bytes.
            unsafe { pointer.write_bytes(0xA5, size) };
            live.push(pointer);
        }
    }
    for pointer in live {
        arena.free(pointer);
    }
    assert_eq!(arena.committed(), 0);
}

#[test]
fn an_arena_driven_to_the_edge_serves_what_it_can_and_refuses_the_rest() {
    // The one place the search is allowed to be linear is here: when no larger
    // class holds anything, the exact class is finished rather than refusing
    // while a block that fits is still in the arena.
    let mut arena = Arena::new(256 * 1024);
    let mut live = Vec::new();
    while let Some(pointer) = arena.allocate(96, 8) {
        live.push(pointer);
    }
    assert!(live.len() > 100, "the fixture must fill the arena");
    let filled = arena.committed();
    assert!(arena.allocate(96, 8).is_none(), "a full arena must refuse");

    // Refusal must not have damaged anything that was already live.
    assert_eq!(
        arena.committed(),
        filled,
        "a refusal changed the accounting"
    );
    for pointer in &live {
        // SAFETY: every pointer is still live and addresses 96 writable bytes.
        unsafe { pointer.write_bytes(0x5A, 96) };
    }
    let count = live.len();
    for pointer in live {
        arena.free(pointer);
    }
    assert_eq!(arena.committed(), 0);
    // And the arena serves again afterwards, at the same capacity.
    let mut again = 0;
    while arena.allocate(96, 8).is_some() {
        again += 1;
    }
    assert_eq!(again, count, "the arena did not come back to full capacity");
}

#[test]
fn repeated_rounds_leave_the_same_layout_and_the_same_search_cost() {
    // Accumulating fragmentation shows up in the block census long before it
    // shows up in the total, and a search that degraded over rounds would show
    // up here and nowhere else.
    let mut arena = Arena::new(ARENA);
    let mut settled: Option<((usize, usize), usize, usize)> = None;
    let mut previous_cost = 0.0f64;
    for round in 0..64 {
        let mut live = Vec::new();
        for index in 0..256 {
            live.push(arena.allocate(24 + (index % 11) * 48, 16).expect("room"));
        }
        for pointer in live.into_iter().rev() {
            arena.free(pointer);
        }
        let state = (arena.census(), arena.committed(), arena.peak_extent());
        match settled {
            None => settled = Some(state),
            Some(first) => assert_eq!(state, first, "round {round} changed the arena"),
        }
        let cost = arena.probes_per_allocation();
        if round > 1 {
            assert!(
                cost <= previous_cost.max(1.0) * 1.5,
                "round {round}: search cost rose to {cost:.2} from {previous_cost:.2}"
            );
        }
        previous_cost = cost;
    }
}
