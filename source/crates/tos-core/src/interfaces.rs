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
    /// The capabilities it requires, in order, before any value
    /// (`SYSTEM_INTERFACE_V1` §4.1). The first is the operation's own
    /// interface — the one the instruction records and `Signature.effects`
    /// names — and every one after it is a separate authority with its own
    /// interface and right.
    pub capabilities: &'static [Requirement],
    /// The parameters after the capabilities, in order.
    pub parameters: &'static [Parameter],
    /// The result type.
    pub result: &'static str,
}

/// One capability an operation requires.
///
/// **The right is half of it**, and the half this schema did not state until an
/// operation needed two capabilities. `docs/42` §2 requires "the capability type,
/// requested operation/right, resource range, and the enclosing `uses` effect"
/// all to match a declared interface contract; a requirement naming only the
/// type would leave "which of this endpoint's three rights does this operation
/// need" to the nucleus alone, where a reader and a verifier cannot see it.
pub struct Requirement {
    /// The interface a capability supplied here must be of.
    pub interface: &'static str,
    /// The right it must carry, by the name its object type declares
    /// (`IPC_V1` §2 for an endpoint, `CAPABILITY_V1` §3 in general).
    pub right: &'static str,
}

impl Requirement {
    const fn of(interface: &'static str, right: &'static str) -> Requirement {
        Requirement { interface, right }
    }
}

/// One value an operation takes (`SYSTEM_INTERFACE_V1` §4.1).
pub struct Parameter {
    /// The type an `extern fn` must declare for it.
    pub ty: &'static str,
    /// How long a value of it may be, where its length is not fixed by its type.
    ///
    /// `None` for a type whose size the type decides. `Some` for one whose does
    /// not, and then it is **part of the contract** rather than the host's
    /// choice: `SYSTEM_ABI_V1` §3 bounds every read by a constant of the
    /// contract and not by a number a caller chose, so an unbounded parameter
    /// would leave how much of a module's value the system looks at to whichever
    /// host ran it.
    pub maximum: Option<u64>,
}

impl Parameter {
    /// A value whose type fixes its size.
    const fn fixed(ty: &'static str) -> Parameter {
        Parameter { ty, maximum: None }
    }
}

/// One interface: a capability type, and the finite set of operations reachable
/// through a capability of it.
pub struct Interface {
    /// The path a capability import declares, which is also the path the IR
    /// records in `Signature.effects`.
    pub path: &'static str,
    /// Which kind of object a capability of this interface names
    /// (`SYSTEM_INTERFACE_V1` §4, ADR-0061).
    ///
    /// A **check** on a grant, never the rule that chooses one: two imports of
    /// one interface are legal, so a kind cannot tell them apart, and what
    /// answers a request is the binding it was declared with.
    pub object: ObjectKind,
    pub operations: &'static [Operation],
}

/// The kinds `CAPABILITY_V1` §3 names.
///
/// Spelled out here rather than carried as the launch record's numbers: this
/// crate is the frontend, it runs on hosts that have no launch record, and a
/// number whose meaning lives in another crate's constants would be a coupling
/// that buys nothing. Whoever launches maps these to its own encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectKind {
    Endpoint,
    Region,
    Process,
    InterfacePublication,
    Reply,
}

/// Every interface `SYSTEM_INTERFACE_V1` §4 declares, and no others.
///
/// An `extern` item naming anything absent from this table is rejected exactly
/// as it was before any schema existed: the rejection did not become
/// conditional on a build flag, it became conditional on a *declaration*.
pub const ACCEPTED: &[Interface] = &[
    Interface {
        path: "system.ipc.Endpoint",
        object: ObjectKind::Endpoint,
        operations: &[
            Operation {
                name: "endpoint_send",
                capabilities: &[Requirement::of("system.ipc.Endpoint", "send")],
                parameters: &[Parameter::fixed("u64")],
                result: "i64",
            },
            Operation {
                name: "endpoint_receive",
                capabilities: &[Requirement::of("system.ipc.Endpoint", "receive")],
                parameters: &[],
                result: "i64",
            },
            Operation {
                name: "endpoint_call",
                capabilities: &[Requirement::of("system.ipc.Endpoint", "call")],
                parameters: &[Parameter::fixed("u64")],
                result: "i64",
            },
        ],
    },
    Interface {
        path: "system.ipc.Reply",
        object: ObjectKind::Reply,
        operations: &[
            Operation {
                name: "endpoint_reply",
                capabilities: &[Requirement::of("system.ipc.Reply", "reply")],
                parameters: &[Parameter::fixed("u64")],
                result: "i64",
            },
            // Two capabilities, and they stay two: the reply it consumes and the
            // endpoint it then waits on, each with its own interface and right,
            // neither derivable from the other (ADR-0063).
            Operation {
                name: "endpoint_reply_receive",
                capabilities: &[
                    Requirement::of("system.ipc.Reply", "reply"),
                    Requirement::of("system.ipc.Endpoint", "receive"),
                ],
                parameters: &[Parameter::fixed("u64")],
                result: "i64",
            },
        ],
    },
    Interface {
        path: "system.process.Control",
        object: ObjectKind::Process,
        operations: &[
            Operation {
                name: "process_terminate",
                capabilities: &[Requirement::of("system.process.Control", "terminate")],
                parameters: &[],
                result: "i64",
            },
            // The module name is a value of variable length, so it declares its
            // maximum here (§4.1). The number is `MAX_MODULE_PATH`'s, which is
            // the bound the nucleus already reads that argument under — stated
            // in the schema rather than borrowed from the ABI's constants,
            // because a module is refused against the contract it was written
            // to and not against a number in another crate.
            Operation {
                name: "process_create",
                capabilities: &[Requirement::of("system.process.Control", "create")],
                parameters: &[Parameter {
                    ty: "string",
                    maximum: Some(256),
                }],
                result: "i64",
            },
        ],
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
