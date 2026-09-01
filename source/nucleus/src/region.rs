// SPDX-License-Identifier: GPL-3.0-or-later
//! Memory authority and region objects, as the nucleus owns them (ADR-0075).
//!
//! Two tables and one rule between them.
//!
//! A **`MemoryAuthority`** is the right to spend a finite amount of memory. It
//! is never created out of nothing: the root one is part of the boot endowment
//! (ADR-0075 §2b), and every other is *reserved* out of a parent by attenuation.
//! Reservation is what keeps the arithmetic honest — a parent's remainder falls
//! when a child is made, and an allocation debits only the authority it was made
//! through, because everything above it already paid.
//!
//! ```text
//! attenuate(parent, n)   parent.remaining -= n, child.budget = child.remaining = n
//! allocate(child, n)     child.remaining -= n, a region appears
//! release / revoke       a child's unspent, unreserved remainder returns upward
//! reclaim                a dead region's bytes return to the authority that
//!                        funded it, and drain upward from there
//! ```
//!
//! A **`RegionObject`** is memory with an identity, a mode and a count of what
//! can still reach it. It is mutable when it is made, immutable for good after
//! the consuming transition, and reclaimed only when nothing — no capability, no
//! mapping, no internal reference — can reach it.
//!
//! **What this module is not.** It maps nothing, it touches no page table and it
//! has no syscall: it is the state machine those will be written against, so
//! that the accounting and the lifecycle can be proved before any of them
//! exists. The ABI numbers and registers ADR-0075 leaves open are not needed
//! here and are not chosen here.
//!
//! Bounded like everything else in the nucleus: fixed tables, no allocation, and
//! a refusal rather than a wait when one is full.

/// How many authorities the nucleus tracks at once.
///
/// A number the nucleus chooses, not one an input chooses: a table sized from a
/// value some process asked for is a table an attacker sizes.
pub const MAX_AUTHORITIES: usize = 64;

/// How many region objects exist at once.
pub const MAX_REGIONS: usize = 64;

/// How many process charges can be outstanding at once.
///
/// One per live process, plus room for the ones being built: a creation charges
/// before it has a slot to keep the receipt in, and a creation that fails
/// returns its charge before anything else needs the room. Bounded like every
/// other table here, and a full one refuses.
pub const MAX_CHARGES: usize = 16;

/// Why an operation on an authority or a region was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// The table is full. Bounded means bounded.
    NoRoom,
    /// The authority or region named does not exist, or its generation is stale.
    NotFound,
    /// The authority has less than was asked of it. The request was well
    /// formed; the memory is not there (`E_LIMIT`).
    Budget,
    /// The request cannot be expressed in the arithmetic that would have to
    /// serve it — a size whose rounding overflows, or a receipt that claims
    /// more than the node it names is holding (`E_BAD_ARGUMENT`).
    ///
    /// Separate from [`Refusal::Budget`] on purpose (ADR-0076 §7): a caller
    /// that asked for something impossible and a caller that asked for
    /// something unaffordable need different answers, and collapsing them tells
    /// the first one to retry later.
    BadArgument,
    /// The region is not in the mode this operation requires.
    WrongMode,
    /// The caller is not the one holder this operation requires.
    NotTheHolder,
    /// A zero-sized authority or region, which is authority over nothing.
    Empty,
    /// The tree has been stopped: the pool and the accounting were found to
    /// disagree, so nothing new may be reserved or spent anywhere in it. Giving
    /// things back is still allowed — see [`Regions::poison`].
    ///
    /// **Not ordinary budget exhaustion, and the ABI has no word for it.** The
    /// accepted status space of `SYSTEM_ABI_V1` is closed, and inventing a
    /// status as an implementation detail is not on offer. When an operation
    /// carries this out to ring 3 it answers the nearest accepted resource
    /// refusal, and what that answer means is written down where the operation
    /// is: the caller is refused fail-closed, the nucleus has already said
    /// `TOS.NUCLEUS.INVARIANT … funding-stopped` on the console, and the
    /// refusal is **not** evidence that the authority's own budget ran out. If
    /// that conflation ever turns out to matter to a caller, a distinct status
    /// is a decision to bring to the Architect, not one to take here.
    Stopped,
    /// The root is the boot's accounting anchor and has no naming lifecycle:
    /// it cannot be retained, released or revoked.
    Anchored,
}

/// A handle into the authority table, with the generation that makes a stale
/// one detectable (`CAPABILITY_V1` §2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorityId {
    pub index: u32,
    pub generation: u32,
}

/// The same, for regions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionId {
    pub index: u32,
    pub generation: u32,
}

/// What a region may be done to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    /// Readable and writable, held by exactly one process, neither shareable
    /// nor transferable (ADR-0037).
    Mutable,
    /// Readable and shareable, transferable into exactly one holder, and never
    /// writable again.
    Immutable,
}

/// A request rounded to what the machine actually spends (ADR-0076 §7).
///
/// An overflow is a refusal rather than a wrap: a rounded figure that came back
/// smaller than what was asked for would charge less than it spent. It is a
/// *domain* refusal — no budget could have satisfied it — which is why it is
/// [`Refusal::BadArgument`] and not [`Refusal::Budget`].
fn round_up(bytes: usize, granule: usize) -> Result<usize, Refusal> {
    if granule <= 1 {
        return Ok(bytes);
    }
    let over = bytes % granule;
    if over == 0 {
        return Ok(bytes);
    }
    bytes
        .checked_add(granule - over)
        .ok_or(Refusal::BadArgument)
}

/// Responsibility for retiring one region, and the only thing that can credit
/// its bytes back.
///
/// **Physical first, accounting second.** The frames a region is made of belong
/// to the pool and the tables that map it belong to the reserve; the bytes are
/// not free until both are back. Neither `Copy` nor `Clone`, so a retirement
/// cannot be finished twice, and it names the slot *and* the generation, so it
/// cannot finish a later occupant of the same slot.
#[must_use = "a retirement that is dropped is memory neither account gets back"]
#[derive(Debug, Eq, PartialEq)]
pub struct Reclaim {
    slot: u32,
    generation: u32,
    bytes: usize,
}

/// A receipt naming one outstanding charge in the ledger.
///
/// **It names the charge, not the amount.** An earlier form carried the slot of
/// the *authority* and the bytes, and checked at refund that the authority was
/// holding at least that much. That is not an identity. One authority funding
/// two processes of 54 MiB holds 108 MiB; the first returns normally and leaves
/// 54 MiB; a duplicate of the first receipt then finds 54 MiB there and takes
/// it — the second process's bytes, returned while that process is still
/// running. Non-`Copy` stops a safe caller from expressing it, but the
/// accounting underneath could not tell the two charges apart, and an
/// accounting model whose safety rests on nobody having a bug in the layer
/// above is not one.
///
/// So each charge gets its own bounded slot, and the receipt is a handle onto
/// it. The ledger — not the caller, and not the authority's total — holds which
/// authority funded it, how many bytes, and whether it is still outstanding.
/// A refund finds *that* charge or finds nothing.
///
/// Still neither `Copy` nor `Clone`, so an ordinary lifecycle cannot even try:
/// losing a receipt leaks budget, which the accounting shows, and there is no
/// way to spend one twice, which would create budget.
///
/// The ledger names the funding node by its **accounting incarnation**, not its
/// capability generation. A revoke bumps the generation and leaves the node
/// standing while what it funded is still alive (ADR-0075 §2a), and a grant made
/// through that capability must still return down the lineage that paid for it.
#[must_use = "a charge that is dropped is memory the tree never gets back"]
#[derive(Debug, Eq, PartialEq)]
pub struct GrantCharge {
    slot: u32,
    incarnation: u32,
    /// What was debited, carried for the caller to report. The ledger's copy is
    /// the one a refund believes.
    charged: usize,
}

impl GrantCharge {
    /// What was actually debited — the rounded figure, which is what the
    /// launch record and the operation's result report (ADR-0076 §7).
    pub fn charged(&self) -> usize {
        self.charged
    }

    /// A second receipt for one charge, so a test can attempt what production
    /// cannot express.
    ///
    /// Production has no way to make one: the type is neither `Copy` nor
    /// `Clone`, and a process slot holds its receipt in an `Option` it `take`s
    /// once. But "the type system prevents it" is a claim about the code that
    /// exists, and the refusal underneath it is worth proving on its own — so
    /// the test build, and only the test build, can forge a duplicate and
    /// watch the accounting refuse it.
    #[cfg(test)]
    pub fn forged_duplicate(&self) -> GrantCharge {
        GrantCharge {
            slot: self.slot,
            incarnation: self.incarnation,
            charged: self.charged,
        }
    }
}

/// One accounting node. Nucleus-owned; no process has a mapping to it.
#[derive(Clone, Copy, Debug, Default)]
struct Authority {
    live: bool,
    /// How many live references can still produce a valid holder of this node:
    /// capability handles, plus a committed delegation still in transit. Not a
    /// count of reservations — every one of them names the same budget.
    names: usize,
    generation: u32,
    /// Which occupant of this slot this is. Moves when the slot is reused and
    /// at no other time, so a charge outlives a revoke and dies with a reuse.
    incarnation: u32,
    /// `None` for the root.
    parent: Option<u32>,
    /// What this node was given.
    budget: usize,
    /// What it may still reserve or spend itself.
    remaining: usize,
    /// The sum of its children's budgets.
    reserved: usize,
    /// The backing live under it, charged to this node itself.
    allocated: usize,
    /// How many children still exist, so a drained node knows it is drained.
    children: usize,
}

/// One region object.
#[derive(Clone, Copy, Debug, Default)]
struct Region {
    live: bool,
    generation: u32,
    bytes: usize,
    mutable: bool,
    /// Which authority paid for it, and that generation.
    charged_to: u32,
    /// How many capabilities name it.
    capabilities: usize,
    /// How many references the nucleus itself holds: a committed delegation
    /// still in transit, and later anything else that must keep a region alive
    /// while no process can reach it (ADR-0075 §6).
    internal: usize,
    /// How many address spaces have it mapped, by mode.
    writable_mappings: usize,
    readable_mappings: usize,
    /// Which process holds it while it is mutable, and while it is immutable
    /// and unshared. `u32::MAX` is nobody.
    holder: u32,
    /// Nothing can reach it any more, and its backing has not been given back
    /// yet. It resolves to nothing from here on, and its slot is not reusable
    /// until the physical retirement has finished.
    reclaimable: bool,
    /// Somebody is performing that retirement, and holds the receipt.
    taken: bool,
}

/// One outstanding process charge, as the ledger holds it.
#[derive(Clone, Copy, Debug, Default)]
struct Charge {
    live: bool,
    /// Which occupant of this slot this is. Moves when a charge is settled, so
    /// the receipt that settled it never resolves again and neither does one
    /// from a previous occupant.
    incarnation: u32,
    /// The accounting node that paid, and which occupant of *that* slot it was.
    authority: u32,
    authority_incarnation: u32,
    bytes: usize,
}

/// The nucleus's memory-authority and region state.
pub struct Regions {
    authorities: [Authority; MAX_AUTHORITIES],
    regions: [Region; MAX_REGIONS],
    charges: [Charge; MAX_CHARGES],
    /// Set once the pool and this tree have been found to disagree, and never
    /// cleared. See [`Regions::poison`].
    stopped: bool,
}

impl Default for Regions {
    fn default() -> Regions {
        Regions::new()
    }
}

impl Regions {
    pub const fn new() -> Regions {
        Regions {
            authorities: [Authority {
                live: false,
                names: 0,
                generation: 0,
                incarnation: 0,
                parent: None,
                budget: 0,
                remaining: 0,
                reserved: 0,
                allocated: 0,
                children: 0,
            }; MAX_AUTHORITIES],
            regions: [Region {
                live: false,
                generation: 0,
                bytes: 0,
                mutable: false,
                charged_to: 0,
                capabilities: 0,
                internal: 0,
                writable_mappings: 0,
                readable_mappings: 0,
                holder: u32::MAX,
                reclaimable: false,
                taken: false,
            }; MAX_REGIONS],
            charges: [Charge {
                live: false,
                incarnation: 0,
                authority: 0,
                authority_incarnation: 0,
                bytes: 0,
            }; MAX_CHARGES],
            stopped: false,
        }
    }

    /// Stops the whole tree from committing or reserving anything further.
    ///
    /// **Why the latch belongs here and not at one caller.** The effect used to
    /// be that `memory::root()` stopped answering, which was enough only while
    /// the root was the one thing anybody could fund from. It stops being
    /// enough the moment a process holds a child `MemoryAuthority` of its own:
    /// operation 16 would reserve out of that child, 17 would spend it, 19 and
    /// 20 would fund from it, and not one of them would have asked the root
    /// anything. A rule of the form "every future system call must remember to
    /// check a flag" is a rule that will be forgotten in exactly one of them.
    ///
    /// So the tree itself refuses. Every path that *increases* what is
    /// reserved or committed — [`Regions::attenuate`], [`Regions::charge_grant`]
    /// and [`Regions::allocate_rounded`], which is all of them — answers
    /// [`Refusal::Stopped`], and an operation added later has to go through one
    /// of those three to spend anything.
    ///
    /// **Giving back keeps working, and that is the point.** Releasing a
    /// capability, revoking an authority, refunding a charge, reclaiming a
    /// region, returning an unused reservation and settling a lineage all still
    /// run: the machine has to be able to shrink safely towards a state
    /// somebody can inspect. What it may not do is grow on numbers that are
    /// known to be wrong.
    pub fn poison(&mut self) {
        self.stopped = true;
    }

    /// Whether the tree has been stopped.
    pub fn stopped(&self) -> bool {
        self.stopped
    }

    /// The root authority over a pool, once the nucleus's fixed reserves are
    /// out of it (ADR-0076 §2).
    ///
    /// **The bootstrap chain, in one call.** `usable` is what the boot admitted;
    /// `reserved` is the part that is bounded and proved before any process
    /// exists — the process table, the page tables a proved maximum of address
    /// spaces needs, and the nucleus's own metadata. What is left is the root
    /// authority, and after this returns there is no other source of dynamic
    /// user memory: a grant is charged through [`charge_grant`], a region
    /// through [`allocate`], and nothing else spends.
    ///
    /// Refused rather than clamped when the reserves are the whole pool: a root
    /// authority over nothing is authority over nothing. That refusal is
    /// [`Refusal::Budget`] and not [`Refusal::BadArgument`] — nothing about the
    /// request is malformed, the machine is simply smaller than what has to
    /// come out of it first.
    pub fn endow_root_after_reserves(
        &mut self,
        usable: usize,
        reserved: usize,
    ) -> Result<AuthorityId, Refusal> {
        let root = usable.checked_sub(reserved).ok_or(Refusal::Budget)?;
        self.endow_root(root)
    }

    /// Charges a process grant to an authority (ADR-0076 §3).
    ///
    /// **A grant is an allocation and the capability is not consumed.** The
    /// charge is held against this authority's node for as long as the process
    /// lives; [`Regions::refund_grant`] returns it up the same funding lineage.
    /// What is charged is the rounded figure, because that is what the machine
    /// spends.
    ///
    /// The receipt is the record of it. Nothing else can undo this charge, and
    /// the receipt cannot undo it twice.
    pub fn charge_grant(
        &mut self,
        authority: AuthorityId,
        bytes: usize,
        granule: usize,
    ) -> Result<GrantCharge, Refusal> {
        if self.stopped {
            return Err(Refusal::Stopped);
        }
        let charged = round_up(bytes, granule)?;
        if charged == 0 {
            return Err(Refusal::Empty);
        }
        let at = self.authority(authority)?;
        if self.authorities[at].remaining < charged {
            return Err(Refusal::Budget);
        }
        let slot = self.free_charge()?;
        self.authorities[at].remaining -= charged;
        self.authorities[at].allocated += charged;
        self.charges[slot] = Charge {
            live: true,
            incarnation: self.charges[slot].incarnation,
            authority: at as u32,
            authority_incarnation: self.authorities[at].incarnation,
            bytes: charged,
        };
        Ok(GrantCharge {
            slot: slot as u32,
            incarnation: self.charges[slot].incarnation,
            charged,
        })
    }

    /// Returns a process grant when the process is reclaimed, consuming the
    /// receipt that made it.
    ///
    /// It travels the way a region's backing does: to the authority that funded
    /// it while that authority is still named, and past it to its parent when it
    /// is not. The caller does not say how much — the receipt does, and it says
    /// the figure that was debited.
    ///
    /// Settling is atomic in the sense that matters: the charge is struck out
    /// of the ledger and its incarnation moved on **before** any budget moves,
    /// so no second attempt can find it half-done.
    ///
    /// Two refusals, and neither of them clamps:
    ///
    /// - a receipt for a charge that is not outstanding names nothing
    ///   ([`Refusal::NotFound`]) — whether it was already settled or its slot
    ///   has since been given to another charge. This is what makes a duplicate
    ///   harmless no matter what else the same authority is funding.
    /// - a ledger entry whose funding node no longer holds it is a defect
    ///   ([`Refusal::BadArgument`]), and refusing keeps the defect a leak
    ///   rather than letting it mint budget.
    pub fn refund_grant(&mut self, charge: GrantCharge) -> Result<(), Refusal> {
        let slot = charge.slot as usize;
        if slot >= MAX_CHARGES {
            return Err(Refusal::NotFound);
        }
        let entry = self.charges[slot];
        if !entry.live || entry.incarnation != charge.incarnation {
            return Err(Refusal::NotFound);
        }
        let at = entry.authority as usize;
        let node = self.authorities[at];
        // Deliberately a refusal and not a `debug_assert`. Reaching here means
        // the lifecycle above lost track of what it funded, and the nucleus's
        // job at that moment is to not compound it: refusing leaves the bytes
        // stranded, which the accounting will show, while asserting would take
        // the machine down over a leak and clamping would turn the leak into
        // invented budget. The refusal is the report.
        if !node.live
            || node.incarnation != entry.authority_incarnation
            || node.allocated < entry.bytes
        {
            return Err(Refusal::BadArgument);
        }
        self.charges[slot].live = false;
        self.charges[slot].incarnation = entry.incarnation.wrapping_add(1);
        self.authorities[at].allocated -= entry.bytes;
        self.give_back_or_keep(at, entry.bytes);
        self.settle_authority(at);
        Ok(())
    }

    /// The root authority of a boot, from the pool the nucleus has left after
    /// its own fixed reserves (ADR-0075 §2b).
    ///
    /// Called once, by the boot path, with a size the launcher decided and the
    /// audit record names. Nothing else may make one: every other authority is
    /// reserved out of one that already exists.
    pub fn endow_root(&mut self, bytes: usize) -> Result<AuthorityId, Refusal> {
        if bytes == 0 {
            return Err(Refusal::Empty);
        }
        let index = self.free_authority()?;
        let generation = self.authorities[index].generation.wrapping_add(1);
        let incarnation = self.authorities[index].incarnation.wrapping_add(1);
        self.authorities[index] = Authority {
            live: true,
            // The root is reachable because the boot made it, not because a
            // capability names it (§4 of the ownership decision): ring 3 never
            // holds it and it must outlive every process.
            names: 1,
            generation,
            incarnation,
            parent: None,
            budget: bytes,
            remaining: bytes,
            reserved: 0,
            allocated: 0,
            children: 0,
        };
        Ok(AuthorityId {
            index: index as u32,
            generation,
        })
    }

    /// Reserves `bytes` out of an authority and returns the child that holds
    /// them.
    ///
    /// **This is the only way a second authority comes to exist**, and it moves
    /// budget rather than copying it: the parent can no longer spend what the
    /// child now may. Attenuation makes authority; it does not make a region.
    ///
    /// The child comes back with **one name, and it is the maker's**. Whoever
    /// called this holds it and has to do one of two things with it: hand it to
    /// a capability — which retains a second name, after which the maker
    /// releases its own, so the count goes 1 → 2 → 1 and never through zero —
    /// or release it, which returns the whole reservation to the parent. A
    /// maker that does neither has reserved memory nobody can spend or return.
    pub fn attenuate(&mut self, parent: AuthorityId, bytes: usize) -> Result<AuthorityId, Refusal> {
        if self.stopped {
            return Err(Refusal::Stopped);
        }
        if bytes == 0 {
            return Err(Refusal::Empty);
        }
        let at = self.authority(parent)?;
        if self.authorities[at].remaining < bytes {
            return Err(Refusal::Budget);
        }
        let index = self.free_authority()?;
        let generation = self.authorities[index].generation.wrapping_add(1);
        let incarnation = self.authorities[index].incarnation.wrapping_add(1);
        self.authorities[at].remaining -= bytes;
        self.authorities[at].reserved += bytes;
        self.authorities[at].children += 1;
        self.authorities[index] = Authority {
            live: true,
            // The capability this operation hands back is the first name.
            names: 1,
            generation,
            incarnation,
            parent: Some(at as u32),
            budget: bytes,
            remaining: bytes,
            reserved: 0,
            allocated: 0,
            children: 0,
        };
        Ok(AuthorityId {
            index: index as u32,
            generation,
        })
    }

    /// Spends `bytes` of an authority and creates a mutable region.
    ///
    /// What is charged is `bytes` rounded up to the allocation granule
    /// (ADR-0076 §7): charging the request while spending a whole frame is a
    /// hidden overcommit, which is the same defect as a second counter.
    ///
    /// Atomic in the sense that matters: either the budget is there and a region
    /// with unique backing exists, or nothing changed. Only this authority is
    /// debited — the ancestors paid when they reserved.
    pub fn allocate(
        &mut self,
        authority: AuthorityId,
        bytes: usize,
        holder: u32,
    ) -> Result<RegionId, Refusal> {
        self.allocate_rounded(authority, bytes, 1, holder)
    }

    /// The same, charged to a granule.
    pub fn allocate_rounded(
        &mut self,
        authority: AuthorityId,
        bytes: usize,
        granule: usize,
        holder: u32,
    ) -> Result<RegionId, Refusal> {
        if self.stopped {
            return Err(Refusal::Stopped);
        }
        if bytes == 0 {
            return Err(Refusal::Empty);
        }
        let bytes = round_up(bytes, granule)?;
        let at = self.authority(authority)?;
        if self.authorities[at].remaining < bytes {
            return Err(Refusal::Budget);
        }
        let index = self.free_region()?;
        let generation = self.regions[index].generation.wrapping_add(1);
        self.authorities[at].remaining -= bytes;
        self.authorities[at].allocated += bytes;
        self.regions[index] = Region {
            live: true,
            generation,
            bytes,
            mutable: true,
            charged_to: at as u32,
            // **Nobody holds it yet.** Building a region is a nucleus
            // transaction, not a capability: the backing has to be laid down
            // and mapped before anything can be given a handle, and a fictitious
            // first capability handed over afterwards would be a holder the
            // nucleus invented to have something to pass. The construction
            // reference keeps it alive for exactly that stretch, and goes when
            // the caller's handle exists.
            capabilities: 0,
            internal: 1,
            writable_mappings: 0,
            readable_mappings: 0,
            holder,
            reclaimable: false,
            taken: false,
        };
        Ok(RegionId {
            index: index as u32,
            generation,
        })
    }

    /// Maps a region into its holder, writably or not.
    ///
    /// A writable mapping of an immutable region is unexpressible rather than
    /// refused politely: the mode is the answer.
    pub fn map(&mut self, region: RegionId, writable: bool) -> Result<(), Refusal> {
        let at = self.region(region)?;
        if writable && !self.regions[at].mutable {
            return Err(Refusal::WrongMode);
        }
        if writable {
            self.regions[at].writable_mappings += 1;
        } else {
            self.regions[at].readable_mappings += 1;
        }
        Ok(())
    }

    /// Drops one mapping.
    ///
    /// An unmapping of something that was never mapped is refused rather than
    /// clamped to zero: a count that silently absorbs an impossible operation
    /// is a count that cannot be used to decide when backing may be reclaimed.
    pub fn unmap(&mut self, region: RegionId, writable: bool) -> Result<(), Refusal> {
        let at = self.region(region)?;
        let held = if writable {
            self.regions[at].writable_mappings
        } else {
            self.regions[at].readable_mappings
        };
        if held == 0 {
            return Err(Refusal::BadArgument);
        }
        if writable {
            self.regions[at].writable_mappings -= 1;
        } else {
            self.regions[at].readable_mappings -= 1;
        }
        self.settle_region(at);
        Ok(())
    }

    /// The consuming transition: a writable region becomes permanently
    /// immutable (ADR-0075 §3).
    ///
    /// In one step, before it returns: every writable mapping is gone, the mode
    /// is fixed, and no future mapping may be writable. The postcondition is the
    /// nucleus's to assert — `writable_aliases == 0` — and it is checked here
    /// rather than promised.
    ///
    /// Only the sole holder may do it, because only a sole holder can be sure
    /// no other writer exists; a `Region<mut T>` has exactly one by
    /// construction (ADR-0037).
    pub fn freeze(&mut self, region: RegionId, holder: u32) -> Result<(), Refusal> {
        let at = self.region(region)?;
        if !self.regions[at].mutable {
            return Err(Refusal::WrongMode);
        }
        if self.regions[at].holder != holder || self.regions[at].capabilities != 1 {
            return Err(Refusal::NotTheHolder);
        }
        self.regions[at].writable_mappings = 0;
        self.regions[at].mutable = false;
        debug_assert_eq!(self.writable_aliases(region), Ok(0));
        Ok(())
    }

    /// How many ways this region can still be written. Zero after a freeze, and
    /// that is the point.
    pub fn writable_aliases(&self, region: RegionId) -> Result<usize, Refusal> {
        let at = self.region(region)?;
        Ok(if self.regions[at].mutable {
            self.regions[at].capabilities + self.regions[at].writable_mappings
        } else {
            self.regions[at].writable_mappings
        })
    }

    /// Moves an immutable region into exactly one other holder.
    ///
    /// Linear: the sender's handle and its mappings are gone before ownership is
    /// considered moved, so there is no instant at which both hold it
    /// (ADR-0075 §5a). A mutable region is not transferable at all.
    pub fn transfer(&mut self, region: RegionId, from: u32, to: u32) -> Result<(), Refusal> {
        let at = self.region(region)?;
        if self.regions[at].mutable {
            return Err(Refusal::WrongMode);
        }
        if self.regions[at].holder != from {
            return Err(Refusal::NotTheHolder);
        }
        self.regions[at].readable_mappings = 0;
        self.regions[at].holder = to;
        Ok(())
    }

    /// A region whose retirement has not been performed, if there is one.
    ///
    /// What the retirement path asks between one release and the next: the
    /// nucleus finds these rather than being told, because the reference that
    /// went may have been anybody's.
    pub fn reclaimable(&self) -> Option<RegionId> {
        (0..MAX_REGIONS)
            .find(|at| {
                self.regions[*at].live && self.regions[*at].reclaimable && !self.regions[*at].taken
            })
            .map(|at| RegionId {
                index: at as u32,
                generation: self.regions[at].generation,
            })
    }

    /// Takes responsibility for retiring a region, and the receipt that says so.
    ///
    /// **The receipt is what keeps the two accounts from diverging.** Between
    /// this and [`Regions::finish_reclaim`] the caller returns the backing
    /// frames to the pool and the lane's tables to the reserve; only the
    /// receipt can then credit the authority, it can only do so once, and it
    /// cannot credit a different region or a later occupant of the slot. The
    /// slot stays live and unreusable until it does.
    pub fn take_reclaim(&mut self, region: RegionId) -> Result<Reclaim, Refusal> {
        let at = region.index as usize;
        if at >= MAX_REGIONS {
            return Err(Refusal::NotFound);
        }
        let held = self.regions[at];
        if !held.live || !held.reclaimable || held.taken || held.generation != region.generation {
            return Err(Refusal::NotFound);
        }
        self.regions[at].taken = true;
        Ok(Reclaim {
            slot: region.index,
            generation: region.generation,
            bytes: held.bytes,
        })
    }

    /// How many bytes a retirement has to give back before it is finished.
    pub fn reclaim_bytes(&self, ticket: &Reclaim) -> usize {
        ticket.bytes
    }

    /// Finishes a retirement: the backing is back, so the budget goes back.
    ///
    /// The last step and never the first. What it credits is what the receipt
    /// says, to the authority that funded it, along the lineage that paid —
    /// and the slot becomes reusable only here, so nothing can occupy it while
    /// its frames are still out.
    pub fn finish_reclaim(&mut self, ticket: Reclaim) -> Result<(), Refusal> {
        let at = ticket.slot as usize;
        if at >= MAX_REGIONS {
            return Err(Refusal::NotFound);
        }
        let held = self.regions[at];
        if !held.live || !held.taken || held.generation != ticket.generation {
            return Err(Refusal::NotFound);
        }
        let funder = held.charged_to as usize;
        if self.authorities[funder].allocated < held.bytes {
            return Err(Refusal::BadArgument);
        }
        self.regions[at].live = false;
        self.regions[at].taken = false;
        self.regions[at].reclaimable = false;
        self.authorities[funder].allocated -= held.bytes;
        self.give_back_or_keep(funder, held.bytes);
        self.settle_authority(funder);
        Ok(())
    }

    /// How many region objects exist, in any state.
    pub fn live_regions(&self) -> usize {
        (0..MAX_REGIONS).filter(|at| self.regions[*at].live).count()
    }

    /// How many address spaces map this region, writably and readably.
    pub fn mappings(&self, region: RegionId) -> Result<(usize, usize), Refusal> {
        let at = self.region(region)?;
        Ok((
            self.regions[at].writable_mappings,
            self.regions[at].readable_mappings,
        ))
    }

    /// How many bytes a region was charged and mapped, which is what its
    /// holder is told rather than what it asked for (ADR-0076 §7).
    pub fn length(&self, region: RegionId) -> Result<usize, Refusal> {
        let at = self.region(region)?;
        Ok(self.regions[at].bytes)
    }

    /// Whether a capability may name this region: only if none does.
    pub fn can_name(&self, region: RegionId) -> bool {
        self.region(region)
            .is_ok_and(|at| self.regions[at].capabilities == 0)
    }

    /// Takes the one capability reference a region may have.
    ///
    /// **Affine, and enforced here rather than at each operation.** A region's
    /// capability count is `{0, 1}` and not a number to add to: ADR-0037 gives
    /// a `Region<mut T>` exactly one holder, and a second handle to a mutable
    /// region is the writable alias the consuming freeze exists to eliminate.
    /// Operation 5 refuses to make one, but an operation that reached `grant`
    /// by another road would too — so the object refuses, and affinity is a
    /// property of the kind rather than something every path must remember.
    pub fn retain_capability(&mut self, region: RegionId) -> Result<(), Refusal> {
        let at = self.region(region)?;
        if self.regions[at].capabilities != 0 {
            return Err(Refusal::NotTheHolder);
        }
        self.regions[at].capabilities = 1;
        Ok(())
    }

    /// Gives it up, reclaiming the region if that was the last way to reach it.
    ///
    /// A release of a capability nobody holds is a defect in the lifecycle
    /// above, not a harmless repeat: it is refused rather than clamped, for the
    /// same reason a second refund of a grant is.
    pub fn release_capability(&mut self, region: RegionId) -> Result<(), Refusal> {
        let at = self.region(region)?;
        if self.regions[at].capabilities == 0 {
            return Err(Refusal::BadArgument);
        }
        self.regions[at].capabilities = 0;
        self.settle_region(at);
        Ok(())
    }

    /// Takes an internal reference: the nucleus itself will keep this region
    /// alive for a while.
    ///
    /// **The interval a committed delegation lives in.** A region sent over an
    /// endpoint has had the sender's mapping and handle taken from it
    /// atomically (ADR-0075 §5a) and the receiver has nothing yet: for that
    /// stretch nothing a process holds names it, and only this keeps it from
    /// being reclaimed out from under the message. It is not a capability and
    /// is not counted as one — ADR-0075 §6 makes reclamation wait on
    /// capabilities, mappings **and** internal references, three counts rather
    /// than one, because they are lost by three different events.
    pub fn retain_internal(&mut self, region: RegionId) -> Result<(), Refusal> {
        let at = self.region(region)?;
        self.regions[at].internal = self.regions[at]
            .internal
            .checked_add(1)
            .ok_or(Refusal::NoRoom)?;
        Ok(())
    }

    /// Drops one internal reference, reclaiming the region if that was the last
    /// way to reach it.
    pub fn release_internal(&mut self, region: RegionId) -> Result<(), Refusal> {
        let at = self.region(region)?;
        if self.regions[at].internal == 0 {
            return Err(Refusal::BadArgument);
        }
        self.regions[at].internal -= 1;
        self.settle_region(at);
        Ok(())
    }

    /// How many internal references the nucleus holds on a region.
    pub fn internal(&self, region: RegionId) -> Result<usize, Refusal> {
        let at = self.region(region)?;
        Ok(self.regions[at].internal)
    }

    /// An address space was destroyed: every mapping it held is physically
    /// gone.
    ///
    /// **The exact event, not a sweep.** Destroying a tree removes every window
    /// in it at once, so zeroing this holder's mapping counts is a statement of
    /// what happened rather than a tidy-up that hides a count nobody kept
    /// straight. Capability references are *not* touched here: they are lost
    /// when the process's table is cleared, which is a different event, and a
    /// region can outlive either one.
    pub fn mappings_destroyed(&mut self, holder: u32) {
        for at in 0..MAX_REGIONS {
            if self.regions[at].live && self.regions[at].holder == holder {
                self.regions[at].writable_mappings = 0;
                self.regions[at].readable_mappings = 0;
                self.settle_region(at);
            }
        }
    }

    /// A process ended and its table went with it: every capability it held
    /// over a region stops naming one.
    pub fn capabilities_destroyed(&mut self, holder: u32) {
        for at in 0..MAX_REGIONS {
            if self.regions[at].live && self.regions[at].holder == holder {
                self.regions[at].capabilities = 0;
                self.settle_region(at);
            }
        }
    }

    /// Takes one more live reference to an authority.
    ///
    /// **Several names, one budget.** A `MemoryAuthority` is not affine: an
    /// alias made by generic attenuation, an entry in an endowment, a
    /// delegation's receiving handle and a committed delegation still in
    /// transit are all names for the same node, the same `remaining` and the
    /// same funding lineage. None of them is a second reservation, and losing
    /// one of them is not losing the authority.
    ///
    /// One counter with typed call sites rather than two. What the invariant
    /// needs is that the node stays reachable for as long as *anything* can
    /// still produce a valid holder, and a send that commits before its
    /// receiver has a table entry is exactly the interval where that would
    /// otherwise fail. So a committed delegation retains **before** the
    /// sender's handle is released and the receiver retains **before** the
    /// transit reference is released: the count never passes through zero, and
    /// an unused remainder cannot return to a parent while a message that
    /// carries the authority is still on its way.
    /// The root is refused: see [`Regions::release_name`] for why it has no
    /// naming lifecycle at all. And the count is `checked_add` rather than
    /// bounded by an argument about how many references can exist — the bound
    /// would have to hold across the capability table, every endowment and
    /// every message in flight, which is a proof that would have to be redone
    /// whenever any of those changes. A refusal costs one branch and needs no
    /// proof.
    pub fn retain(&mut self, authority: AuthorityId) -> Result<(), Refusal> {
        let at = self.authority(authority)?;
        if self.authorities[at].parent.is_none() {
            return Err(Refusal::Anchored);
        }
        self.authorities[at].names = self.authorities[at]
            .names
            .checked_add(1)
            .ok_or(Refusal::NoRoom)?;
        Ok(())
    }

    /// Drops one live reference: a handle released, a process that held one
    /// ending, or a delegation that never arrived.
    ///
    /// Only the loss of the **last** one performs ADR-0075 §2a's transition —
    /// the unused remainder returns upward, the generation moves on so stale
    /// handles fail, and the accounting node lives on while allocations,
    /// charges or descendants remain, returning their bytes along the original
    /// lineage when they drain. Releasing one of several aliases returns
    /// nothing and leaves the others working, which is the whole difference
    /// between a release and a revoke.
    /// **The root has no naming lifecycle.** It is refused here, in
    /// [`Regions::retain`] and in [`Regions::revoke`], rather than merely
    /// surviving them: making `settle_authority` a no-op for it would leave a
    /// caller able to drive its count to zero, and an anchor whose synthetic
    /// boot reference can be released by anybody is not an anchor. Charging,
    /// attenuating and refunding into it are the operations it does have.
    ///
    /// The subtraction cannot underflow: [`Regions::authority`] resolves only a
    /// node with at least one name, so a handle that got this far names one.
    pub fn release_name(&mut self, authority: AuthorityId) -> Result<(), Refusal> {
        let at = self.authority(authority)?;
        if self.authorities[at].parent.is_none() {
            return Err(Refusal::Anchored);
        }
        self.authorities[at].names -= 1;
        if self.authorities[at].names > 0 {
            return Ok(());
        }
        let returning = self.authorities[at].remaining;
        self.authorities[at].remaining = 0;
        self.authorities[at].generation = self.authorities[at].generation.wrapping_add(1);
        self.give_back(at, returning);
        self.settle_authority(at);
        Ok(())
    }

    /// Whether `count` more names could be taken, without taking any.
    ///
    /// **A preflight needs a pure question.** The endowment commit is meant to
    /// be infallible, and it can only be that if every fallible step has been
    /// asked in advance — including this one, and including the case where one
    /// endowment names the same authority several times, whose cost is the sum
    /// rather than each entry on its own.
    pub fn can_retain(&self, authority: AuthorityId, count: usize) -> bool {
        let Ok(at) = self.authority(authority) else {
            return false;
        };
        self.authorities[at].parent.is_some()
            && self.authorities[at].names.checked_add(count).is_some()
    }

    /// How many live references name this node.
    pub fn names(&self, authority: AuthorityId) -> Result<usize, Refusal> {
        let at = self.authority(authority)?;
        Ok(self.authorities[at].names)
    }

    /// Drops **every** name at once, whoever held them.
    ///
    /// **Not what `capability_release` does.** Under the several-names model a
    /// release drops one reference and a revoke ends the node for all of them,
    /// so wiring one to the other would let any alias holder destroy an
    /// authority two other processes were relying on. This stays the
    /// state-machine mechanism for whole-node invalidation and the evidence
    /// that it works; what public operation may authorise it, if any, is a
    /// separate scoped-revocation decision and is not settled by operation 16.
    ///
    /// Its unspent, unreserved remainder returns to its parent at once. The node
    /// itself survives while anything it funded is still alive, because a live
    /// region does not become free budget by losing the authority that paid for
    /// it; when the last of them drains, the node goes and what it still holds
    /// travels up the lineage that funded it.
    pub fn revoke(&mut self, authority: AuthorityId) -> Result<(), Refusal> {
        let at = self.authority(authority)?;
        if self.authorities[at].parent.is_none() {
            return Err(Refusal::Anchored);
        }
        let returning = self.authorities[at].remaining;
        self.authorities[at].remaining = 0;
        self.authorities[at].names = 0;
        self.authorities[at].generation = self.authorities[at].generation.wrapping_add(1);
        self.give_back(at, returning);
        self.settle_authority(at);
        Ok(())
    }

    /// What a region may be done to, for a caller that holds one.
    pub fn mode(&self, region: RegionId) -> Result<Mode, Refusal> {
        let at = self.region(region)?;
        Ok(if self.regions[at].mutable {
            Mode::Mutable
        } else {
            Mode::Immutable
        })
    }

    /// The bytes an authority may still spend itself.
    pub fn remaining(&self, authority: AuthorityId) -> Result<usize, Refusal> {
        let at = self.authority(authority)?;
        Ok(self.authorities[at].remaining)
    }

    /// What is allocated under an authority and everything reserved from it.
    pub fn allocated(&self, authority: AuthorityId) -> Result<usize, Refusal> {
        let at = self.authority(authority)?;
        Ok(self.subtree_allocated(at))
    }

    /// Whether the accounting holds for every authority: nothing spent twice,
    /// and no subtree spending more than it was given.
    ///
    /// `allocated + reserved + free == budget`, at every node. A test asserts
    /// it after every step rather than at the end, because an invariant that
    /// only holds at rest is not an invariant.
    pub fn accounting_holds(&self) -> bool {
        for at in 0..MAX_AUTHORITIES {
            let node = &self.authorities[at];
            if !node.live {
                continue;
            }
            if node.allocated + node.reserved + node.remaining != node.budget {
                return false;
            }
            let mut reserved = 0usize;
            let mut children = 0usize;
            for other in 0..MAX_AUTHORITIES {
                if self.authorities[other].live && self.authorities[other].parent == Some(at as u32)
                {
                    reserved += self.authorities[other].budget;
                    children += 1;
                }
            }
            if reserved != node.reserved || children != node.children {
                return false;
            }
        }
        true
    }

    /// The backing charged to this node and everything under it.
    fn subtree_allocated(&self, at: usize) -> usize {
        let mut total = self.authorities[at].allocated;
        for other in 0..MAX_AUTHORITIES {
            if self.authorities[other].live && self.authorities[other].parent == Some(at as u32) {
                total += self.subtree_allocated(other);
            }
        }
        total
    }

    /// Marks a region whose last way of being reached has gone.
    ///
    /// **It does not credit anybody.** Once a region has physical backing, the
    /// bytes are not free when the last reference goes — they are free when the
    /// frames are back in the pool. Crediting here would let the authority tree
    /// hand out memory that `Frames` still holds, which is the two-counters
    /// defect wearing a different hat. So this only records that the region has
    /// become reclaimable; [`Regions::take_reclaim`] and
    /// [`Regions::finish_reclaim`] are what actually retire it, in that order,
    /// with the physical work in between.
    fn settle_region(&mut self, at: usize) {
        let region = self.regions[at];
        if !region.live
            || region.reclaimable
            || region.capabilities != 0
            || region.internal != 0
            || region.writable_mappings != 0
            || region.readable_mappings != 0
        {
            return;
        }
        self.regions[at].reclaimable = true;
        self.regions[at].holder = u32::MAX;
    }

    /// Returns reclaimed bytes to the authority that funded them, or past it to
    /// its parent when it is no longer named by any capability.
    fn give_back_or_keep(&mut self, at: usize, bytes: usize) {
        if self.authorities[at].names > 0 {
            self.authorities[at].remaining += bytes;
        } else {
            self.give_back(at, bytes);
        }
    }

    /// Hands bytes to a node's parent, or lets them leave the tree at the root.
    fn give_back(&mut self, at: usize, bytes: usize) {
        if bytes == 0 {
            return;
        }
        match self.authorities[at].parent {
            Some(parent) => {
                let parent = parent as usize;
                self.authorities[parent].reserved -= bytes;
                self.authorities[parent].remaining += bytes;
                self.authorities[at].budget -= bytes;
            }
            // The root's own remainder simply stays where it is: there is
            // nowhere above it, and its budget is what the boot endowed.
            None => self.authorities[at].remaining += bytes,
        }
    }

    /// Retires an authority that is no longer named and has nothing left under
    /// it.
    fn settle_authority(&mut self, at: usize) {
        let node = self.authorities[at];
        // The root is never retired: it has nowhere to return to, it is not
        // kept alive by anybody's handle, and a boot whose accounting anchor
        // could be settled by a process ending would have no anchor.
        if !node.live
            || node.parent.is_none()
            || node.names > 0
            || node.allocated != 0
            || node.children != 0
        {
            return;
        }
        let remainder = node.remaining;
        self.give_back(at, remainder);
        self.authorities[at].remaining = 0;
        self.authorities[at].live = false;
        if let Some(parent) = node.parent {
            let parent = parent as usize;
            self.authorities[parent].children -= 1;
            self.settle_authority(parent);
        }
    }

    fn authority(&self, id: AuthorityId) -> Result<usize, Refusal> {
        let at = id.index as usize;
        if at >= MAX_AUTHORITIES {
            return Err(Refusal::NotFound);
        }
        let node = &self.authorities[at];
        if !node.live || node.names == 0 || node.generation != id.generation {
            return Err(Refusal::NotFound);
        }
        Ok(at)
    }

    fn region(&self, id: RegionId) -> Result<usize, Refusal> {
        let at = id.index as usize;
        if at >= MAX_REGIONS {
            return Err(Refusal::NotFound);
        }
        let region = &self.regions[at];
        // A region awaiting retirement resolves to nothing: nothing can reach
        // it, and its backing is still out.
        if !region.live || region.reclaimable || region.generation != id.generation {
            return Err(Refusal::NotFound);
        }
        Ok(at)
    }

    /// What is charged, and what a whole tree has spent — the two numbers a
    /// caller checks the pool against.
    pub fn committed(&self) -> usize {
        (0..MAX_AUTHORITIES)
            .filter(|at| self.authorities[*at].live)
            .map(|at| self.authorities[at].allocated)
            .sum()
    }

    fn free_authority(&self) -> Result<usize, Refusal> {
        (0..MAX_AUTHORITIES)
            .find(|at| !self.authorities[*at].live)
            .ok_or(Refusal::NoRoom)
    }

    fn free_region(&self) -> Result<usize, Refusal> {
        (0..MAX_REGIONS)
            .find(|at| !self.regions[*at].live)
            .ok_or(Refusal::NoRoom)
    }

    fn free_charge(&self) -> Result<usize, Refusal> {
        (0..MAX_CHARGES)
            .find(|at| !self.charges[*at].live)
            .ok_or(Refusal::NoRoom)
    }
}
