// SPDX-License-Identifier: GPL-3.0-or-later
//! Choosing a grant that never overlaps memory somebody else owns.

use tos_runtime::region::{
    derive, largest_free, GrantRefused, Span, GRANT_ALIGNMENT, MAX_GRANT, MIN_GRANT,
};
use tos_runtime::GRANT_VERSION;

const MIB: u64 = 1024 * 1024;

#[test]
fn a_span_inside_a_free_range_splits_it_instead_of_discarding_it() {
    // This is the shape a real boot produces: the nucleus sits in the middle of
    // one large conventional range, and its `.bss` past the loaded file image
    // is still reported free. Discarding the whole range would leave nothing to
    // grant on a machine that plainly has memory.
    let free = [Span::new(MIB, 201 * MIB)];
    let occupied = [Span::new(32 * MIB, 36 * MIB)];
    let region = largest_free(free, &occupied).expect("a free piece exists");
    assert_eq!(
        region.start,
        36 * MIB,
        "the piece past the nucleus is larger"
    );
    assert_eq!(region.length(), 165 * MIB);
}

#[test]
fn the_piece_before_an_occupied_span_wins_when_it_is_the_larger_one() {
    let free = [Span::new(MIB, 201 * MIB)];
    let occupied = [Span::new(180 * MIB, 184 * MIB)];
    let region = largest_free(free, &occupied).expect("a free piece exists");
    assert_eq!(region.start, MIB);
    assert_eq!(region.end, 180 * MIB);
}

#[test]
fn a_grant_never_overlaps_anything_it_was_told_about() {
    let free = [Span::new(MIB, 201 * MIB)];
    let occupied = [
        Span::new(32 * MIB, 36 * MIB),
        Span::new(100 * MIB, 101 * MIB),
    ];
    let grant = derive(free, &occupied, 7).expect("a grant is possible");
    let base = grant.base as u64;
    let end = base + grant.length as u64;
    for taken in &occupied {
        assert!(
            end <= taken.start || base >= taken.end,
            "grant {base:#x}..{end:#x} overlaps {:#x}..{:#x}",
            taken.start,
            taken.end
        );
    }
    assert_eq!(grant.identity, 7);
    assert_eq!(grant.version, GRANT_VERSION);
    assert_eq!(grant.alignment, GRANT_ALIGNMENT);
}

#[test]
fn every_occupied_span_is_avoided_however_they_are_ordered() {
    // Occupied spans arrive in whatever order a caller assembled them, so the
    // search must not assume they are sorted.
    let free = [Span::new(0, 400 * MIB)];
    let occupied = [
        Span::new(300 * MIB, 310 * MIB),
        Span::new(10 * MIB, 12 * MIB),
        Span::new(200 * MIB, 201 * MIB),
    ];
    let grant = derive(free, &occupied, 0).expect("a grant is possible");
    let base = grant.base as u64;
    let end = base + grant.length as u64;
    for taken in &occupied {
        assert!(
            end <= taken.start || base >= taken.end,
            "{base:#x}..{end:#x}"
        );
    }
}

#[test]
fn a_region_too_small_to_run_in_is_refused_rather_than_granted() {
    let free = [Span::new(MIB, 5 * MIB)];
    assert_eq!(derive(free, &[], 0), Err(GrantRefused::TooSmall(4 * MIB)));
    assert!(4 * MIB < MIN_GRANT as u64);
}

#[test]
fn no_free_memory_at_all_is_refused() {
    assert_eq!(derive([], &[], 0), Err(GrantRefused::NoRegion));
}

#[test]
fn a_free_range_entirely_covered_by_an_occupied_span_yields_nothing() {
    let free = [Span::new(32 * MIB, 36 * MIB)];
    let occupied = [Span::new(32 * MIB, 36 * MIB)];
    assert_eq!(largest_free(free, &occupied), None);
}

#[test]
fn the_grant_is_capped_even_when_the_machine_is_large() {
    let free = [Span::new(MIB, 4097 * MIB)];
    let grant = derive(free, &[], 0).expect("a grant is possible");
    assert_eq!(grant.length, MAX_GRANT);
    assert!(grant.base.is_multiple_of(GRANT_ALIGNMENT));
}

#[test]
fn a_granted_region_is_one_the_heap_accepts() {
    // The two halves of ADR-0041 have to agree: a region this module chooses
    // must be one `BoundedHeap::adopt` will take, or the nucleus and the
    // runtime would each be correct on their own and unusable together.
    let arena = vec![0u8; 16 * MIB as usize];
    let base = arena.as_ptr() as u64;
    let free = [Span::sized(base, arena.len() as u64).expect("no wrap")];
    let grant = derive(free, &[], 1).expect("a grant is possible");
    assert!(grant.base as u64 >= base);
    assert!(grant.base as u64 + grant.length as u64 <= base + arena.len() as u64);

    let mut heap = tos_runtime::BoundedHeap::ungranted();
    // SAFETY: `arena` owns those bytes for the rest of this test, the grant
    // names a sub-range of them, and no other reference to that range is used
    // while the heap holds it.
    unsafe { heap.adopt(&grant) }.expect("the nucleus and the heap agree");
    // SAFETY: the heap adopted a live grant just above.
    let pointer =
        unsafe { heap.try_allocate(std::alloc::Layout::from_size_align(64, 64).unwrap()) };
    assert!(pointer.is_some(), "a granted region must actually serve");
    drop(arena);
}

#[test]
fn spans_that_would_wrap_the_address_space_are_not_representable() {
    assert_eq!(Span::sized(u64::MAX - 3, 8), None);
    assert_eq!(
        Span::sized(u64::MAX - 8, 8),
        Some(Span::new(u64::MAX - 8, u64::MAX))
    );
}
