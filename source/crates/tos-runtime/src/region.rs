// SPDX-License-Identifier: GPL-3.0-or-later
//! What a granted region is, and what it may never be.
//!
//! ADR-0041 puts memory discovery in the nucleus and hands the runtime a single
//! bounded region. This module holds the vocabulary both sides of that contract
//! need — a physical span, the bounds a grant must satisfy, and the reasons a
//! grant can be refused — kept apart from whoever knows the machine. It has no
//! idea what a memory map, a firmware table or a boot record is, which is what
//! keeps `tos-runtime` free of the boot ABI.
//!
//! **Who chooses the region.** Until Stage 3 this module also chose it, by
//! taking the largest hole in the map. ADR-0050 section 1 moves that decision
//! to the nucleus's frame allocator (`tos-frames`), because a system with many
//! processes needs an owner of physical frames rather than one derivation
//! performed once. The bounds below did not change hands with it: they are what
//! a grant *is*, not who hands it out.

/// Alignment guaranteed for the base of a granted region.
pub const GRANT_ALIGNMENT: usize = 4096;

/// The largest region that will be granted.
///
/// A cap, not a target. The reference path's measured need is far smaller
/// (docs/evidence/STAGE2_ARENA_BOUND.md); granting all of memory would make an
/// over-allocating run look healthy right up to the point where it was not.
pub const MAX_GRANT: usize = 96 * 1024 * 1024;

/// The smallest region that will be granted.
///
/// Below this the reference path cannot be expected to run at all, and running
/// it anyway would report an implementation exhaustion as though it were a fact
/// about the module being run.
pub const MIN_GRANT: usize = 8 * 1024 * 1024;

/// A half-open physical span.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

impl Span {
    pub fn new(start: u64, end: u64) -> Span {
        Span { start, end }
    }

    /// The span starting at `start` with `length` bytes, unless that wraps.
    pub fn sized(start: u64, length: u64) -> Option<Span> {
        Some(Span {
            start,
            end: start.checked_add(length)?,
        })
    }

    pub fn holds(&self, address: u64) -> bool {
        self.start <= address && address < self.end
    }

    pub fn length(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }
}

/// Why no grant could be made.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrantRefused {
    /// No free span avoids everything already spoken for.
    NoRegion,
    /// The largest such span is smaller than [`MIN_GRANT`].
    TooSmall(u64),
}
