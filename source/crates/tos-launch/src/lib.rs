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
/// Version 2 adds the endowment (ADR-0055) and the message slot the inline IPC
/// payload crosses in. Version 1 carried memory and text and no authority at
/// all, which is the state in which no process could ever hold a capability.
pub const LAUNCH_VERSION: u32 = 2;

/// What kind of object a capability names (`CAPABILITY_V1` §3).
///
/// Zero is not an object kind and never will be, for the reason operation zero
/// is not an operation: a field nobody wrote holds zero, and giving that a
/// meaning turns an omission into a grant.
pub const OBJECT_ENDPOINT: u32 = 1;
pub const OBJECT_REGION: u32 = 2;
pub const OBJECT_PROCESS: u32 = 3;
pub const OBJECT_INTERFACE: u32 = 4;

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
}

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
    /// Where the inline payload of a message crosses the boundary.
    ///
    /// `SYSTEM_ABI_V1` §3 admits values and handles as arguments and no pointer
    /// the nucleus walks; six registers cannot carry `IPC_V1`'s 256 inline
    /// bytes. So the payload does not travel in the call at all: it sits in a
    /// region the launcher mapped and the nucleus knows the address of, exactly
    /// as the report region does, and the call names only how much of it is a
    /// message.
    pub message_base: u64,
    pub message_length: u64,
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
