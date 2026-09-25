// SPDX-License-Identifier: GPL-3.0-or-later
//! What this boot is able to launch.
//!
//! Every process this system builds is made of the same things: the verified
//! runtime image, the capsule's source set, the validated memory map, and the
//! identity of the nucleus granting memory. Which module a process runs and what
//! authority it holds vary; **what the system is made of does not.**
//!
//! That distinction is why this is nucleus state rather than a launcher's local.
//! A supervisor asking for a process to be created chooses the module and the
//! authority; it does not, and must not, supply the image or the source set —
//! those are facts about this boot that were validated once, before anything ran
//! at CPL 3, and a process able to restate them would be a process able to
//! launch something the boot never accepted.
//!
//! Every borrow here is `'static` and that is a claim about the machine, not a
//! convenience: the loader reserved these ranges, the nucleus identity-maps them
//! in every address space it builds, and nothing ever unmaps them.

use tos_boot_protocol::{BootInfo, MemoryRange};
use tos_runtime::region::Span;

/// How many source units one boot may offer the frontend.
///
/// A fixed bound, chosen by the nucleus and not derived from capsule input: the
/// nucleus must not size an array from a number an attacker chose. It bounds
/// the **set offered**, which is every `.tos` file the capsule carries — not
/// the closure the entry actually reaches, which resolution computes and which
/// docs/44 caps at 256. Those are different numbers: a capsule may legitimately
/// carry more source than any one module imports, and the Stage 1 performance
/// fixture carries a thousand files for exactly that reason. A capsule offering
/// more than this is refused rather than truncated, because a silently shortened
/// set would run a program whose dependencies are missing.
pub const MAX_BOOT_MODULES: usize = 1024;

/// A capsule path as a module-root-relative one: what docs/42 §1 derives a
/// module name from, so the capsule's leading slash is not part of it.
fn relative(path: &[u8]) -> &[u8] {
    match path.split_first() {
        Some((b'/', rest)) => rest,
        _ => path,
    }
}

/// One source unit: its capsule path, and its bytes.
type Unit = (&'static [u8], &'static [u8]);

/// What every process of this boot is built from.
pub struct Template {
    /// The handoff record and the validated map, for the address space each
    /// process gets.
    pub bi: &'static BootInfo,
    pub descs: &'static [MemoryRange],
    /// The verified runtime image, and the capsule its source comes from.
    pub image: Span,
    pub capsule: Span,
    /// The source set offered to every process of this boot.
    units: [Unit; MAX_BOOT_MODULES],
    unit_count: usize,
    /// Which nucleus build grants memory (ADR-0050).
    pub identity: u64,
    /// The declared identity of the source set, and how much of it is written.
    source_set: [u8; 96],
    source_set_length: usize,
    /// The boot-canonical file's content digest (ADR-0102 §4f).
    ///
    /// **Retained, not recomputed.** `CAPSULE_FORMAT_V1` §4.2 gives every file a
    /// SHA-256 the parser has already checked against the exact content bytes, and
    /// §5 makes exactly one file boot-canonical. Copying that value here is all the
    /// nucleus does for it: hashing again would be a second computation of a fact
    /// the boot has already established, and the nucleus does no cryptographic work
    /// on behalf of operation 32.
    ///
    /// **One digest, not a catalog.** What is retained is the boot-canonical file's
    /// and nothing else's; ADR-0102 exposes no capsule enumeration.
    boot_content_digest: [u8; 32],
}

impl Template {
    /// The source set this boot offers.
    pub fn units(&self) -> &[Unit] {
        &self.units[..self.unit_count]
    }

    /// The declared identity of that set.
    pub fn source_set(&self) -> &[u8] {
        &self.source_set[..self.source_set_length]
    }

    /// The boot-canonical file's validated content digest (ADR-0102 §4f).
    pub fn boot_content_digest(&self) -> &[u8; 32] {
        &self.boot_content_digest
    }

    /// Whether `index` names a unit of this boot's set.
    pub fn holds(&self, index: usize) -> bool {
        index < self.unit_count
    }

    /// Which unit a module-root-relative path names, if any.
    ///
    /// A supervisor reasoning about `/system/policy/` holds a module's *name*.
    /// An ordinal is a position in a list nobody published, and two boots whose
    /// capsules differ would give the same ordinal to different modules — so a
    /// path is not a convenience here, it is the only stable way to say which
    /// module is meant.
    pub fn index_of(&self, path: &[u8]) -> Option<usize> {
        self.units()
            .iter()
            .position(|(name, _)| relative(name) == path)
    }
}

/// What this boot can launch, once it has been said.
///
/// `Option` rather than a zeroed value and a flag beside it: "there is no
/// template yet" is a state of the template, and a type that can express it is
/// one nothing can read past. A struct pre-filled with placeholders would need
/// a discipline to keep anyone from trusting the placeholders, and a discipline
/// is what a type is for.
static mut TEMPLATE: Option<Template> = None;

/// What this boot can say about the source it came from.
///
/// **Two facts and one parameter, because they are one thing**: the declared
/// identity of the source set, and the digest of the one file
/// `CAPSULE_FORMAT_V1` §5 makes boot-canonical. Both come from the same verified
/// capsule header and both are read back by the same operation (ADR-0102 §4).
pub struct SourceFacts<'a> {
    /// `"git:<hex>"` or `"detached:<hex>"`, as the nucleus flattened it.
    pub set: &'a [u8],
    /// The boot-canonical file's SHA-256, already validated by the parser.
    pub boot_content_digest: &'a [u8; 32],
}

/// Fixes what this boot can launch. Called once, before the first process.
///
/// # Safety
///
/// `bi` and `descs` passed the Boot ABI v1 validation; `image` names the
/// verified runtime image and `capsule` the verified capsule, both physically
/// contiguous and identity-mapped for this nucleus; and every unit's bytes lie
/// inside `capsule`.
// SAFETY: the caller's promise that all four inputs are the validated ones is
// what makes every process built from this template a process built from what
// the boot accepted.
pub unsafe fn establish(
    bi: &'static BootInfo,
    descs: &'static [MemoryRange],
    image: Span,
    capsule: Span,
    units: &[Unit],
    identity: u64,
    source: SourceFacts<'_>,
) {
    let mut template = Template {
        bi,
        descs,
        image,
        capsule,
        units: [(&[], &[]); MAX_BOOT_MODULES],
        unit_count: units.len().min(MAX_BOOT_MODULES),
        identity,
        source_set: [0; 96],
        source_set_length: source.set.len().min(96),
        boot_content_digest: *source.boot_content_digest,
    };
    template.units[..template.unit_count].copy_from_slice(&units[..template.unit_count]);
    template.source_set[..template.source_set_length]
        .copy_from_slice(&source.set[..template.source_set_length]);
    // Assigned whole, so there is no instant at which a reader could see a
    // template that is only partly filled.
    // SAFETY: single-context nucleus at boot; nothing else touches this and no
    // process exists yet.
    unsafe { *core::ptr::addr_of_mut!(TEMPLATE) = Some(template) };
}

/// What this boot can launch, or nothing when the boot has not said yet.
pub fn template() -> Option<&'static Template> {
    // SAFETY: single-context nucleus; the template is written once, whole, and
    // never cleared.
    unsafe { (*core::ptr::addr_of!(TEMPLATE)).as_ref() }
}
