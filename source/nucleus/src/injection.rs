// SPDX-License-Identifier: GPL-3.0-or-later
//! Where a process creation is made to fail, so the unwinding can be watched.
//!
//! **Deterministic, and at named points rather than after a count.** A
//! rollback is only worth as much as the failures it has actually survived, and
//! the failures that matter are not evenly spread: the interesting ones are the
//! instant between a frame leaving the pool and its leaf existing, and the
//! moment a carved run is half-mapped. Counting allocations would reach those
//! by arithmetic that changes whenever the image does. Naming them does not.
//!
//! Test builds only. The production nucleus does not contain this module, and
//! every call site is behind the same feature.

/// A point at which a creation is made to fail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Case {
    /// The pool refuses part-way through the image's writable data.
    DataFrame,
    /// The pool refuses part-way through the arena.
    GrantFrame,
    /// The reserve refuses **after** a user frame has left the pool and before
    /// its leaf exists — the one frame no unwinding can find by walking page
    /// tables.
    GrantTable,
    /// The pool cannot carve the launch record.
    RecordCarve,
    /// The reserve refuses part-way through mapping the launch record, which
    /// leaves a carved run the page tables only partly name.
    RecordMapping,
    /// A record larger than any record may be, refused before anything moves.
    RecordTooLarge,
    /// Bytes that are not a runtime image, refused before anything moves.
    BadHeader,
    /// A funding request larger than the root authority holds.
    OverBudget,
}

/// The case armed for the next creation, if any.
static mut ARMED: Option<Case> = None;

/// Arms one case for the next creation.
pub fn arm(case: Case) {
    // SAFETY: single-context nucleus; the boot driver is the only writer.
    unsafe { ARMED = Some(case) };
}

/// Disarms, so the boot's real creation is not touched.
pub fn disarm() {
    // SAFETY: as `arm`.
    unsafe {
        ARMED = None;
        LIVE = false;
    };
}

/// Whether the reserve is being made to refuse *now*.
///
/// The reserve is asked for a table long before the interesting moment — the
/// nucleus builds a whole address space first — so a case that simply refused
/// would fail the creation before a single user frame had moved, which is a
/// different test. `create` turns this on at the exact point it wants: with a
/// frame already out of the pool and its leaf not yet written.
static mut LIVE: bool = false;

/// Arms one case for the next creation.
pub fn enable() {
    // SAFETY: as `arm`.
    unsafe { LIVE = true };
}

/// Whether this point is the armed one.
pub fn armed(case: Case) -> bool {
    // SAFETY: single-context nucleus; written only between creations.
    unsafe { ARMED == Some(case) }
}

/// Whether the reserve should refuse the table it is being asked for.
pub fn tables_refuse() -> bool {
    // SAFETY: as `armed`.
    unsafe { LIVE && matches!(ARMED, Some(Case::GrantTable) | Some(Case::RecordMapping)) }
}
