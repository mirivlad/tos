// SPDX-License-Identifier: GPL-3.0-or-later
//! DMA regions: memory a device may reach, and the proof that it no longer can
//! (ADR-0084).
//!
//! **The first object with two ancestries at once.** A `DmaRegion` is a region
//! in ADR-0076's one physical account — funded, charged, reclaimed — *and* a
//! bus-mastering descendant of a PCI function assignment under ADR-0082 §5d.
//! Neither is decorative: without the first, nothing funds a driver's buffers
//! and the account has a hole in it; without the second, the assignment could
//! end while the device still held the address.
//!
//! **Why teardown is the hard part, and `BME=0` is not it.** Every earlier
//! hardware object could be taken away by ring 0 — a window is unmapped, an
//! interrupt entry is masked. This memory goes back to the allocator and is
//! handed to somebody else, while the device's copy of the address sits in a
//! register whose meaning the nucleus does not know. Clearing Bus Master Enable
//! blocks *new* requests and proves nothing about requests already issued.
//!
//! So frames are **quarantined** rather than returned, and released only when
//! [`drained`] holds:
//!
//! ```text
//! 1  the assignment has no live bus-mastering descendant  ⇒ BME is clear
//! 2  then a read of the function's PCI Express Device Status returns
//!    Transactions Pending = 0
//! ```
//!
//! One transaction discharges both obligations of ADR-0084 §5a: its **value**
//! proves no non-posted request the function issued is outstanding, and its
//! **completion** proves earlier posted writes reached their destination,
//! because a completion may not pass a previously issued posted request and step
//! 1 guarantees nothing can be posted after it.
//!
//! **Fail-closed, with no way out.** If the proof cannot be established the
//! frames stay out of the pool, the funding charge stays outstanding, and the
//! assignment does not end — so the BDF cannot be claimed again and the
//! quarantine can never lose the owner that would retry it. There is no timeout
//! and no reset fallback: a timeout converts a guess into permission.

use tos_frames::FRAME_SIZE;
use tos_runtime::region::Span;

use crate::pci::{self, Needs};
use crate::region::{AuthorityId, GrantCharge};

/// How many DMA regions may exist at once, across all processes.
///
/// A fixed nucleus bound over statically reserved slots, in the class of
/// `MAX_CAPABILITIES`.
pub const MAX_REGIONS: usize = 8;

/// How many quarantined runs may be waiting for their assignment to go quiet.
///
/// Bounded for the same reason, and separately from [`MAX_REGIONS`]: a region
/// leaves the table the moment its last name goes, and its backing stays here
/// until the proof holds.
pub const MAX_QUARANTINED: usize = 8;

/// The largest region this contract will allocate, and the granule it charges
/// to. A DMA region is whole frames because it is mapped, and because the
/// device-visible extent must begin where the mapping does.
pub const MAX_REGION_BYTES: u64 = 2 * 1024 * 1024;

/// One live DMA region.
///
/// **Deliberately neither `Copy` nor `Clone`**, because its charge is not: a
/// `GrantCharge` is a receipt, losing one leaks budget and duplicating one
/// creates budget, and the type refuses to let an ordinary lifecycle try either.
/// So this table is read through [`View`] and written in place.
struct DmaRegion {
    /// The assignment it descends from, and that assignment's generation.
    assignment: u32,
    assignment_generation: u32,
    /// The process whose address space holds it, and the lane it is in.
    holder: u32,
    lane: u32,
    /// The physically contiguous backing (ADR-0084 §6a). **One extent**, so the
    /// device address of a bounded offset inside it is that base plus that
    /// offset — which is what makes §6b's subobject addressing well defined.
    run: Span,
    /// What the funding lineage was charged, held until the frames actually
    /// return (§5f). **Not refunded on release**: a refund while the memory is
    /// still occupied would let one budget be spent twice. It is *moved* into
    /// the quarantine rather than copied, so there is exactly one receipt for
    /// exactly one debit at every instant.
    charge: Option<GrantCharge>,
    generation: u32,
    names: u32,
    live: bool,
}

/// One backing run waiting for its assignment to be provably quiet.
struct Quarantined {
    assignment: u32,
    assignment_generation: u32,
    run: Span,
    charge: Option<GrantCharge>,
    /// The process it belonged to, for the record only.
    holder: u32,
    live: bool,
}

/// The read-only facts of a region, which are `Copy` where the region is not.
#[derive(Clone, Copy)]
struct View {
    assignment: u32,
    assignment_generation: u32,
    holder: u32,
    lane: u32,
    run: Span,
}

impl DmaRegion {
    const EMPTY: Self = Self {
        assignment: 0,
        assignment_generation: 0,
        holder: 0,
        lane: 0,
        run: Span { start: 0, end: 0 },
        charge: None,
        // Generations start at one, so a handle of all zeros names nothing.
        generation: 1,
        names: 0,
        live: false,
    };
}

impl Quarantined {
    const EMPTY: Self = Self {
        assignment: 0,
        assignment_generation: 0,
        run: Span { start: 0, end: 0 },
        charge: None,
        holder: 0,
        live: false,
    };
}

static mut REGIONS: [DmaRegion; MAX_REGIONS] = [const { DmaRegion::EMPTY }; MAX_REGIONS];
static mut QUARANTINE: [Quarantined; MAX_QUARANTINED] =
    [const { Quarantined::EMPTY }; MAX_QUARANTINED];

/// The region table.
///
/// # Safety
///
/// The nucleus is single-context and the dispatcher runs with interrupts
/// masked, so there is never a second live reference to this static.
// SAFETY: the caller is nucleus code, which is the only writer.
unsafe fn table() -> &'static mut [DmaRegion; MAX_REGIONS] {
    // SAFETY: the function's contract, through a raw pointer so no reference to
    // the static itself is formed.
    unsafe { &mut *core::ptr::addr_of_mut!(REGIONS) }
}

/// The quarantine.
///
/// # Safety
///
/// As [`table`].
// SAFETY: as above.
unsafe fn quarantine() -> &'static mut [Quarantined; MAX_QUARANTINED] {
    // SAFETY: as above.
    unsafe { &mut *core::ptr::addr_of_mut!(QUARANTINE) }
}

/// Why a DMA region could not be made.
pub enum Refused {
    /// A length that is zero, unaligned, or past what this contract allocates.
    BadArgument,
    /// The assignment has gone, or the function is not DMA-capable: without the
    /// PCI Express capability there is no `Transactions Pending` bit, so a
    /// region's reclaim could never be proved (ADR-0084 §5c, P1).
    OutOfScope,
    /// No slot, no contiguous memory, no quarantine room, or the authority's
    /// budget is spent.
    Limit,
    /// The address space would not take it.
    Paging,
}

/// Allocates one DMA region under **two** capabilities' authority.
///
/// The caller has already presented a function with `dma` and a memory
/// authority with `spend`; what arrives here is the pair, resolved. Nothing in
/// the arguments is an address (ADR-0084 §3).
pub fn allocate(
    assignment: u32,
    assignment_generation: u32,
    authority: AuthorityId,
    holder: usize,
    bytes: u64,
) -> Result<(u32, u32, u64, u64), Refused> {
    if bytes == 0 || bytes > MAX_REGION_BYTES {
        return Err(Refused::BadArgument);
    }
    let bytes = bytes.div_ceil(FRAME_SIZE) * FRAME_SIZE;
    if pci::is_live(assignment, assignment_generation) {
        // **P1, checked and not assumed** (ADR-0084 §5c). A function whose
        // reclaim could never be proved must not be given memory to reach.
        if !pci::supports_dma(assignment, assignment_generation) {
            return Err(Refused::OutOfScope);
        }
    } else {
        return Err(Refused::OutOfScope);
    }
    // SAFETY: single-context nucleus with interrupts masked; the only writer.
    let slots = unsafe { table() };
    // A process's own regions are bounded separately from the machine's, so one
    // driver cannot exhaust the table for every other.
    let held = slots
        .iter()
        .filter(|region| region.live && region.holder == holder as u32)
        .count();
    if held >= crate::process::MAX_DMA_REGIONS {
        return Err(Refused::Limit);
    }
    let index = slots
        .iter()
        .position(|region| !region.live)
        .ok_or(Refused::Limit)?;
    let live_regions = slots.iter().filter(|region| region.live).count();
    // **A quarantine slot is reserved by being counted now**, because a region
    // whose backing could not be quarantined at release would have nowhere safe
    // to go and the only remaining answers would be to leak it or to return it
    // unproved. Neither is admissible, so the shortage is reported here.
    let quarantined = {
        // SAFETY: as above.
        let quarantine = unsafe { quarantine() };
        quarantine.iter().filter(|entry| entry.live).count()
    };
    if quarantined + live_regions >= MAX_QUARANTINED {
        return Err(Refused::Limit);
    }

    // The charge first, so a region that cannot be funded costs nothing else.
    // SAFETY: single-context nucleus; nothing else holds the tree.
    let charge = unsafe { crate::memory::authority() }
        .charge_grant(authority, bytes as usize, FRAME_SIZE as usize)
        .map_err(|_| Refused::Limit)?;
    // **Physically contiguous** (ADR-0084 §6a): one base plus an offset must be
    // one address, because on the no-IOMMU backend there is nothing between the
    // address and the memory.
    // SAFETY: single-context nucleus; nothing else holds the pool.
    let frames = unsafe { crate::memory::frames() };
    let Some(run) = frames.carve(bytes, FRAME_SIZE) else {
        // SAFETY: as above; the charge was taken a moment ago and nothing
        // observed it.
        let _ = unsafe { crate::memory::authority() }.refund_grant(charge);
        return Err(Refused::Limit);
    };
    // Cleared before anything can reach it: a device region handed out holding
    // a previous occupant's bytes would be a disclosure through the pool.
    // SAFETY: the run is this pool's, identity-mapped, and nothing else
    // references it.
    unsafe {
        core::ptr::write_bytes(
            core::ptr::with_exposed_provenance_mut::<u8>(run.start as usize),
            0,
            (run.end - run.start) as usize,
        )
    };
    let mut taken = [false; crate::process::MAX_DMA_REGIONS];
    for region in slots.iter() {
        if region.live && region.holder == holder as u32 {
            if let Some(slot) = taken.get_mut(region.lane as usize) {
                *slot = true;
            }
        }
    }
    let lane = first_free_lane(&taken).ok_or(Refused::Limit)?;
    let base = match crate::process::map_dma(holder, lane, run.start, bytes) {
        Ok(base) => base,
        Err(()) => {
            // SAFETY: nothing mapped it and nothing else references it.
            unsafe { frames.release_run(run) };
            // SAFETY: as above.
            let _ = unsafe { crate::memory::authority() }.refund_grant(charge);
            return Err(Refused::Paging);
        }
    };
    // The descendant is taken **after** the backing exists and before the
    // capability is made, and it is what turns bus mastering on (ADR-0082 §5d).
    if pci::take_descendant(assignment, assignment_generation, Needs::DMA).is_err() {
        crate::process::unmap_dma(holder, lane);
        // SAFETY: as above.
        unsafe { crate::memory::frames().release_run(run) };
        // SAFETY: as above.
        let _ = unsafe { crate::memory::authority() }.refund_grant(charge);
        return Err(Refused::Limit);
    }
    // SAFETY: as above.
    let slots = unsafe { table() };
    let region = &mut slots[index];
    region.assignment = assignment;
    region.assignment_generation = assignment_generation;
    region.holder = holder as u32;
    region.lane = lane;
    region.run = run;
    region.charge = Some(charge);
    region.names = 0;
    region.live = true;
    Ok((index as u32, region.generation, base, bytes))
}

/// The lowest lane this process is not already using.
///
/// **Not the count of live regions**, which is the shape this started as and is
/// wrong the moment a lifecycle is not last-in-first-out: with A at lane 0 and B
/// at lane 1, releasing A leaves one live region, and a count would put the next
/// allocation at lane 1 — on top of B. Separated out so the arithmetic can be
/// tested without a machine under it.
fn first_free_lane(taken: &[bool]) -> Option<u32> {
    taken.iter().position(|used| !used).map(|lane| lane as u32)
}

/// The live region a capability names, as the facts a reader may copy.
fn view(index: u32, generation: u32) -> Option<View> {
    let index = index as usize;
    if index >= MAX_REGIONS {
        return None;
    }
    // SAFETY: single-context nucleus; a read under a checked index.
    let entry = &unsafe { table() }[index];
    if !entry.live || entry.generation != generation {
        return None;
    }
    Some(View {
        assignment: entry.assignment,
        assignment_generation: entry.assignment_generation,
        holder: entry.holder,
        lane: entry.lane,
        run: entry.run,
    })
}

/// Whether the region a capability names is still usable authority.
pub fn is_live(index: u32, generation: u32) -> bool {
    view(index, generation).is_some()
}

/// The device-visible address of a bounded offset inside one region
/// (ADR-0084 §6b).
///
/// **The caller presents a capability and an offset, never an address.** The
/// nucleus does the arithmetic, and the offset is checked against the region's
/// own extent and fails closed — exactly as an MMIO offset is. What comes back
/// is data a driver writes into its own device, and no operation of any contract
/// accepts it back.
///
/// On the no-IOMMU reference profile this is the physical address, which
/// ADR-0084 §6c states plainly rather than implying otherwise. Under an IOMMU it
/// would be an IOVA and no contract would change.
pub fn device_address(index: u32, generation: u32, offset: u64) -> Option<u64> {
    let entry = view(index, generation)?;
    let length = entry.run.end - entry.run.start;
    if offset >= length {
        return None;
    }
    entry.run.start.checked_add(offset)
}

/// The function a region reaches, for the audit record.
pub fn describe(index: u32, generation: u32) -> Option<(u16, u8, u8, u8)> {
    let entry = view(index, generation)?;
    pci::describe(entry.assignment, entry.assignment_generation)
}

/// Takes a name on a region.
pub fn retain(index: u32, generation: u32) -> Result<(), ()> {
    let usable = index as usize;
    if view(index, generation).is_none() {
        return Err(());
    }
    // SAFETY: single-context nucleus; the index was checked by `view`.
    let entry = &mut unsafe { table() }[usable];
    entry.names = entry.names.checked_add(1).ok_or(())?;
    Ok(())
}

/// Drops a name, and quarantines the backing when it was the last one.
pub fn release(index: u32, generation: u32) -> Result<(), ()> {
    let usable = index as usize;
    let Some(entry) = view(index, generation) else {
        return Err(());
    };
    // SAFETY: single-context nucleus; the index was checked by `view`.
    let slot = &mut unsafe { table() }[usable];
    slot.names = slot.names.checked_sub(1).ok_or(())?;
    if slot.names != 0 {
        return Ok(());
    }
    destroy(usable, entry);
    Ok(())
}

/// Ends a region that was never named, so a failed grant leaves nothing.
pub fn abandon(index: u32, generation: u32) {
    let usable = index as usize;
    let Some(entry) = view(index, generation) else {
        return;
    };
    // SAFETY: as above.
    if unsafe { table() }[usable].names != 0 {
        return;
    }
    destroy(usable, entry);
}

/// Quarantines every region a process held, when it dies.
///
/// **Death is not a proof of quiescence.** The mapping goes with the address
/// space and the descendant goes here, but the frames go to the quarantine like
/// any other release: a dead driver's device is no quieter than a live one's.
pub fn clear_process(process: usize) {
    let mut at = 0;
    while at < MAX_REGIONS {
        // SAFETY: single-context nucleus; nothing else holds the table.
        let slot = &unsafe { table() }[at];
        let mine = slot.live && slot.holder == process as u32;
        let generation = slot.generation;
        if mine {
            if let Some(entry) = view(at as u32, generation) {
                destroy(at, entry);
            }
        }
        at += 1;
    }
}

/// Ends one region: the mapping goes, the descendant goes, the backing waits.
fn destroy(index: usize, entry: View) {
    crate::process::unmap_dma(entry.holder as usize, entry.lane);
    // SAFETY: single-context nucleus; the index is this table's own.
    let slot = &mut unsafe { table() }[index];
    slot.live = false;
    slot.names = 0;
    slot.generation = slot.generation.wrapping_add(1);
    if slot.generation == 0 {
        slot.generation = 1;
    }
    // **Moved, not copied.** One receipt for one debit, at every instant.
    let charge = slot.charge.take();
    // **The backing is quarantined, not returned, and its charge stays
    // outstanding** (ADR-0084 §5f). A refund here would let a holder allocate,
    // release and allocate again from a budget it had not actually given back.
    let kept = {
        // SAFETY: as above.
        let quarantine = unsafe { quarantine() };
        match quarantine.iter_mut().find(|slot| !slot.live) {
            Some(slot) => {
                *slot = Quarantined {
                    assignment: entry.assignment,
                    assignment_generation: entry.assignment_generation,
                    run: entry.run,
                    charge,
                    holder: entry.holder,
                    live: true,
                };
                true
            }
            None => false,
        }
    };
    if !kept {
        // The allocation path reserves a quarantine slot per live region, so
        // this is a nucleus defect rather than a caller's. Failing closed means
        // the frames stay out of the pool and the charge stays outstanding —
        // the memory is lost and the account says so, which is the safe
        // direction.
        crate::memory::note_divergence(b"dma-quarantine-full");
    }
    report_quarantined(index, &entry, kept);
    // The descendant goes **after** the backing is safely accounted for, so the
    // predicate that may clear bus mastering is re-evaluated once, with the
    // quarantine already recorded.
    pci::drop_descendant(entry.assignment, entry.assignment_generation, Needs::DMA);
    // And the condition is tried immediately: a driver that released its only
    // DMA region and holds nothing else gets its memory back in the same call.
    sweep(entry.assignment, entry.assignment_generation);
}

/// Whether an assignment is provably quiet (ADR-0084 §5b).
///
/// Both obligations, in order, and the second is one configuration read whose
/// value and completion each prove one of them.
fn drained(assignment: u32, generation: u32) -> bool {
    // 1. Nothing may still be issuing. This is ADR-0082 §5d's predicate, and
    //    an interrupt source of the same function keeps it false — which is
    //    exactly the sibling case ADR-0084 §5e is about.
    if pci::bus_mastering(assignment, generation) != Some(false) {
        return false;
    }
    // 2. And nothing already issued may still be in flight. `None` is "the
    //    question was not answered", which is never treated as a negative.
    pci::transactions_pending(assignment, generation) == Some(false)
}

/// Returns every quarantined run of one assignment, if it is provably quiet.
///
/// Called wherever the predicate can have changed. **Nothing here can decide to
/// proceed without the proof**: the only path that returns frames is the one
/// [`drained`] guards.
pub fn sweep(assignment: u32, generation: u32) {
    if !drained(assignment, generation) {
        return;
    }
    let mut at = 0;
    while at < MAX_QUARANTINED {
        // SAFETY: single-context nucleus; nothing else holds the table.
        let slot = &mut unsafe { quarantine() }[at];
        if slot.live && slot.assignment == assignment && slot.assignment_generation == generation {
            let run = slot.run;
            let holder = slot.holder;
            let charge = slot.charge.take();
            *slot = Quarantined::EMPTY;
            // **Physical reclamation before accounting refund** (ADR-0075, and
            // ADR-0084 §5f's reuse of it). The frames go back as a *run*, so a
            // later contiguous allocation can use them.
            // SAFETY: the run was carved from this pool, nothing maps it any
            // more — the region's lane went with `destroy` — and no part of it
            // has been released.
            unsafe { crate::memory::frames().release_run(run) };
            if let Some(charge) = charge {
                // SAFETY: single-context nucleus; nothing else holds the tree.
                if unsafe { crate::memory::authority() }
                    .refund_grant(charge)
                    .is_err()
                {
                    crate::memory::note_divergence(b"dma-charge-refund");
                }
            } else {
                crate::memory::note_divergence(b"dma-charge-missing");
            }
            report_reclaimed(holder, run);
        }
        at += 1;
    }
}

/// Whether anything is still waiting on this assignment, which is what keeps the
/// assignment itself alive (ADR-0084 §5d).
///
/// **This is the fail-closed lifetime.** An assignment whose drain has not been
/// proved does not end, so its BDF cannot be claimed again, no second driver is
/// handed a device that was never shown to be quiescent, and the quarantine
/// keeps an owner that can retry it.
pub fn quarantined_under(assignment: u32, generation: u32) -> u32 {
    // SAFETY: single-context nucleus; a read of a plain table.
    unsafe { quarantine() }
        .iter()
        .filter(|entry| {
            entry.live
                && entry.assignment == assignment
                && entry.assignment_generation == generation
        })
        .count() as u32
}

/// How many frames the quarantine is holding.
///
/// **On the memory account line**, because quarantined memory is the one kind
/// that is neither in the pool nor reachable by anybody: a reader comparing
/// `available` against `pool_frames` would otherwise see a shortfall with no
/// name.
pub fn quarantined_frames() -> u64 {
    // SAFETY: as above.
    unsafe { quarantine() }
        .iter()
        .filter(|entry| entry.live)
        .map(|entry| (entry.run.end - entry.run.start) / FRAME_SIZE)
        .sum()
}

/// Says that a region's backing has gone into quarantine rather than back.
fn report_quarantined(index: usize, entry: &View, kept: bool) {
    tos_serial::puts(b"TOS.RUN.DMA_QUARANTINED region=");
    tos_serial::put_u32_decimal(index as u32);
    tos_serial::puts(b" process=");
    tos_serial::put_u32_decimal(entry.holder);
    tos_serial::puts(b" frames=");
    tos_serial::put_u32_decimal(((entry.run.end - entry.run.start) / FRAME_SIZE) as u32);
    tos_serial::puts(b" charge_outstanding=1 kept=");
    tos_serial::put_u32_decimal(u32::from(kept));
    tos_serial::puts(b" asserted_by=nucleus\r\n");
}

/// Says that a quarantined run has been proved safe and returned.
fn report_reclaimed(holder: u32, run: Span) {
    tos_serial::puts(b"TOS.RUN.DMA_RECLAIMED process=");
    tos_serial::put_u32_decimal(holder);
    tos_serial::puts(b" frames=");
    tos_serial::put_u32_decimal(((run.end - run.start) / FRAME_SIZE) as u32);
    tos_serial::puts(b" drained=1 refunded=1 asserted_by=nucleus\r\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The non-LIFO lifecycle the count-as-lane form got wrong: allocate A and
    /// B, release A, allocate C while B is live.
    #[test]
    fn a_freed_lane_is_reused_and_a_live_one_is_not() {
        // A at 0, B at 1.
        let mut taken = [false; 4];
        taken[0] = true;
        assert_eq!(first_free_lane(&taken), Some(1));
        taken[1] = true;
        // A goes; B stays at lane 1.
        taken[0] = false;
        assert_eq!(
            first_free_lane(&taken),
            Some(0),
            "the next region was put on top of a live one"
        );
    }

    #[test]
    fn a_full_process_has_no_lane() {
        assert_eq!(first_free_lane(&[true, true, true, true]), None);
        assert_eq!(first_free_lane(&[]), None);
    }
}
