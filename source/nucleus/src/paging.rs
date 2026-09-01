// SPDX-License-Identifier: GPL-3.0-or-later
//! The address space the nucleus builds for itself.
//!
//! Until now the nucleus ran on the page tables UEFI left behind: an identity
//! map it never wrote, never checked and could not describe. That was tolerable
//! while there was exactly one execution context in the system, and it stops
//! being tolerable at ADR-0048, which makes the page tables — not the verifier
//! — the thing that keeps one process out of another's memory. A boundary the
//! nucleus does not own is not a boundary it can enforce.
//!
//! **Built from the validated map, out of a reserve and not out of the pool.**
//! Every table is a frame from [`crate::memory::Tables`], which is carved out
//! of the pool before the root memory authority is endowed; every mapping comes
//! from the memory map the Boot ABI validation already accepted. Nothing here
//! reads a firmware table, and — since ADR-0076 §2 — nothing here can name the
//! pool at all, so a page table cannot be built out of memory the authority
//! tree has already promised to somebody.
//!
//! **What this address space deliberately does not have.**
//!
//! - *Physical page zero.* A null dereference has to fault, and the only way to
//!   make it fault is to leave the page absent. It costs one page table.
//! - *A writable-and-executable mapping.* The nucleus image is split at its
//!   section boundaries: text is read-only and executable, everything else is
//!   writable and not executable. `CR0.WP` is set, because without it a ring-0
//!   write ignores the read-only bit and the split would be decorative.
//! - *A user-accessible leaf.* No page of this space carries `U/S`, and a leaf
//!   without it is supervisor-only however permissive the path above it is.
//!   Ring 3 arrives with its own space in a later task of this phase, and a
//!   process that could reach the nucleus's mappings would make that task
//!   pointless.
//! - *Memory that does not exist.* Only what the map describes, plus the
//!   framebuffer the loader declared, is mapped at all.

use crate::msr::{self, EFER_NXE, IA32_EFER};
use tos_boot_protocol::{BootInfo, MemoryRange};
use tos_frames::FRAME_SIZE;

use crate::memory::Tables;
use tos_frames::Frames;

/// Where a page table's frame comes from.
///
/// **Two sources, one at a time, and the reason is the firmware's map.** Until
/// the nucleus activates its own address space it runs on the one UEFI left
/// behind, and that map is not ours to describe: some of what the memory map
/// reports usable — memory that genuinely is ours once boot services have
/// exited — is still mapped read-only by the firmware. Writing to it faults.
///
/// So the nucleus's own space is built from the pool, before anything can
/// promise those frames, exactly as it always was; the page-table reserve is
/// taken afterwards, once every admitted frame is mapped writable by a map the
/// nucleus wrote. Everything after that point — every process space, every
/// region lane — comes from the reserve and never from the pool (ADR-0076 §2).
pub trait TableSource {
    fn take(&mut self) -> Option<u64>;

    /// Gives one back, to whoever it came from. A tree half-built and then
    /// abandoned returns its frames to the same place it took them.
    ///
    /// # Safety
    ///
    /// The frame came from this source, nothing maps it, and no page-table
    /// entry anywhere still points at it.
    // SAFETY: the caller's promise that the frame is unreachable is the whole
    // contract; each implementation adds nothing to it.
    unsafe fn give_back(&mut self, frame: u64);
}

impl TableSource for Tables {
    fn take(&mut self) -> Option<u64> {
        self.allocate_frame()
    }

    // SAFETY: per the trait's contract.
    unsafe fn give_back(&mut self, frame: u64) {
        // SAFETY: as above.
        unsafe { self.release_frame(frame) }
    }
}

impl TableSource for Frames {
    fn take(&mut self) -> Option<u64> {
        self.allocate_frame()
    }

    // SAFETY: per the trait's contract.
    unsafe fn give_back(&mut self, frame: u64) {
        // SAFETY: as above.
        unsafe { self.release_frame(frame) }
    }
}
use tos_runtime::region::Span;

/// Page-table entry bits, as the architecture defines them.
const PRESENT: u64 = 1;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const WRITE_THROUGH: u64 = 1 << 3;
const CACHE_DISABLE: u64 = 1 << 4;
const HUGE: u64 = 1 << 7;
const NO_EXECUTE: u64 = 1 << 63;
/// The physical-address field of an entry.
const ADDRESS: u64 = 0x000f_ffff_ffff_f000;

/// Bytes one entry of a page directory covers.
const HUGE_SIZE: u64 = 2 * 1024 * 1024;
/// Entries in every table at every level.
const ENTRIES: u64 = 512;

/// `CR0.WP` — without it a ring-0 store ignores a read-only mapping.
const CR0_WP: u64 = 1 << 16;

/// Why the nucleus could not build its own address space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PagingRefused {
    /// The pool had no frame left for a table.
    NoFrame,
    /// The map describes memory above what four-level paging can address.
    AddressTooHigh(u64),
    /// A 2 MiB mapping is already there and a 4 KiB one was asked for inside
    /// it, or the reverse. Two granularities over one region is a mistake in
    /// the caller, and silently dropping either mapping would hide it.
    Granularity(u64),
}

/// A four-level page table tree, named by the physical address of its root.
pub struct AddressSpace {
    root: u64,
}

impl AddressSpace {
    /// The physical address of this tree's root — what `CR3` holds while this
    /// space is the live one.
    pub fn root(&self) -> u64 {
        self.root
    }

    /// An empty space: a cleared root table and nothing mapped.
    pub fn new(tables: &mut dyn TableSource) -> Result<AddressSpace, PagingRefused> {
        let root = tables.take().ok_or(PagingRefused::NoFrame)?;
        Ok(AddressSpace { root })
    }

    /// Reads one entry of a table.
    fn entry(table: u64, index: u64) -> u64 {
        // SAFETY: `table` is a frame this space allocated from the pool and
        // cleared, and `index` is a table index below 512, so the read is
        // aligned and inside that frame. Tables are identity-mapped: this space
        // maps them, and the firmware map that precedes it does too.
        unsafe {
            core::ptr::with_exposed_provenance::<u64>((table + index * 8) as usize).read_volatile()
        }
    }

    /// Writes one entry of a table.
    fn write(table: u64, index: u64, value: u64) {
        // SAFETY: as `entry`, and the nucleus is the only writer of its own
        // tables: this runs before any other context exists.
        unsafe {
            core::ptr::with_exposed_provenance_mut::<u64>((table + index * 8) as usize)
                .write_volatile(value)
        };
    }

    /// The next table down, allocating it when it is absent and widening it
    /// when the leaf below needs more than the path currently allows.
    ///
    /// Interior entries are **permissive** — present, writable and, when
    /// something below them is user-accessible, user-accessible too — because
    /// the architecture takes the *intersection* along the path: a restriction
    /// stated at an interior entry applies to everything under it, including
    /// mappings that were never meant to be restricted. Every real permission
    /// is stated on the leaf, where it describes exactly one page, and a leaf
    /// without `U/S` stays supervisor-only however permissive its path is.
    ///
    /// The widening is not cosmetic. An interior entry created for a
    /// supervisor-only mapping and then reused for a user page would leave the
    /// user page unreachable — mapped, present, and faulting — which is the
    /// kind of defect that reads as a hang.
    fn descend(
        &self,
        tables: &mut dyn TableSource,
        table: u64,
        index: u64,
        interior: u64,
    ) -> Result<u64, PagingRefused> {
        let existing = Self::entry(table, index);
        if existing & PRESENT != 0 {
            if existing & HUGE != 0 {
                // A 2 MiB leaf where a table must go. The caller asked for two
                // granularities over one region, which is a bug in the caller,
                // not something to paper over by dropping the huge mapping.
                return Err(PagingRefused::Granularity(table));
            }
            if existing & interior != interior {
                Self::write(table, index, existing | interior);
            }
            return Ok(existing & ADDRESS);
        }
        let frame = tables.take().ok_or(PagingRefused::NoFrame)?;
        Self::write(table, index, frame | PRESENT | WRITABLE | interior);
        Ok(frame)
    }

    /// Maps one 4 KiB page where nothing is mapped, and refuses where something
    /// is.
    ///
    /// **The difference from [`AddressSpace::map_page`] is the refusal, and it
    /// matters for region backing.** The ordinary primitive overwrites a leaf,
    /// which is right for building a fresh space where the caller knows the
    /// tree is empty. It is wrong wherever a leaf already naming a physical
    /// frame would mean the lifecycle has lost track of that frame: replacing
    /// it silently would strand memory the pool believes is out and hand the
    /// same address two owners. An occupied leaf here is a nucleus invariant,
    /// not an invitation.
    pub fn map_empty_page(
        &mut self,
        tables: &mut dyn TableSource,
        virt: u64,
        phys: u64,
        flags: u64,
    ) -> Result<(), PagingRefused> {
        let (l4, l3, l2, l1) = indices(virt)?;
        let interior = Self::interior_for(flags);
        let pdpt = self.descend(tables, self.root, l4, interior)?;
        let pd = self.descend(tables, pdpt, l3, interior)?;
        let pt = self.descend(tables, pd, l2, interior)?;
        if Self::entry(pt, l1) & PRESENT != 0 {
            return Err(PagingRefused::Granularity(virt));
        }
        Self::write(pt, l1, (phys & ADDRESS) | flags | PRESENT);
        Ok(())
    }

    /// Clears one leaf and says what it named, without touching the processor.
    ///
    /// For trees that are never loaded into `CR3`: the region backing index is
    /// metadata, so `invlpg` on its addresses would be flushing a translation
    /// the processor was never given. [`AddressSpace::unmap_page`] is what a
    /// live space uses.
    pub fn clear_leaf(&mut self, virt: u64) -> Option<u64> {
        let (l4, l3, l2, l1) = indices(virt).ok()?;
        let mut table = self.root;
        for index in [l4, l3, l2] {
            let entry = Self::entry(table, index);
            if entry & PRESENT == 0 || entry & HUGE != 0 {
                return None;
            }
            table = entry & ADDRESS;
        }
        let leaf = Self::entry(table, l1);
        if leaf & PRESENT == 0 {
            return None;
        }
        Self::write(table, l1, 0);
        Some(leaf & ADDRESS)
    }

    /// Returns the page tables under one top-level entry, and the entry itself.
    ///
    /// **One region lane is exactly one top-level entry**, which is what makes
    /// this expressible at all: there is no shared ancestry to prove anything
    /// about, so clearing the entry detaches the whole subtree at once and
    /// every table beneath it is unreachable by construction. Leaves are not
    /// touched — what they name is the pool's or the loader's, and returning it
    /// here would hand the same frame out twice.
    ///
    /// Answers whether a branch was there. An ordinary lifecycle release of a
    /// lane that is absent is a defect in the caller; a rollback of a
    /// construction that never got that far is not, which is why this reports
    /// rather than refusing.
    ///
    /// # Safety
    ///
    /// Nothing under this branch is mapped in any other space, and no processor
    /// is using a translation from it — either this space is not live, or the
    /// caller flushes before returning to ring 3.
    // SAFETY: the caller's promise that the branch is unshared and unused is
    // what makes every table below it unreachable.
    pub unsafe fn release_branch(&mut self, tables: &mut dyn TableSource, virt: u64) -> bool {
        let Ok((l4, _, _, _)) = indices(virt) else {
            return false;
        };
        let entry = Self::entry(self.root, l4);
        if entry & PRESENT == 0 || entry & HUGE != 0 {
            return false;
        }
        let pdpt = entry & ADDRESS;
        Self::write(self.root, l4, 0);
        for l3 in 0..ENTRIES {
            let entry = Self::entry(pdpt, l3);
            if entry & PRESENT == 0 || entry & HUGE != 0 {
                continue;
            }
            let directory = entry & ADDRESS;
            for l2 in 0..ENTRIES {
                let entry = Self::entry(directory, l2);
                if entry & PRESENT == 0 || entry & HUGE != 0 {
                    continue;
                }
                // SAFETY: a page table of a branch nothing reaches.
                unsafe { tables.give_back(entry & ADDRESS) };
            }
            // SAFETY: as above, and every table under it has gone back.
            unsafe { tables.give_back(directory) };
        }
        // SAFETY: as above.
        unsafe { tables.give_back(pdpt) };
        true
    }

    /// Returns every page table this space is made of to the reserve.
    ///
    /// **The tables, and never what they map.** A leaf points at memory that is
    /// somebody else's — a frame the pool handed the process and has already
    /// taken back, or the loader's image and capsule, which were never the
    /// pool's at all — so every leaf is skipped and only the interior frames go
    /// back. A 1 GiB or 2 MiB entry *is* a leaf despite sitting at an interior
    /// level, which is why the huge bit is checked at both.
    ///
    /// # Safety
    ///
    /// This space is not the live one, no other space shares a table with it,
    /// and nothing will use it again. Every process is built its own tree by
    /// [`build`], which is what makes the second condition true.
    // SAFETY: the caller's promise that this tree is unreachable is what makes
    // handing its frames back sound.
    pub unsafe fn release_tables(&mut self, tables: &mut dyn TableSource) {
        for l4 in 0..ENTRIES {
            let entry = Self::entry(self.root, l4);
            if entry & PRESENT == 0 {
                continue;
            }
            let pdpt = entry & ADDRESS;
            for l3 in 0..ENTRIES {
                let entry = Self::entry(pdpt, l3);
                if entry & PRESENT == 0 || entry & HUGE != 0 {
                    continue;
                }
                let directory = entry & ADDRESS;
                for l2 in 0..ENTRIES {
                    let entry = Self::entry(directory, l2);
                    if entry & PRESENT == 0 || entry & HUGE != 0 {
                        continue;
                    }
                    // SAFETY: a page table of a tree nothing reaches.
                    unsafe { tables.give_back(entry & ADDRESS) };
                }
                // SAFETY: as above, and every table under it has gone back.
                unsafe { tables.give_back(directory) };
            }
            // SAFETY: as above.
            unsafe { tables.give_back(pdpt) };
        }
        // SAFETY: as above; this was the last frame of the tree.
        unsafe { tables.give_back(self.root) };
        self.root = 0;
    }

    /// What the path to a leaf with these flags must itself allow.
    fn interior_for(flags: u64) -> u64 {
        PRESENT | WRITABLE | (flags & USER)
    }

    /// Maps one 4 KiB page, identity or otherwise.
    pub fn map_page(
        &mut self,
        tables: &mut dyn TableSource,
        virt: u64,
        phys: u64,
        flags: u64,
    ) -> Result<(), PagingRefused> {
        let (l4, l3, l2, l1) = indices(virt)?;
        let interior = Self::interior_for(flags);
        let pdpt = self.descend(tables, self.root, l4, interior)?;
        let pd = self.descend(tables, pdpt, l3, interior)?;
        let pt = self.descend(tables, pd, l2, interior)?;
        Self::write(pt, l1, (phys & ADDRESS) | flags | PRESENT);
        Ok(())
    }

    /// Maps one 2 MiB page.
    pub fn map_huge(
        &mut self,
        tables: &mut dyn TableSource,
        virt: u64,
        phys: u64,
        flags: u64,
    ) -> Result<(), PagingRefused> {
        let (l4, l3, l2, _) = indices(virt)?;
        let interior = Self::interior_for(flags);
        let pdpt = self.descend(tables, self.root, l4, interior)?;
        let pd = self.descend(tables, pdpt, l3, interior)?;
        Self::write(pd, l2, (phys & ADDRESS) | flags | HUGE | PRESENT);
        Ok(())
    }

    /// Removes one 4 KiB mapping, if the path to it exists.
    ///
    /// The tables on the path are kept. They are four frames at most, they will
    /// be needed again by the next process at the same address, and freeing an
    /// interior table means proving nothing else under it is mapped — a proof
    /// this does not attempt and therefore does not claim.
    pub fn unmap_page(&mut self, virt: u64) {
        let Ok((l4, l3, l2, l1)) = indices(virt) else {
            return;
        };
        let mut table = self.root;
        for index in [l4, l3, l2] {
            let entry = Self::entry(table, index);
            if entry & PRESENT == 0 || entry & HUGE != 0 {
                return;
            }
            table = entry & ADDRESS;
        }
        Self::write(table, l1, 0);
        // SAFETY: the entry is gone from the live tree, so the processor's
        // cached translation of it must go too; `invlpg` names one address and
        // touches no memory of its own.
        unsafe { core::arch::asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags)) };
    }

    /// The frame a 4 KiB page maps to, when it maps to one.
    ///
    /// The nucleus asks this of a *process's* space, about the process's own
    /// pages, when the process is over and its memory has to go back. Reading
    /// the answer out of the tables rather than remembering it means the
    /// bookkeeping cannot drift from the mappings — there is only one record of
    /// what a process held, and it is the one the processor used.
    pub fn translate(&self, virt: u64) -> Option<u64> {
        let (l4, l3, l2, l1) = indices(virt).ok()?;
        let mut table = self.root;
        for index in [l4, l3, l2] {
            let entry = Self::entry(table, index);
            if entry & PRESENT == 0 || entry & HUGE != 0 {
                return None;
            }
            table = entry & ADDRESS;
        }
        let leaf = Self::entry(table, l1);
        (leaf & PRESENT != 0).then_some(leaf & ADDRESS)
    }

    /// Whether one mapped page may be written, if it is mapped at all.
    ///
    /// The leaf's own bit, not the path's: every real permission is stated on
    /// the leaf here, and the interiors are deliberately permissive.
    pub fn writable(&self, virt: u64) -> Option<bool> {
        let (l4, l3, l2, l1) = indices(virt).ok()?;
        let mut table = self.root;
        for index in [l4, l3, l2] {
            let entry = Self::entry(table, index);
            if entry & PRESENT == 0 || entry & HUGE != 0 {
                return None;
            }
            table = entry & ADDRESS;
        }
        let leaf = Self::entry(table, l1);
        (leaf & PRESENT != 0).then_some(leaf & WRITABLE != 0)
    }

    /// Discards every cached translation of the live space.
    ///
    /// **One reload rather than an address at a time.** A region lane is half a
    /// terabyte of address space; naming each of its pages to `invlpg` would be
    /// a loop the size of the lane, and the entries being discarded are a whole
    /// top-level branch rather than a page. Reloading `CR3` drops every
    /// non-global translation at once, which is what a branch disappearing
    /// actually means.
    ///
    /// # Safety
    ///
    /// The live space still maps this nucleus at the addresses it is running
    /// at — which is true of every space this nucleus builds — and the caller
    /// is between two well-defined points, not part-way through editing the
    /// tree it is about to be running on.
    // SAFETY: the caller's promise that the live tree is complete and maps the
    // nucleus is what makes reloading it safe.
    pub unsafe fn flush() {
        // SAFETY: reading and writing `CR3` with the value it already holds
        // changes no mapping; it discards cached translations.
        unsafe {
            let root: u64;
            core::arch::asm!("mov {}, cr3", out(reg) root, options(nomem, nostack));
            core::arch::asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags));
        }
    }

    /// Makes this space the one the processor is using.
    ///
    /// # Safety
    ///
    /// The caller states that this space maps, at the same addresses, every
    /// byte the nucleus will touch from here on — its own image, its stack, the
    /// handoff record, the memory map, the capsule, its page tables and the
    /// framebuffer if there is one — and that no other context is running.
    /// Everything else in the machine becomes unreachable at this instruction,
    /// which is the point of it.
    // SAFETY: the caller's promise that this space covers the running nucleus is
    // the whole contract; a missing mapping shows up as the fault it is.
    pub unsafe fn activate(&self) {
        // SAFETY: `IA32_EFER` is architected on every x86_64 processor, `NXE`
        // is the bit that makes the `NX` flag legal rather than reserved, and
        // this must precede the load of tables that use it.
        unsafe { msr::write(IA32_EFER, msr::read(IA32_EFER) | EFER_NXE) };
        // SAFETY: `CR0.WP` only makes ring-0 stores respect the read-only bit;
        // nothing in the nucleus writes its own text or rodata.
        unsafe {
            let mut cr0: u64;
            core::arch::asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
            core::arch::asm!("mov cr0, {}", in(reg) cr0 | CR0_WP, options(nostack, preserves_flags));
        }
        // SAFETY: per this function's contract; `load` is the instruction that
        // performs the change, and the two register writes above are the mode
        // bits the tables were built assuming.
        unsafe { self.load() };
    }

    /// Loads this space's tables, and does nothing else.
    ///
    /// [`activate`](Self::activate) sets the two mode bits first, and does so
    /// every time because it is called at points where the mode is not yet
    /// known to be right. **A context switch is not such a point**: `EFER.NXE`
    /// and `CR0.WP` were set before the first process existed and nothing
    /// clears them, so a switch writes `CR3` and nothing more. The distinction
    /// is not micro-optimization — it is that a preemption handler should
    /// change exactly one thing about the machine.
    ///
    /// # Safety
    ///
    /// As [`activate`](Self::activate): this space maps, at the same addresses,
    /// every byte the nucleus will touch from the next instruction on.
    // SAFETY: the caller's promise that this space covers the running nucleus is
    // the whole contract; the write itself is one architected register.
    pub unsafe fn load(&self) {
        // SAFETY: `root` is a cleared, page-aligned frame at the top of a
        // complete four-level tree; loading it makes that tree the live one.
        unsafe { load_root(self.root) };
    }
}

/// Loads a page-table root, and does nothing else.
///
/// The one instruction in this nucleus that changes which address space the
/// processor is in. The scheduler names a root rather than a space — a process
/// slot records where its tree is, and one process runs in a tree the nucleus
/// does not own — so the operation is published here at that shape, rather than
/// written out a second time somewhere it would be easier to get wrong.
///
/// # Safety
///
/// The tree at `root` maps this nucleus at the addresses it is running at, from
/// the next instruction on: its text, the stack in use, and any device register
/// the current path is about to touch.
// SAFETY: the caller's promise about the tree is the whole contract; a missing
// mapping shows up as the fault it is.
pub unsafe fn load_root(root: u64) {
    // SAFETY: per the caller's contract.
    unsafe { core::arch::asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags)) };
}

/// The four table indices of a virtual address.
fn indices(virt: u64) -> Result<(u64, u64, u64, u64), PagingRefused> {
    if virt >= 1 << 48 {
        return Err(PagingRefused::AddressTooHigh(virt));
    }
    Ok((
        (virt >> 39) & (ENTRIES - 1),
        (virt >> 30) & (ENTRIES - 1),
        (virt >> 21) & (ENTRIES - 1),
        (virt >> 12) & (ENTRIES - 1),
    ))
}

extern "C" {
    static __tos_image_start: u8;
    static __tos_text_end: u8;
    static __tos_rodata_end: u8;
    static __tos_image_end: u8;
}

/// The three parts of the nucleus image, as the linker script marks them.
fn image_parts() -> (Span, Span, Span) {
    // Only the addresses of these symbols are taken; nothing is read through
    // them, which is why no unsafe block is needed here.
    let start = core::ptr::addr_of!(__tos_image_start) as u64;
    let text_end = core::ptr::addr_of!(__tos_text_end) as u64;
    let rodata_end = core::ptr::addr_of!(__tos_rodata_end) as u64;
    let end = core::ptr::addr_of!(__tos_image_end) as u64;
    (
        Span::new(start, text_end),
        Span::new(text_end, rodata_end),
        Span::new(rodata_end, end),
    )
}

/// The framebuffer's span, when the loader declared one.
fn framebuffer(bi: &BootInfo) -> Option<Span> {
    let length = u64::from(bi.framebuffer_pitch) * u64::from(bi.framebuffer_height);
    if bi.framebuffer_phys == 0 || length == 0 {
        return None;
    }
    Span::sized(bi.framebuffer_phys, length)
}

/// What one 4 KiB page of physical memory may be used for, or nothing at all.
///
/// This is the whole permission policy of the nucleus's address space, in one
/// place, so that reading it is how one learns what the nucleus can do to its
/// own memory — rather than assembling the answer from call sites.
fn permission(address: u64, fb: Option<Span>) -> Option<u64> {
    if address < FRAME_SIZE {
        // Page zero stays absent so a null dereference faults.
        return None;
    }
    if let Some(fb) = fb {
        if fb.holds(address) {
            // Uncacheable: this is a device's memory, and a cached write to it
            // is a write whose arrival is nobody's promise.
            return Some(WRITABLE | NO_EXECUTE | CACHE_DISABLE | WRITE_THROUGH);
        }
    }
    let (text, rodata, data) = image_parts();
    if text.holds(address) {
        return Some(0); // present, read-only, executable
    }
    if rodata.holds(address) {
        return Some(NO_EXECUTE);
    }
    if data.holds(address) {
        return Some(WRITABLE | NO_EXECUTE);
    }
    Some(WRITABLE | NO_EXECUTE)
}

/// Whether a 2 MiB region must be mapped one page at a time.
fn needs_pages(chunk: u64, image: Span, fb: Option<Span>) -> bool {
    let end = chunk + HUGE_SIZE;
    let overlaps = |span: Span| span.start < end && span.end > chunk;
    chunk == 0 || overlaps(image) || fb.is_some_and(overlaps)
}

/// Whether anything the machine reported lives in this 2 MiB region.
fn described(chunk: u64, descs: &[MemoryRange], fb: Option<Span>) -> bool {
    let end = chunk + HUGE_SIZE;
    let overlaps = |span: Span| span.start < end && span.end > chunk;
    descs
        .iter()
        .filter_map(|d| Span::sized(d.phys_start, d.phys_length))
        .any(overlaps)
        || fb.is_some_and(overlaps)
}

/// An upper bound on the page-table frames [`build`] allocates for one address
/// space on this machine.
///
/// **It mirrors the strategy rather than assuming the worst one.** `build` maps
/// the bulk of described memory with 2 MiB leaves, and a 2 MiB leaf needs no
/// page table at all; only the chunks that hold the nucleus image or the
/// framebuffer are broken into 4 KiB pages. A bound that counted a page table
/// per 2 MiB of address space would be two orders of magnitude out — on a
/// machine whose framebuffer sits near the top of the 32-bit range it would
/// reserve gigabytes to map a few hundred megabytes — so the count walks the
/// same chunks `build` walks and asks the same question about each.
///
/// Still an upper bound: a chunk that needs 4 KiB pages is counted a page table
/// even when every page in it turns out to be one this space deliberately
/// leaves absent, and the local APIC's path is counted whether or not it shares
/// one with something described.
pub fn build_tables(bi: &BootInfo, descs: &[MemoryRange]) -> u64 {
    let fb = framebuffer(bi);
    let (text, _, data) = image_parts();
    let image = Span::new(text.start, data.end);
    let mut top = fb.map_or(0, |span| span.end);
    for desc in descs {
        if let Some(span) = Span::sized(desc.phys_start, desc.phys_length) {
            top = top.max(span.end);
        }
    }
    let top = top.div_ceil(HUGE_SIZE) * HUGE_SIZE;

    // Counted over the chunks that are actually described, in increasing
    // order, so an interior table is counted when the walk first enters the
    // unit of address space it covers. Counting per unit of the whole span
    // instead would size the reserve by where the firmware put its highest
    // range — a page directory per gigabyte of a terabyte-wide map, for a
    // machine with a few hundred megabytes in it.
    let mut pdpts = 0;
    let mut directories = 0;
    let mut fine = 0;
    let mut in_pdpt = u64::MAX;
    let mut in_directory = u64::MAX;
    let mut chunk = 0u64;
    while chunk < top {
        if described(chunk, descs, fb) {
            if chunk >> 39 != in_pdpt {
                in_pdpt = chunk >> 39;
                pdpts += 1;
            }
            if chunk >> 30 != in_directory {
                in_directory = chunk >> 30;
                directories += 1;
            }
            if needs_pages(chunk, image, fb) {
                fine += 1;
            }
        }
        chunk += HUGE_SIZE;
    }
    // The root table; the interiors above; one page table per chunk broken
    // into 4 KiB pages; and the three levels the local APIC's page may need
    // for itself.
    1 + pdpts + directories + fine + 3
}

/// Builds the nucleus's own address space over this machine.
///
/// Identity-mapped throughout: the nucleus was linked at a physical address and
/// the loader placed it there, so a nucleus that moved its own text under itself
/// would have to be relocatable to survive the instruction that did it. Ring 3
/// address spaces are a separate question and are not this one.
pub fn build(
    bi: &BootInfo,
    descs: &[MemoryRange],
    tables: &mut dyn TableSource,
) -> Result<AddressSpace, PagingRefused> {
    let mut space = AddressSpace::new(tables)?;
    match fill(&mut space, bi, descs, tables) {
        Ok(()) => Ok(space),
        Err(refused) => {
            // A half-built space is nobody's, and its tables are the reserve's.
            // Leaving them behind would make a failed creation cost the machine
            // an address space's worth of reserve until the next boot.
            // SAFETY: this space was never activated, nothing else shares a
            // table with it, and it is dropped on the next line.
            unsafe { space.release_tables(tables) };
            Err(refused)
        }
    }
}

/// Everything `build` maps, separated so a failure part-way has somewhere to
/// return the tables to.
fn fill(
    space: &mut AddressSpace,
    bi: &BootInfo,
    descs: &[MemoryRange],
    tables: &mut dyn TableSource,
) -> Result<(), PagingRefused> {
    let fb = framebuffer(bi);
    let (text, _, data) = image_parts();
    let image = Span::new(text.start, data.end);

    let mut top = fb.map_or(0, |span| span.end);
    for desc in descs {
        if let Some(span) = Span::sized(desc.phys_start, desc.phys_length) {
            top = top.max(span.end);
        }
    }
    let top = top.div_ceil(HUGE_SIZE) * HUGE_SIZE;

    let mut chunk = 0u64;
    while chunk < top {
        if !described(chunk, descs, fb) {
            chunk += HUGE_SIZE;
            continue;
        }
        if needs_pages(chunk, image, fb) {
            let mut page = chunk;
            while page < chunk + HUGE_SIZE {
                if let Some(flags) = permission(page, fb) {
                    space.map_page(tables, page, page, flags)?;
                }
                page += FRAME_SIZE;
            }
        } else {
            space.map_huge(tables, chunk, chunk, WRITABLE | NO_EXECUTE)?;
        }
        chunk += HUGE_SIZE;
    }
    // The local APIC, in every address space this nucleus builds: a timer
    // interrupt taken at CPL 3 runs the nucleus's handler without changing CR3,
    // and that handler acknowledges the interrupt by writing a device register.
    // Uncacheable, supervisor-only, and not executable.
    space.map_page(
        tables,
        crate::apic::LOCAL_APIC,
        crate::apic::LOCAL_APIC,
        WRITABLE | NO_EXECUTE | CACHE_DISABLE | WRITE_THROUGH,
    )?;
    Ok(())
}

/// Reads the one address this space deliberately leaves absent.
///
/// Test-only, and excluded from every production artifact. It exists because
/// "page zero is unmapped" is a claim about hardware behaviour, and the only
/// evidence for a claim about hardware is the hardware refusing: the read below
/// is expected to take vector 14 with `CR2 = 0` and end the boot on the ordinary
/// fatal path, which is what the gate asserts.
#[cfg(feature = "test-paging-unmapped")]
pub fn test_injection() {
    tos_serial::puts(b"TOS.TEST.PAGING.UNMAPPED\r\n");
    // SAFETY: this isolated test-only artifact dereferences the null page after
    // `activate()` established the nucleus's own tables, where that page is
    // absent by construction. The page-fault handler is fatal and never
    // resumes, so no value is ever read.
    unsafe { core::ptr::with_exposed_provenance::<u64>(0).read_volatile() };
}

/// Writes to the nucleus's own text.
///
/// Test-only, and excluded from every production artifact. "Text is read-only"
/// is a claim about two things at once — the mapping the nucleus wrote and
/// `CR0.WP`, without which a ring-0 store ignores that mapping — and neither is
/// proved by reading the table back. The store below is expected to take vector
/// 14 with the architecture's protection-violation error code and `CR2` naming
/// the first byte of the image.
#[cfg(feature = "test-paging-readonly-text")]
pub fn test_readonly_text() {
    tos_serial::puts(b"TOS.TEST.PAGING.READONLY.TEXT\r\n");
    let text = core::ptr::addr_of!(__tos_image_start) as u64;
    // SAFETY: this isolated test-only artifact writes to the first byte of the
    // nucleus image, which `activate()` mapped read-only and executable. The
    // page-fault handler is fatal and never resumes, so the write never lands
    // and no instruction of the image is ever modified.
    unsafe { core::ptr::with_exposed_provenance_mut::<u8>(text as usize).write_volatile(0) };
}
