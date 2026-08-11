// SPDX-License-Identifier: GPL-3.0-or-later
//! Choosing the one region a runtime is granted.
//!
//! ADR-0041 puts memory discovery in the nucleus and hands the runtime a single
//! bounded region. This module is the *choosing*, kept apart from whoever knows
//! the machine: it is given free spans and spans that are already spoken for,
//! and returns a grant. It has no idea what a memory map, a firmware table or a
//! boot record is, which is what keeps `tos-runtime` free of the boot ABI while
//! still owning the rule that a grant may never overlap live memory.
//!
//! **No allocation.** This runs before any heap exists. It builds no
//! collection, keeps no scratch buffer, and consumes the free spans in one
//! pass.

use crate::{RuntimeMemoryGrant, GRANT_VERSION};

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

/// The largest free piece that avoids every occupied span.
///
/// Only the largest piece matters, so the search never builds the set of
/// pieces: every free piece begins either at a free span's start or at the end
/// of an occupied span, and runs to the next occupied start or the free span's
/// end. That is a bounded scan with no storage, which is what a pre-heap
/// caller needs.
pub fn largest_free(free: impl IntoIterator<Item = Span>, occupied: &[Span]) -> Option<Span> {
    let mut best: Option<Span> = None;
    for span in free {
        let mut consider = |start: u64| {
            if start >= span.end || occupied.iter().any(|taken| taken.holds(start)) {
                return;
            }
            let mut end = span.end;
            for taken in occupied {
                if taken.start > start && taken.start < end {
                    end = taken.start;
                }
            }
            let base = align_up(start, GRANT_ALIGNMENT as u64);
            if base >= end {
                return;
            }
            let length = (end - base) / GRANT_ALIGNMENT as u64 * GRANT_ALIGNMENT as u64;
            if length == 0 {
                return;
            }
            let piece = Span::new(base, base + length);
            if best.is_none_or(|current| piece.length() > current.length()) {
                best = Some(piece);
            }
        };
        consider(span.start);
        for taken in occupied {
            if taken.end > span.start && taken.end < span.end {
                consider(taken.end);
            }
        }
    }
    best
}

fn align_up(value: u64, to: u64) -> u64 {
    value.div_ceil(to) * to
}

/// Derives the one region a runtime is granted.
///
/// `identity` names the build making the grant, so a runtime that records what
/// it was given also records who gave it.
pub fn derive(
    free: impl IntoIterator<Item = Span>,
    occupied: &[Span],
    identity: u64,
) -> Result<RuntimeMemoryGrant, GrantRefused> {
    let region = largest_free(free, occupied).ok_or(GrantRefused::NoRegion)?;
    if region.length() < MIN_GRANT as u64 {
        return Err(GrantRefused::TooSmall(region.length()));
    }
    Ok(RuntimeMemoryGrant {
        version: GRANT_VERSION,
        base: region.start as usize,
        length: region.length().min(MAX_GRANT as u64) as usize,
        alignment: GRANT_ALIGNMENT,
        identity,
    })
}
