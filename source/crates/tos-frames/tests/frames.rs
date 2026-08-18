// SPDX-License-Identifier: GPL-3.0-or-later
//! The nucleus's frame allocator, tested against memory that exists.
//!
//! Every pool here is admitted over a real, frame-aligned host allocation, so
//! the tests exercise the pointer writes the allocator actually performs — the
//! released-frame links it threads through free frames, and the clearing
//! ADR-0050 section 3 requires. An allocator checked only against its own
//! arithmetic is checked against its own assumptions.

use std::alloc::{alloc, dealloc, Layout};

use tos_frames::{Frames, FRAME_SIZE, MAX_SPANS};
use tos_runtime::region::{GrantRefused, Span, MAX_GRANT, MIN_GRANT};
use tos_runtime::GRANT_VERSION;

/// Real, frame-aligned backing memory for a pool.
///
/// Deliberately **not** zeroed: firmware memory is not, and a pool that only
/// looks clean because its backing store was would prove nothing about the
/// clearing it performs.
struct Backing {
    base: *mut u8,
    layout: Layout,
}

impl Backing {
    fn new(bytes: usize) -> Backing {
        let layout = Layout::from_size_align(bytes, FRAME_SIZE as usize).expect("a valid layout");
        // SAFETY: the layout has a non-zero size and a power-of-two alignment,
        // and the returned pointer is checked for null before any use.
        let base = unsafe { alloc(layout) };
        assert!(!base.is_null(), "the host refused the backing allocation");
        // SAFETY: writing the whole allocation is in bounds of the object just
        // returned, and the pattern is what makes a later "cleared" assertion
        // mean something.
        unsafe { base.write_bytes(0xa5, bytes) };
        // The pool addresses frames as integers, the way the nucleus addresses
        // physical memory, so this allocation's provenance must be exposed for
        // those integers to be usable as pointers.
        base.expose_provenance();
        Backing { base, layout }
    }

    fn span(&self) -> Span {
        Span::new(
            self.base as u64,
            self.base as u64 + self.layout.size() as u64,
        )
    }

    fn at(&self, offset: u64) -> u64 {
        self.base as u64 + offset
    }
}

impl Drop for Backing {
    fn drop(&mut self) {
        // SAFETY: `base` and `layout` are exactly the pointer and layout of the
        // one allocation this value owns, and no pool outlives it.
        unsafe { dealloc(self.base, self.layout) };
    }
}

/// Reads the first eight bytes of a frame the pool handed out.
fn peek(frame: u64) -> u64 {
    // SAFETY: `frame` was handed out by a pool admitted over live backing
    // memory, so it is a mapped, frame-aligned address the caller owns.
    unsafe { std::ptr::with_exposed_provenance::<u64>(frame as usize).read() }
}

/// Writes a pattern over a whole frame the pool handed out.
fn scribble(frame: u64) {
    // SAFETY: as `peek`, and the caller owns the frame for the whole write.
    unsafe {
        std::ptr::with_exposed_provenance_mut::<u8>(frame as usize)
            .write_bytes(0x5c, FRAME_SIZE as usize)
    };
}

/// A pool over `bytes` of real memory, with nothing occupied inside it.
fn pool(backing: &Backing) -> Frames {
    let mut frames = Frames::new();
    // SAFETY: the span is the whole of one live host allocation owned by
    // `backing`, which outlives every use of the pool below, and nothing else
    // reads or writes it.
    let admission = unsafe { frames.admit([backing.span()], &[]) };
    assert_eq!(admission.refused, 0);
    frames
}

#[test]
fn a_map_becomes_a_pool_minus_everything_that_is_spoken_for() {
    let backing = Backing::new(64 * FRAME_SIZE as usize);
    let occupied = [Span::new(
        backing.at(8 * FRAME_SIZE),
        backing.at(12 * FRAME_SIZE),
    )];
    let mut frames = Frames::new();
    // SAFETY: the free span is the whole live allocation; the occupied span is
    // inside it and is not touched by the pool afterwards.
    let admission = unsafe { frames.admit([backing.span()], &occupied) };

    assert_eq!(
        admission.spans, 2,
        "the occupied span splits the pool in two"
    );
    assert_eq!(admission.frames, 60, "four frames are spoken for");
    assert_eq!(frames.total(), 60);
    assert_eq!(frames.available(), 60);

    // Nothing the pool hands out may fall inside what it was told to avoid.
    for _ in 0..60 {
        let frame = frames.allocate_frame().expect("the pool has frames");
        assert!(
            !occupied[0].holds(frame),
            "the pool handed out occupied memory at 0x{frame:x}"
        );
    }
    assert_eq!(
        frames.allocate_frame(),
        None,
        "the pool is empty, not generous"
    );
}

#[test]
fn one_frame_is_never_handed_to_two_owners() {
    let backing = Backing::new(32 * FRAME_SIZE as usize);
    let mut frames = pool(&backing);
    let mut handed = Vec::new();
    while let Some(frame) = frames.allocate_frame() {
        assert!(!handed.contains(&frame), "0x{frame:x} was handed out twice");
        handed.push(frame);
    }
    assert_eq!(handed.len(), 32);
    assert_eq!(frames.in_use(), 32);
    assert_eq!(frames.available(), 0);
}

#[test]
fn a_frame_is_clean_whether_it_is_new_or_reused() {
    let backing = Backing::new(4 * FRAME_SIZE as usize);
    let mut frames = pool(&backing);

    // Never handed out before: the backing memory holds 0xa5, and the pool
    // clears it anyway. A frame carrying what firmware left in it would be a
    // disclosure channel with no owner.
    let first = frames.allocate_frame().expect("a frame");
    assert_eq!(peek(first), 0, "a first-use frame was not cleared");

    scribble(first);
    assert_ne!(peek(first), 0, "the test wrote nothing to reclaim");
    // SAFETY: `first` came from this pool, nothing references it after the
    // scribble above, and it has not been released before.
    unsafe { frames.release_frame(first) };

    let again = frames
        .allocate_frame()
        .expect("the released frame comes back");
    assert_eq!(
        again, first,
        "a released frame is reused before untouched memory"
    );
    assert_eq!(
        peek(again),
        0,
        "a reused frame still carried its old contents"
    );
}

#[test]
fn released_frames_come_back_in_the_order_they_were_given_up() {
    let backing = Backing::new(8 * FRAME_SIZE as usize);
    let mut frames = pool(&backing);
    let a = frames.allocate_frame().expect("a frame");
    let b = frames.allocate_frame().expect("a frame");
    assert_eq!(frames.in_use(), 2);

    // SAFETY: both frames came from this pool, nothing references them, and
    // neither has been released before.
    unsafe {
        frames.release_frame(a);
        frames.release_frame(b);
    }
    assert_eq!(frames.in_use(), 0);
    assert_eq!(frames.available(), 8);

    assert_eq!(frames.allocate_frame(), Some(b));
    assert_eq!(frames.allocate_frame(), Some(a));
    assert_eq!(frames.in_use(), 2);
}

#[test]
fn a_carve_is_contiguous_aligned_and_never_made_of_released_frames() {
    let backing = Backing::new(16 * FRAME_SIZE as usize);
    let mut frames = pool(&backing);

    let run = frames
        .carve(4 * FRAME_SIZE, 2 * FRAME_SIZE)
        .expect("the pool has room");
    assert_eq!(run.length(), 4 * FRAME_SIZE);
    assert_eq!(
        run.start % (2 * FRAME_SIZE),
        0,
        "the carve ignored its alignment"
    );

    // Take everything that is left one frame at a time, then give a frame back.
    // The pool now holds a released frame and no untouched memory, and a carve
    // must still refuse: satisfying it would mean defragmenting, which nothing
    // here implements and nothing here claims.
    while frames.allocate_frame().is_some() {}
    let spare = run.start;
    // SAFETY: `spare` is the first frame of a run carved from this pool; the
    // rest of the run is not released, and nothing references this frame.
    unsafe { frames.release_frame(spare) };
    assert_eq!(frames.available(), 1);
    assert_eq!(frames.carve(FRAME_SIZE, FRAME_SIZE), None);
    assert_eq!(
        frames.allocate_frame(),
        Some(spare),
        "the released frame is still available one frame at a time"
    );
}

#[test]
fn a_released_run_returns_frame_by_frame_and_comes_back_clean() {
    let backing = Backing::new(16 * FRAME_SIZE as usize);
    let mut frames = pool(&backing);
    let run = frames.carve(4 * FRAME_SIZE, FRAME_SIZE).expect("room");
    assert_eq!(frames.in_use(), 4);

    // A carve is not cleared when it is made: its caller takes memory nobody
    // has seen. It is cleared when it comes back, which is the moment another
    // owner could see it.
    scribble(run.start);
    // SAFETY: the run was carved from this pool, nothing references it, and no
    // part of it has been released.
    unsafe { frames.release(run) };
    assert_eq!(frames.in_use(), 0);
    assert_eq!(frames.available(), 16);

    let reused = frames.allocate_frame().expect("a frame");
    assert!(run.holds(reused), "the run's frames did not come back");
    assert_eq!(
        peek(reused),
        0,
        "a reclaimed frame carried its old contents"
    );
}

#[test]
fn the_same_piece_offered_twice_is_admitted_once() {
    // Two occupied spans ending at the same address offer the piece after them
    // twice. Admitting it twice would hand the same physical memory to two
    // owners — the one bookkeeping error this allocator cannot survive.
    let backing = Backing::new(16 * FRAME_SIZE as usize);
    let occupied = [
        Span::new(backing.at(0), backing.at(4 * FRAME_SIZE)),
        Span::new(backing.at(2 * FRAME_SIZE), backing.at(4 * FRAME_SIZE)),
    ];
    let mut frames = Frames::new();
    // SAFETY: the free span is the whole live allocation and the occupied spans
    // are inside it; the pool never touches them.
    let admission = unsafe { frames.admit([backing.span()], &occupied) };

    assert_eq!(admission.spans, 1);
    assert_eq!(admission.frames, 12);

    let mut handed = Vec::new();
    while let Some(frame) = frames.allocate_frame() {
        assert!(!handed.contains(&frame), "0x{frame:x} was handed out twice");
        handed.push(frame);
    }
    assert_eq!(handed.len(), 12);
}

#[test]
fn memory_past_the_fixed_bound_is_refused_rather_than_dropped_quietly() {
    // A map with more pieces than the nucleus sizes its array for. The surplus
    // stays outside the pool — unused, not misused — and is counted, because a
    // pool that silently held less than the machine has would make an
    // exhausted system look like a small one.
    let pieces = MAX_SPANS + 4;
    let backing = Backing::new(pieces * 2 * FRAME_SIZE as usize);
    let occupied: Vec<Span> = (0..pieces)
        .map(|index| {
            let base = backing.at((index as u64) * 2 * FRAME_SIZE + FRAME_SIZE);
            Span::new(base, base + FRAME_SIZE)
        })
        .collect();
    let mut frames = Frames::new();
    // SAFETY: the free span is the whole live allocation; the occupied spans
    // are inside it and are never handed out.
    let admission = unsafe { frames.admit([backing.span()], &occupied) };

    assert_eq!(admission.spans, MAX_SPANS);
    assert_eq!(admission.refused, 4);
    assert_eq!(admission.frames, MAX_SPANS as u64);
}

#[test]
fn a_grant_is_a_v1_grant_carved_from_the_pool() {
    let backing = Backing::new(MIN_GRANT * 2);
    let mut frames = pool(&backing);
    let grant = frames
        .grant(0x1234_5678)
        .expect("the pool can back a grant");

    assert_eq!(
        grant.version, GRANT_VERSION,
        "the grant contract is still V1"
    );
    assert_eq!(grant.identity, 0x1234_5678, "a grant names who made it");
    assert_eq!(grant.length, MIN_GRANT * 2, "the pool granted what it had");
    assert!(grant.length <= MAX_GRANT);
    assert_eq!(grant.base as u64 % grant.alignment as u64, 0);
    assert!(backing.span().holds(grant.base as u64));
    assert_eq!(frames.available(), 0, "the grant is accounted for");
}

#[test]
fn a_grant_never_overlaps_anything_the_pool_was_told_about() {
    // The shape a real boot produces: the nucleus sits inside one large
    // conventional range and its `.bss` past the loaded image is still reported
    // free, so a grant that trusted the map alone would land on top of it.
    let backing = Backing::new(MIN_GRANT * 3);
    let occupied = [
        Span::new(backing.at(0), backing.at(MIN_GRANT as u64)),
        Span::new(
            backing.at(2 * MIN_GRANT as u64),
            backing.at(2 * MIN_GRANT as u64 + FRAME_SIZE),
        ),
    ];
    let mut frames = Frames::new();
    // SAFETY: the free span is the whole live allocation and the occupied spans
    // are inside it; the pool never hands them out.
    unsafe { frames.admit([backing.span()], &occupied) };
    let grant = frames.grant(7).expect("a grant is possible");

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
}

#[test]
fn every_occupied_span_is_avoided_however_they_are_ordered() {
    // Occupied spans arrive in whatever order a caller assembled them, so
    // admission must not assume they are sorted.
    let backing = Backing::new(64 * FRAME_SIZE as usize);
    let occupied = [
        Span::new(backing.at(40 * FRAME_SIZE), backing.at(44 * FRAME_SIZE)),
        Span::new(backing.at(2 * FRAME_SIZE), backing.at(3 * FRAME_SIZE)),
        Span::new(backing.at(20 * FRAME_SIZE), backing.at(21 * FRAME_SIZE)),
    ];
    let mut frames = Frames::new();
    // SAFETY: the free span is the whole live allocation and every occupied
    // span is inside it.
    unsafe { frames.admit([backing.span()], &occupied) };

    while let Some(frame) = frames.allocate_frame() {
        for taken in &occupied {
            assert!(!taken.holds(frame), "the pool handed out 0x{frame:x}");
        }
    }
}

#[test]
fn a_free_range_entirely_covered_by_an_occupied_span_yields_nothing() {
    let backing = Backing::new(4 * FRAME_SIZE as usize);
    let occupied = [backing.span()];
    let mut frames = Frames::new();
    // SAFETY: the free span is the whole live allocation, and it is entirely
    // occupied, so the pool never touches it.
    let admission = unsafe { frames.admit([backing.span()], &occupied) };
    assert_eq!(admission.spans, 0);
    assert_eq!(admission.frames, 0);
    assert_eq!(frames.allocate_frame(), None);
}

#[test]
fn the_grant_is_capped_even_when_the_machine_is_large() {
    // A cap, not a target (`MAX_GRANT`): granting all of memory would make an
    // over-allocating run look healthy right up to the point where it was not,
    // and would leave the pool with nothing to build a process out of.
    let backing = Backing::new(MAX_GRANT + 16 * MIN_GRANT);
    let mut frames = pool(&backing);
    let grant = frames.grant(0).expect("a grant is possible");
    assert_eq!(grant.length, MAX_GRANT);
    assert!(
        frames.available() > 0,
        "the grant took the pool with it, leaving nothing for a process"
    );
}

#[test]
fn a_granted_region_is_one_the_heap_accepts() {
    // The two halves of ADR-0041 have to agree: a region the nucleus chooses
    // must be one `BoundedHeap::adopt` will take, or the nucleus and the
    // runtime would each be correct on their own and unusable together.
    let backing = Backing::new(MIN_GRANT * 2);
    let mut frames = pool(&backing);
    let grant = frames.grant(1).expect("a grant is possible");
    assert!(backing.span().holds(grant.base as u64));
    assert!(grant.base as u64 + grant.length as u64 <= backing.span().end);

    let mut heap = tos_runtime::BoundedHeap::ungranted();
    // SAFETY: `backing` owns those bytes for the rest of this test, the grant
    // names a sub-range of them carved by the pool, and nothing else reads or
    // writes that range while the heap holds it.
    unsafe { heap.adopt(&grant) }.expect("the nucleus and the heap agree");
    // SAFETY: the heap adopted a live grant just above.
    let pointer = unsafe { heap.try_allocate(Layout::from_size_align(64, 64).unwrap()) };
    assert!(pointer.is_some(), "a granted region must actually serve");
}

#[test]
fn a_pool_too_small_to_back_a_grant_says_so_instead_of_granting_less() {
    let backing = Backing::new(MIN_GRANT / 2);
    let mut frames = pool(&backing);
    assert_eq!(
        frames.grant(0),
        Err(GrantRefused::TooSmall(MIN_GRANT as u64 / 2))
    );
}

#[test]
fn an_empty_pool_grants_nothing() {
    let mut frames = Frames::new();
    assert_eq!(frames.total(), 0);
    assert_eq!(frames.allocate_frame(), None);
    assert_eq!(frames.grant(0), Err(GrantRefused::NoRegion));
}
