// SPDX-License-Identifier: GPL-3.0-or-later
//! Launch plans: what a process decides a child will hold, written down before
//! the child exists.
//!
//! **A plan is bounded nucleus metadata, not authority over an external
//! object.** It names capabilities its owner already holds, at rights no wider
//! than the owner's own, under the bindings the owner chose. Nothing here can
//! make authority, widen it, or reach anything the owner could not reach by
//! calling directly — what it can do is hold the decision still between the
//! moment it is made and the moment a child is created from it, and hold it
//! *again* when that child has to be replaced.
//!
//! Three states and two of them are public, in the shape ADR-0075 §4 gave a
//! region:
//!
//!   - a **builder** is affine and writable. Entries are added to it one at a
//!     time (operation 22), each through the capability being delegated;
//!   - **sealing** (operation 23) consumes the builder and replaces it in the
//!     same capability slot with its generation advanced, exactly as `freeze`
//!     does to a region. After that the entries cannot change;
//!   - a **sealed plan** is affine and immutable, and — unlike a frozen
//!     region — is *not* consumed by the creation that reads it. That is the
//!     whole point of it: a restart is the same policy applied to a new process
//!     instance, and a plan that a creation consumed would make the second
//!     launch a second decision.
//!
//! **The plan holds the references its entries describe.** An entry is a
//! derived reference to an object, retained when the entry is written and
//! released exactly once when the plan is destroyed — by an explicit release of
//! its capability, or by the death of the process that held it. A creator may
//! therefore release the original capability after placing an entry in a plan;
//! the plan is then an explicit holder of that authority, which is the opposite
//! of inheritance. Nothing is held implicitly and nothing is held twice.
//!
//! No entry carries a reservation of its own. An endowment of a memory
//! authority is another *name* for one budget (ADR-0076 §2b), and a plan that
//! reserved on its own account would be a way to spend a supervisor's funding
//! by writing policy.

use crate::capability::{Binding, Object};

/// How many plans exist at once.
///
/// One per process slot: a plan is a supervisor's statement about a child, and
/// a system with four process slots has no use for more launch policies than it
/// has processes to apply them to. Bounded like every other nucleus table, and
/// for the same reason — a table that grew on demand would be a process
/// deciding how much of the nucleus it occupies.
pub const MAX_PLANS: usize = crate::process::MAX_PROCESSES;

/// One entry: which request it answers, what it names, and at what rights.
///
/// The same four facts a `CREATE_ENDOWMENT` record carried, which is not a
/// coincidence — this replaces that table as the authoritative input to a
/// creation. What changed is *when* they are decided and *who* holds them in
/// between: a record in the caller's argument region was read at the instant of
/// creation and belonged to nobody until then, and this is an object with an
/// owner, a lifetime, and a reference of its own on everything it names.
#[derive(Clone, Copy)]
pub struct Entry {
    pub binding: Binding,
    pub object: Object,
    pub rights: u32,
    pub scope: u64,
}

impl Entry {
    const EMPTY: Entry = Entry {
        binding: Binding::NONE,
        object: Object::None,
        rights: 0,
        scope: 0,
    };
}

/// One plan.
struct Plan {
    live: bool,
    sealed: bool,
    /// Advanced when the slot is freed and again when the builder is sealed, so
    /// that a handle to either earlier state names nothing.
    generation: u32,
    entries: [Entry; tos_launch::MAX_ENDOWMENT as usize],
    count: usize,
}

impl Plan {
    const EMPTY: Plan = Plan {
        live: false,
        sealed: false,
        // One, not zero: a handle of all zeros must name nothing anywhere.
        generation: 1,
        entries: [Entry::EMPTY; tos_launch::MAX_ENDOWMENT as usize],
        count: 0,
    };
}

static mut PLANS: [Plan; MAX_PLANS] = [const { Plan::EMPTY }; MAX_PLANS];

/// The plan table.
///
/// # Safety
///
/// The nucleus is single-context: everything that reaches this is the
/// system-call edge, which runs with interrupts masked, or the launcher, which
/// runs before any process does.
// SAFETY: the caller is nucleus code, which is the only writer, and the
// single-context argument above is why no second borrow can exist.
unsafe fn plans() -> &'static mut [Plan; MAX_PLANS] {
    // SAFETY: the static is initialized at link time and lives for the whole
    // boot; this is the only way it is ever named.
    unsafe { &mut *core::ptr::addr_of_mut!(PLANS) }
}

/// Which plan, and which incarnation of that slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanId {
    pub index: u32,
    pub generation: u32,
}

/// Why an operation on a plan was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// No slot, or no room for another entry.
    Full,
    /// The id names no live plan of that incarnation.
    Unknown,
    /// The plan is in the other state: a sealed one cannot be written, and a
    /// builder is not a decision anything may be created from.
    WrongState,
}

/// Makes an empty builder, or refuses because every slot is in use.
pub fn create() -> Result<PlanId, Refusal> {
    // SAFETY: single-context nucleus; this is the only writer.
    let plans = unsafe { plans() };
    let index = plans
        .iter()
        .position(|plan| !plan.live)
        .ok_or(Refusal::Full)?;
    let plan = &mut plans[index];
    plan.live = true;
    plan.sealed = false;
    plan.count = 0;
    plan.entries = [Entry::EMPTY; tos_launch::MAX_ENDOWMENT as usize];
    Ok(PlanId {
        index: index as u32,
        generation: plan.generation,
    })
}

/// The live plan an id names, in the state the caller requires.
fn at(id: PlanId, sealed: bool) -> Result<&'static mut Plan, Refusal> {
    // SAFETY: single-context nucleus; this is the only writer.
    let plans = unsafe { plans() };
    let plan = plans
        .get_mut(id.index as usize)
        .ok_or(Refusal::Unknown)
        .and_then(|plan| match plan.live && plan.generation == id.generation {
            true => Ok(plan),
            false => Err(Refusal::Unknown),
        })?;
    match plan.sealed == sealed {
        true => Ok(plan),
        false => Err(Refusal::WrongState),
    }
}

/// Adds one entry to a builder.
///
/// **The reference is taken here**, so that the plan is a holder from the moment
/// the entry exists rather than from the moment a child is created. A creator
/// that then releases its own capability has handed the authority to the plan;
/// one that does not has two names for it, which is what having two names
/// means. Neither is a special case.
pub fn endow(id: PlanId, entry: Entry) -> Result<(), Refusal> {
    let plan = at(id, false)?;
    if plan.count >= plan.entries.len() {
        return Err(Refusal::Full);
    }
    // Nothing partial: the reference is taken before the entry is written, and
    // an object that cannot be named once more leaves the plan as it was.
    crate::capability::retain_for_plan(entry.object).map_err(|_| Refusal::Full)?;
    plan.entries[plan.count] = entry;
    plan.count += 1;
    Ok(())
}

/// Seals a builder, advancing the slot's generation.
///
/// The caller replaces its own capability in place, which is the other half of
/// this: the object is the same object and the handle that named the builder
/// names nothing afterwards (ADR-0075 §5b, applied to a different kind).
pub fn seal(id: PlanId) -> Result<PlanId, Refusal> {
    let plan = at(id, false)?;
    plan.sealed = true;
    plan.generation = plan.generation.wrapping_add(1);
    Ok(PlanId {
        index: id.index,
        generation: plan.generation,
    })
}

/// The entries of a sealed plan, for the creation that is about to apply them.
///
/// Sealed only. A builder is a decision still being made, and creating from one
/// would be creating from whatever had been written by the time the call
/// happened.
pub fn entries(id: PlanId) -> Result<&'static [Entry], Refusal> {
    let plan = at(id, true)?;
    Ok(&plan.entries[..plan.count])
}

/// Whether an id names a live plan in either state, for the liveness question
/// every capability kind is asked.
pub fn is_live(id: PlanId) -> bool {
    // SAFETY: single-context nucleus; this is a read.
    let plans = unsafe { plans() };
    plans
        .get(id.index as usize)
        .is_some_and(|plan| plan.live && plan.generation == id.generation)
}

/// Destroys a plan and releases every reference its entries held, exactly once.
///
/// Reached from the loss of the one capability that named it — an explicit
/// release, or the clearing of a dead process's table. There is no other way
/// for a plan to end, because there is no second name for one.
pub fn destroy(id: PlanId) {
    // SAFETY: single-context nucleus; this is the only writer.
    let plans = unsafe { plans() };
    let Some(plan) = plans.get_mut(id.index as usize) else {
        return;
    };
    if !plan.live || plan.generation != id.generation {
        return;
    }
    for entry in &plan.entries[..plan.count] {
        crate::capability::release_for_plan(entry.object);
    }
    plan.live = false;
    plan.sealed = false;
    plan.count = 0;
    plan.entries = [Entry::EMPTY; tos_launch::MAX_ENDOWMENT as usize];
    plan.generation = plan.generation.wrapping_add(1);
}

/// How many plans are live, for the boot's own accounting line.
pub fn live() -> usize {
    // SAFETY: single-context nucleus; this is a read.
    unsafe { plans() }.iter().filter(|plan| plan.live).count()
}
