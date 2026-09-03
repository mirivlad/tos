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
    /// A region of memory in one of its two **affine** states, named by its
    /// object in the region table.
    ///
    /// **Affine, and that is what makes the type model hold.** ADR-0037 gives a
    /// `Region<mut T>` exactly one holder and ADR-0075 §5a makes its transfer
    /// linear, so a second handle to the same region is not an attenuation of
    /// authority — it is the writable alias the freeze exists to eliminate.
    /// Generic attenuation cannot make one, because generic attenuation does
    /// not consume its input and would therefore leave two.
    Region { index: u32, generation: u32 },
    /// The same region table, after `share` has consumed the affine form.
    ///
    /// **A separate variant rather than a flag, because affinity is structural
    /// and must not be read off the rights.** A capability's rights say what
    /// its holder may do; they do not say whether a second holder may exist.
    /// Deciding "may this be copied?" by testing for the absence of `write`
    /// would make an attenuation that dropped a right into a change of the
    /// object's kind — and the two questions genuinely differ, because an
    /// immutable *affine* region carries no write right either and still may
    /// not be copied.
    ///
    /// Both variants describe themselves to a process as `OBJECT_REGION`: the
    /// public kind space is not widened by an internal distinction, and a
    /// process learns what it may do from the rights it was granted.
    SharedRegion { index: u32, generation: u32 },
    /// A launch plan still being written (`SYSTEM_ABI_V1` §5 operations 21, 22).
    ///
    /// **Affine, like the two things it is shaped after.** A plan is a decision
    /// about a child, and a second holder of a builder is a second author of one
    /// decision — with no rule about which of them the seal catches. It is also
    /// the *holder* of every reference its entries describe, and an object whose
    /// destruction releases references must have exactly one death.
    LaunchPlanBuilder { index: u32, generation: u32 },
    /// A PCI bus scope, named by its bus object (ADR-0079 §5).
    ///
    /// **The only kind here that nothing makes.** Every other variant is
    /// produced by an operation, or by a launcher out of something it already
    /// held; a bus exists because the machine has one. So its origin is the
    /// third class `CAPABILITY_V1` §2 admits — minted at the boot/platform
    /// boundary, named with its scope in the launch record — and no operation of
    /// any contract returns one. Like an endpoint, the object outlives every
    /// handle to it and is a table slot for the life of the boot.
    PciBus(u32),
    /// One assignment of one PCI function (ADR-0079 §10).
    ///
    /// **Not affine, and the assignment's exclusivity is not the reason it might
    /// look affine.** Exactly one claim exists per function while it lives, but
    /// that is a property of `pci_function_claim` — several capabilities may name
    /// one assignment, because a later split between a bus manager and a driver
    /// needs attenuation to make a second, narrower name. So names are counted,
    /// as a memory authority's are, and the claim ends when the last one goes.
    PciFunction { index: u32, generation: u32 },
    /// The same plan after operation 23 consumed the builder.
    ///
    /// A separate variant for the reason `SharedRegion` is one — the state is
    /// structural and is not read off the rights — and a separate **public**
    /// kind, which the region deliberately is not. The difference is that a
    /// region's two forms declare the same operations while these two declare
    /// different ones: a builder is written to and sealed, a sealed plan is
    /// created from, and neither may be used where the other belongs. A launcher
    /// answering a request for one with the other would be answering a request
    /// for a decision that has been made with one that has not.
    LaunchPlan { index: u32, generation: u32 },
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
            Object::Region { .. } | Object::SharedRegion { .. } => tos_launch::OBJECT_REGION,
            Object::LaunchPlanBuilder { .. } => tos_launch::OBJECT_LAUNCH_PLAN_BUILDER,
            Object::LaunchPlan { .. } => tos_launch::OBJECT_LAUNCH_PLAN,
            Object::PciBus(_) => tos_launch::OBJECT_PCI_BUS,
            Object::PciFunction { .. } => tos_launch::OBJECT_PCI_FUNCTION,
        }
    }

    /// Whether a second capability may name the same object.
    ///
    /// **Affine kinds answer no, and no operation overrides it.** For a region
    /// the reason is the type model: a second handle to a mutable region is a
    /// second writer, which is exactly what the consuming freeze exists to make
    /// impossible (ADR-0037, ADR-0075 §5a). Generic attenuation is refinement
    /// and does not consume its input, so it could only ever *add* a name —
    /// which is why it refuses these rather than narrowing them.
    ///
    /// A shared region answers no in the other direction, and it is the variant
    /// that says so rather than the rights: `share` consumed the affine form to
    /// produce it, so a second name is a second reader of something that has no
    /// owner left to duplicate.
    /// A launch plan answers no in both states, and for a third reason: it is
    /// the holder of the references its entries took, so a second name for one
    /// would be a second release of every authority it describes.
    pub fn is_affine(&self) -> bool {
        matches!(
            self,
            Object::Region { .. } | Object::LaunchPlanBuilder { .. } | Object::LaunchPlan { .. }
        )
    }

    /// The plan this names, in either state.
    pub fn plan(&self) -> Option<crate::plan::PlanId> {
        match *self {
            Object::LaunchPlanBuilder { index, generation }
            | Object::LaunchPlan { index, generation } => {
                Some(crate::plan::PlanId { index, generation })
            }
            _ => None,
        }
    }

    /// Whether this names a region at all, in either state.
    ///
    /// Asked by every path that must refuse both: generic capability transfer,
    /// which copies, and `Endowment::Existing`, which copies too. A region
    /// travels in its own bound of two and by its own rules, or it does not
    /// travel.
    pub fn is_region(&self) -> bool {
        matches!(self, Object::Region { .. } | Object::SharedRegion { .. })
    }

    /// The region this names, whichever state it is in.
    pub fn region(&self) -> Option<crate::region::RegionId> {
        match *self {
            Object::Region { index, generation } | Object::SharedRegion { index, generation } => {
                Some(crate::region::RegionId { index, generation })
            }
            _ => None,
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
        Object::None
        | Object::Endpoint(_)
        | Object::Process { .. }
        | Object::Reply { .. }
        // A bus object is a table slot for the life of the boot, exactly as an
        // endpoint is: nothing destroys it, so nothing has to count its names.
        | Object::PciBus(_) => Ok(()),
        Object::MemoryAuthority { index, generation } => {
            // SAFETY: single-context nucleus; nothing else holds the tree.
            unsafe { crate::memory::authority() }
                .retain(crate::region::AuthorityId { index, generation })
                .map_err(|_| NotGranted::NoRoom)
        }
        // An assignment ends when its last name goes, so every name is counted
        // through the same door — which is what makes releasing one mean the
        // same thing however it was made.
        Object::PciFunction { index, generation } => {
            crate::pci::retain(index, generation).map_err(|_| NotGranted::NoRoom)
        }
        // Affine or not, the **region** is what says so. Operation 5 refuses to
        // make a second handle to an affine one, but an operation that reached
        // `grant` by another road would too — so the refusal lives in the
        // region rather than in every path that might one day get here, and the
        // same call serves the shared form, where a second name is admissible
        // and is counted.
        Object::Region { index, generation } | Object::SharedRegion { index, generation } => {
            // SAFETY: single-context nucleus; nothing else holds the tree.
            unsafe { crate::memory::authority() }
                .retain_capability(crate::region::RegionId { index, generation })
                .map_err(|_| NotGranted::NoRoom)
        }
        // Affine in both states, so there is never a second name to count. The
        // one name is made by the operation that made the plan and moved in
        // place by the one that seals it; the plan's own death is the loss of
        // that name, which `release_capability` performs.
        Object::LaunchPlanBuilder { .. } | Object::LaunchPlan { .. } => Ok(()),
    }
}

/// The reference a plan entry holds on the object it names.
///
/// The same door as a capability entry's, deliberately: a plan entry and a
/// capability entry are both *names* for one object, and an accounting that
/// counted them differently would be one where releasing a plan and releasing a
/// handle mean different things to the object they released.
pub(crate) fn retain_for_plan(object: Object) -> Result<(), NotGranted> {
    retain_capability(object)
}

/// And the release of it, once, when the plan is destroyed.
pub(crate) fn release_for_plan(object: Object) {
    release_capability(object)
}

/// Drops the reference a destroyed capability entry held.
fn release_capability(object: Object) {
    match object {
        Object::None | Object::Endpoint(_) | Object::Process { .. } | Object::Reply { .. } => {}
        // One capability naming a region goes, which is one of the three
        // ways a region can become unreachable (ADR-0075 §6).
        Object::Region { index, generation } | Object::SharedRegion { index, generation } => {
            // SAFETY: single-context nucleus; nothing else holds the tree.
            if unsafe { crate::memory::authority() }
                .release_capability(crate::region::RegionId { index, generation })
                .is_err()
            {
                // An entry named a region that was not holding a capability,
                // so a handle existed the object never counted. Fail closed.
                crate::memory::note_divergence(b"region-capability-release");
            }
        }
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
        // The one name for a plan goes, so the plan goes — and with it, exactly
        // once, every reference its entries took. This is the only path by
        // which a plan ends: an explicit release, or `clear` walking a dead
        // process's table, and both arrive here.
        Object::LaunchPlanBuilder { index, generation }
        | Object::LaunchPlan { index, generation } => {
            crate::plan::destroy(crate::plan::PlanId { index, generation });
        }
        // Nothing to give back: the bus object outlives every name for it.
        Object::PciBus(_) => {}
        // The last name going is what ends the claim, so the function becomes
        // claimable again by exactly the event that made it unreachable.
        Object::PciFunction { index, generation } => {
            if crate::pci::release(index, generation).is_err() {
                // The entry named an assignment the table does not recognise,
                // so a name was destroyed that the claim never counted. Fail
                // closed rather than go on assigning from a table that is a
                // reference out.
                crate::memory::note_divergence(b"pci-function-name-release");
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
        Object::Region { index, generation } | Object::SharedRegion { index, generation } => {
            // SAFETY: as above.
            unsafe { crate::memory::authority() }
                .mode(crate::region::RegionId { index, generation })
                .is_ok()
        }
        Object::LaunchPlanBuilder { index, generation }
        | Object::LaunchPlan { index, generation } => {
            crate::plan::is_live(crate::plan::PlanId { index, generation })
        }
        Object::PciBus(index) => crate::pci::bus_is_live(index),
        // A released assignment is not usable authority even where the slot has
        // been claimed again: the generation moved, so a handle kept across the
        // gap names the first claim and finds nothing.
        Object::PciFunction { index, generation } => crate::pci::is_live(index, generation),
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

/// Replaces one prevalidated affine entry with what it became, in the same
/// slot, under a new generation.
///
/// **The shape a consuming transition needs, and nothing else has it.** A
/// freeze and a share each take one affine region capability and produce
/// another naming the same region: the underlying object keeps exactly one
/// capability reference throughout, the old handle must stop resolving, and the
/// new one must exist by the time the caller is answered. Doing that with the
/// operations already here would need a `grant` — which takes a *second*
/// reference the affine region refuses, and a second table slot a full table
/// may not have — followed by a `release` that drops the first. Two fallible
/// steps to express one that cannot fail, with a window in between where the
/// region is named twice.
///
/// So the entry is rewritten where it stands. The generation advances, which is
/// what makes the caller's old handle detectably stale (`CAPABILITY_V1` §2),
/// and no reference is taken or dropped, which is what keeps the region's count
/// at one without it ever passing through zero or two.
///
/// **Prevalidated** is part of the contract: the caller has already resolved
/// this handle, checked the region's state and performed the transition. What
/// is left here is bookkeeping, and the only refusals are the ones that say the
/// handle stopped naming what the caller resolved — which, between two
/// statements of a single-context nucleus, cannot happen.
pub fn replace_in_place(
    process: usize,
    handle: u64,
    object: Object,
    rights: u32,
    scope: u64,
) -> Result<u64, Refused> {
    let (index, generation) = parts(handle);
    if process >= MAX_PROCESSES || index >= MAX_CAPABILITIES {
        return Err(Refused::BadHandle);
    }
    // SAFETY: single-context nucleus; bounds checked immediately above.
    let entry = unsafe { &mut tables()[process][index] };
    if entry.generation != generation || entry.object == Object::None {
        return Err(Refused::NoCapability);
    }
    entry.object = object;
    entry.rights = rights;
    entry.scope = scope;
    entry.generation = entry.generation.wrapping_add(1);
    Ok(self::handle(index, entry.generation))
}

/// How many of a process's entries name one object.
///
/// **A shared region's mapping lives one level above its handles**, and this is
/// the question that keeps the two in step: a process may hold several shared
/// capabilities for one region and still have exactly one window onto it, so a
/// release must know whether the handle going is the last local one before it
/// decides whether the window goes too.
///
/// A scan of a fixed table of sixteen entries, at a release, which is not a
/// path anything measures. Resolution itself is untouched and remains the
/// constant-time index-and-generation lookup `IPC_V1` §8 requires.
pub fn names_held(process: usize, object: Object) -> usize {
    if process >= MAX_PROCESSES || object == Object::None {
        return 0;
    }
    // SAFETY: single-context nucleus; bounds checked immediately above.
    let table = unsafe { tables() };
    table[process]
        .iter()
        .filter(|entry| entry.object == object)
        .count()
}

/// How many free slots a process's table still has.
///
/// The plural of [`has_room`], because a message's preflight has to ask about
/// everything it carries at once: four capabilities and two regions can need up
/// to six, and asking "is there room for one?" six times is a different
/// question that a full-but-one table answers yes to every time.
pub fn room(process: usize) -> usize {
    if process >= MAX_PROCESSES {
        return 0;
    }
    // SAFETY: single-context nucleus; bounds checked immediately above.
    let table = unsafe { tables() };
    table[process]
        .iter()
        .filter(|entry| entry.object == Object::None)
        .count()
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
/// **On an affine object it refuses.** A region has exactly one capability by
/// construction, and refinement adds rather than moves — see the refusal below.
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
    // **An affine object has no second name, and refinement can only make
    // one.** Attenuation does not consume its input (`CAPABILITY_V1` §4), so
    // for a region it would leave two handles where the type model allows one —
    // and for a mutable region that second handle is the writable alias the
    // consuming freeze exists to eliminate (ADR-0037, ADR-0075 §5a). Refused as
    // "the caller does not hold what this operation needs", which is what it
    // is: there is no attenuation of this object to hold.
    if entry.object.is_affine() {
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
        Object::None
        | Object::Endpoint(_)
        | Object::Process { .. }
        | Object::Reply { .. }
        // Nothing counts a bus object's names, so a delegated one takes nothing.
        | Object::PciBus(_) => Ok(()),
        // A name like any other: one budget, several ways of reaching it.
        Object::MemoryAuthority { .. } => retain_capability(object),
        // The same, for an assignment: a delegation makes another name for one
        // claim, and the claim outlives whichever name goes first.
        Object::PciFunction { .. } => retain_capability(object),
        // Unreachable: a region does not travel in the generic transfer table
        // at all. It has a bound of its own (`IPC_V1` §3) and a lifecycle of
        // its own — an internal reference rather than a name (ADR-0075 §6) —
        // and both `resolve_transfers` and `endowable` refuse it before
        // anything gets here. Refused rather than quietly counted, so a future
        // path that reached this by mistake stops rather than making a region
        // one more delegated capability.
        //
        // A launch plan is refused here for the same shape of reason and a
        // different one underneath: it is affine, so a delegation that copied
        // it would produce a second holder of the decision *and* a second
        // release of every reference its entries took.
        Object::Region { .. }
        | Object::SharedRegion { .. }
        | Object::LaunchPlanBuilder { .. }
        | Object::LaunchPlan { .. } => Err(NotGranted::NoRoom),
    }
}

/// Drops a reference a message held.
fn release_transit(object: Object) {
    match object {
        Object::None
        | Object::Endpoint(_)
        | Object::Process { .. }
        | Object::Reply { .. }
        | Object::PciBus(_) => {}
        Object::MemoryAuthority { .. } | Object::PciFunction { .. } => release_capability(object),
        // As above: never taken, so never given back.
        Object::Region { .. }
        | Object::SharedRegion { .. }
        | Object::LaunchPlanBuilder { .. }
        | Object::LaunchPlan { .. } => {}
    }
}

/// Takes the **internal** reference a queued message holds on a region.
///
/// Not a name, and the difference is the whole of why regions do not travel in
/// the generic table. An affine region in transit has had its sender's handle
/// and mapping taken from it and its receiver has nothing yet: for that stretch
/// nothing a process holds names it, and only this keeps it from being
/// reclaimed out from under the message. A shared one in transit is still named
/// by its sender, and this is what keeps it alive if the sender releases the
/// last of its own handles before the receiver has one.
pub fn retain_region_in_transit(region: crate::region::RegionId) -> Result<(), NotGranted> {
    // SAFETY: single-context nucleus; nothing else holds the tree.
    unsafe { crate::memory::authority() }
        .retain_internal(region)
        .map_err(|_| NotGranted::NoRoom)
}

/// Drops it: the message was delivered, or it was never queued.
///
/// After delivery this runs once the receiver already holds its own capability
/// and its own mapping, so the region is never unreachable in between.
pub fn release_region_from_transit(region: crate::region::RegionId) {
    // SAFETY: single-context nucleus; nothing else holds the tree.
    if unsafe { crate::memory::authority() }
        .release_internal(region)
        .is_err()
    {
        crate::memory::note_divergence(b"region-transit-release");
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

/// Whether every object a delivered message carries could be granted to one
/// process, asked before any of it is.
///
/// **The same all-or-nothing question [`endowable`] asks, for the other way
/// authority arrives.** A receive used to grant until something refused and
/// write a zero handle for the rest, which is a partial delivery wearing a
/// success status: the receiver would be told it had a message and be short of
/// exactly the authority the message was about. `IPC_V1` §3 says a message is
/// delivered whole or not at all, and the only way to mean that is to ask
/// first.
///
/// Table room is **not** asked here, because a message's regions need slots
/// too and one question about the total is not the same as several about the
/// parts. The caller counts what it needs and asks [`room`] once.
///
/// What is checked is exactly what [`grant`] checks, entry by entry, and
/// nothing more: that no entry claims receive rights on an endpoint **another
/// process** already receives on (`IPC_V1` §2 is about which context a message
/// is delivered to, so two handles inside one table do not create the
/// question), and that every name that will be taken can be — summed, because
/// one message naming one authority twice costs it two names and asking twice
/// whether one more fits is a different question.
///
/// **Whether the object is still *usable* is deliberately not asked**, because
/// `grant` does not ask it either. A capability's lifetime is bounded by its
/// object (`CAPABILITY_V1` §3) and [`resolve`] is where that is enforced, once,
/// when the holder tries to act through it. Asking here would refuse a message
/// the commit would have delivered — and one case of that is not hypothetical:
/// `endpoint_call` hands the receiver the right to answer **before** the caller
/// blocks, so at the instant of delivery the reply capability names a call that
/// has not begun to wait yet.
///
/// A preflight stricter than its commit refuses legal messages; one looser
/// fails half way. This is neither.
pub fn can_grant_all(process: usize, granted: &[(Object, u32, u64)]) -> bool {
    for (object, rights, _) in granted.iter() {
        if *object == Object::None {
            continue;
        }
        if let Object::MemoryAuthority { index, generation } = *object {
            let names = granted
                .iter()
                .filter(|(other, _, _)| {
                    matches!(*other, Object::MemoryAuthority { index: i, generation: g }
                        if i == index && g == generation)
                })
                .count();
            // SAFETY: single-context nucleus; nothing else holds the tree.
            let tree = unsafe { crate::memory::authority() };
            if !tree.can_retain(crate::region::AuthorityId { index, generation }, names) {
                return false;
            }
        }
        if rights & tos_launch::RIGHT_RECEIVE == 0 {
            continue;
        }
        let Object::Endpoint(endpoint) = *object else {
            continue;
        };
        if receiver_of(endpoint).is_some_and(|holder| holder != process) {
            return false;
        }
    }
    true
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
        // **An endowment copies, and a region does not arrive by being
        // copied.** For the affine forms the reason is the type model: the
        // parent keeps its handle while the child is given one, which is two
        // holders where exactly one is allowed. For the shared form a copy
        // would be admissible in principle and is still refused, because the
        // capability is only half of what a holder needs — the other half is a
        // mapping in the child's address space, and that space does not exist
        // when the endowment is decided. Operations 19 and 20 are where a
        // process is created *with* a region, and until they exist there is no
        // honest way to say it here.
        if object.is_region() {
            return Err(NotGranted::ReceiverExists);
        }
        // And a plan is refused because a plan is what this *is*. An entry
        // naming another plan would give a child a decision its parent is still
        // holding, with two holders of one affine object at the end of it; the
        // way to give a child launch policy is to create it from a plan, not to
        // hand it one.
        if object.plan().is_some() {
            return Err(NotGranted::ReceiverExists);
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
