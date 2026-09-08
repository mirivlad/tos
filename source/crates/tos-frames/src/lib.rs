// SPDX-License-Identifier: GPL-3.0-or-later
//! The physical frames the nucleus owns (ADR-0050 section 1).
//!
//! ADR-0041 settled who owns memory when there is one runtime: the nucleus
//! grants and the runtime never discovers. Stage 3 keeps that property and adds
//! plurality — many address spaces, many grants, created and destroyed while
//! the system runs — and plurality needs an owner of physical frames.
//!
//! This is that owner. It is given the free spans of the validated memory map
//! and the spans already spoken for, and from then on it is the only thing in
//! the system that decides which physical memory is in use. **The subtraction
//! does not weaken**: memory a process could write over is not protected by one
//! component's bookkeeping being correct, so everything the Stage 2 derivation
//! subtracted — the nucleus image with its `.bss`, the capsule, the handoff
//! record, the converted map, the framebuffer, the nucleus stack — is
//! subtracted here too, and the pool is what is left.
//!
//! **No allocation, no heap.** This runs before any heap exists and continues
//! to run underneath every heap in the system. It keeps its spans in a fixed
//! array sized by a nucleus constant, and it keeps its list of released frames
//! *inside the released frames*, which are by definition not in use.
//!
//! **Two ways out, and they are not the same.** [`Frames::allocate_frame`]
//! hands out one 4 KiB frame at a time and is what an address space, a page
//! table or a per-process grant is built from. [`Frames::carve`] takes a
//! physically contiguous run and is for the few structures that must be
//! contiguous because nothing maps them yet — at boot, before paging, that is
//! how the Stage 2 heap grant is made. A carve is never satisfied from released
//! frames: the pool would have to defragment to promise that, and a promise
//! nothing implements is worse than an absent one.
//!
//! **Clearing, and where it is honest to claim it.** A released frame is
//! cleared when it is released (ADR-0050 section 3), and an allocated frame is
//! cleared again before it is handed out, so a frame is clean whether it comes
//! from the free list or from memory this pool has never handed out. A carve is
//! **not** cleared, and says so: its one caller at boot takes memory no process
//! has ever seen, and clearing 96 MiB on every boot would buy nothing but
//! seconds. When a carve is released, its frames go back through the release
//! path and are cleared there — which is the point at which another owner could
//! see them.

#![no_std]

use tos_runtime::region::{GrantRefused, Span, GRANT_ALIGNMENT, MAX_GRANT, MIN_GRANT};
use tos_runtime::{RuntimeMemoryGrant, GRANT_VERSION};

/// The size of one physical frame.
pub const FRAME_SIZE: u64 = 4096;

/// How many disjoint pieces of the memory map the pool will hold.
///
/// A fixed bound, sized for a machine's map and not derived from the map: the
/// nucleus must not size an array from a number the firmware chose. A map
/// offering more pieces than this leaves the surplus **outside** the pool,
/// which is memory unused rather than memory misused, and [`Admission::refused`]
/// reports it rather than letting it disappear quietly.
pub const MAX_SPANS: usize = 64;

/// One admitted piece of physical memory, consumed from `frontier` upward.
#[derive(Clone, Copy)]
struct Piece {
    frontier: u64,
    end: u64,
}

impl Piece {
    const EMPTY: Piece = Piece {
        frontier: 0,
        end: 0,
    };

    fn room(&self) -> u64 {
        self.end.saturating_sub(self.frontier)
    }
}

/// What admitting a memory map produced.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Admission {
    /// Pieces admitted to the pool.
    pub spans: usize,
    /// Pieces the fixed bound could not admit, and which are therefore not
    /// part of the pool.
    pub refused: usize,
    /// Frames admitted.
    pub frames: u64,
}

/// How many returned contiguous runs the pool will hold for re-carving.
///
/// **A bound, not a heap** (ADR-0084 §6a). DMA backing must be physically
/// contiguous, and a `carve` that could only ever move a frontier forward would
/// make every DMA region a permanent loss of contiguous memory — which would
/// make the quarantine that returns them achieve nothing. This table is what a
/// returned run comes back through, and it is fixed-size for the same reason
/// every other nucleus table is.
pub const MAX_RETURNED_RUNS: usize = 8;

/// One contiguous run that was handed out and given back.
#[derive(Clone, Copy)]
struct Returned {
    start: u64,
    end: u64,
}

impl Returned {
    const EMPTY: Self = Self { start: 0, end: 0 };

    fn bytes(&self) -> u64 {
        self.end - self.start
    }

    fn live(&self) -> bool {
        self.end > self.start
    }
}

/// The nucleus's physical frame allocator.
pub struct Frames {
    pieces: [Piece; MAX_SPANS],
    count: usize,
    /// Contiguous runs returned by [`Frames::release_run`], available to
    /// [`Frames::carve`] again. **Deliberately separate from the released-frame
    /// list**: that list threads a link through each frame and loses adjacency,
    /// which is exactly the property a contiguous carve needs.
    returned: [Returned; MAX_RETURNED_RUNS],
    /// Head of the released-frame list, `None` when empty. The link to the
    /// next released frame lives in the first eight bytes of each released
    /// frame; no frame in the pool has physical address zero, so a stored zero
    /// unambiguously ends the list.
    released: Option<u64>,
    frames_total: u64,
    frames_carved: u64,
    frames_released: u64,
}

impl Default for Frames {
    fn default() -> Self {
        Self::new()
    }
}

impl Frames {
    /// An empty pool, owning nothing.
    ///
    /// A pool that has admitted no memory hands out nothing, which is
    /// ADR-0041's property one level down: the nucleus does not have frames
    /// because it is the nucleus, it has the frames it accepted from a
    /// validated map.
    pub const fn new() -> Frames {
        Frames {
            pieces: [Piece::EMPTY; MAX_SPANS],
            count: 0,
            returned: [Returned::EMPTY; MAX_RETURNED_RUNS],
            released: None,
            frames_total: 0,
            frames_carved: 0,
            frames_released: 0,
        }
    }

    /// Admits the free memory of a validated map, minus everything occupied.
    ///
    /// Every maximal piece of a free span that avoids every occupied span is
    /// admitted, aligned inward to whole frames. Physical address zero is never
    /// admitted: the released-frame list stores its link inside a frame and
    /// reads a zero as the end of the list.
    ///
    /// # Safety
    ///
    /// The caller states that every span in `free` is real memory of this
    /// machine, reported usable by a validated memory map, currently mapped
    /// readable and writable at the same address, and owned by no one else —
    /// and that `occupied` names everything that is spoken for. From this
    /// call on, the pool writes to any frame it hands out or takes back.
    // SAFETY: the caller's promise that every free span is real, exclusively owned, mapped memory is the whole contract; the pool validates the rest.
    pub unsafe fn admit(
        &mut self,
        free: impl IntoIterator<Item = Span>,
        occupied: &[Span],
    ) -> Admission {
        let mut refused = 0;
        for span in free {
            // Every maximal free piece begins either at the span's start or at
            // the end of an occupied span, and runs to the next occupied start
            // or to the span's end. The same enumeration the Stage 2 grant
            // derivation used, except that here every piece is kept rather than
            // only the largest.
            if !self.consider(span, span.start, occupied) {
                refused += 1;
            }
            for taken in occupied {
                if taken.end > span.start
                    && taken.end < span.end
                    && !self.consider(span, taken.end, occupied)
                {
                    refused += 1;
                }
            }
        }
        Admission {
            spans: self.count,
            refused,
            frames: self.frames_total,
        }
    }

    /// Admits the piece starting at `start`, if one starts there. Returns
    /// whether the piece was admitted; a candidate that is not a piece at all
    /// is admitted vacuously, because nothing was lost.
    fn consider(&mut self, span: Span, start: u64, occupied: &[Span]) -> bool {
        if start >= span.end || occupied.iter().any(|taken| taken.holds(start)) {
            return true;
        }
        let mut end = span.end;
        for taken in occupied {
            if taken.start > start && taken.start < end {
                end = taken.start;
            }
        }
        let base = start.div_ceil(FRAME_SIZE) * FRAME_SIZE;
        let base = if base == 0 { FRAME_SIZE } else { base };
        let end = end / FRAME_SIZE * FRAME_SIZE;
        if base >= end {
            return true;
        }
        // Two occupied spans can end at the same address inside one free span,
        // which offers the same piece twice. Admitting it twice would hand the
        // same physical memory to two owners, so a piece already held is not a
        // second piece.
        if self.pieces[..self.count]
            .iter()
            .any(|piece| piece.frontier == base)
        {
            return true;
        }
        if self.count == self.pieces.len() {
            return false;
        }
        self.pieces[self.count] = Piece {
            frontier: base,
            end,
        };
        self.count += 1;
        self.frames_total += (end - base) / FRAME_SIZE;
        true
    }

    /// Frames admitted to the pool.
    pub fn total(&self) -> u64 {
        self.frames_total
    }

    /// Frames the pool can still hand out: never-carved memory plus everything
    /// released back to it.
    pub fn available(&self) -> u64 {
        self.frames_total - (self.frames_carved - self.frames_released)
    }

    /// Frames currently held by someone other than the pool.
    pub fn in_use(&self) -> u64 {
        self.frames_carved - self.frames_released
    }

    /// Bytes the pool can still hand out.
    pub fn bytes_available(&self) -> u64 {
        self.available() * FRAME_SIZE
    }

    /// The largest physically contiguous run the pool could still carve.
    ///
    /// **Both sources, because `carve` consults both.** A figure that described
    /// only the frontier would under-report a pool holding a large returned run,
    /// and an allocator whose "largest" is smaller than what it will actually
    /// hand out is one whose callers refuse requests it could serve.
    pub fn largest_contiguous(&self) -> u64 {
        self.largest_frontier().max(self.largest_returned())
    }

    /// The largest run the never-handed-out frontier could still carve.
    fn largest_frontier(&self) -> u64 {
        self.pieces[..self.count]
            .iter()
            .map(Piece::room)
            .max()
            .unwrap_or(0)
    }

    /// One frame, cleared, or `None` when the pool has none left.
    ///
    /// The frame comes from the released list when there is one, so a system
    /// that creates and destroys processes reuses memory rather than walking
    /// its pool frontier until it runs out.
    pub fn allocate_frame(&mut self) -> Option<u64> {
        let frame = match self.released {
            Some(frame) => {
                // SAFETY: `frame` was released to this pool, so nothing else
                // owns it, and the release path wrote the link to the next
                // released frame into its first eight bytes. The admission
                // contract states the frame is mapped and writable.
                let next = unsafe { read_link(frame) };
                self.released = if next == 0 { None } else { Some(next) };
                self.frames_released -= 1;
                frame
            }
            None => self.carve_run(FRAME_SIZE, FRAME_SIZE)?.start,
        };
        // SAFETY: the frame is owned by this pool and by nothing else at this
        // instant — it was either released back to the pool or has never been
        // handed out — and the admission contract states it is mapped and
        // writable. Clearing here, and not only on release, is what makes a
        // frame from the pool frontier as clean as a reused one.
        unsafe { clear(frame, FRAME_SIZE) };
        Some(frame)
    }

    /// Gives one frame back, clearing it.
    ///
    /// # Safety
    ///
    /// The caller states that `frame` was handed out by this pool, that no
    /// mapping or reference to it survives, and that it is not already
    /// released. A frame released twice would be handed to two owners.
    // SAFETY: the caller's promise that the frame came from this pool and is unreferenced is the whole contract; nothing else can establish it.
    pub unsafe fn release_frame(&mut self, frame: u64) {
        // SAFETY: the caller's contract makes this frame unreferenced and
        // owned by the pool again; the admission contract makes it writable.
        unsafe {
            clear(frame, FRAME_SIZE);
            write_link(frame, self.released.unwrap_or(0));
        }
        self.released = Some(frame);
        self.frames_released += 1;
    }

    /// A physically contiguous run of `bytes`, aligned to `alignment`.
    ///
    /// **It carries no previous owner's bytes**, which is the invariant that
    /// matters and is not the same as "it is zeroed". A run may come from memory
    /// this pool has never handed out — untouched since firmware, and not
    /// cleared — or from the returned table, which [`Frames::release_run`]
    /// cleared on the way in. Never from the released-frame *list*, whose
    /// entries carry a link written into their first bytes.
    ///
    /// `alignment` must be a power of two and at least a frame.
    pub fn carve(&mut self, bytes: u64, alignment: u64) -> Option<Span> {
        if bytes == 0 || alignment < FRAME_SIZE || !alignment.is_power_of_two() {
            return None;
        }
        self.carve_run(bytes.div_ceil(FRAME_SIZE) * FRAME_SIZE, alignment)
    }

    /// Gives a carved run back, frame by frame.
    ///
    /// **The frames come back and their adjacency does not**: each one is
    /// threaded onto the released list, which a later [`Frames::carve`] cannot
    /// use. For a run that must be re-carvable as a run, use
    /// [`Frames::release_run`].
    ///
    /// # Safety
    ///
    /// The caller states that `run` was carved from this pool, that no
    /// mapping or reference to it survives, and that no part of it has been
    /// released already.
    // SAFETY: as `release_frame`, for every frame of the run.
    pub unsafe fn release(&mut self, run: Span) {
        let mut frame = run.start;
        while frame < run.end {
            // SAFETY: every frame of a run carved from this pool satisfies
            // `release_frame`'s contract exactly when the run does.
            unsafe { self.release_frame(frame) };
            frame += FRAME_SIZE;
        }
    }

    /// Gives a carved run back **as a run**, so a later carve can use it whole.
    ///
    /// ADR-0084 §6a's obligation: DMA backing is physically contiguous, and a
    /// pool that could not take contiguous memory back would spend it once per
    /// boot. Adjacent runs coalesce, so releasing two halves of a region leaves
    /// one entry rather than two.
    ///
    /// **When the table cannot hold it, this falls back to threading the frames
    /// onto the released list** rather than refusing. The frames are never lost;
    /// what is lost is their adjacency, which is a bounded degradation and not a
    /// leak.
    ///
    /// # Safety
    ///
    /// As [`Frames::release`].
    // SAFETY: the caller's promise that the run is unreachable is what makes it
    // safe to hand out again.
    pub unsafe fn release_run(&mut self, run: Span) {
        if run.end <= run.start {
            return;
        }
        // **The invariant this table is kept under**: a run sitting in it has
        // already been taken out of `frames_carved`, so it counts as not in use
        // exactly once. Every path in and out of the table adjusts the count on
        // the way, and coalescing therefore adjusts nothing — the neighbour it
        // absorbs was accounted when *it* was returned.
        self.frames_carved -= (run.end - run.start) / FRAME_SIZE;
        // **Cleared here, at the boundary, and not by whoever carves it next.**
        // Before returned runs existed, `carve` could honestly say it clears
        // nothing: everything it handed out was memory this pool had never
        // handed out before. A run coming back breaks that, and the invariant
        // worth keeping is the one about *owners* rather than about zeroes —
        // memory returned to the pool and later given to somebody else carries
        // none of the previous owner's bytes. Putting it here means no present
        // or future caller of `carve` has to remember, which is the only version
        // of this rule that stays true.
        //
        // SAFETY: the caller states the run was carved from this pool, is
        // identity-mapped and unreachable, so this writes memory nothing else
        // references.
        unsafe {
            core::ptr::write_bytes(
                core::ptr::with_exposed_provenance_mut::<u8>(run.start as usize),
                0,
                (run.end - run.start) as usize,
            )
        };
        if self.keep_returned(run.start, run.end) {
            return;
        }
        // The table could not hold it, and nothing was absorbed, so this run is
        // the only thing to give back frame by frame.
        self.frames_carved += (run.end - run.start) / FRAME_SIZE;
        let mut frame = run.start;
        while frame < run.end {
            // SAFETY: per the contract; every frame of a run this pool carved
            // satisfies `release_frame`'s contract exactly when the run does.
            unsafe { self.release_frame(frame) };
            frame += FRAME_SIZE;
        }
    }

    /// Puts one already-accounted run into the returned table, coalescing.
    ///
    /// Returns whether it fits. **It cannot fail after absorbing anything**: an
    /// absorbed neighbour frees its own slot, so a coalescing insertion always
    /// has somewhere to go, and a failure therefore leaves the table exactly as
    /// it found it.
    fn keep_returned(&mut self, start: u64, end: u64) -> bool {
        let mut start = start;
        let mut end = end;
        let mut reuse: Option<usize> = None;
        for index in 0..MAX_RETURNED_RUNS {
            let entry = self.returned[index];
            if !entry.live() {
                continue;
            }
            if entry.end == start {
                start = entry.start;
                self.returned[index] = Returned::EMPTY;
                reuse = Some(index);
            } else if entry.start == end {
                end = entry.end;
                self.returned[index] = Returned::EMPTY;
                reuse = Some(index);
            }
        }
        let slot = reuse.or_else(|| self.returned.iter().position(|entry| !entry.live()));
        match slot {
            Some(index) => {
                self.returned[index] = Returned { start, end };
                true
            }
            // Nothing was absorbed — `reuse` is `None` — so the table is
            // untouched and the caller's run is all there is to deal with.
            None => false,
        }
    }

    /// The largest returned run this pool could hand out contiguously.
    pub fn largest_returned(&self) -> u64 {
        self.returned
            .iter()
            .filter(|entry| entry.live())
            .map(|entry| entry.bytes())
            .max()
            .unwrap_or(0)
    }

    /// Takes a run out of the returned table, putting the remainders back.
    fn carve_returned(&mut self, bytes: u64, alignment: u64) -> Option<Span> {
        let mut chosen: Option<(usize, u64, u64)> = None;
        for (index, entry) in self.returned.iter().enumerate() {
            if !entry.live() {
                continue;
            }
            let base = entry.start.div_ceil(alignment) * alignment;
            let Some(end) = base.checked_add(bytes) else {
                continue;
            };
            if end > entry.end {
                continue;
            }
            // Best fit, so a large returned run is not spent on a small request
            // while a snug one sits beside it.
            let waste = (base - entry.start) + (entry.end - end);
            if chosen.is_none_or(|(_, _, best)| waste < best) {
                chosen = Some((index, base, waste));
            }
        }
        let (index, base, _) = chosen?;
        let entry = self.returned[index];
        self.returned[index] = Returned::EMPTY;
        // The whole entry leaves the table, so all of it becomes carved; the
        // pieces that go back are then released again by the same door. Doing it
        // in that order is what keeps one rule — "in the table means not in
        // use" — true at every step instead of nearly always.
        self.frames_carved += entry.bytes() / FRAME_SIZE;
        // SAFETY: both remainders are parts of a run this pool handed out and
        // has taken back, and nothing references either.
        unsafe {
            if base > entry.start {
                self.release_run(Span::new(entry.start, base));
            }
            if entry.end > base + bytes {
                self.release_run(Span::new(base + bytes, entry.end));
            }
        }
        Some(Span::new(base, base + bytes))
    }

    /// The region a Stage 2 grant is made of.
    ///
    /// This is ADR-0041's grant, unchanged in shape and in property, made from
    /// the pool instead of from the largest hole in the map: a **V1** grant,
    /// because a nucleus granting to a single runtime with no process substrate
    /// is exactly what V1 describes (ADR-0050 section 2). `owner` and
    /// `generation` arrive with the process that needs them, not before.
    pub fn grant(&mut self, identity: u64) -> Result<RuntimeMemoryGrant, GrantRefused> {
        let room = self.largest_contiguous();
        if room == 0 {
            return Err(GrantRefused::NoRegion);
        }
        if room < MIN_GRANT as u64 {
            return Err(GrantRefused::TooSmall(room));
        }
        let length = room.min(MAX_GRANT as u64) / FRAME_SIZE * FRAME_SIZE;
        let region = self
            .carve(length, GRANT_ALIGNMENT as u64)
            .ok_or(GrantRefused::NoRegion)?;
        Ok(RuntimeMemoryGrant {
            version: GRANT_VERSION,
            base: region.start as usize,
            length: region.length() as usize,
            alignment: GRANT_ALIGNMENT,
            identity,
        })
    }

    /// Takes `bytes` of never-carved memory from the piece that leaves the
    /// least behind, so a large carve does not have to be the first one made.
    fn carve_run(&mut self, bytes: u64, alignment: u64) -> Option<Span> {
        // **Returned runs first**, so contiguous memory that came back is spent
        // before the frontier moves again. A pool that preferred the frontier
        // would keep a returned run for a request that never comes while
        // consuming memory it cannot get back.
        if let Some(run) = self.carve_returned(bytes, alignment) {
            return Some(run);
        }
        let mut chosen: Option<(usize, u64, u64)> = None;
        for (index, piece) in self.pieces[..self.count].iter().enumerate() {
            let base = piece.frontier.div_ceil(alignment) * alignment;
            let Some(end) = base.checked_add(bytes) else {
                continue;
            };
            if end > piece.end {
                continue;
            }
            let left = piece.end - end;
            if chosen.is_none_or(|(_, _, best)| left < best) {
                chosen = Some((index, base, left));
            }
        }
        let (index, base, _) = chosen?;
        let frontier = self.pieces[index].frontier;
        self.pieces[index].frontier = base + bytes;
        // Frames skipped to satisfy `alignment` are below the new frontier and
        // can never be handed out again, so they are counted as carved. An
        // allocator whose "available" figure includes memory it can no longer
        // reach is an allocator that reports success until the moment it
        // cannot.
        self.frames_carved += (base + bytes - frontier) / FRAME_SIZE;
        Some(Span::new(base, base + bytes))
    }
}

/// Reads the released-frame link out of a released frame.
///
/// # Safety
///
/// `frame` is a released frame of an admitted pool: mapped, writable, aligned
/// to at least eight bytes, and owned by the pool.
// SAFETY: the caller names a released frame of an admitted pool, which the release path wrote the link into.
unsafe fn read_link(frame: u64) -> u64 {
    // SAFETY: the caller's contract makes the address a live, frame-aligned,
    // pool-owned mapping, so the read is aligned and in bounds of an object the
    // release path wrote.
    unsafe { core::ptr::with_exposed_provenance::<u64>(frame as usize).read() }
}

/// Writes the released-frame link into a released frame.
///
/// # Safety
///
/// As [`read_link`], and nothing else may reference the frame.
// SAFETY: as `read_link`, and the frame is referenced by nothing else.
unsafe fn write_link(frame: u64, next: u64) {
    // SAFETY: the caller's contract makes the address a live, frame-aligned,
    // pool-owned mapping that nothing else references.
    unsafe { core::ptr::with_exposed_provenance_mut::<u64>(frame as usize).write(next) };
}

/// Clears `bytes` from `base`.
///
/// # Safety
///
/// `[base, base + bytes)` is inside one admitted piece of the pool, mapped
/// writable, and referenced by nothing else.
// SAFETY: the caller names a range inside one admitted piece that nothing else references.
unsafe fn clear(base: u64, bytes: u64) {
    // SAFETY: the caller's contract makes the whole range a live, exclusively
    // owned, writable mapping; `u8` has no alignment requirement.
    unsafe {
        core::ptr::write_bytes(
            core::ptr::with_exposed_provenance_mut::<u8>(base as usize),
            0,
            bytes as usize,
        )
    };
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    /// A pool over memory this test owns, so the allocator may write to it.
    ///
    /// Leaked deliberately: the pool writes links into released frames and
    /// zeroes returned runs, so the buffer must outlive every call.
    fn pool(frames: usize) -> Frames {
        let bytes = (frames + 1) * FRAME_SIZE as usize;
        let buffer = vec![0xAAu8; bytes].leak();
        let base = buffer.as_ptr() as u64;
        let start = base.div_ceil(FRAME_SIZE) * FRAME_SIZE;
        let mut frames_pool = Frames::new();
        // SAFETY: the span is memory this test owns and leaked, mapped
        // readable and writable at its own address, and referenced by nothing
        // else after `leak`.
        unsafe {
            frames_pool.admit(
                [Span::new(start, start + (frames as u64) * FRAME_SIZE)],
                &[],
            )
        };
        frames_pool
    }

    /// ADR-0084 §6a's whole point: a returned run can be carved again.
    #[test]
    fn a_returned_run_can_be_carved_again() {
        let mut pool = pool(8);
        let first = pool.carve(2 * FRAME_SIZE, FRAME_SIZE).expect("carve");
        // SAFETY: the run is this pool's and nothing references it.
        unsafe { pool.release_run(first) };
        let again = pool.carve(2 * FRAME_SIZE, FRAME_SIZE).expect("re-carve");
        assert_eq!(again.start, first.start);
    }

    /// **The invariant the returned table could otherwise break.** Before it,
    /// `carve` only ever produced memory this pool had never handed out. A
    /// caller that wrote a pattern and gave the run back must not have that
    /// pattern reappear in somebody else's allocation.
    #[test]
    fn a_returned_run_carries_no_previous_owners_bytes() {
        let mut pool = pool(8);
        let first = pool.carve(2 * FRAME_SIZE, FRAME_SIZE).expect("carve");
        let length = (first.end - first.start) as usize;
        // SAFETY: the run is this pool's, owned by this test for its lifetime.
        let bytes = unsafe { core::slice::from_raw_parts_mut(first.start as *mut u8, length) };
        bytes.fill(0x5A);
        // SAFETY: nothing references the run once the pattern is written.
        unsafe { pool.release_run(first) };
        let again = pool.carve(2 * FRAME_SIZE, FRAME_SIZE).expect("re-carve");
        // SAFETY: as above.
        let seen = unsafe { core::slice::from_raw_parts(again.start as *const u8, length) };
        assert!(
            seen.iter().all(|byte| *byte == 0),
            "a re-carved run carried the previous owner's bytes"
        );
    }

    /// Accounting exactly once, across a release and a re-carve.
    #[test]
    fn a_run_is_accounted_once_through_the_returned_table() {
        let mut pool = pool(8);
        let free = pool.available();
        let run = pool.carve(3 * FRAME_SIZE, FRAME_SIZE).expect("carve");
        assert_eq!(pool.available(), free - 3);
        // SAFETY: as above.
        unsafe { pool.release_run(run) };
        assert_eq!(
            pool.available(),
            free,
            "a returned run was not credited once"
        );
        let again = pool.carve(3 * FRAME_SIZE, FRAME_SIZE).expect("re-carve");
        assert_eq!(
            pool.available(),
            free - 3,
            "a re-carved run was not debited once"
        );
        // SAFETY: as above.
        unsafe { pool.release_run(again) };
        assert_eq!(pool.available(), free);
    }

    /// Coalescing must not credit the neighbour a second time.
    #[test]
    fn coalescing_two_halves_credits_each_once() {
        let mut pool = pool(8);
        let free = pool.available();
        let whole = pool.carve(4 * FRAME_SIZE, FRAME_SIZE).expect("carve");
        let low = Span::new(whole.start, whole.start + 2 * FRAME_SIZE);
        let high = Span::new(low.end, whole.end);
        // SAFETY: both halves are parts of a run this pool carved.
        unsafe {
            pool.release_run(low);
            pool.release_run(high);
        }
        assert_eq!(pool.available(), free, "coalescing double-counted a half");
        // And they really did coalesce: the whole run is carvable again.
        assert_eq!(pool.largest_returned(), 4 * FRAME_SIZE);
        let again = pool
            .carve(4 * FRAME_SIZE, FRAME_SIZE)
            .expect("re-carve whole");
        assert_eq!(again.start, whole.start);
    }

    /// `largest_contiguous` must describe what `carve` will actually serve.
    #[test]
    fn largest_contiguous_sees_the_returned_table() {
        let mut pool = pool(8);
        // Spend the frontier entirely, then give half of it back.
        let all = pool.carve(8 * FRAME_SIZE, FRAME_SIZE).expect("carve");
        assert_eq!(pool.largest_contiguous(), 0);
        let half = Span::new(all.start, all.start + 4 * FRAME_SIZE);
        // SAFETY: the half is part of a run this pool carved.
        unsafe { pool.release_run(half) };
        assert_eq!(
            pool.largest_contiguous(),
            4 * FRAME_SIZE,
            "the pool under-reported memory it would have handed out"
        );
        assert!(pool.carve(4 * FRAME_SIZE, FRAME_SIZE).is_some());
    }
}
