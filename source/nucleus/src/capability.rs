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
//! **Nothing here creates authority.** No operation of `SYSTEM_ABI_V1` produces
//! a capability, deliberately: an operation that manufactures authority and is
//! reachable without any is ambient authority with a handle in front of it. A
//! table is written by [`endow`] before its process is entered, from what
//! whoever launched it decided (ADR-0055). A process can shrink its table
//! (`capability_release`) or refine it (`capability_attenuate`); it has no way
//! to widen it.
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
}

impl Object {
    /// The `OBJECT_*` number this kind is described to a process by.
    fn kind(&self) -> u32 {
        match self {
            Object::None => 0,
            Object::Endpoint(_) => tos_launch::OBJECT_ENDPOINT,
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
    Ok(entry.object)
}

/// Gives a process a capability, and returns the handle it will name it by.
///
/// The only way an entry is ever written. It is reachable from the launcher and
/// from attenuation, and from nothing a process can call directly — which is
/// the whole of "no operation produces a capability".
pub fn grant(process: usize, object: Object, rights: u32, scope: u64) -> Option<u64> {
    // SAFETY: single-context nucleus; this is the only writer.
    let table = unsafe { tables() };
    if process >= MAX_PROCESSES {
        return None;
    }
    let index = table[process]
        .iter()
        .position(|entry| entry.object == Object::None)?;
    let entry = &mut table[process][index];
    entry.object = object;
    entry.rights = rights;
    entry.scope = scope;
    Some(handle(index, entry.generation))
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
    entry.object = Object::None;
    entry.rights = 0;
    entry.scope = 0;
    entry.generation = entry.generation.wrapping_add(1);
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
    grant(process, entry.object, narrowed, scope).ok_or(Refused::NoCapability)
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
    for entry in table[process].iter_mut() {
        if entry.object != Object::None {
            entry.object = Object::None;
            entry.rights = 0;
            entry.scope = 0;
            entry.generation = entry.generation.wrapping_add(1);
        }
    }
}

/// Writes a process's whole endowment, and describes it back for the record the
/// process reads (ADR-0055).
///
/// Returns how many entries were written. The description is what the process
/// will find in its launch record: which handle names what, so that a process
/// does not have to discover its own authority by guessing indices.
pub fn endow(
    process: usize,
    endowment: &[(Object, u32, u64)],
    out: &mut [LaunchCapability],
) -> u32 {
    let mut written = 0;
    for (object, rights, scope) in endowment.iter().take(out.len()) {
        let Some(handle) = grant(process, *object, *rights, *scope) else {
            break;
        };
        out[written] = LaunchCapability {
            handle,
            object: object.kind(),
            rights: *rights,
            scope: *scope,
        };
        written += 1;
    }
    written as u32
}
