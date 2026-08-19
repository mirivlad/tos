// SPDX-License-Identifier: GPL-3.0-or-later
//! The accepted interface schemas, as the frontend must know them.
//!
//! `SYSTEM_INTERFACE_V1` is a document; this is the same content in the form a
//! checker can compare against, and a gate holds the two together. It is not a
//! second source of truth: the document decides, and a disagreement between them
//! is a defect in this file rather than a new interface.
//!
//! **Nothing here is a capability and nothing here grants one.** An interface
//! declares which operations exist, what they take and what they return. Whether
//! a module may reach one is decided by whether it holds a capability of that
//! interface, which is decided by whoever launched it (ADR-0055) — three
//! separate questions, and this table answers only the first.

/// One operation an interface declares.
pub struct Operation {
    /// The name an `extern fn` must have to be this operation.
    pub name: &'static str,
    /// The parameter types after the capability, in order. The capability is
    /// the first parameter of every operation and is not repeated here.
    pub parameters: &'static [&'static str],
    /// The result type.
    pub result: &'static str,
}

/// One interface: a capability type, and the finite set of operations reachable
/// through a capability of it.
pub struct Interface {
    /// The path a capability import declares, which is also the path the IR
    /// records in `Signature.effects`.
    pub path: &'static str,
    pub operations: &'static [Operation],
}

/// Every interface `SYSTEM_INTERFACE_V1` §4 declares, and no others.
///
/// An `extern` item naming anything absent from this table is rejected exactly
/// as it was before any schema existed: the rejection did not become
/// conditional on a build flag, it became conditional on a *declaration*.
pub const ACCEPTED: &[Interface] = &[
    Interface {
        path: "system.ipc.Endpoint",
        operations: &[
            Operation {
                name: "endpoint_send",
                parameters: &["u64"],
                result: "i64",
            },
            Operation {
                name: "endpoint_receive",
                parameters: &[],
                result: "i64",
            },
            Operation {
                name: "endpoint_call",
                parameters: &["u64"],
                result: "i64",
            },
        ],
    },
    Interface {
        path: "system.ipc.Reply",
        operations: &[Operation {
            name: "endpoint_reply",
            parameters: &["u64"],
            result: "i64",
        }],
    },
    Interface {
        path: "system.process.Control",
        operations: &[Operation {
            name: "process_terminate",
            parameters: &[],
            result: "i64",
        }],
    },
];

/// The interface with this path, if an accepted schema declares one.
pub fn interface(path: &str) -> Option<&'static Interface> {
    ACCEPTED.iter().find(|interface| interface.path == path)
}

impl Interface {
    /// The operation of this interface with this name, if it declares one.
    pub fn operation(&self, name: &str) -> Option<&'static Operation> {
        self.operations
            .iter()
            .find(|operation| operation.name == name)
    }
}
