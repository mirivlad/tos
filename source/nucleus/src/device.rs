// SPDX-License-Identifier: GPL-3.0-or-later
//! Mapped device memory, and its lifetime under a PCI function assignment.
//!
//! **Not a `RegionObject`, and the difference is not cosmetic** (ADR-0081 §5).
//! An ordinary region is pool memory: funded by a `MemoryAuthority`, charged
//! against one physical account (ADR-0076), and returned to the allocator when
//! the last reference goes. Device memory is none of those things — it is
//! pre-existing external hardware state, it costs the account nothing, and
//! releasing it returns nothing to anybody. Making it a region would have put
//! device registers into the memory tree, where every rule about reclamation
//! would have been wrong about them.
//!
//! What it *is* is a **descendant of one function assignment** (§14). The
//! assignment stays live while either a `FunctionConfig` capability names it or
//! a mapping exists under it, so releasing the last function handle cannot let
//! the same BDF be claimed again and reached through a window somebody still
//! holds. Only when both are gone does the assignment end and its generation
//! advance.
//!
//! The nucleus knows PCI BAR mechanics and page mappings. It does not know what
//! is behind the window.

/// How many device mappings may exist at once, across all processes.
///
/// A fixed nucleus bound over statically reserved slots, in the class of
/// `MAX_CAPABILITIES`: what a table decides must not be sized by its users.
pub const MAX_MAPPINGS: usize = 8;

/// One mapped device window.
#[derive(Clone, Copy)]
struct Mapping {
    /// The assignment this is a descendant of, and the epoch it was made in.
    /// Both, because a slot is reused: the pair is what makes a stale mapping
    /// detectably stale rather than silently attached to the next claim.
    assignment: u32,
    assignment_generation: u32,
    /// The process whose address space holds the window, and the lane it is in.
    holder: u32,
    lane: u32,
    /// The physical extent, which is nucleus state and never public.
    physical: u64,
    length: u64,
    writable: bool,
    /// Advances when the slot is reused, so a handle held across a release
    /// resolves to nothing.
    generation: u32,
    /// How many capabilities name this mapping.
    names: u32,
    live: bool,
}

impl Mapping {
    const EMPTY: Self = Self {
        assignment: 0,
        assignment_generation: 0,
        holder: 0,
        lane: 0,
        physical: 0,
        length: 0,
        writable: false,
        // Generations start at one, so a handle of all zeros names nothing.
        generation: 1,
        names: 0,
        live: false,
    };
}

static mut MAPPINGS: [Mapping; MAX_MAPPINGS] = [Mapping::EMPTY; MAX_MAPPINGS];

/// The mapping table.
///
/// # Safety
///
/// The nucleus is single-context and the dispatcher runs with interrupts
/// masked, so there is never a second live reference to this static.
// SAFETY: the caller is nucleus code, which is the only writer, and the
// single-context argument above is why no second borrow can exist.
unsafe fn table() -> &'static mut [Mapping; MAX_MAPPINGS] {
    // SAFETY: the function's contract, reached through a raw pointer so no
    // reference to the static itself is ever formed.
    unsafe { &mut *core::ptr::addr_of_mut!(MAPPINGS) }
}

/// Why a mapping could not be made.
pub enum Refused {
    /// No slot, or the process already holds as many windows as it may.
    Limit,
    /// The address space would not take it.
    Paging,
}

/// Maps a validated device range into a process, as a descendant of one
/// assignment.
///
/// The physical range has already been derived from the live assignment's own
/// BAR state; nothing here takes an address from a caller.
pub fn map(
    assignment: u32,
    assignment_generation: u32,
    holder: usize,
    physical: u64,
    length: u64,
    writable: bool,
) -> Result<(u32, u32, u64), Refused> {
    // SAFETY: single-context nucleus with interrupts masked; the only writer.
    let table = unsafe { table() };
    // A process's own windows are bounded separately from the machine's, so one
    // driver cannot exhaust the table for every other.
    let held = table
        .iter()
        .filter(|mapping| mapping.live && mapping.holder == holder as u32)
        .count();
    if held >= crate::process::MAX_DEVICE_MAPPINGS {
        return Err(Refused::Limit);
    }
    let index = table
        .iter()
        .position(|mapping| !mapping.live)
        .ok_or(Refused::Limit)?;
    // The lane is the process's own, and is free because the count above says
    // this process holds fewer windows than there are lanes.
    let lane = held as u32;
    let base = crate::process::map_device(holder, lane, physical, length, writable)
        .map_err(|()| Refused::Paging)?;
    // The descendant is recorded **after** the mapping exists and before the
    // capability is made: an assignment that counted a descendant which failed
    // to map could never be released.
    if crate::pci::take_descendant(assignment, assignment_generation).is_err() {
        crate::process::unmap_device(holder, lane);
        return Err(Refused::Limit);
    }
    let entry = &mut table[index];
    entry.assignment = assignment;
    entry.assignment_generation = assignment_generation;
    entry.holder = holder as u32;
    entry.lane = lane;
    entry.physical = physical;
    entry.length = length;
    entry.writable = writable;
    entry.names = 0;
    entry.live = true;
    Ok((index as u32, entry.generation, base))
}

/// The live mapping a capability names, if its generation still matches.
fn mapping(index: u32, generation: u32) -> Option<Mapping> {
    let index = index as usize;
    if index >= MAX_MAPPINGS {
        return None;
    }
    // SAFETY: single-context nucleus; the index is checked above.
    let entry = unsafe { table() }[index];
    (entry.live && entry.generation == generation).then_some(entry)
}

/// Whether the mapping a capability names is still usable authority.
pub fn is_live(index: u32, generation: u32) -> bool {
    mapping(index, generation).is_some()
}

/// Takes a name on a mapping.
pub fn retain(index: u32, generation: u32) -> Result<(), ()> {
    let usable = index as usize;
    if mapping(index, generation).is_none() {
        return Err(());
    }
    // SAFETY: single-context nucleus; the index was checked by `mapping`.
    let entry = &mut unsafe { table() }[usable];
    entry.names = entry.names.checked_add(1).ok_or(())?;
    Ok(())
}

/// Drops a name, and ends the mapping when it was the last one.
///
/// Ending it unmaps the window, frees the slot, advances the generation and
/// tells the assignment it has one fewer descendant — which is what may finally
/// let that assignment end.
pub fn release(index: u32, generation: u32) -> Result<(), ()> {
    let usable = index as usize;
    let Some(entry) = mapping(index, generation) else {
        return Err(());
    };
    // SAFETY: single-context nucleus; the index was checked by `mapping`.
    let slot = &mut unsafe { table() }[usable];
    slot.names = slot.names.checked_sub(1).ok_or(())?;
    if slot.names != 0 {
        return Ok(());
    }
    destroy(usable, entry);
    Ok(())
}

/// Ends a mapping that was never named, so a failed grant leaves nothing.
pub fn abandon(index: u32, generation: u32) {
    let usable = index as usize;
    let Some(entry) = mapping(index, generation) else {
        return;
    };
    // SAFETY: as above.
    if unsafe { table() }[usable].names != 0 {
        return;
    }
    destroy(usable, entry);
}

/// Removes every mapping a process held, when it dies.
///
/// **Process death cannot leave an untracked window.** The pages go with the
/// address space, the slots are freed, and each one tells its assignment that a
/// descendant has gone — so an assignment whose driver died becomes releasable
/// rather than staying live forever.
pub fn clear_process(process: usize) {
    let mut at = 0;
    while at < MAX_MAPPINGS {
        // SAFETY: single-context nucleus; nothing else holds the table.
        let entry = unsafe { table() }[at];
        if entry.live && entry.holder == process as u32 {
            destroy(at, entry);
        }
        at += 1;
    }
}

/// Unmaps a window and lets its assignment know.
fn destroy(index: usize, entry: Mapping) {
    crate::process::unmap_device(entry.holder as usize, entry.lane);
    // SAFETY: single-context nucleus; the index is this table's own.
    let slot = &mut unsafe { table() }[index];
    slot.live = false;
    slot.names = 0;
    slot.generation = slot.generation.wrapping_add(1);
    if slot.generation == 0 {
        slot.generation = 1;
    }
    crate::pci::drop_descendant(entry.assignment, entry.assignment_generation);
}
