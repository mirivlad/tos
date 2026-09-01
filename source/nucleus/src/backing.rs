// SPDX-License-Identifier: GPL-3.0-or-later
//! Which physical frames each region is made of.
//!
//! **A region's backing has to outlive every mapping of it.** A committed
//! delegation takes the sender's mapping with the handle and the receiver has
//! nothing yet (ADR-0075 §5a); a supervisor restarting a target maps the same
//! bundle again after the previous target's address space is gone. If the only
//! record of which frames a region occupies lived in a process's page tables,
//! either of those would lose the region along with the process.
//!
//! So the nucleus keeps its own record, and keeps it in the shape the machine
//! already has a walker for: a page-table tree, indexed by the same
//! deterministic lane the region is mapped at in every address space, so an
//! address in a process and an address in this index are the same arithmetic.
//!
//! **It is not an address space.** It is never loaded into `CR3`, no process
//! has a mapping to it, and nothing here can make one — the type does not
//! expose `activate`, and that is the whole of the protection it needs. Its
//! leaves carry no permission bits that mean anything, because nothing walks
//! them but this file.
//!
//! Its root frame is permanent: taken from the reserve at boot, before the root
//! memory authority is endowed, and never given back. Everything below it comes
//! and goes with the regions it describes.

use tos_frames::{Frames, FRAME_SIZE};

use crate::memory::Tables;
use crate::paging::{AddressSpace, PagingRefused};

/// Leaf flags. Present is all that is read; the rest exist so the walk this
/// shares with real page tables sees a well-formed entry rather than one it
/// would have to special-case.
const PRESENT_METADATA: u64 = 1 << 1 | 1 << 63;

/// Why the index could not do what was asked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Malformed {
    /// A leaf is already there. The lifecycle above lost track of a frame.
    Occupied,
    /// The reserve is empty. Inside a proved bound this is an invariant, not a
    /// resource condition — see `process::table_reserve`.
    NoTable,
    /// The lane does not describe the region it is supposed to: a page missing
    /// inside the committed length, or one present beyond it.
    WrongShape,
}

/// The nucleus's record of every region's frames.
pub struct RegionBackingSpace {
    tree: AddressSpace,
}

impl RegionBackingSpace {
    /// Takes the one permanent frame this index is rooted at.
    ///
    /// Called once, at boot, after the reserve exists and before the root
    /// authority is endowed. That frame is part of the accepted reserve bound
    /// and is never returned, so the reserve's runtime baseline is one below
    /// its size for the life of the boot.
    pub fn create(tables: &mut Tables) -> Option<RegionBackingSpace> {
        Some(RegionBackingSpace {
            tree: AddressSpace::new(tables).ok()?,
        })
    }

    /// Records that `page` of this lane is made of `frame`.
    ///
    /// Refuses an occupied leaf rather than replacing it: a leaf already naming
    /// a frame means the retirement of a previous region did not finish, and
    /// overwriting it would strand memory the pool believes is out.
    pub fn construct(
        &mut self,
        tables: &mut Tables,
        lane: u64,
        page: u64,
        frame: u64,
    ) -> Result<(), Malformed> {
        match self
            .tree
            .map_empty_page(tables, lane + page * FRAME_SIZE, frame, PRESENT_METADATA)
        {
            Ok(()) => Ok(()),
            Err(PagingRefused::NoFrame) => Err(Malformed::NoTable),
            Err(_) => Err(Malformed::Occupied),
        }
    }

    /// Which frame backs one page of a region, if the index knows.
    pub fn frame_at(&self, lane: u64, page: u64) -> Option<u64> {
        self.tree.translate(lane + page * FRAME_SIZE)
    }

    /// Whether this lane describes exactly `pages` pages and nothing else.
    ///
    /// **Asked before the first frame is given back, never during.** A drain
    /// that discovered halfway through that the index was malformed would have
    /// already returned frames it could no longer account for; refusing first
    /// leaves the memory stranded, which the accounting shows, instead of
    /// handing it out twice.
    pub fn describes(&self, lane: u64, pages: u64) -> bool {
        (0..pages).all(|page| self.frame_at(lane, page).is_some())
            && self.frame_at(lane, pages).is_none()
    }

    /// Gives a lane's frames back to the pool and its tables back to the
    /// reserve.
    ///
    /// **Each leaf is cleared before its frame is released, and that order is
    /// the whole of the safety.** A frame the pool considers free while this
    /// index still names it is a frame two owners can reach; the reverse is a
    /// frame nothing names for an instant, which costs nothing.
    ///
    /// # Safety
    ///
    /// No process maps any of these frames: every address space that did has
    /// had its lane released or has been destroyed.
    // SAFETY: the caller's promise that no mapping survives is what makes each
    // frame unreferenced at the moment it is released.
    pub unsafe fn drain(
        &mut self,
        tables: &mut Tables,
        frames: &mut Frames,
        lane: u64,
        pages: u64,
    ) -> Result<(), Malformed> {
        if !self.describes(lane, pages) {
            return Err(Malformed::WrongShape);
        }
        for page in 0..pages {
            let Some(frame) = self.tree.clear_leaf(lane + page * FRAME_SIZE) else {
                return Err(Malformed::WrongShape);
            };
            // SAFETY: per this function's contract, and the index has just
            // stopped naming it.
            unsafe { frames.release_frame(frame) };
        }
        // SAFETY: the lane is one top-level entry of a tree no processor uses,
        // and every leaf under it has been cleared.
        unsafe { self.tree.release_branch(tables, lane) };
        Ok(())
    }

    /// Undoes a construction that failed part-way.
    ///
    /// Construction fills a lane from page zero upwards, so what was built is
    /// exactly the prefix and there is no list of frames to keep: the count is
    /// the record. Unlike [`RegionBackingSpace::drain`] this tolerates a lane
    /// whose branch was never created, because a failure before the first leaf
    /// is a failure that built nothing.
    ///
    /// # Safety
    ///
    /// As `drain`: nothing maps these frames.
    // SAFETY: a partially built lane is reachable only from this transaction.
    pub unsafe fn discard(
        &mut self,
        tables: &mut Tables,
        frames: &mut Frames,
        lane: u64,
        built: u64,
    ) {
        for page in 0..built {
            if let Some(frame) = self.tree.clear_leaf(lane + page * FRAME_SIZE) {
                // SAFETY: per this function's contract.
                unsafe { frames.release_frame(frame) };
            }
        }
        // SAFETY: as above; absent is an ordinary outcome here.
        unsafe { self.tree.release_branch(tables, lane) };
    }
}
