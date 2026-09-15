// SPDX-License-Identifier: GPL-3.0-or-later
//! The hard limits the verifier checks before anything expensive.
//!
//! docs/44 section 2 requires an implementation to publish exact numeric limits
//! before it accepts untrusted IR, and to check gross counts before graph
//! traversal or type work. These are the accepted V1 ceilings; a lower cap is
//! allowed if it is reported in a declared conformance profile, and raising one
//! is a versioned contract change.

/// Published ceilings, checked in verification step 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Entries in any one table of the module.
    pub table_entries: usize,
    /// Modules in the dependency closure (docs/44 section 2).
    pub modules: usize,
    /// Unique `(caller, resolved dependency)` relationships in one resolved
    /// closure (`docs/44` §2, ADR-0090).
    ///
    /// **A topology quota over the whole closure**, not a per-module count and
    /// not `docs/41` §6's `resource imports`, which bounds transitive module
    /// dependencies. It exists because an imported call is checked against the
    /// dependency artifact this launch verified, and the number of times that
    /// authenticated reopen happens is exactly the number of these edges: a
    /// closure within every other accepted ceiling could otherwise demand tens
    /// of thousands of them.
    pub closure_edges: usize,
    /// Fields or variants a nominal type may declare.
    pub fields: usize,
    /// Parameters a function may declare.
    pub parameters: usize,
    /// Basic blocks in one function.
    pub blocks_per_function: usize,
    /// Instructions in one basic block.
    pub instructions_per_block: usize,
    /// Source-map entries in a module.
    pub source_map_entries: usize,
}

impl Default for Limits {
    /// The accepted V1 ceiling.
    fn default() -> Limits {
        Limits {
            table_entries: 65_536,
            modules: 256,
            closure_edges: 1024,
            fields: 1024,
            parameters: 128,
            blocks_per_function: 4096,
            instructions_per_block: 65_536,
            source_map_entries: 262_144,
        }
    }
}
