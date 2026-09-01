// SPDX-License-Identifier: GPL-3.0-or-later
//! Authority, as a table the nucleus owns and a process cannot address.
//!
//! `CAPABILITY_V1` §2 fixes the shape: a handle is a process-local index into a
//! nucleus table, it is not a pointer, not a token, not a signed bearer value,
//! and it carries no rights in its own bits. What it carries is *where to look*
//! — an index and a generation — and everything that decides what the holder
//! may do is at the other end of that lookup, in memory no process has a
//! mapping to.
//!
//! **`SYSTEM_ABI_V1` creates no ambient authority.** Several operations return
//! a capability — 5 a refined one, `process_create` a handle to the child, a
//! call the right to answer it, 16 a child authority — and every one of them
//! has an explicit normative origin: the result is bounded by authority the
//! caller presented, or by a creation rule the contract accepted, and it never
//! widens either. What does not exist is an operation that manufactures
//! authority and is reachable without any, which is ambient authority with a
//! handle in front of it. A table is first written by [`endow`] before its
//! process is entered, from what whoever launched it decided (ADR-0055); a
//! process can shrink its table (`capability_release`) or refine it
//! (`capability_attenuate`), and has no way to widen it.
//!
//! **The generation is what makes a stale handle detectably stale.** Releasing a
//! capability and reusing its slot must not let an old index silently address
//! the new occupant, so the slot's generation advances on release and the old
//! handle stops matching. Generations start at one, so a handle of all zeros —
//! the value of a register nobody wrote — names nothing in any table.

use tos_launch::LaunchCapability;

use crate::process::MAX_PROCESSES;

/// How many capabilities one process may hold.
///
/// A fixed nucleus bound over statically reserved slots. It is not derived from
/// anything a caller says, because the table is the thing that decides what a
/// caller may do, and a table sized by its user is not a bound.
pub const MAX_CAPABILITIES: usize = 16;

/// What kind of object a capability names (`CAPABILITY_V1` §3).
///
/// `Endpoint` is constructed by whoever decides an endowment, and on a
/// canonical boot nobody does: `system.boot.init` requests no capability and
/// the launcher's constant grants nothing unrequested (ADR-0055). So a
/// production build genuinely never constructs it, and that is the policy
/// working rather than code going unused — the allow below says so rather than
/// letting the warning imply the variant is surplus.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Object {
    /// Nothing. Not a kind — the state of a slot nobody has been given.
    None,
    /// An IPC endpoint, named by its index in the nucleus's endpoint table.
    Endpoint(u32),
    /// A process, named by its slot. Authority *over a process* rather than
    /// over "processes": `CAPABILITY_V1` §3 admits an object and rules out a
    /// class, so there is no capability meaning "may create anything". A
    /// process that may create holds authority over the process a child is
    /// created under — its own — and that is what makes the chain terminate at
    /// whoever the launcher endowed.
    Process { slot: u32, generation: u32 },
    /// The right to answer one call, naming the context waiting for that answer
    /// (`IPC_V1` §4).
    ///
    /// **Single-use, and that is what the generation is for.** Replying
    /// consumes it, and so does anything else that ends the call — a
    /// cancellation, or the caller ending — because its lifetime is bounded by
    /// the caller's. What makes it single-use is not a flag anyone has to
    /// remember to clear: the counter it names moves, and the capability stops
    /// resolving. A reply that could be sent twice would be an unbounded channel
    /// back into a process that asked one question.
    Reply { caller: u32, generation: u32 },
    /// A finite memory authority, named by its accounting node (ADR-0076 §2b).
    ///
    /// **Several capabilities may name one of these**, and that is the whole
    /// difference from every kind above it. An endpoint outlives its handles, a
    /// process slot has its own generation, a reply is single-use — none of them
    /// has to be told when a name appears or goes. An authority does: the loss
    /// of its *last* name returns an unspent reservation up a funding lineage,
    /// and the aliases in between are names for one budget rather than several
    /// reservations.
    MemoryAuthority { index: u32, generation: u32 },
}

impl Object {
    /// The `OBJECT_*` number this kind is described to a process by.
    fn kind(&self) -> u32 {
        match self {
            Object::None => 0,
            Object::Endpoint(_) => tos_launch::OBJECT_ENDPOINT,
            Object::Process { .. } => tos_launch::OBJECT_PROCESS,
            Object::Reply { .. } => tos_launch::OBJECT_REPLY,
            Object::MemoryAuthority { .. } => tos_launch::OBJECT_MEMORY_AUTHORITY,
        }
    }
}

/// One entry: object, rights, scope, lifetime, generation (`CAPABILITY_V1` §2).
///
/// Lifetime is not a field yet and is not pretended to be one. Every capability
/// this stage issues is bounded by the life of the process that holds it, which
/// is the strictest lifetime there is and the one `CAPABILITY_V1` §3 requires as
/// a ceiling ("never longer than the grantor's own"). A field will be needed
/// when a capability can outlive its holder, and inventing it now would be a
/// column nothing writes and nothing checks.
#[derive(Clone, Copy)]
struct Entry {
    object: Object,
    rights: u32,
    scope: u64,
    generation: u32,
}

impl Entry {
    const EMPTY: Entry = Entry {
        object: Object::None,
        rights: 0,
        scope: 0,
        // One, not zero: a handle of all zeros must name nothing anywhere.
        generation: 1,
    };
}

/// Every process's table, indexed by process slot.
static mut TABLES: [[Entry; MAX_CAPABILITIES]; MAX_PROCESSES] =
    [[Entry::EMPTY; MAX_CAPABILITIES]; MAX_PROCESSES];

/// The tables.
///
/// # Safety
///
/// The nucleus is single-context: a process is not running while this is
/// reached, because everything that reaches it is either the launcher or the
/// system-call edge, and the edge runs with interrupts masked.
// SAFETY: the caller is nucleus code, which is the only writer, and the
// single-context argument above is why no second borrow can exist.
unsafe fn tables() -> &'static mut [[Entry; MAX_CAPABILITIES]; MAX_PROCESSES] {
    // SAFETY: the static is initialized at link time and lives for the whole
    // boot; this is the only way it is ever named.
    unsafe { &mut *core::ptr::addr_of_mut!(TABLES) }
}

/// A handle: the index in the low half, the generation in the high half.
///
/// Both halves are necessary and neither is a right. `CAPABILITY_V1` §2 states
/// validity as "index in range **and** generation matching", which a bare index
/// cannot express — there would be nothing to match against. The composition is
/// therefore not a choice about representation so much as the only shape that
/// rule admits.
pub fn handle(index: usize, generation: u32) -> u64 {
    (u64::from(generation) << 32) | index as u64
}

fn parts(handle: u64) -> (usize, u32) {
    ((handle & 0xffff_ffff) as usize, (handle >> 32) as u32)
}

/// Why a handle did not name what the caller needed.
///
/// The two are distinct and are never merged: the first says the caller named
/// nothing at all, the second that it holds the wrong authority. An audit log
/// that cannot tell them apart cannot describe an attack (`SYSTEM_ABI_V1` §4).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refused {
    /// The index is outside the process's table.
    BadHandle,
    /// The index is inside it, and the entry is stale, empty, of another type,
    /// or lacks the right.
    NoCapability,
}

/// Resolves a handle to what it names, in the refusal order ADR-0056 fixes:
/// **index bounds, then generation, then type, then rights**.
///
/// The first failure decides the answer, which is what makes the status a fact
/// about the call rather than about the caller. A process whose table is empty
/// gets [`Refused::BadHandle`] at every index — because there is no index
/// inside a table of size zero, and a process that holds nothing names nothing.
pub fn resolve(process: usize, handle: u64, rights: u32) -> Result<Object, Refused> {
    let (index, generation) = parts(handle);
    if process >= MAX_PROCESSES || index >= MAX_CAPABILITIES {
        return Err(Refused::BadHandle);
    }
    // SAFETY: single-context nucleus; bounds checked immediately above.
    let entry = unsafe { tables()[process][index] };
    if entry.generation != generation || entry.object == Object::None {
        return Err(Refused::NoCapability);
    }
    if entry.rights & rights != rights {
        return Err(Refused::NoCapability);
    }
    // A capability's lifetime is bounded by its object (`CAPABILITY_V1` §3), so
    // an entry whose object has ended is not a capability. Checked here, once,
    // rather than in each operation: an operation that had to remember to ask
    // is an operation that will one day forget.
    if !object_is_live(entry.object) {
        return Err(Refused::NoCapability);
    }
    Ok(entry.object)
}

/// The rights a handle carries, or none when it names nothing.
///
/// For deriving a new capability from an old one at the point of granting:
/// a parent hands on what it held, and this is what it held.
pub fn rights_of(process: usize, handle: u64) -> u32 {
    let (index, generation) = parts(handle);
    if process >= MAX_PROCESSES || index >= MAX_CAPABILITIES {
        return 0;
    }
    // SAFETY: single-context nucleus; bounds checked immediately above.
    let entry = unsafe { tables()[process][index] };
    if entry.generation != generation || entry.object == Object::None {
        return 0;
    }
    entry.rights
}

/// Why a capability was not given.
///
/// Two reasons, kept apart because they are two different facts about the
/// system and a caller that reported one as the other would send whoever reads
/// the log looking in the wrong place. A full table is a bound this nucleus
/// chose; a second receiver is a rule `IPC_V1` §2 chose.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotGranted {
    /// The process's table has no free slot.
    NoRoom,
    /// `IPC_V1` §2: another process already holds receive rights on that
    /// endpoint.
    ReceiverExists,
}

/// Which process holds receive rights on an endpoint, if one does.
///
/// `IPC_V1` §2: "An endpoint has exactly one receive-rights holder at a time. A
/// second one would make delivery non-deterministic in a way no schema could
/// describe." A *holder* is read here as a process rather than a capability:
/// what a second one would make non-deterministic is which context a message is
/// delivered to, and two handles inside one context do not create that question.
/// So a process may attenuate its own receive right and hold both results; a
/// second process may not hold one at all.
fn receiver_of(endpoint: u32) -> Option<usize> {
    // SAFETY: single-context nucleus; this reads and does not write, and no
    // `&mut` to the tables is alive — the one caller takes its own after this
    // returns, which is what keeps the two borrows from overlapping.
    let table = unsafe { tables() };
    (0..MAX_PROCESSES).find(|process| {
        table[*process].iter().any(|entry| {
            entry.object == Object::Endpoint(endpoint)
                && entry.rights & tos_launch::RIGHT_RECEIVE != 0
        })
    })
}

/// Takes a reference **a capability** holds, for what it is about to name.
///
/// **Two kinds of reference, not one, and the difference is not cosmetic.** A
/// memory authority counts both the same way — a handle and a message in flight
/// are each a name for one budget — and a region cannot: ADR-0075 §6 makes its
/// reclamation wait on capability references, mappings and internal references
/// separately, because the three are lost by three different events, and a
/// count that merged them would free backing while a message still carried it.
/// So the split is here, before the kind that needs it arrives.
///
/// **One door for every object kind, before there is a kind that needs it.**
/// Today `Endpoint`, `Process` and `Reply` count nothing: an endpoint outlives
/// its handles, a process's slot has its own generation, and a reply is
/// single-use by that generation. `MemoryAuthority` will not be like them — the
/// loss of its last name returns an unspent reservation up a funding lineage
/// (ADR-0075 §2a) — and the way that goes wrong is a special case appearing in
/// four places that today do not look at the kind at all. So the four places
/// route through here first.
///
/// Fallible on purpose. An authority's name count is bounded and refuses rather
/// than wrapping, so this is the step that can say no.
fn retain_capability(object: Object) -> Result<(), NotGranted> {
    match object {
        // Nothing to count, and nothing that a later kind should inherit by
        // being forgotten here: each arm is a decision.
        Object::None | Object::Endpoint(_) | Object::Process { .. } | Object::Reply { .. } => {
            Ok(())
        }
        Object::MemoryAuthority { index, generation } => {
            // SAFETY: single-context nucleus; nothing else holds the tree.
            unsafe { crate::memory::authority() }
                .retain(crate::region::AuthorityId { index, generation })
                .map_err(|_| NotGranted::NoRoom)
        }
    }
}

/// Drops the reference a destroyed capability entry held.
fn release_capability(object: Object) {
    match object {
        Object::None | Object::Endpoint(_) | Object::Process { .. } | Object::Reply { .. } => {}
        Object::MemoryAuthority { index, generation } => {
            // SAFETY: single-context nucleus; nothing else holds the tree.
            if unsafe { crate::memory::authority() }
                .release_name(crate::region::AuthorityId { index, generation })
                .is_err()
            {
                // The entry named a node the tree does not recognise, so a name
                // was destroyed that the accounting never counted. Fail closed
                // rather than go on funding from a tree that is a reference out.
                crate::memory::note_divergence(b"authority-name-release");
            }
        }
    }
}

/// Whether the object a handle names is still usable authority.
///
/// Distinct from the handle's own generation: an object can outlive the last
/// capability that named it — a memory-authority node does, while what it
/// funded is still alive — without being something a caller may still act
/// through.
fn object_is_live(object: Object) -> bool {
    match object {
        Object::None => false,
        // An endpoint is a table slot for the life of the boot.
        Object::Endpoint(_) => true,
        Object::Process { slot, generation } => {
            crate::process::generation(slot as usize) == Some(generation)
        }
        Object::Reply { caller, generation } => {
            crate::process::reply_token(caller as usize) == Some(generation)
        }
        // A node that survives only because allocations, charges or descendants
        // remain is not usable authority: its last name went, its generation
        // moved on, and its remainder was returned to whoever funded it. The
        // tree refusing to resolve it is what says so.
        Object::MemoryAuthority { index, generation } => {
            // SAFETY: single-context nucleus; nothing else holds the tree.
            unsafe { crate::memory::authority() }
                .remaining(crate::region::AuthorityId { index, generation })
                .is_ok()
        }
    }
}

/// Gives a process a capability, and returns the handle it will name it by.
///
/// The only way an entry is ever written, and the reason that matters is not
/// the one this comment used to give.
///
/// **The invariant is that no operation produces ambient authority**, not that
/// no operation produces a capability — several do, and always did. Operation 5
/// returns a derived one, `process_create` returns a handle to the child it
/// made, a call hands the receiver the right to answer it, and operation 16
/// will return a child authority. What every one of them has in common is an
/// explicit normative origin: the result is bounded by authority the caller
/// presented, or by a creation rule the contract accepted. Nothing here mints a
/// capability out of nothing, and that — rather than a count of operations that
/// return one — is what docs/02 rules out.
///
/// **This is where `IPC_V1` §2's one-receiver rule is kept**, because this is
/// the only door authority comes through. Checking it at `endpoint_receive`
/// instead would be checking it after it was already broken: two processes would
/// hold the right, and which of them got the message would depend on which
/// called first — the non-determinism the rule exists to prevent, refused one
/// step too late to matter.
pub fn grant(process: usize, object: Object, rights: u32, scope: u64) -> Result<u64, NotGranted> {
    if process >= MAX_PROCESSES {
        return Err(NotGranted::NoRoom);
    }
    // Before the table is borrowed, not during: `receiver_of` reads every
    // process's table, and holding a `&mut` across it would be a second borrow
    // of the same static.
    if rights & tos_launch::RIGHT_RECEIVE != 0 {
        if let Object::Endpoint(endpoint) = object {
            if receiver_of(endpoint).is_some_and(|holder| holder != process) {
                return Err(NotGranted::ReceiverExists);
            }
        }
    }
    // SAFETY: single-context nucleus; this is the only writer.
    let table = unsafe { tables() };
    let index = table[process]
        .iter()
        .position(|entry| entry.object == Object::None)
        .ok_or(NotGranted::NoRoom)?;
    // The last thing that can fail, and then nothing that can. A retain
    // followed by a fallible step would need a rollback; a retain followed only
    // by writes needs none, and the entry and the name appear together as far
    // as anything outside this function can tell.
    retain_capability(object)?;
    let entry = &mut table[process][index];
    entry.object = object;
    entry.rights = rights;
    entry.scope = scope;
    Ok(handle(index, entry.generation))
}

/// Releases a handle, and makes every copy of it stale.
///
/// The generation advances, so the index the caller just gave up cannot address
/// whatever occupies the slot next. Returns whether the handle named anything —
/// a release of something the caller does not hold is a refusal, not a no-op,
/// because a caller that believes it released authority it never had has been
/// told something false.
pub fn release(process: usize, handle: u64) -> Result<(), Refused> {
    let (index, generation) = parts(handle);
    if process >= MAX_PROCESSES || index >= MAX_CAPABILITIES {
        return Err(Refused::BadHandle);
    }
    // SAFETY: single-context nucleus; bounds checked immediately above.
    let entry = unsafe { &mut tables()[process][index] };
    if entry.generation != generation || entry.object == Object::None {
        return Err(Refused::NoCapability);
    }
    let object = entry.object;
    entry.object = Object::None;
    entry.rights = 0;
    entry.scope = 0;
    entry.generation = entry.generation.wrapping_add(1);
    // The entry is gone and the name goes with it, with nothing in between that
    // anything could observe: a released handle whose object still counted it,
    // or an object decremented while its entry stood, are the two halves of the
    // same defect.
    release_capability(object);
    Ok(())
}

/// Produces a capability whose rights and scope are each a subset of an
/// existing one's (`CAPABILITY_V1` §4).
///
/// The nucleus checks the subset relation; it does not take the caller's word.
/// **Widening is not an error code here, it is unreachable**: the rights of the
/// result are the intersection of what was asked for with what was held, so a
/// caller asking for more receives less rather than more. The scope is the
/// caller's, and a scope that is not the input's is refused rather than
/// silently narrowed, because a scope this stage cannot compare is a scope it
/// must not claim to have checked.
///
/// **On a `MemoryAuthority` this is an alias and can be nothing else.** The
/// result names the same accounting node, spending from the same remainder, and
/// it may keep `RIGHT_SPEND` — two handles spending one budget is what decision
/// B means, not two budgets. There is no path from here to a smaller authority:
/// the amount is not in this entry to narrow, it lives in the tree, and the
/// only thing that moves it is scoped attenuation, which makes a child node and
/// takes the parent's remainder down by what the child may spend. The scope
/// check above is what keeps a caller from expressing anything else here — an
/// authority's entry carries scope zero, so a caller inventing an amount is
/// refused before rights are even considered.
pub fn attenuate(process: usize, handle: u64, rights: u32, scope: u64) -> Result<u64, Refused> {
    let (index, generation) = parts(handle);
    if process >= MAX_PROCESSES || index >= MAX_CAPABILITIES {
        return Err(Refused::BadHandle);
    }
    // SAFETY: single-context nucleus; bounds checked immediately above.
    let entry = unsafe { tables()[process][index] };
    if entry.generation != generation || entry.object == Object::None {
        return Err(Refused::NoCapability);
    }
    // Every dimension a subset, and the intersection is what a subset means.
    let narrowed = entry.rights & rights;
    if narrowed == 0 {
        // A capability with no rights is not an attenuation, it is a slot.
        return Err(Refused::NoCapability);
    }
    if scope != entry.scope {
        return Err(Refused::NoCapability);
    }
    // Attenuation grants to the process that already held it, so the
    // one-receiver rule cannot fire here: the holder is the same holder.
    grant(process, entry.object, narrowed, scope).map_err(|_| Refused::NoCapability)
}

/// Empties a process's table.
///
/// Called when a process ends: its authority ends with it, and the generations
/// advance so that nothing written down about the old occupant addresses the
/// next one.
pub fn clear(process: usize) {
    // SAFETY: single-context nucleus; the process is over.
    let table = unsafe { tables() };
    if process >= MAX_PROCESSES {
        return;
    }
    // **One decrement per entry actually destroyed**, not one per object. A
    // process holding three aliases of one authority was holding three names,
    // and a sweep that noticed the authority once would leave two behind.
    for entry in table[process].iter_mut() {
        if entry.object != Object::None {
            let object = entry.object;
            entry.object = Object::None;
            entry.rights = 0;
            entry.scope = 0;
            entry.generation = entry.generation.wrapping_add(1);
            release_capability(object);
        }
    }
}

/// What a launcher decides to give a process.
///
/// Two shapes rather than one, because the second cannot be written as the
/// first: authority over the process being created names an object that does
/// not exist until the moment it is granted. Only whoever creates a process can
/// give it that, which is exactly why a process cannot obtain it, and cannot
/// spawn without having been given it.
///
/// Neither shape is constructed by a canonical boot, and the allow below says so
/// rather than letting the warning imply they are surplus: the launcher's
/// constant for `system.boot.init` grants nothing, because the module requests
/// nothing (ADR-0055). An endowment nobody writes is the policy holding.
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub enum Endowment {
    /// A capability naming an object that already exists.
    Existing {
        /// Which `import capability` of the module this answers (ADR-0061).
        binding: Binding,
        object: Object,
        rights: u32,
        scope: u64,
    },
    /// Authority over the process being created.
    Own { binding: Binding, rights: u32 },
    /// Everything the authority that funded this creation has left once the
    /// creation is paid for, as a child of it (ADR-0076 §3).
    ///
    /// Like [`Endowment::Own`] and for the same reason, it names something that
    /// does not exist when the launcher decides on it: the remainder is not
    /// known until the footprint has been charged, and the child node is made
    /// out of it there. **Not the funder itself** — a supervisor is given an
    /// ordinary child authority with an ordinary reference lifecycle, never the
    /// boot's accounting anchor, so a supervisor ending or being restarted
    /// cannot take the anchor with it.
    Remainder { binding: Binding, rights: u32 },
}

impl Endowment {
    /// The request this grant answers.
    fn binding(&self) -> &Binding {
        match self {
            Endowment::Existing { binding, .. }
            | Endowment::Own { binding, .. }
            | Endowment::Remainder { binding, .. } => binding,
        }
    }
}

/// The name a grant answers, as the nucleus carries it.
///
/// Bytes rather than text, and the nucleus never looks at them. Whether a name
/// is one the module actually declared is a question about a module, and the
/// nucleus does not read modules — the process does, at startup, and reports
/// `CapabilityDenied` for a request nothing answered. What the nucleus owes is
/// to carry the name whole or refuse it, which is why the only judgement here is
/// about length.
///
/// Carried by value rather than by reference: a launcher's constant is static
/// text and a parent's is in a region that a child's launch record outlives, and
/// one type that copies is simpler than two that borrow differently.
#[derive(Clone, Copy)]
pub struct Binding {
    bytes: [u8; tos_launch::MAX_BINDING as usize],
    length: u32,
}

impl Binding {
    /// A grant that answers no request. A launcher may make one; a module that
    /// asked for nothing simply never looks at it.
    pub const NONE: Binding = Binding {
        bytes: [0; tos_launch::MAX_BINDING as usize],
        length: 0,
    };

    /// A name, or nothing when it is too long for the record to carry it.
    ///
    /// Refused rather than truncated: a truncated name is still a name, of a
    /// request the module did not make.
    pub fn new(name: &[u8]) -> Option<Binding> {
        if name.len() as u64 > tos_launch::MAX_BINDING {
            return None;
        }
        let mut binding = Binding::NONE;
        binding.bytes[..name.len()].copy_from_slice(name);
        binding.length = name.len() as u32;
        Some(binding)
    }
}

/// Takes a name for every object a committed message carries.
///
/// **A message is a holder.** Delegation copies the object into the queue and
/// leaves the sender's handle alone (`CAPABILITY_V1` §4), so between a send
/// committing and a receive granting there is an interval in which the queue is
/// the only thing that will ever produce a holder — the sender may release its
/// handle, or end, before anybody receives. An object whose lifecycle counts
/// names has to be told about that interval or its last name goes while a
/// message still carries it.
///
/// Taken **before** the message is queued, so a failure here leaves nothing
/// half-committed. Today every kind counts nothing and this is structure rather
/// than effect; `MemoryAuthority` is what it is structure for.
fn retain_transit(object: Object) -> Result<(), NotGranted> {
    match object {
        Object::None | Object::Endpoint(_) | Object::Process { .. } | Object::Reply { .. } => {
            Ok(())
        }
        // A name like any other: one budget, several ways of reaching it.
        Object::MemoryAuthority { .. } => retain_capability(object),
        // Not a name. A region in transit has had its sender's handle and
        // mapping taken from it and its receiver has nothing yet, so this is
        // the nucleus's own reference and is counted as one (ADR-0075 §6).
        #[allow(unreachable_patterns)]
        _ => Ok(()),
    }
}

/// Drops a reference a message held.
fn release_transit(object: Object) {
    match object {
        Object::None | Object::Endpoint(_) | Object::Process { .. } | Object::Reply { .. } => {}
        Object::MemoryAuthority { .. } => release_capability(object),
        #[allow(unreachable_patterns)]
        _ => {}
    }
}

pub fn retain_in_transit(granted: &[(Object, u32, u64)]) -> Result<(), NotGranted> {
    for (position, (object, _, _)) in granted.iter().enumerate() {
        if *object == Object::None {
            continue;
        }
        if let Err(refused) = retain_transit(*object) {
            // Whatever was taken before the refusal goes back, so a send that
            // did not happen leaves no name behind it.
            release_from_transit(&granted[..position]);
            return Err(refused);
        }
    }
    Ok(())
}

/// Drops a message's names: it was delivered, or it was never queued.
///
/// After delivery this runs **once the receiver already holds its own names**,
/// so the count does not pass through zero on the way from one holder to the
/// next.
pub fn release_from_transit(granted: &[(Object, u32, u64)]) {
    for (object, _, _) in granted {
        if *object != Object::None {
            release_transit(*object);
        }
    }
}

/// Whether a process's table has room for one more capability.
///
/// Asked before an operation makes the object a capability would name, so that
/// a full table refuses before anything exists rather than after: a child
/// authority with no handle to it is a reservation nobody can spend or return.
pub fn has_room(process: usize) -> bool {
    if process >= MAX_PROCESSES {
        return false;
    }
    // SAFETY: single-context nucleus; bounds checked immediately above.
    let table = unsafe { tables() };
    table[process]
        .iter()
        .any(|entry| entry.object == Object::None)
}

/// How many capabilities a process holds, for evidence that a refusal left the
/// table as it found it.
#[cfg_attr(not(feature = "test-creation-rollback"), allow(dead_code))]
pub fn held(process: usize) -> usize {
    if process >= MAX_PROCESSES {
        return 0;
    }
    // SAFETY: single-context nucleus; bounds checked immediately above.
    let table = unsafe { tables() };
    table[process]
        .iter()
        .filter(|entry| entry.object != Object::None)
        .count()
}

/// Whether a whole endowment can be written, asked before any of it is.
///
/// **All or nothing, and the way to get that is to ask first.** ADR-0055 makes
/// a half-endowed child invalid: a process holding two of the three
/// capabilities its launcher decided on is a process nobody decided on, and it
/// has no way to know what it is missing. `endow` used to grant until something
/// refused and then stop — and it ran *after* the process was already in the
/// table, so the child was published, runnable and short of authority.
///
/// The alternative to this is a transaction log and a rollback. It is not
/// needed here: everything that can refuse is countable in advance against
/// fixed tables, so checking first turns the commit into a sequence of writes.
/// A rollback that never runs is a rollback nothing proves.
///
/// What is checked, in the order a refusal is most likely:
///
/// - the endowment fits a process's table at all;
/// - no entry claims the receive right on an endpoint somebody already
///   receives on (`IPC_V1` §2), which `grant` checks one entry at a time;
/// - and no **two entries of this endowment** claim it either — the check
///   inside `grant` cannot see that, because the first of the pair has not been
///   written when the second is validated.
///
/// `Endowment::Own` is deliberately not checked: it names the process being
/// created, whose slot does not exist yet. It cannot fail after the slot does,
/// which is the only place `endow` is called from.
pub fn endowable(endowment: &[Endowment]) -> Result<(), NotGranted> {
    if endowment.len() > MAX_CAPABILITIES {
        return Err(NotGranted::NoRoom);
    }
    // Every object named has to still be one, and every name that will be taken
    // has to be takeable. The second is a *sum*: an endowment naming one memory
    // authority three times costs it three names, and asking three times
    // whether one more would fit is not the same question.
    for (position, entry) in endowment.iter().enumerate() {
        let Endowment::Existing { object, .. } = *entry else {
            continue;
        };
        if !object_is_live(object) {
            return Err(NotGranted::NoRoom);
        }
        if let Object::MemoryAuthority { index, generation } = object {
            let names = endowment
                .iter()
                .filter(|other| {
                    matches!(
                        **other,
                        Endowment::Existing {
                            object: Object::MemoryAuthority { index: i, generation: g },
                            ..
                        } if i == index && g == generation
                    )
                })
                .count();
            // Asked once, at the first entry naming it, for all of them.
            if position == 0 || names > 0 {
                // SAFETY: single-context nucleus; nothing else holds the tree.
                let tree = unsafe { crate::memory::authority() };
                if !tree.can_retain(crate::region::AuthorityId { index, generation }, names) {
                    return Err(NotGranted::NoRoom);
                }
            }
        }
    }
    for (position, entry) in endowment.iter().enumerate() {
        let Endowment::Existing { object, rights, .. } = *entry else {
            continue;
        };
        if rights & tos_launch::RIGHT_RECEIVE == 0 {
            continue;
        }
        let Object::Endpoint(endpoint) = object else {
            continue;
        };
        if receiver_of(endpoint).is_some() {
            return Err(NotGranted::ReceiverExists);
        }
        let claimed_earlier = endowment[..position].iter().any(|earlier| {
            matches!(
                *earlier,
                Endowment::Existing {
                    object: Object::Endpoint(other),
                    rights: earlier_rights,
                    ..
                } if other == endpoint && earlier_rights & tos_launch::RIGHT_RECEIVE != 0
            )
        });
        if claimed_earlier {
            return Err(NotGranted::ReceiverExists);
        }
    }
    Ok(())
}

/// Writes a process's whole endowment, and describes it back for the record the
/// process reads (ADR-0055).
///
/// **Infallible by the time it runs.** [`endowable`] answered everything that
/// could refuse before the process was published, so a refusal here is a defect
/// in this file rather than a decision about the caller — it is reported as one
/// and the entries already written stand, because by now there is a running
/// process that they belong to.
///
/// Returns how many entries were written. The description is what the process
/// will find in its launch record: which handle names what, so that a process
/// does not have to discover its own authority by guessing indices.
pub fn endow(process: usize, endowment: &[Endowment], out: &mut [LaunchCapability]) -> u32 {
    // Not `take(out.len())`. A silent truncation would leave a process short of
    // authority its launcher decided on, which is the defect the preflight
    // exists to prevent, arriving by a different door. The record is sized from
    // this same slice by the only caller, so a mismatch is the nucleus having
    // lost track of its own sizing rather than anything a caller did.
    if out.len() < endowment.len() {
        crate::memory::note_divergence(b"endowment-record-undersized");
        return 0;
    }
    let mut written = 0;
    for entry in endowment.iter() {
        // The name before the authority: a grant whose binding does not fit is
        // refused rather than truncated, because a truncated name is a name —
        // of a request the module did not make (ADR-0061).
        // A `Binding` cannot be over-long by construction, so there is nothing
        // to check here: whoever built one was refused at that point instead.
        let binding = *entry.binding();
        let (object, rights, scope) = match *entry {
            Endowment::Existing {
                object,
                rights,
                scope,
                ..
            } => (object, rights, scope),
            // Resolved into an `Existing` before this runs: the remainder is
            // only known once the creation has been charged, and turning it
            // into a child node is a step that can refuse, which is not a thing
            // this function is allowed to do any more.
            Endowment::Remainder { .. } => break,
            Endowment::Own { rights, .. } => {
                let Some(generation) = crate::process::generation(process) else {
                    break;
                };
                (
                    Object::Process {
                        slot: process as u32,
                        generation,
                    },
                    rights,
                    0,
                )
            }
        };
        let handle = match grant(process, object, rights, scope) {
            Ok(handle) => handle,
            Err(refused) => {
                // The launcher's own constant asked for something the system
                // refuses. Silence here would leave a process holding less
                // authority than whoever launched it decided, with nothing on
                // the record saying so — and `CAPABILITY_V1` §2 requires the
                // endowment to be named rather than implied.
                tos_serial::puts(b"TOS.NUCLEUS.INVARIANT reason=endowment-preflight process=");
                tos_serial::put_u32_decimal(process as u32);
                tos_serial::puts(match refused {
                    NotGranted::NoRoom => b" reason=table-full\r\n",
                    NotGranted::ReceiverExists => b" reason=endpoint-already-received\r\n",
                });
                break;
            }
        };
        out[written] = LaunchCapability {
            handle,
            object: object.kind(),
            rights,
            scope,
            binding: binding.bytes,
            binding_length: binding.length,
            reserved: 0,
        };
        written += 1;
    }
    written as u32
}
