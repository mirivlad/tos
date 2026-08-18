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
//! **Built from the validated map, from frames the nucleus owns.** Every table
//! is a frame from [`tos_frames::Frames`], and every mapping comes from the
//! memory map the Boot ABI validation already accepted. Nothing here reads a
//! firmware table.
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
use tos_frames::{Frames, FRAME_SIZE};
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
    /// An empty space: a cleared root table and nothing mapped.
    pub fn new(frames: &mut Frames) -> Result<AddressSpace, PagingRefused> {
        let root = frames.allocate_frame().ok_or(PagingRefused::NoFrame)?;
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
        frames: &mut Frames,
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
        let frame = frames.allocate_frame().ok_or(PagingRefused::NoFrame)?;
        Self::write(table, index, frame | PRESENT | WRITABLE | interior);
        Ok(frame)
    }

    /// What the path to a leaf with these flags must itself allow.
    fn interior_for(flags: u64) -> u64 {
        PRESENT | WRITABLE | (flags & USER)
    }

    /// Maps one 4 KiB page, identity or otherwise.
    pub fn map_page(
        &mut self,
        frames: &mut Frames,
        virt: u64,
        phys: u64,
        flags: u64,
    ) -> Result<(), PagingRefused> {
        let (l4, l3, l2, l1) = indices(virt)?;
        let interior = Self::interior_for(flags);
        let pdpt = self.descend(frames, self.root, l4, interior)?;
        let pd = self.descend(frames, pdpt, l3, interior)?;
        let pt = self.descend(frames, pd, l2, interior)?;
        Self::write(pt, l1, (phys & ADDRESS) | flags | PRESENT);
        Ok(())
    }

    /// Maps one 2 MiB page.
    pub fn map_huge(
        &mut self,
        frames: &mut Frames,
        virt: u64,
        phys: u64,
        flags: u64,
    ) -> Result<(), PagingRefused> {
        let (l4, l3, l2, _) = indices(virt)?;
        let interior = Self::interior_for(flags);
        let pdpt = self.descend(frames, self.root, l4, interior)?;
        let pd = self.descend(frames, pdpt, l3, interior)?;
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
            // SAFETY: `root` is a cleared, page-aligned frame at the top of a
            // complete four-level tree built above; loading it replaces the
            // firmware's map with this one.
            core::arch::asm!("mov cr3, {}", in(reg) self.root, options(nostack, preserves_flags));
        }
    }
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

/// Builds the nucleus's own address space over this machine.
///
/// Identity-mapped throughout: the nucleus was linked at a physical address and
/// the loader placed it there, so a nucleus that moved its own text under itself
/// would have to be relocatable to survive the instruction that did it. Ring 3
/// address spaces are a separate question and are not this one.
pub fn build(
    bi: &BootInfo,
    descs: &[MemoryRange],
    frames: &mut Frames,
) -> Result<AddressSpace, PagingRefused> {
    let mut space = AddressSpace::new(frames)?;
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
                    space.map_page(frames, page, page, flags)?;
                }
                page += FRAME_SIZE;
            }
        } else {
            space.map_huge(frames, chunk, chunk, WRITABLE | NO_EXECUTE)?;
        }
        chunk += HUGE_SIZE;
    }
    // The local APIC, in every address space this nucleus builds: a timer
    // interrupt taken at CPL 3 runs the nucleus's handler without changing CR3,
    // and that handler acknowledges the interrupt by writing a device register.
    // Uncacheable, supervisor-only, and not executable.
    space.map_page(
        frames,
        crate::apic::LOCAL_APIC,
        crate::apic::LOCAL_APIC,
        WRITABLE | NO_EXECUTE | CACHE_DISABLE | WRITE_THROUGH,
    )?;
    Ok(space)
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
