// SPDX-License-Identifier: GPL-3.0-or-later
//! What a nucleus hands a runtime image when it starts a process.
//!
//! ADR-0053 delivers the runtime image beside the capsule, which makes it an
//! artifact that can be replaced without rebuilding the nucleus. That is
//! exactly what AGENTS.md section 8 means by a public boundary, so it is
//! versioned from its first commit: an image that declares another version is
//! refused rather than started, in both directions.
//!
//! **Everything here is addressed in the process's own address space.** The
//! nucleus builds that space, so it knows both sides; the image knows only its
//! own. No field is a physical address, and the image never learns one.
//!
//! **Nothing here is a capability, and that is still true of the endowment.**
//! The record carries memory the process was given, text it was told to run,
//! and — since version 2 (ADR-0055) — a *description* of the authority it was
//! endowed with: which handles it holds and what they name. The authority
//! itself is an entry in a nucleus table the process cannot address
//! (`CAPABILITY_V1` §2); what travels here is the process's copy of its own
//! index, so that it does not have to guess at what it was given. Deleting a
//! line from this table takes nothing away, and adding one grants nothing: the
//! table the nucleus checks is elsewhere.

#![no_std]

/// The version of this record. A nucleus and an image that disagree do not run
/// together.
///
/// Version 4 gives each granted capability the **binding it answers** (ADR-0061),
/// so that a process can tell which of its `import capability` requests each
/// grant is for. Before it, a record carried authority with nothing saying what
/// the module had asked for, which is why no module could use any.
///
/// Version 3 renames the message slot to what ADR-0058 makes it: the region a
/// call's arguments live in when they do not fit in registers. Version 2 added
/// the endowment (ADR-0055) and that slot; version 1 carried memory and text and
/// no authority at all, which is the state in which no process could ever hold a
/// capability.
pub const LAUNCH_VERSION: u32 = 4;

/// What kind of object a capability names (`CAPABILITY_V1` §3).
///
/// Zero is not an object kind and never will be, for the reason operation zero
/// is not an operation: a field nobody wrote holds zero, and giving that a
/// meaning turns an omission into a grant.
pub const OBJECT_ENDPOINT: u32 = 1;
pub const OBJECT_REGION: u32 = 2;
pub const OBJECT_PROCESS: u32 = 3;
pub const OBJECT_INTERFACE: u32 = 4;
/// The right to answer one call (`IPC_V1` §4). An object rather than a status
/// bit, because answering a call is an authority somebody was given and can
/// therefore be refused, delegated once, or lost with the caller.
pub const OBJECT_REPLY: u32 = 5;

/// The one right a reply capability has: `endpoint_reply` (4) is the only
/// operation that names one.
pub const RIGHT_REPLY: u32 = 1 << 5;

/// Endpoint rights (`IPC_V1` §2). They are separate: holding the right to be
/// called is not the right to call.
pub const RIGHT_SEND: u32 = 1;
pub const RIGHT_RECEIVE: u32 = 1 << 1;
pub const RIGHT_CALL: u32 = 1 << 2;

/// Process rights.
///
/// `CAPABILITY_V1` §3 says rights are "a finite set from the object type's
/// declared rights", and no accepted document declares a process object's. They
/// are not invented here: the one object type whose rights *are* declared shows
/// the rule. `IPC_V1` §2 gives an endpoint `send`, `receive` and `call` — which
/// are exactly the three operations of `SYSTEM_ABI_V1` §5 that name an endpoint.
/// **An object's rights are the operations that name it.** §5 names two over a
/// process: `process_create` (8) and `process_terminate` (9).
///
/// The bits are distinct across object types rather than reused per type. The
/// type is compared before the rights are, so reuse would be safe; distinct
/// bits are simply readable in a log, where a rights mask should not need its
/// object beside it to be understood.
pub const RIGHT_CREATE: u32 = 1 << 3;
pub const RIGHT_TERMINATE: u32 = 1 << 4;

/// One capability the launcher endowed this process with, described to the
/// process that holds it.
///
/// The capability itself lives in a nucleus table the process cannot address
/// (`CAPABILITY_V1` §2). This is the process's copy of *what it was given* and
/// *what to call it* — without which a process would hold authority it could
/// not name, and would have to guess indices to discover its own endowment.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LaunchCapability {
    /// The handle: index and generation, as `CAPABILITY_V1` §2's validity rule
    /// requires both.
    pub handle: u64,
    /// One of the `OBJECT_*` kinds.
    pub object: u32,
    /// The rights, as a mask of that object type's declared rights.
    pub rights: u32,
    /// The scope the rights apply to, where the object has one; zero where it
    /// does not.
    pub scope: u64,
    /// **Which capability request this grant answers** (ADR-0061): the name the
    /// module bound its `import capability` to, in UTF-8, padded with zeros.
    ///
    /// Not the position of this entry in the table, and that is the whole
    /// decision. Two imports of one interface are legal, so an interface path
    /// cannot tell them apart; an entry's position could, but then reordering
    /// two `import capability` lines would silently swap which authority each
    /// name receives, and a policy could only be written against a number.
    ///
    /// It is also what makes a denial nameable: the requested set is the
    /// module's bindings, the granted set is these, and
    /// `PROCESS_IDENTITY_V1` §7.3 wants the difference and wants it named.
    ///
    /// An empty binding names no request. It is what a launcher writes when the
    /// grant answers nothing a module asked for, which is a thing a launcher may
    /// do and a module may ignore.
    pub binding: [u8; MAX_BINDING as usize],
    pub binding_length: u32,
    pub reserved: u32,
}

/// The longest capability binding a launch record carries.
///
/// A bound of this contract, like [`MAX_MODULE_PATH`], and for the same reason:
/// a record is a fixed shape read at addresses known in advance, so the name in
/// it cannot be sized by whoever writes it. A module whose binding does not fit
/// is refused at startup rather than truncated to something that names a
/// different request — 64 bytes is far beyond any identifier anyone writes, and
/// the refusal exists so that the bound is never discovered as a silent rename.
pub const MAX_BINDING: u64 = 64;

/// One source unit of the set the process is to execute.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LaunchUnit {
    /// Module-root-relative path, UTF-8, in the process's address space.
    pub path: u64,
    pub path_length: u64,
    /// The unit's source bytes, in the process's address space.
    pub bytes: u64,
    pub bytes_length: u64,
}

/// The record itself, at a fixed address the entry point receives in `rdi`.
#[repr(C)]
pub struct Launch {
    pub version: u32,
    pub unit_count: u32,
    /// Which unit is the entry module.
    pub entry_index: u32,
    pub grant_version: u32,
    /// The one region this process has to allocate out of (ADR-0041, ADR-0050).
    pub grant_base: u64,
    pub grant_length: u64,
    /// Which nucleus build made the grant.
    pub grant_identity: u64,
    /// The unit table: `unit_count` × [`LaunchUnit`].
    pub units: u64,
    /// Where the runtime writes what it has to say about the run. The nucleus
    /// drains it whenever the process enters the edge, so a line written before
    /// a call is on the log by the time the call returns — which is what keeps
    /// "the last event names the stage that hung" true across the boundary.
    pub report_base: u64,
    pub report_length: u64,
    /// The stack this process runs on, so that the runtime can measure what it
    /// actually used rather than have someone else guess for it.
    pub stack_base: u64,
    pub stack_length: u64,
    /// Where a call's arguments live when they do not fit in registers
    /// (ADR-0058).
    ///
    /// `SYSTEM_ABI_V1` §3 admits values and handles as arguments and no pointer
    /// the nucleus walks; six registers cannot carry `IPC_V1`'s 256 inline
    /// bytes, nor four transferred handles, nor a module's name. So those
    /// arguments do not travel in the call at all: they sit in a region the
    /// launcher mapped and the nucleus knows the address of, exactly as the
    /// report region does, and the call names only how much of it to read.
    ///
    /// **It belongs to an execution context, not to a process.** Stage 3 gives a
    /// process one context; the day it has two, two calls in flight would
    /// otherwise share one buffer. It is also arguments and never a channel:
    /// nothing persists in it between calls, and nothing reports through it.
    pub arguments_base: u64,
    pub arguments_length: u64,
    /// The endowment: `capability_count` × [`LaunchCapability`], in this
    /// process's address space, read-only. Zero of them is a legitimate
    /// endowment and the commonest one — a process is given what whoever
    /// launched it decided, and "nothing" is a decision (ADR-0055).
    pub capabilities: u64,
    pub capability_count: u32,
    pub reserved: u32,
    /// The declared identity of the source set, NUL-padded UTF-8.
    pub source_set: [u8; 96],
}

/// The report region's header: the runtime writes `written`, the nucleus reads
/// it and sets `drained`. One writer per field, and neither ever moves the
/// other's.
#[repr(C)]
pub struct ReportHeader {
    pub written: u64,
    pub drained: u64,
}

/// Where a message's parts sit inside the argument region (`IPC_V1` §3,
/// ADR-0058).
///
/// Fixed offsets rather than a packed layout the counts imply: the nucleus
/// reads each part at an address it knows before it has read anything, which is
/// the property that makes "no pointer the nucleus walks" more than a slogan.
pub const MESSAGE_PAYLOAD: u64 = 0;
pub const MESSAGE_CAPABILITIES: u64 = 256;
pub const MESSAGE_REGIONS: u64 = MESSAGE_CAPABILITIES + 8 * MAX_TRANSFERRED_CAPABILITIES;
/// The counts ADR-0057 fixed.
pub const MAX_TRANSFERRED_CAPABILITIES: u64 = 4;
pub const MAX_TRANSFERRED_REGIONS: u64 = 2;

/// Where `process_create`'s arguments sit inside the argument region
/// (`SYSTEM_ABI_V1` §5, ADR-0058).
///
/// Fixed offsets, for the reason the message's are fixed: the nucleus reads each
/// part at an address it knew before it read anything.
pub const CREATE_ENDOWMENT: u64 = 0;
/// How many capabilities a parent may hand a child at creation.
pub const MAX_ENDOWMENT: u64 = 4;
/// One entry of that table, in bytes. Named rather than written as a literal
/// because two things are laid out from it and a literal would let them drift.
pub const ENDOWMENT_ENTRY_BYTES: u64 = 16 + MAX_BINDING;
/// Which request the child's authority over **itself** answers (ADR-0061).
///
/// The rights travel in a register, because they are a value (ADR-0058); the
/// name cannot, so it is here. It is a slot of its own rather than an endowment
/// entry for the reason `Endowment::Own` is a variant of its own: an endowment
/// entry names a capability the parent holds, and this one names a process that
/// does not exist until the instant it is granted.
pub const CREATE_SELF_BINDING: u64 = ENDOWMENT_ENTRY_BYTES * MAX_ENDOWMENT;
pub const CREATE_MODULE: u64 = CREATE_SELF_BINDING + MAX_BINDING;
/// The longest module path `process_create` will read. A bound of this contract
/// rather than of the region: the nucleus must not size a read from a number a
/// caller chose, even where the region would have held more.
pub const MAX_MODULE_PATH: u64 = 256;

/// One entry of the endowment a parent gives a child.
///
/// The parent names a capability **it holds**, the rights it wants the child to
/// have, and **which of the child's capability requests this answers**
/// (ADR-0061). What the child gets is the intersection of the rights: a parent
/// cannot give what it does not hold, so widening is not refused so much as
/// unexpressible.
///
/// The binding is the parent's statement about the *child's* source, not about
/// its own. A parent granting authority to a name the child never requested has
/// granted something the child cannot use, which is a policy mistake the child
/// reports rather than a refusal the nucleus makes: the nucleus does not read
/// the child's module.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CreateEndowment {
    pub handle: u64,
    pub rights: u32,
    pub binding_length: u32,
    pub binding: [u8; MAX_BINDING as usize],
}

/// The header a runtime image carries in its first bytes.
///
/// A raw image has no symbol table, so this is how it tells a nucleus which of
/// its bytes are text — mapped read-only and executable — which are data, and
/// how much memory it needs beyond what the file carries. The linker emits it
/// from the section boundaries themselves, because the only thing that knows
/// where a section ended is the thing that placed it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ImageHeader {
    pub magic: u64,
    /// Offset of the entry point from the image's base.
    pub entry: u64,
    /// Bytes of read-only, executable text, from the base. Page-aligned.
    pub text: u64,
    /// Bytes the file carries. Page-aligned.
    pub file: u64,
    /// Bytes the image needs mapped, `.bss` included. Page-aligned.
    pub memory: u64,
}

/// `"SOTIMG1\0"`, little-endian: the image format's version is in its magic,
/// so a nucleus that reads an image of another version refuses it by the same
/// comparison that finds one that is not an image at all.
pub const IMAGE_MAGIC: u64 = 0x53_4f_54_49_4d_47_31_00;
