// SPDX-License-Identifier: GPL-3.0-or-later
//! The vocabulary of a granted region.
//!
//! Choosing the region moved to the nucleus's frame allocator with ADR-0050
//! section 1, and the properties of that choice — never overlapping occupied
//! memory, refusing rather than granting less than a runtime can use, and
//! producing a region `BoundedHeap::adopt` accepts — are tested where the
//! choice is now made, in `tos-frames`. What is left here is what a span is.

use tos_runtime::region::Span;

#[test]
fn spans_that_would_wrap_the_address_space_are_not_representable() {
    assert_eq!(Span::sized(u64::MAX - 3, 8), None);
    assert_eq!(
        Span::sized(u64::MAX - 8, 8),
        Some(Span::new(u64::MAX - 8, u64::MAX))
    );
}

#[test]
fn a_span_holds_its_start_and_stops_before_its_end() {
    let span = Span::new(0x1000, 0x2000);
    assert!(span.holds(0x1000));
    assert!(span.holds(0x1fff));
    assert!(!span.holds(0x2000), "a span is half-open");
    assert_eq!(span.length(), 0x1000);
}
