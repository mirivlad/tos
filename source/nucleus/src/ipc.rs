// SPDX-License-Identifier: GPL-3.0-or-later
//! Endpoints, and the one way a message crosses between two processes.
//!
//! `IPC_V1` fixes the shape and ADR-0057 fixes the three numbers it said it
//! declared: **256 inline bytes, 4 transferred capabilities, 2 transferred
//! regions**. This module implements the inline half — a message of bytes,
//! delivered whole or not at all, through a bounded queue that is never grown
//! to accept anything.
//!
//! **Where the payload actually crosses.** `SYSTEM_ABI_V1` §3 admits values and
//! handles as arguments and no pointer the nucleus walks; six registers cannot
//! carry 256 bytes. So the payload does not travel in the call: each process has
//! a message slot the launcher mapped at a fixed address, and the call names
//! only how many of its bytes are a message. The nucleus reads and writes that
//! slot through its own identity map, never through the process's — the same
//! arrangement the report region has used since the first process, and for the
//! same reason: the process's mapping is a thing the process could in principle
//! change, and this is the boundary.
//!
//! **Acceptance is a transaction, and this module is split so that it can be.**
//! A message carries capabilities and regions (`IPC_V1` §5, §6), and delivering
//! it is not one act but three: find out what the oldest message contains, build
//! everything it needs in the receiver, and only then take it off the queue.
//! Doing those in one step is what makes a partial delivery expressible — a
//! message dequeued and then found unacceptable is a message nobody has and
//! nobody can retry. So [`peek`] answers what is there without moving it,
//! the caller commits, and [`take`] copies the payload and pops. If the caller
//! cannot commit, it never calls [`take`] and the queue is exactly as it was.
//!
//! The send side has the same shape for the same reason: [`has_room`] is a pure
//! question, asked before a linear region is taken away from its sender, so a
//! full queue can never be discovered after the sender has already lost it.

use tos_frames::FRAME_SIZE;

use crate::capability::Object;

/// The bounds ADR-0057 fixed for this contract version.
pub const MAX_INLINE_BYTES: u64 = 256;

/// How many endpoints this nucleus has, and how deep each queue is.
///
/// Both are fixed nucleus bounds over statically reserved storage: `IPC_V1` §7
/// requires that a queue never be grown to accept a message, and the way to
/// never grow one is to never have allocated it.
pub const MAX_ENDPOINTS: usize = 4;
const QUEUE_DEPTH: usize = 4;

/// One message in flight: bytes, how many of them mean anything, and the
/// authority travelling with them.
///
/// **What is queued is the *object*, not the sender's handle.** A handle is a
/// name in one process's table and means nothing in another's, and the sender
/// may release it — or end — between the send and the delivery. So the send
/// resolves what it was given and the queue carries that; the receiver's own
/// handle is made when the message reaches it, in its own table, with its own
/// generation, which is what `CAPABILITY_V1` §4 says delegation is.
#[derive(Clone, Copy)]
struct Message {
    bytes: [u8; MAX_INLINE_BYTES as usize],
    length: u64,
    granted: [(Object, u32, u64); MAX_TRANSFERRED as usize],
    granted_count: usize,
    regions: [Object; MAX_TRANSFERRED_REGIONS as usize],
    region_count: usize,
}

impl Message {
    const EMPTY: Message = Message {
        bytes: [0; MAX_INLINE_BYTES as usize],
        length: 0,
        granted: [(Object::None, 0, 0); MAX_TRANSFERRED as usize],
        granted_count: 0,
        regions: [Object::None; MAX_TRANSFERRED_REGIONS as usize],
        region_count: 0,
    };
}

/// What is in the oldest message on an endpoint, without taking it.
///
/// **Everything a receiver's preflight needs and not one byte of payload.** The
/// preflight has to decide whether it can build a table entry for each
/// capability, an address-space window for each region and the page tables both
/// need; none of those questions is about the 256 bytes, and copying them here
/// would be a third payload copy for a message `IPC_V1` §8 budgets two.
#[derive(Clone, Copy)]
pub struct Pending {
    pub granted: [(Object, u32, u64); MAX_TRANSFERRED as usize],
    pub granted_count: usize,
    pub regions: [Object; MAX_TRANSFERRED_REGIONS as usize],
    pub region_count: usize,
}

/// How many capabilities one message may carry (ADR-0057).
pub const MAX_TRANSFERRED: u64 = tos_launch::MAX_TRANSFERRED_CAPABILITIES;

/// And how many regions, which is a separate bound over a separate area
/// (`IPC_V1` §3).
///
/// Spelled in full rather than as `MAX_REGIONS`, because the region **table**
/// has a bound of that name and the two are unrelated numbers: this is how many
/// regions one message may carry, and that is how many exist at once.
pub const MAX_TRANSFERRED_REGIONS: u64 = tos_launch::MAX_TRANSFERRED_REGIONS;

/// An endpoint: a bounded queue, and the count of what is in it.
struct Endpoint {
    live: bool,
    queue: [Message; QUEUE_DEPTH],
    /// How many messages are queued, and where the oldest is.
    count: usize,
    head: usize,
}

impl Endpoint {
    const EMPTY: Endpoint = Endpoint {
        live: false,
        queue: [Message::EMPTY; QUEUE_DEPTH],
        count: 0,
        head: 0,
    };
}

static mut ENDPOINTS: [Endpoint; MAX_ENDPOINTS] = [Endpoint::EMPTY; MAX_ENDPOINTS];

/// How many messages have been handed to a receiver since the boot began.
///
/// The scheduler's one question when it finds nothing to run and somebody
/// waiting: has anything moved since the last time it asked? A count of
/// deliveries answers it without the scheduler knowing anything about messages,
/// and it is the difference between "waiting for something that has not happened
/// yet" and "waiting for each other" (ADR-0059).
static mut DELIVERIES: u64 = 0;

/// Payload copies made, and messages queued, since the boot began.
///
/// `IPC_V1` §8 bounds an inline message at two payload copies, and §9.7 asks for
/// the count to be **counted** rather than estimated. These are the count: one
/// increment beside each `copy_nonoverlapping` of a payload, so a copy that
/// nobody remembered to account for would have to be written past the counter
/// standing next to it.
static mut PAYLOAD_COPIES: u64 = 0;
static mut MESSAGES: u64 = 0;

/// Those two.
pub fn cost() -> (u64, u64) {
    // SAFETY: single-context nucleus; the writers are the send and receive
    // paths, which run with interrupts masked.
    unsafe { (MESSAGES, PAYLOAD_COPIES) }
}

/// That count.
pub fn deliveries() -> u64 {
    // SAFETY: single-context nucleus; the only writer is `receive`, which runs
    // with interrupts masked.
    unsafe { DELIVERIES }
}

/// The endpoint table.
///
/// # Safety
///
/// The nucleus is single-context, and everything that reaches this is either
/// the launcher or the system-call edge, which runs with interrupts masked.
// SAFETY: the caller is nucleus code, which is the only writer, and the
// single-context argument above is why no second borrow can exist.
unsafe fn endpoints() -> &'static mut [Endpoint; MAX_ENDPOINTS] {
    // SAFETY: the static is initialized at link time and lives for the whole
    // boot; this is the only way it is ever named.
    unsafe { &mut *core::ptr::addr_of_mut!(ENDPOINTS) }
}

/// Creates an endpoint and returns its index, or nothing when there is no room.
///
/// Reachable from the launcher and from nothing a process can call:
/// `SYSTEM_ABI_V1` §5 assigns no operation that creates an object, and an
/// object a process could conjure would be authority it granted itself.
///
/// On a canonical boot nothing calls it, because the launcher's constant endows
/// the boot process with nothing (ADR-0055). That is the policy holding, not a
/// function waiting to be deleted.
#[allow(dead_code)]
pub fn create() -> Option<u32> {
    // SAFETY: single-context nucleus; this is the only writer.
    let endpoints = unsafe { endpoints() };
    let index = endpoints.iter().position(|endpoint| !endpoint.live)?;
    endpoints[index] = Endpoint::EMPTY;
    endpoints[index].live = true;
    Some(index as u32)
}

/// Why a message did not move.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refused {
    /// The declared length is beyond what this contract version carries inline,
    /// or the endpoint index names nothing.
    BadArgument,
    /// The queue is full. `IPC_V1` §7: the system never grows a queue to accept
    /// a message, and backpressure is visible to the sender.
    Limit,
    /// A non-blocking receive with nothing to take.
    WouldBlock,
}

/// Queues `length` bytes of `from` on an endpoint. Delivered whole or not at
/// all: there is no partial send, so no receiver ever has half a message.
///
/// # Safety
///
/// `from` is the physical address of the sending process's message slot, which
/// the launcher mapped and the nucleus can read through its own identity map.
// SAFETY: the caller's promise that `from` is the launcher's own mapping is what
// makes the read below a read of nucleus-known memory rather than of an address
// a process chose.
pub unsafe fn send(
    endpoint: u32,
    from: u64,
    length: u64,
    granted: &[(Object, u32, u64)],
    regions: &[Object],
) -> Result<(), Refused> {
    if length > MAX_INLINE_BYTES
        || granted.len() as u64 > MAX_TRANSFERRED
        || regions.len() as u64 > MAX_TRANSFERRED_REGIONS
    {
        // Refused, not truncated (`IPC_V1` §9.1). A message shortened to the
        // bound and reported as sent would make the receiver's copy a different
        // message from the sender's.
        return Err(Refused::BadArgument);
    }
    // SAFETY: single-context nucleus; this is the only writer.
    let endpoints = unsafe { endpoints() };
    let slot = endpoints
        .get_mut(endpoint as usize)
        .filter(|endpoint| endpoint.live)
        .ok_or(Refused::BadArgument)?;
    if slot.count == QUEUE_DEPTH {
        return Err(Refused::Limit);
    }
    let at = (slot.head + slot.count) % QUEUE_DEPTH;
    let message = &mut slot.queue[at];
    // SAFETY: `from` is the launcher's mapping of a whole frame, per the
    // caller's contract, and `length` is bounded by `MAX_INLINE_BYTES` above,
    // which is far inside it.
    unsafe {
        core::ptr::copy_nonoverlapping(
            core::ptr::with_exposed_provenance::<u8>(from as usize),
            message.bytes.as_mut_ptr(),
            length as usize,
        )
    };
    // SAFETY: single-context nucleus with interrupts masked; this is one of the
    // two writers, and it stands beside the copy it counts.
    unsafe {
        PAYLOAD_COPIES += 1;
        MESSAGES += 1;
    }
    message.length = length;
    message.granted_count = granted.len();
    message.granted[..granted.len()].copy_from_slice(granted);
    message.regions = [Object::None; MAX_TRANSFERRED_REGIONS as usize];
    message.region_count = regions.len();
    message.regions[..regions.len()].copy_from_slice(regions);
    slot.count += 1;
    Ok(())
}

/// Whether this endpoint has room for one more message.
///
/// **A pure question, and that is its whole purpose.** A send that carries a
/// linear region takes the region away from its sender before it queues
/// anything; discovering a full queue *after* that would leave the region
/// belonging to nobody, and putting it back means rebuilding a mapping, which
/// needs page tables and can fail on its own. So the room is asked for first,
/// while nothing has moved.
pub fn has_room(endpoint: u32) -> Result<(), Refused> {
    // SAFETY: single-context nucleus; this reads and does not write.
    let endpoints = unsafe { endpoints() };
    let slot = endpoints
        .get(endpoint as usize)
        .filter(|endpoint| endpoint.live)
        .ok_or(Refused::BadArgument)?;
    if slot.count == QUEUE_DEPTH {
        return Err(Refused::Limit);
    }
    Ok(())
}

/// What the oldest message on an endpoint carries, leaving it where it is.
///
/// The first step of an acceptance: the receiver's preflight is answered from
/// this, and if it cannot be satisfied nothing has been dequeued and the
/// message is still there for a later attempt.
pub fn peek(endpoint: u32) -> Result<Pending, Refused> {
    // SAFETY: single-context nucleus; this reads and does not write.
    let endpoints = unsafe { endpoints() };
    let slot = endpoints
        .get(endpoint as usize)
        .filter(|endpoint| endpoint.live)
        .ok_or(Refused::BadArgument)?;
    if slot.count == 0 {
        return Err(Refused::WouldBlock);
    }
    let message = &slot.queue[slot.head];
    Ok(Pending {
        granted: message.granted,
        granted_count: message.granted_count,
        regions: message.regions,
        region_count: message.region_count,
    })
}

/// Copies the oldest message's payload into a process's message slot and takes
/// it off the queue.
///
/// The last step of an acceptance, and the only one that changes the queue.
/// Every grant and every mapping the message needed already exists by the time
/// this runs, which is what makes "delivered whole or not at all" a property of
/// the code rather than a claim about it.
///
/// # Safety
///
/// `into` is the physical address of the receiving process's message slot, at
/// least one frame long, which the launcher mapped, and the caller has already
/// committed everything the message carries.
// SAFETY: as `send`, for the write side.
pub unsafe fn take(endpoint: u32, into: u64) -> Result<u64, Refused> {
    // SAFETY: single-context nucleus; this is the only writer.
    let endpoints = unsafe { endpoints() };
    let slot = endpoints
        .get_mut(endpoint as usize)
        .filter(|endpoint| endpoint.live)
        .ok_or(Refused::BadArgument)?;
    if slot.count == 0 {
        return Err(Refused::WouldBlock);
    }
    let message = &slot.queue[slot.head];
    let length = message.length;
    // SAFETY: `into` is the launcher's mapping of a whole frame, per the
    // caller's contract, and the length was bounded when the message was
    // queued.
    unsafe {
        core::ptr::copy_nonoverlapping(
            message.bytes.as_ptr(),
            core::ptr::with_exposed_provenance_mut::<u8>(into as usize),
            length as usize,
        )
    };
    slot.head = (slot.head + 1) % QUEUE_DEPTH;
    slot.count -= 1;
    // SAFETY: single-context nucleus; this is the only writer. The second of
    // the two copies an inline message costs.
    unsafe {
        DELIVERIES = DELIVERIES.wrapping_add(1);
        PAYLOAD_COPIES += 1;
    };
    Ok(length)
}

/// Copies a reply's payload straight from one context's argument region into
/// another's.
///
/// A reply does not go through a queue: it is not waiting for a receiver, it is
/// the answer to a context that is already waiting for it. One copy, bounded by
/// the same inline maximum as everything else.
///
/// # Safety
///
/// Both addresses are argument regions the launcher mapped, at least one frame
/// long, which the nucleus reaches through its own identity map.
// SAFETY: the caller's promise that both are launcher mappings is what makes
// this a copy between two regions the nucleus chose the addresses of.
pub unsafe fn hand(from: u64, into: u64, length: u64) -> Result<u64, Refused> {
    if length > MAX_INLINE_BYTES {
        return Err(Refused::BadArgument);
    }
    // SAFETY: both regions are whole frames per the caller's contract, and
    // `length` is bounded well inside one.
    unsafe {
        core::ptr::copy_nonoverlapping(
            core::ptr::with_exposed_provenance::<u8>(from as usize),
            core::ptr::with_exposed_provenance_mut::<u8>(into as usize),
            length as usize,
        )
    };
    // SAFETY: single-context nucleus; this is the only writer.
    //
    // A reply is **one** message and **one** copy, not two: it goes from the
    // replier's region straight into the waiting caller's, because there is
    // nobody to queue it for. Counted as a message all the same, so that a
    // reader dividing copies by messages sees the reply pulling the average
    // *below* two rather than a message that cost nothing.
    unsafe {
        DELIVERIES = DELIVERIES.wrapping_add(1);
        PAYLOAD_COPIES += 1;
        MESSAGES += 1;
    };
    Ok(length)
}

/// The message slot is one frame, of which this contract version uses 256
/// bytes. Asserting it here means a change to either number that made the
/// payload not fit stops the build rather than producing a copy past the
/// mapping.
const _: () = assert!(MAX_INLINE_BYTES <= FRAME_SIZE);
