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
//! **Nothing here is a capability.** The record carries memory the process was
//! given and text it was told to run. Authority is a handle in a table the
//! nucleus owns (`CAPABILITY_V1`), and none of it travels in a struct.

#![no_std]

/// The version of this record. A nucleus and an image that disagree do not run
/// together.
pub const LAUNCH_VERSION: u32 = 1;

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
