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

/// The nucleus's physical frame allocator.
pub struct Frames {
    pieces: [Piece; MAX_SPANS],
    count: usize,
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
    pub fn largest_contiguous(&self) -> u64 {
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
    /// Not cleared, and never satisfied from released frames — see the module
    /// header. `alignment` must be a power of two and at least a frame.
    pub fn carve(&mut self, bytes: u64, alignment: u64) -> Option<Span> {
        if bytes == 0 || alignment < FRAME_SIZE || !alignment.is_power_of_two() {
            return None;
        }
        self.carve_run(bytes.div_ceil(FRAME_SIZE) * FRAME_SIZE, alignment)
    }

    /// Gives a carved run back, frame by frame.
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
