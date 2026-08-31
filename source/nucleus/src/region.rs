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

/// Why an operation on an authority or a region was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// The table is full. Bounded means bounded.
    NoRoom,
    /// The authority or region named does not exist, or its generation is stale.
    NotFound,
    /// The authority has less than was asked of it.
    Budget,
    /// The region is not in the mode this operation requires.
    WrongMode,
    /// The caller is not the one holder this operation requires.
    NotTheHolder,
    /// A zero-sized authority or region, which is authority over nothing.
    Empty,
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

/// One accounting node. Nucleus-owned; no process has a mapping to it.
#[derive(Clone, Copy, Debug, Default)]
struct Authority {
    live: bool,
    /// Whether a capability still names this node. A revoked node stays until
    /// its allocations and descendants drain (ADR-0075 §2a).
    named: bool,
    generation: u32,
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
    /// How many address spaces have it mapped, by mode.
    writable_mappings: usize,
    readable_mappings: usize,
    /// Which process holds it while it is mutable, and while it is immutable
    /// and unshared. `u32::MAX` is nobody.
    holder: u32,
}

/// The nucleus's memory-authority and region state.
pub struct Regions {
    authorities: [Authority; MAX_AUTHORITIES],
    regions: [Region; MAX_REGIONS],
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
                named: false,
                generation: 0,
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
                writable_mappings: 0,
                readable_mappings: 0,
                holder: u32::MAX,
            }; MAX_REGIONS],
        }
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
        self.authorities[index] = Authority {
            live: true,
            named: true,
            generation,
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
    pub fn attenuate(&mut self, parent: AuthorityId, bytes: usize) -> Result<AuthorityId, Refusal> {
        if bytes == 0 {
            return Err(Refusal::Empty);
        }
        let at = self.authority(parent)?;
        if self.authorities[at].remaining < bytes {
            return Err(Refusal::Budget);
        }
        let index = self.free_authority()?;
        let generation = self.authorities[index].generation.wrapping_add(1);
        self.authorities[at].remaining -= bytes;
        self.authorities[at].reserved += bytes;
        self.authorities[at].children += 1;
        self.authorities[index] = Authority {
            live: true,
            named: true,
            generation,
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
    /// Atomic in the sense that matters: either the budget is there and a region
    /// with unique backing exists, or nothing changed. Only this authority is
    /// debited — the ancestors paid when they reserved.
    pub fn allocate(
        &mut self,
        authority: AuthorityId,
        bytes: usize,
        holder: u32,
    ) -> Result<RegionId, Refusal> {
        if bytes == 0 {
            return Err(Refusal::Empty);
        }
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
            capabilities: 1,
            writable_mappings: 0,
            readable_mappings: 0,
            holder,
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
    pub fn unmap(&mut self, region: RegionId, writable: bool) -> Result<(), Refusal> {
        let at = self.region(region)?;
        if writable {
            self.regions[at].writable_mappings =
                self.regions[at].writable_mappings.saturating_sub(1);
        } else {
            self.regions[at].readable_mappings =
                self.regions[at].readable_mappings.saturating_sub(1);
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

    /// Releases one capability over a region, reclaiming it if that was the
    /// last way to reach it.
    pub fn release(&mut self, region: RegionId) -> Result<(), Refusal> {
        let at = self.region(region)?;
        self.regions[at].capabilities = self.regions[at].capabilities.saturating_sub(1);
        self.settle_region(at);
        Ok(())
    }

    /// A process ended: its handles and its mappings go with it.
    pub fn process_died(&mut self, holder: u32) {
        for at in 0..MAX_REGIONS {
            if self.regions[at].live && self.regions[at].holder == holder {
                self.regions[at].capabilities = 0;
                self.regions[at].writable_mappings = 0;
                self.regions[at].readable_mappings = 0;
                self.settle_region(at);
            }
        }
    }

    /// Revokes an authority's capability.
    ///
    /// Its unspent, unreserved remainder returns to its parent at once. The node
    /// itself survives while anything it funded is still alive, because a live
    /// region does not become free budget by losing the authority that paid for
    /// it; when the last of them drains, the node goes and what it still holds
    /// travels up the lineage that funded it.
    pub fn revoke(&mut self, authority: AuthorityId) -> Result<(), Refusal> {
        let at = self.authority(authority)?;
        let returning = self.authorities[at].remaining;
        self.authorities[at].remaining = 0;
        self.authorities[at].named = false;
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

    /// Reclaims a region whose last way of being reached has gone.
    fn settle_region(&mut self, at: usize) {
        let region = self.regions[at];
        if !region.live
            || region.capabilities != 0
            || region.writable_mappings != 0
            || region.readable_mappings != 0
        {
            return;
        }
        let funder = region.charged_to as usize;
        self.regions[at].live = false;
        self.regions[at].holder = u32::MAX;
        self.authorities[funder].allocated -= region.bytes;
        self.give_back_or_keep(funder, region.bytes);
        self.settle_authority(funder);
    }

    /// Returns reclaimed bytes to the authority that funded them, or past it to
    /// its parent when it is no longer named by any capability.
    fn give_back_or_keep(&mut self, at: usize, bytes: usize) {
        if self.authorities[at].named {
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
        if !node.live || node.named || node.allocated != 0 || node.children != 0 {
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
        if !node.live || !node.named || node.generation != id.generation {
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
        if !region.live || region.generation != id.generation {
            return Err(Refusal::NotFound);
        }
        Ok(at)
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
}
