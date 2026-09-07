// SPDX-License-Identifier: GPL-3.0-or-later
//! Routed interrupt authority, and the delivery it names (ADR-0082).
//!
//! **Where authority comes from.** A `platform.irq.Source` descends from a live
//! `platform.pci.FunctionConfig` and from nothing else. There is no
//! interrupt-controller capability, no "may route anything" authority and no
//! module-name rule: a process holding a function may ask for that function's
//! interrupts, and a process holding no function cannot ask for anybody's.
//!
//! ```text
//! platform.pci.Bus            minted at the launch boundary
//!     ↓  pci_function_claim
//! platform.pci.FunctionConfig one exclusive assignment
//!     ↓  pci_interrupt_claim, right `interrupt`
//! platform.irq.Source         one interrupt of that function
//!     ↓  irq_wait, right `wait`
//! a blocked context, woken by the device
//! ```
//!
//! **A number is not authority.** No operation of any contract takes a vector, a
//! GSI, a legacy IRQ, an MSI address/data pair or a BDF, and none of those
//! appears in this object's public description. The one number a caller supplies
//! is an MSI-X table entry index *within the function its capability already
//! names* — the same class of argument as a BAR index, selecting among things
//! the capability covers and unable to reach outside them.
//!
//! **What ring 0 learns here, and what it must not.** This module knows the
//! layout of an MSI-X table entry, how to allocate a CPU vector, how to
//! acknowledge through the local APIC, and that a source is entry N of a
//! function. It does not know what the device is, which entry serves which
//! queue, or that anything called a queue exists — that is told to the *device*
//! by the driver, through the window the driver already maps, and a gate asserts
//! mechanically that no device vocabulary reaches ring 0.

use crate::apic;
use crate::pci::{self, InterruptRefused, MsiXEntry, Needs};
use crate::process::{self, Waiting};

/// How many routed sources may exist at once, across every process.
///
/// A fixed nucleus bound over statically reserved slots, in the class of
/// `MAX_CAPABILITIES`: what a table decides must not be sized by its users.
pub const MAX_SOURCES: usize = 8;

/// The first CPU vector this stage hands to a device, and how many there are.
///
/// Above the timer (32) and clear of the spurious vector (255). Sixteen, which
/// is what a **retiring** allocator can afford to spend: §5f trades a bounded
/// supply for a stale-authority proof the generation mechanism cannot give, and
/// a supply that is bounded has to be a number somebody chose.
pub const FIRST_DEVICE_VECTOR: u8 = 48;
pub const DEVICE_VECTORS: usize = 16;

/// One MSI-X table entry's four words, from its own start.
const ENTRY_ADDRESS_LOW: u64 = 0;
const ENTRY_ADDRESS_HIGH: u64 = 4;
const ENTRY_DATA: u64 = 8;
const ENTRY_VECTOR_CONTROL: u64 = 12;
/// Bit 0 of Vector Control: this entry is masked.
const ENTRY_MASKED: u32 = 1;

/// One routed interrupt source: nucleus state, reachable only through a
/// capability (ADR-0082 §6).
#[derive(Clone, Copy)]
struct Source {
    /// The assignment it descends from, and that assignment's generation. Both,
    /// because a slot is reused: the pair is what makes a source of a released
    /// assignment detectably stale instead of silently attached to the next
    /// claim of the same BDF.
    assignment: u32,
    assignment_generation: u32,
    /// Which MSI-X table entry of that function this is.
    entry: u64,
    /// The vector the nucleus allocated. **Never public**, never an argument,
    /// never a result, and never returned to the allocator (§5f).
    vector: u8,
    /// The process the delivery goes to, when one is inside `irq_wait`.
    ///
    /// **At most one** (§7). Several capabilities may name the source; only one
    /// wait is ever outstanding, and a second is `E_LIMIT`.
    waiter: Option<u32>,
    /// One bit, and deliberately not a count (§7). An interrupt that arrives
    /// with nobody waiting sets it; the next `irq_wait` clears it and returns
    /// without blocking. A count would invite a driver to pair wakeups with
    /// completions, which is false of any device that coalesces — and coalescing
    /// is required rather than merely permitted by `docs/35` §Stage 4.
    pending: bool,
    /// How many interrupts this source has taken, for the record.
    deliveries: u32,
    /// The process that claimed it, so its death sweeps the source.
    holder: u32,
    /// Advances when the slot is reused, so a handle held across a release
    /// resolves to nothing.
    generation: u32,
    /// How many capabilities name it.
    names: u32,
    live: bool,
}

impl Source {
    const EMPTY: Self = Self {
        assignment: 0,
        assignment_generation: 0,
        entry: 0,
        vector: 0,
        waiter: None,
        pending: false,
        deliveries: 0,
        holder: 0,
        // Generations start at one, so a handle of all zeros — the value of a
        // register nobody wrote — names nothing here either.
        generation: 1,
        names: 0,
        live: false,
    };
}

static mut SOURCES: [Source; MAX_SOURCES] = [Source::EMPTY; MAX_SOURCES];

/// How many vectors have ever been handed out.
///
/// **Monotonic, and that is the whole mechanism** (ADR-0082 §5f). A vector is
/// retired when its source ends and is never returned: a stale MSI carries a
/// vector and does not carry the source's generation, so no accepted mechanism
/// could prove that a message emitted before masking cannot arrive after the
/// vector has been reused. Every other stale-authority property in this system
/// is proved by a generation and this one cannot be, so the conservative rule
/// stands in for the proof.
static mut VECTORS_TAKEN: usize = 0;

/// Interrupts that arrived on a device vector with no live source behind it.
static mut SPURIOUS: u32 = 0;

/// The source table.
///
/// # Safety
///
/// The nucleus is single-context; the dispatcher runs with interrupts masked and
/// the delivery handler cannot be re-entered, so there is never a second live
/// reference to this static.
// SAFETY: the caller is nucleus code, which is the only writer.
unsafe fn table() -> &'static mut [Source; MAX_SOURCES] {
    // SAFETY: the function's contract. Reached through a raw pointer so that no
    // reference to the static itself is ever formed.
    unsafe { &mut *core::ptr::addr_of_mut!(SOURCES) }
}

/// Why a source could not be derived.
pub enum Refused {
    /// The entry index names no entry of this function's table.
    BadArgument,
    /// The assignment has gone, or the function has no MSI-X capability, or its
    /// table is somewhere this nucleus cannot reach.
    OutOfScope,
    /// A live source already occupies this (assignment, entry); the table is
    /// full; or the vector supply is spent.
    Limit,
}

impl From<InterruptRefused> for Refused {
    fn from(refused: InterruptRefused) -> Self {
        match refused {
            InterruptRefused::BadArgument => Refused::BadArgument,
            InterruptRefused::OutOfScope => Refused::OutOfScope,
        }
    }
}

/// Takes the next CPU vector, or nothing when the supply is spent.
fn allocate_vector() -> Option<u8> {
    // SAFETY: single-context nucleus with interrupts masked.
    let taken = unsafe { core::ptr::addr_of!(VECTORS_TAKEN).read() };
    if taken >= DEVICE_VECTORS {
        return None;
    }
    // SAFETY: as above; the only writer.
    unsafe { core::ptr::addr_of_mut!(VECTORS_TAKEN).write(taken + 1) };
    Some(FIRST_DEVICE_VECTOR + taken as u8)
}

/// Reads one word of an MSI-X table entry.
///
/// # Safety
///
/// `entry` was derived by [`pci::msix_entry`] from a live assignment's measured
/// BAR state, its page is mapped supervisor-only into every address space this
/// nucleus runs, and the function's memory decoding is on.
// SAFETY: the caller's promise that the page is mapped and the function decodes
// is what makes this a device register rather than an address.
unsafe fn read_entry(entry: &MsiXEntry, word: u64) -> u32 {
    // SAFETY: per the contract; an aligned volatile 32-bit read of a device
    // register the nucleus owns.
    unsafe {
        core::ptr::with_exposed_provenance::<u32>((entry.physical + word) as usize).read_volatile()
    }
}

/// Writes one word of an MSI-X table entry.
///
/// # Safety
///
/// As [`read_entry`].
// SAFETY: as above.
unsafe fn write_entry(entry: &MsiXEntry, word: u64, value: u32) {
    // SAFETY: per the contract; an aligned volatile 32-bit write.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<u32>((entry.physical + word) as usize)
            .write_volatile(value)
    };
}

/// The message an entry is programmed with, for one vector.
///
/// **Neither half ever leaves ring 0** (ADR-0082 §4, §6). The address is the
/// architected local-APIC message window with a physical destination of the boot
/// processor; the data is the vector, delivered fixed and edge-triggered. A
/// caller cannot supply either and cannot read either back.
fn message_for(vector: u8) -> (u64, u32) {
    // Destination 0, physical destination mode, no redirection hint.
    (apic::LOCAL_APIC, u32::from(vector))
}

/// Derives one routed interrupt source from a live function assignment.
///
/// The creation order is ADR-0082 §5e's, and each step is there because leaving
/// it out loses something:
///
/// ```text
/// 0  the IDT gate for every device vector exists from boot
/// 1  the entry is masked
/// 2  message address and data are programmed while it is masked
/// 3  the same entry is read back, which proves the table writes have landed
/// 4  BME and MSE reach the state their predicates dictate
/// 5  MSI-X Enable and Function Mask reach their intended state
/// 6  the entry is unmasked — only now can this source fire
/// ```
///
/// with the source **published before step 6**, so a delivered message never
/// finds a handler with no source to wake, and the capability reaching CPL 3
/// only after all of it.
pub fn claim(
    assignment: u32,
    assignment_generation: u32,
    holder: usize,
    entry_index: u64,
) -> Result<(u32, u32), Refused> {
    let located = pci::msix_entry(assignment, assignment_generation, entry_index)?;
    // SAFETY: single-context nucleus with interrupts masked; the only writer.
    let table = unsafe { table() };
    // Exclusivity first, and over the whole table rather than the free slot: a
    // second source for one entry is refused whether or not there is room,
    // because the refusal is about the entry and not about capacity (§6).
    if table.iter().any(|source| {
        source.live
            && source.assignment == assignment
            && source.assignment_generation == assignment_generation
            && source.entry == entry_index
    }) {
        return Err(Refused::Limit);
    }
    let slot = table
        .iter()
        .position(|source| !source.live)
        .ok_or(Refused::Limit)?;
    // **Before the table is touched**, because a page ring 0 cannot reach is a
    // page it cannot mask again either. Registration is what makes every address
    // space built later map it; the call after it is what gives the spaces that
    // already exist.
    if crate::paging::register_nucleus_device_page(located.page).is_err() {
        return Err(Refused::Limit);
    }
    if process::map_nucleus_device_page(located.page).is_err() {
        crate::paging::withdraw_nucleus_device_page(located.page);
        return Err(Refused::Limit);
    }
    // The vector is taken next, because it is the one resource that cannot be
    // given back: taking it after a failure below would retire a vector for a
    // source that never existed.
    let vector = allocate_vector().ok_or(Refused::Limit)?;

    // §5b's one permitted exception: the table is in a BAR, so the function has
    // to decode for the length of this section — during which no CPL-3
    // instruction runs, and at the end of which the predicate is what decides.
    pci::begin_source_initialisation(assignment, assignment_generation);

    // 1 and 2. Masked first and programmed masked, so a device that was somehow
    // already enabled cannot deliver a half-written message.
    // SAFETY: the entry was derived from the live assignment's measured BAR, its
    // page is mapped supervisor-only above, and memory decoding is on.
    unsafe {
        write_entry(&located, ENTRY_VECTOR_CONTROL, ENTRY_MASKED);
        let (address, data) = message_for(vector);
        write_entry(&located, ENTRY_ADDRESS_LOW, address as u32);
        write_entry(&located, ENTRY_ADDRESS_HIGH, (address >> 32) as u32);
        write_entry(&located, ENTRY_DATA, data);
    }
    // 3. **A configuration read is not a fence for an MMIO write** (§5e). What
    // proves the posted writes have landed at the function is reading back the
    // same entry, and the mask bit is the word to read: it is the one this
    // sequence wrote first and will write last.
    // SAFETY: as above.
    let landed = unsafe { read_entry(&located, ENTRY_VECTOR_CONTROL) };
    if landed & ENTRY_MASKED == 0 {
        // The device did not take the mask, so nothing here can be trusted to
        // have landed. Leave the function as it was found.
        pci::restore_enables(assignment, assignment_generation);
        return Err(Refused::OutOfScope);
    }
    // 4. Both predicates, through the one door that owns them: an interrupt
    // source needs the function to decode *and* to master the bus, and taking
    // the descendant is what turns each on (§5b, §5d).
    if pci::take_descendant(assignment, assignment_generation, Needs::MSIX).is_err() {
        pci::restore_enables(assignment, assignment_generation);
        return Err(Refused::Limit);
    }
    // 5. The function's own enable, which the claim left off and masked.
    pci::set_msix_enabled(assignment, assignment_generation, true);

    // Published before it can fire.
    let source = &mut table[slot];
    source.assignment = assignment;
    source.assignment_generation = assignment_generation;
    source.entry = entry_index;
    source.vector = vector;
    source.waiter = None;
    source.pending = false;
    source.deliveries = 0;
    source.holder = holder as u32;
    source.names = 0;
    source.live = true;
    let generation = source.generation;

    // 6. And only now.
    // SAFETY: as above.
    unsafe { write_entry(&located, ENTRY_VECTOR_CONTROL, 0) };
    Ok((slot as u32, generation))
}

/// The live source a capability names, if its generation still matches **and**
/// the assignment it descends from is still the one it was derived from.
///
/// Two questions rather than one, because they can differ: an assignment ends
/// only when nothing reaches it, and a source is one of the things that reaches
/// it — so the second check is not redundant, it is what makes a source of a
/// released assignment refuse rather than reach the next claim of the same BDF.
fn source(index: u32, generation: u32) -> Option<Source> {
    let index = index as usize;
    if index >= MAX_SOURCES {
        return None;
    }
    // SAFETY: single-context nucleus; a read under a checked index.
    let entry = unsafe { table() }[index];
    if !entry.live || entry.generation != generation {
        return None;
    }
    pci::is_live(entry.assignment, entry.assignment_generation).then_some(entry)
}

/// Whether the source a capability names is still usable authority.
pub fn is_live(index: u32, generation: u32) -> bool {
    source(index, generation).is_some()
}

/// The function and entry a source names, for the audit record.
pub fn describe(index: u32, generation: u32) -> Option<(u16, u8, u8, u8, u64)> {
    let entry = source(index, generation)?;
    let (segment, bus, device, function) =
        pci::describe(entry.assignment, entry.assignment_generation)?;
    Some((segment, bus, device, function, entry.entry))
}

/// Takes a name on a source, refusing rather than wrapping.
pub fn retain(index: u32, generation: u32) -> Result<(), ()> {
    let usable = index as usize;
    if source(index, generation).is_none() {
        return Err(());
    }
    // SAFETY: single-context nucleus; the index was checked by `source`.
    let entry = &mut unsafe { table() }[usable];
    entry.names = entry.names.checked_add(1).ok_or(())?;
    Ok(())
}

/// Drops a name, and destroys the source when it was the last one.
pub fn release(index: u32, generation: u32) -> Result<(), ()> {
    let usable = index as usize;
    let Some(entry) = source(index, generation) else {
        return Err(());
    };
    // SAFETY: single-context nucleus; the index was checked by `source`.
    let slot = &mut unsafe { table() }[usable];
    slot.names = slot.names.checked_sub(1).ok_or(())?;
    if slot.names != 0 {
        return Ok(());
    }
    destroy(usable, entry);
    Ok(())
}

/// Ends a source that was never named, so a failed grant leaves nothing.
pub fn abandon(index: u32, generation: u32) {
    let usable = index as usize;
    let Some(entry) = source(index, generation) else {
        return;
    };
    // SAFETY: as above.
    if unsafe { table() }[usable].names != 0 {
        return;
    }
    destroy(usable, entry);
}

/// Releases every source a process claimed, when it dies.
///
/// **A dead process cannot leave a routed interrupt pointing at a slot** (§7).
/// The entry is masked, the descendant goes — which re-evaluates both enable
/// predicates once — and the vector is retired rather than returned.
pub fn clear_process(process: usize) {
    let mut at = 0;
    while at < MAX_SOURCES {
        // SAFETY: single-context nucleus; nothing else holds the table.
        let entry = unsafe { table() }[at];
        if entry.live && entry.holder == process as u32 {
            destroy(at, entry);
        } else if entry.live && entry.waiter == Some(process as u32) {
            // A source somebody else holds, whose waiter has died: the wait is
            // gone with the process, and leaving the slot's opinion behind would
            // make the next `irq_wait` on it `E_LIMIT` forever.
            // SAFETY: as above.
            (unsafe { table() })[at].waiter = None;
        }
        at += 1;
    }
}

/// Ends one source, in ADR-0082 §5f's order.
///
/// ```text
/// 1  mask the table entry
/// 2  read back the same entry, so the mask has landed at the function
/// 3  disable MSI-X or apply Function Mask, if this was the last source
/// 4  apply the §5d and §5b transitions
/// 5  cancel any waiter with E_CANCELLED
/// 6  unpublish the source
/// 7  keep an IDT handler for the retired vector
/// 8  a late interrupt on it is acknowledged, counted spurious, and wakes nobody
/// 9  the allocator never hands that vector to another source this boot
/// ```
///
/// Steps 7 to 9 are not code here because they are properties of what this
/// function does *not* do: the gates are installed once at boot for the whole
/// device range and are never removed, and the allocator only ever counts up.
fn destroy(index: usize, entry: Source) {
    // 1 and 2, while the descendant is still live and the function therefore
    // still decodes. Masking after step 4 would be masking through a window the
    // function had stopped answering.
    if let Ok(located) = pci::msix_entry(entry.assignment, entry.assignment_generation, entry.entry)
    {
        // SAFETY: the entry was derived from the live assignment, its page is
        // mapped supervisor-only in every space this nucleus runs, and memory
        // decoding is still on because this source's descendant has not gone.
        unsafe {
            write_entry(&located, ENTRY_VECTOR_CONTROL, ENTRY_MASKED);
            let _ = read_entry(&located, ENTRY_VECTOR_CONTROL);
        }
    }
    // 3. The function's own enable goes back to the state a claim leaves when
    // this was the last source of that function — Enable off and Function Mask
    // on — and stays as it is while another source of the same function lives.
    let last = {
        // SAFETY: single-context nucleus; nothing else holds the table.
        let table = unsafe { table() };
        !table.iter().enumerate().any(|(at, other)| {
            at != index
                && other.live
                && other.assignment == entry.assignment
                && other.assignment_generation == entry.assignment_generation
        })
    };
    if last {
        pci::set_msix_enabled(entry.assignment, entry.assignment_generation, false);
    }
    // 6, before 5: a waiter woken with `E_CANCELLED` becomes runnable, and it
    // must not find the source it was waiting on still answering.
    // SAFETY: single-context nucleus; the index is this table's own.
    let slot = &mut unsafe { table() }[index];
    let vector = slot.vector;
    let waiter = slot.waiter;
    slot.live = false;
    slot.names = 0;
    slot.waiter = None;
    slot.pending = false;
    slot.generation = slot.generation.wrapping_add(1);
    if slot.generation == 0 {
        slot.generation = 1;
    }
    // 4. Both predicates re-evaluated once, through the door that owns them.
    // This may also be what finally lets the assignment end.
    pci::drop_descendant(entry.assignment, entry.assignment_generation, Needs::MSIX);
    // 5. `SYSTEM_ABI_V1` §6's cancellation path, at the instant the source goes
    // (§7). It is also what stops a stranded wait from keeping the system
    // falsely live: after this, the census finds no routed source.
    if let Some(waiter) = waiter {
        // SAFETY: the waiter is a context blocked in `irq_wait`, and this is the
        // answer to that call.
        unsafe {
            process::wake(
                waiter as usize,
                crate::syscall::Answer::status(crate::syscall::E_CANCELLED),
            )
        };
    }
    tos_serial::puts(b"TOS.RUN.IRQ_RELEASED source=");
    tos_serial::put_u32_decimal(index as u32);
    tos_serial::puts(b" entry=");
    tos_serial::put_u32_decimal(entry.entry as u32);
    tos_serial::puts(b" vector=");
    tos_serial::put_u32_decimal(u32::from(vector));
    tos_serial::puts(b" deliveries=");
    tos_serial::put_u32_decimal(entry.deliveries);
    tos_serial::puts(b" cancelled_waiter=");
    tos_serial::put_u32_decimal(u32::from(waiter.is_some()));
    tos_serial::puts(b" vector_retired=1 asserted_by=nucleus\r\n");
}

/// What `irq_wait` found.
pub enum Wait {
    /// The latch was set: it is cleared and the call returns without blocking.
    Ready,
    /// Nothing has arrived and this context is now the source's one waiter.
    Blocked,
    /// A context is already waiting on this source.
    Occupied,
    /// The source has gone.
    Gone,
}

/// Registers a context as the source's waiter, or answers it immediately.
///
/// The one-bit latch of ADR-0082 §7, and the property it exists to have: **the
/// only completion event cannot be lost by racing the wait call.** An interrupt
/// that arrives before the driver enters `irq_wait` sets the bit, and the next
/// wait returns `OK` without blocking.
pub fn begin_wait(index: u32, generation: u32, waiter: usize) -> Wait {
    let usable = index as usize;
    if source(index, generation).is_none() {
        return Wait::Gone;
    }
    // SAFETY: single-context nucleus; the index was checked by `source`.
    let slot = &mut unsafe { table() }[usable];
    if slot.pending {
        slot.pending = false;
        return Wait::Ready;
    }
    if slot.waiter.is_some() {
        return Wait::Occupied;
    }
    slot.waiter = Some(waiter as u32);
    Wait::Blocked
}

/// The blocking reason one wait on this source is recorded under.
pub fn waiting_on(index: u32, generation: u32) -> Waiting {
    Waiting::Interrupt(index, generation)
}

/// One routed device interrupt has arrived.
///
/// Called only by the stub for the vector it names. Everything it does is
/// bounded: one device-register store to acknowledge, a walk of a fixed table,
/// and at most one frame write. It allocates nothing and takes no lock — the
/// discipline ADR-0023 set and ADR-0049 §5 extended to the only handler that
/// resumes a context.
///
/// **Acknowledgement is a local-APIC EOI and nothing else** (§7). MSI-X is
/// edge-delivered and unshared, so ending one needs no device register — which
/// is exactly why this transport was chosen and INTx was not: ending an INTx
/// would have meant reading a device's own status register from ring 0.
#[no_mangle]
extern "C" fn device_interrupt(slot: u32) {
    let vector = FIRST_DEVICE_VECTOR.wrapping_add(slot as u8);
    // First, and unconditionally: the interrupt is over whatever this nucleus
    // decides about it, and an unacknowledged edge would stop every later one.
    // SAFETY: the APIC page is mapped in every address space this nucleus runs,
    // and a zero to the EOI register is how an interrupt is acknowledged.
    unsafe { apic::end_of_interrupt() };
    // SAFETY: single-context nucleus; this handler cannot be re-entered, because
    // it runs with interrupts masked and nested interrupts are not enabled.
    let table = unsafe { table() };
    let found = table
        .iter()
        .position(|source| source.live && source.vector == vector);
    let Some(index) = found else {
        // **A late message on a retired vector ends here** (§5f, §7). Waking
        // "whoever used to hold this" would be delivering an event to authority
        // that has ended, so it is recorded and dropped.
        // SAFETY: as above; the only writer.
        let count = unsafe {
            let seen = core::ptr::addr_of!(SPURIOUS).read().wrapping_add(1);
            core::ptr::addr_of_mut!(SPURIOUS).write(seen);
            seen
        };
        tos_serial::puts(b"TOS.RUN.IRQ_SPURIOUS vector=");
        tos_serial::put_u32_decimal(u32::from(vector));
        tos_serial::puts(b" count=");
        tos_serial::put_u32_decimal(count);
        tos_serial::puts(b" woke=0 asserted_by=nucleus\r\n");
        return;
    };
    let source = &mut table[index];
    source.deliveries = source.deliveries.wrapping_add(1);
    let waiter = source.waiter.take();
    if waiter.is_none() {
        // Nobody is inside `irq_wait`, so the latch remembers that something
        // happened. A second interrupt before the first is collected coalesces
        // into the same bit, which is the honest statement: "something happened
        // since you last looked".
        source.pending = true;
    }
    let entry = source.entry;
    let deliveries = source.deliveries;
    // **The record of the delivery, made by the party that took it.** This is
    // the evidence that a wake came from the device through the source the
    // holder named, rather than from anything the host supplied: no other
    // context runs between the message arriving and this line.
    tos_serial::puts(b"TOS.RUN.IRQ_DELIVERED source=");
    tos_serial::put_u32_decimal(index as u32);
    tos_serial::puts(b" entry=");
    tos_serial::put_u32_decimal(entry as u32);
    tos_serial::puts(b" vector=");
    tos_serial::put_u32_decimal(u32::from(vector));
    tos_serial::puts(b" deliveries=");
    tos_serial::put_u32_decimal(deliveries);
    tos_serial::puts(b" woke=");
    tos_serial::put_u32_decimal(match waiter {
        Some(_) => 1,
        None => 0,
    });
    tos_serial::puts(b" latched=");
    tos_serial::put_u32_decimal(u32::from(waiter.is_none()));
    tos_serial::puts(b" asserted_by=nucleus\r\n");
    if let Some(waiter) = waiter {
        // SAFETY: the waiter is a context blocked in `irq_wait`, and `OK` is the
        // answer to that call. `wake` names the table it writes and this handler
        // holds no borrow of it.
        unsafe {
            process::wake(
                waiter as usize,
                crate::syscall::Answer::status(crate::syscall::OK),
            )
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_carries_the_vector_and_nothing_a_caller_chose() {
        let (address, data) = message_for(FIRST_DEVICE_VECTOR);
        // The architected local-APIC message window, physical destination 0.
        assert_eq!(address, apic::LOCAL_APIC);
        // Fixed delivery, edge, no level assert: the data word *is* the vector.
        assert_eq!(data, u32::from(FIRST_DEVICE_VECTOR));
    }

    #[test]
    fn the_device_range_clears_the_two_vectors_this_system_already_routes() {
        let last = FIRST_DEVICE_VECTOR as usize + DEVICE_VECTORS - 1;
        assert!(FIRST_DEVICE_VECTOR > apic::TIMER_VECTOR);
        assert!(last < apic::SPURIOUS_VECTOR as usize);
    }
}
