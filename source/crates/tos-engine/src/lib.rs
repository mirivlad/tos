// SPDX-License-Identifier: GPL-3.0-or-later
//! The bounded Bootstrap reference interpreter (docs/43 section 7, docs/44
//! section 6 step 7).
//!
//! This is a semantic oracle, not an execution shim. It is the component that
//! says what a TOS Core program *means*, so every future engine — a bytecode
//! engine, a native backend — is measured against it rather than against the
//! host.
//!
//! Two rules shape it.
//!
//! **It executes verified IR only.** [`run`] takes a [`VerifiedModule`] receipt
//! and checks that the receipt names the digest of the module it was handed. A
//! receipt for a different module is not a receipt for this one, and a module
//! without one is not executable. There is no path that runs IR the verifier
//! has not seen.
//!
//! **Correctness never rests on the host.** Nothing here depends on Rust panics
//! or unwinding, host exceptions, an ambient filesystem or network, libc
//! semantics, or host threads. Integer arithmetic is checked against the
//! declared TOS type width rather than a Rust integer's, and every failure of a
//! dynamic precondition is a [`Trap`] with a stable code and the source span it
//! came from.
//!
//! Bootstrap may serialize parallel scopes (docs/43 section 7), and it does:
//! the lowered Bootstrap subset contains no unserialized concurrency, so the
//! result is one the language allows.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

// The test harness is a host program by construction, so it keeps `std`.
#[cfg(test)]
extern crate std;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tos_ir::{
    BinaryOp, CallTarget, Constant, Instruction, IntKind, Module, Op, Operand, Place, PlaceStep,
    SourceRef, Terminator, UnaryOp,
};
use tos_verifier::VerifiedModule;

/// A runtime value.
///
/// The representation is the language's, not the host's: an integer carries the
/// TOS type that bounds it, so a check is against `i32` because the program said
/// `i32`, never because the host chose a width.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Unit,
    Bool(bool),
    Int(IntKind, i128),
    Size(u128),
    Duration(u128),
    Text(String),
    Bytes(Vec<u8>),
    /// A record, tuple or array, in declared order.
    Aggregate(Vec<Value>),
    /// An enum, `Option`, `Result` or `TaskResult` value.
    Variant {
        index: usize,
        payload: Vec<Value>,
    },
    /// A closure: the body it runs and the values it captured, in order.
    Closure {
        body: usize,
        captures: Vec<Value>,
    },
    /// A scoped child task.
    ///
    /// Bootstrap serializes parallel scopes (docs/43 section 7). This engine
    /// serializes by deferring the child to its join rather than running it at
    /// the spawn, which keeps `cancel` meaningful: a child cancelled before it
    /// is joined never starts, and `Cancelled` is an outcome docs/41 section 2
    /// allows.
    Task {
        body: usize,
        captures: Vec<Value>,
        cancelled: bool,
    },
    /// Authority this run was given, as an opaque handle.
    ///
    /// The engine never reads the number. It cannot be compared, printed,
    /// converted or computed with, and no operation of the language produces
    /// one: a capability arrives as an argument of the run and leaves as an
    /// argument of an interface operation, and the only thing the engine does
    /// with it in between is carry it. `docs/42` §2 requires exactly that —
    /// authority appears in identity, imports and audit, while "the concrete
    /// secret/handle representation does not" — so this variant is deliberately
    /// a dead end everywhere except at the boundary it crosses.
    Capability(Handle),
}

/// Authority, as the engine holds it.
///
/// A newtype rather than a bare `u64` so that the rule has somewhere to live.
/// `Debug` is written out rather than derived because derived `Debug` on a
/// handle is how a handle reaches a log: `docs/42` §2 admits authority into
/// provenance by *interface path* and keeps the representation out, and a
/// diagnostic is not an exception to that.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Handle(u64);

impl Handle {
    /// The handle a host is giving this run.
    pub fn new(value: u64) -> Handle {
        Handle(value)
    }

    /// The handle, for the host that has to name it to the system.
    ///
    /// The only reader, and it is the boundary: inside the engine the number
    /// does not exist.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl core::fmt::Debug for Handle {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("capability")
    }
}

/// A defined failure of a dynamic language precondition (docs/41 section 7).
///
/// A trap is not an error value and cannot be caught as `Result`; it ends the
/// process. It carries a stable code and the source span the operation came
/// from, so a runtime failure names the text that caused it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trap {
    pub code: &'static str,
    pub detail: String,
    /// The source-map entry index of the operation that trapped, in the module
    /// that trapped.
    pub source: SourceRef,
    /// The resolved source-map entry, when the trap crossed a module boundary.
    ///
    /// An index is only meaningful in its own module's table. Once a trap
    /// leaves the module that raised it, the index alone would resolve against
    /// the wrong map and name a real location in the wrong file — which is
    /// worse than naming none. It is resolved where the module is still in
    /// hand, and the innermost module wins.
    ///
    /// Boxed because a trap travels inside every `Result` the engine returns,
    /// and a rare field carried by value would make the common path pay for it.
    pub site: Option<alloc::boxed::Box<tos_ir::SourceMapEntry>>,
}

impl Trap {
    /// A trap, from anywhere. Public because a [`System`] is a host in another
    /// crate and ending a run is one of the two things it may do.
    pub fn new(code: &'static str, detail: impl ToString, source: SourceRef) -> Trap {
        Trap {
            code,
            detail: detail.to_string(),
            source,
            site: None,
        }
    }
}

/// What one run consumed and produced.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub value: Value,
    pub fuel_used: u128,
    pub max_call_depth: u128,
    pub tasks_started: u128,
    /// The most of the declared allocation budget held at any one moment.
    pub allocation_peak: u128,
    /// The most cleanups registered and unrun at any one moment.
    pub cleanups_peak: u128,
    /// The most of the declared `shared` budget held at any one moment.
    pub shared_peak: u128,
    /// The most synchronization guards live at any one moment.
    pub sync_peak: u128,
}

/// Why a module could not be run at all, before any instruction executed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// The receipt names a different module than the one supplied.
    ReceiptDoesNotMatch,
    /// The named entry function is not exported by this module.
    NoSuchEntry(String),
    /// The entry function's arity does not match the arguments supplied.
    EntryArity { expected: usize, actual: usize },
    /// A capability the module requested was not granted (`docs/42` §2).
    ///
    /// Carries the binding, because that is what the module called it and what
    /// a reader has to look for in the source; and the interface, because that
    /// is what was wanted. `PROCESS_IDENTITY_V1` §7.3 asks a denial to be
    /// nameable, and this is the name.
    CapabilityDenied { binding: String, interface: String },
}

/// One capability request of a module, put to the host that answers it.
#[derive(Clone, Copy, Debug)]
pub struct Request<'a> {
    /// The accepted interface the module asked for.
    pub interface: &'a str,
    /// The name it bound the request to, which is the identity of the request
    /// (ADR-0061). Unique inside a module, and covered by the module digest, so
    /// a host may hold policy against it.
    pub binding: &'a str,
    /// Where in the module's own import list it is. Offered because it is free
    /// and occasionally useful for reporting; it is **not** the identity, and a
    /// host that matched on it would be matching on something a source reorder
    /// changes.
    pub position: usize,
}

/// One call leaving the engine for an accepted interface schema.
///
/// Everything about it is a string and a slice of values the engine already
/// held: it names *which* interface and *which* operation, and it does not
/// describe how either is performed. That is the whole of the separation — the
/// engine knows the call happened and in what order, and knows nothing about
/// what answering it involves.
#[derive(Clone, Copy, Debug)]
pub struct Reach<'a> {
    /// The accepted interface path the instruction carries, which the verifier
    /// has already proved the module imported and the enclosing function
    /// declared (`docs/42` §2, ADR-0060).
    pub interface: &'a str,
    /// The operation of that interface.
    pub operation: &'a str,
    /// The capability first (ADR-0056), then the operation's values.
    pub arguments: &'a [Value],
    /// Where in the source the call is, so that a refusal names the text that
    /// made it rather than the host that answered it.
    pub source: SourceRef,
}

/// What an engine reaches when a module calls an operation of an interface.
///
/// **The engine leaves here and is re-entered here**, which is the property
/// ADR-0060 §3 requires of it: an operation may block, a blocked operation makes
/// its *process* not runnable, and the engine's own frame is simply not running
/// for as long as that lasts. Nothing in the engine is unwound, retried or
/// resumed from a saved position — the call boundary is the boundary, and what
/// carries the suspended run across it is whatever the host suspends. In the
/// TOS runtime image that is the process's own trap frame, which the nucleus
/// sets down and picks up (ADR-0059); in a host test it is an ordinary return.
///
/// **Determinism is split here, deliberately.** ADR-0060 fixes it: the order of
/// effects is deterministic and the verifier proves it, and the values effects
/// return are not. The engine's side of that is that it makes the same calls in
/// the same order over the same inputs, which is a property of `docs/40`'s
/// evaluation order and is unchanged by this trait existing. Everything a `reach`
/// returns is outside it.
pub trait System {
    /// Which capability answers one of the module's requests.
    ///
    /// Asked once per `import capability`, **before the first instruction
    /// runs** — which is what makes an unanswered request a startup failure
    /// rather than a surprise at a call site (`docs/42` §2,
    /// `SYSTEM_INTERFACE_V1` §10.3). The engine supplies what the artifact
    /// declares and nothing else: the interface path and the name the module
    /// bound it to (ADR-0061). Which grant that is, and whether there is one,
    /// is entirely the host's.
    ///
    /// `None` denies the request. It is not an empty authority and not a zero
    /// handle: `docs/42` §2 forbids a denial being fabricated as either.
    fn granted(&mut self, request: Request<'_>) -> Option<Handle>;

    /// Performs one operation, or ends the run.
    ///
    /// A refused operation is an ordinary value — every operation of
    /// `SYSTEM_INTERFACE_V1` returns the status `SYSTEM_ABI_V1` assigns, and a
    /// module reads a number. A [`Trap`] is for the other thing: a call this
    /// host cannot perform at all, which is not an answer the program could
    /// have handled.
    fn reach(&mut self, call: Reach<'_>) -> Result<Value, Trap>;
}

/// A system with nothing on the other side of it.
///
/// For a run of a module that reaches no interface — every Bootstrap module, by
/// `docs/42` §3 — and for a caller that has no business answering one. It is not
/// a stub that returns zero: a module that reaches an interface through this has
/// reached a system that does not exist, and saying so is the only honest
/// answer available.
pub struct Unreachable;

impl System for Unreachable {
    fn granted(&mut self, _request: Request<'_>) -> Option<Handle> {
        None
    }

    fn reach(&mut self, call: Reach<'_>) -> Result<Value, Trap> {
        Err(Trap::new(
            "RUNTIME_INTERFACE_UNREACHABLE",
            alloc::format!(
                "`{}` of `{}` was called on a run with no system to reach",
                call.operation,
                call.interface
            ),
            call.source,
        ))
    }
}

/// Runs an entry function of a verified module.
///
/// The receipt is checked against the module's own digest first: an engine
/// accepts executable IR only with a receipt for that exact module (docs/43
/// section 5).
/// One module of a set, with the receipt the verifier issued for it.
///
/// Paired rather than parallel, because the pairing is the property: a module
/// runs because its **own** receipt matches it, and no module is ever admitted
/// on the strength of the one that calls it.
#[derive(Clone, Copy, Debug)]
pub struct Verified<'a> {
    pub module: &'a Module,
    pub receipt: &'a VerifiedModule,
}

/// Runs an entry function of one verified module.
pub fn run(
    module: &Module,
    receipt: &VerifiedModule,
    entry: &str,
    arguments: Vec<Value>,
    system: &mut dyn System,
) -> Result<Result<Outcome, Trap>, Refusal> {
    run_set(&[Verified { module, receipt }], 0, entry, arguments, system)
}

/// Runs an entry function of one module of a verified set.
///
/// A cross-module call is resolved against this set and nothing else: the
/// engine never loads, searches for or fabricates a module, so a call that
/// names something the set does not contain refuses rather than finding
/// something else.
///
/// **The run is governed by the entry module's declared envelope.** docs/41
/// section 6 fixes that: a call is permitted only when the callee's declared
/// worst-case contract fits the caller's envelope, so one run has one budget,
/// and a callee's own declaration is a statement about the callee rather than a
/// second budget that resets when the boundary is crossed.
///
/// **The system is an argument, not a default.** A run reaches whatever the
/// caller hands it and nothing else, so "which interfaces could this run have
/// used" is answered by the call site rather than by an ambient environment the
/// engine went looking for. A caller with nothing to offer says so by handing
/// over [`Unreachable`].
pub fn run_set(
    set: &[Verified<'_>],
    entry_module: usize,
    entry: &str,
    arguments: Vec<Value>,
    system: &mut dyn System,
) -> Result<Result<Outcome, Trap>, Refusal> {
    // Every module of the set, before any of them runs: a receipt checked only
    // when a call reaches it would let a program choose which modules get
    // checked by choosing which branch it takes.
    for verified in set {
        if verified.receipt.module_digest != tos_ir::module_digest(verified.module) {
            return Err(Refusal::ReceiptDoesNotMatch);
        }
    }
    let Some(entry_verified) = set.get(entry_module) else {
        return Err(Refusal::NoSuchEntry(entry.to_string()));
    };
    let module = entry_verified.module;
    let receipt = entry_verified.receipt;
    let _ = receipt;
    let Some(index) = module
        .functions
        .iter()
        .position(|function| function.signature.name == entry)
    else {
        return Err(Refusal::NoSuchEntry(entry.to_string()));
    };
    let expected = module.functions[index].signature.parameters.len();
    if expected != arguments.len() {
        return Err(Refusal::EntryArity {
            expected,
            actual: arguments.len(),
        });
    }

    // Every request answered before the first instruction, or none of them run.
    // A module that got as far as a call before discovering it holds nothing
    // would have already done work under an assumption that was false.
    let mut imports = Vec::with_capacity(module.capability_imports.len());
    for (position, request) in module.capability_imports.iter().enumerate() {
        let held = system.granted(Request {
            interface: &request.interface,
            binding: &request.binding,
            position,
        });
        match held {
            Some(handle) => imports.push(Value::Capability(handle)),
            None => {
                return Err(Refusal::CapabilityDenied {
                    binding: request.binding.clone(),
                    interface: request.interface.clone(),
                })
            }
        }
    }

    let envelope = &module.header.resource_envelope;
    let mut engine = Engine {
        module,
        set,
        system,
        imports,
        fuel_limit: envelope.fuel,
        recursion_limit: envelope.recursion.max(1),
        fuel_used: 0,
        depth: 0,
        max_depth: 0,
        task_limit: envelope.tasks,
        tasks_started: 0,
        allocation_limit: envelope.allocation,
        allocation_held: 0,
        allocation_peak: 0,
        shared_limit: envelope.shared,
        shared_held: 0,
        shared_peak: 0,
        sync_limit: envelope.sync,
        sync_held: 0,
        sync_peak: 0,
        cleanup_limit: envelope.cleanup,
        cleanups_live: 0,
        cleanups_peak: 0,
        worker_limit: envelope.workers,
        workers_held: 0,
        frame_allocation: 0,
        frame_cleanups: 0,
        frame_sync: 0,
    };
    // docs/41 section 6: a reservation is checked before the thing it pays for
    // happens. A module that declares no worker cannot run one instruction.
    if let Err(trap) = engine.reserve_worker() {
        return Ok(Err(trap));
    }
    let outcome = engine.call(index, arguments);
    engine.release_worker();
    Ok(outcome.map(|value| Outcome {
        value,
        fuel_used: engine.fuel_used,
        max_call_depth: engine.max_depth,
        tasks_started: engine.tasks_started,
        allocation_peak: engine.allocation_peak,
        cleanups_peak: engine.cleanups_peak,
        shared_peak: engine.shared_peak,
        sync_peak: engine.sync_peak,
    }))
}

struct Engine<'module, 'system> {
    /// The module whose function is executing. It changes for the duration of a
    /// cross-module call and is restored when that call returns.
    module: &'module Module,
    /// Every module this run may reach, each with its own receipt.
    set: &'module [Verified<'module>],
    /// What this run reaches when it leaves. Its own lifetime, because a host
    /// has no reason to outlive the modules it is running.
    system: &'system mut dyn System,
    /// What each of the entry module's capability requests was answered with,
    /// in the order the module declares them. Fixed before the run starts and
    /// never written again: a run's authority is what it was given, and there is
    /// no instruction that adds to this.
    imports: Vec<Value>,
    fuel_limit: u128,
    recursion_limit: u128,
    fuel_used: u128,
    depth: u128,
    max_depth: u128,
    task_limit: u128,
    tasks_started: u128,
    /// Live synchronization guards, and the most ever live at once.
    sync_limit: u128,
    sync_held: u128,
    sync_peak: u128,
    /// Bytes of the declared `shared` budget currently held, and the most ever
    /// held at once. ADR-0037 makes a `Shared<T>` count against it, so sharing
    /// is bounded by the envelope like every other resource.
    shared_limit: u128,
    shared_held: u128,
    shared_peak: u128,
    /// Bytes of the declared allocation budget currently held.
    allocation_limit: u128,
    allocation_held: u128,
    allocation_peak: u128,
    /// Cleanups registered and not yet left behind.
    cleanup_limit: u128,
    cleanups_live: u128,
    cleanups_peak: u128,
    /// Execution contexts reserved. Bootstrap serializes, so exactly one.
    worker_limit: u128,
    workers_held: u128,
    /// What the frame currently running has charged, released when it returns.
    frame_allocation: u128,
    /// Guards this frame took, released when it returns.
    frame_sync: u128,
    frame_cleanups: u128,
}

/// The accounted size of one runtime value.
///
/// docs/41 section 6 makes `allocation` a declared limit in bytes, so the
/// engine has to say what a value costs. The cost is a property of the value's
/// shape rather than of the host's representation of it: the same program on
/// the same input accounts the same bytes on any engine that adopts this rule.
const CELL_BYTES: u128 = 16;

/// How a block finished.
enum Exit {
    Return(Value),
    Goto(usize),
}

impl Engine<'_, '_> {
    /// Charges one unit of fuel, trapping when the declared budget is gone.
    ///
    /// docs/41 section 6 makes fuel the accounting that bounds work. The budget
    /// is the module's declared limit, so exhaustion is deterministic: the same
    /// program on the same input runs out at the same operation.
    fn spend(&mut self, source: SourceRef) -> Result<(), Trap> {
        self.fuel_used += 1;
        if self.fuel_used > self.fuel_limit {
            return Err(Trap::new(
                "RUNTIME_FUEL_EXHAUSTED",
                alloc::format!("the declared budget of {} is spent", self.fuel_limit),
                source,
            ));
        }
        Ok(())
    }

    /// Reserves one execution context before any instruction runs.
    fn reserve_worker(&mut self) -> Result<(), Trap> {
        if self.workers_held + 1 > self.worker_limit {
            return Err(Trap::new(
                "RUNTIME_WORKER_LIMIT",
                alloc::format!(
                    "the declared worker budget of {} admits no execution context",
                    self.worker_limit
                ),
                0,
            ));
        }
        self.workers_held += 1;
        Ok(())
    }

    fn release_worker(&mut self) {
        self.workers_held = self.workers_held.saturating_sub(1);
    }

    /// Charges the declared allocation budget for a value about to be built.
    ///
    /// docs/41 section 6 requires the reservation to be checked *before* the
    /// effect: the charge is made first, and the value is only constructed if it
    /// fits. A module that would exceed its budget never builds the value at
    /// all, so the trap is a refusal rather than a report after the fact.
    fn reserve_allocation(&mut self, cells: usize, source: SourceRef) -> Result<u128, Trap> {
        let bytes = CELL_BYTES * (cells as u128 + 1);
        if self.allocation_held + bytes > self.allocation_limit {
            return Err(Trap::new(
                "RUNTIME_ALLOCATION_LIMIT",
                alloc::format!(
                    "{bytes} more bytes exceeds the declared budget of {}, of which {} is held",
                    self.allocation_limit,
                    self.allocation_held
                ),
                source,
            ));
        }
        self.allocation_held += bytes;
        self.allocation_peak = self.allocation_peak.max(self.allocation_held);
        Ok(bytes)
    }

    /// Reserves one live synchronization guard.
    fn reserve_sync(&mut self, source: SourceRef) -> Result<(), Trap> {
        if self.sync_held + 1 > self.sync_limit {
            return Err(Trap::new(
                "RUNTIME_SYNC_LIMIT",
                alloc::format!(
                    "one more live guard exceeds the declared limit of {}",
                    self.sync_limit
                ),
                source,
            ));
        }
        self.sync_held += 1;
        self.sync_peak = self.sync_peak.max(self.sync_held);
        Ok(())
    }

    /// Charges the declared `shared` budget for a value about to be shared.
    ///
    /// ADR-0037 section 4 makes the `Shared<T>` a `share` produces count against
    /// the module's declared `shared` limit. The reservation is checked before
    /// the effect, like every other resource: a module that would exceed its
    /// budget never produces the handle. The cost uses the same declared cell
    /// model as `allocation`, because the engine has no other measure of a
    /// value's size and inventing a second one would make two budgets
    /// incomparable.
    fn reserve_shared(&mut self, cells: usize, source: SourceRef) -> Result<(), Trap> {
        let bytes = CELL_BYTES * (cells as u128 + 1);
        if self.shared_held + bytes > self.shared_limit {
            return Err(Trap::new(
                "RUNTIME_SHARED_LIMIT",
                alloc::format!(
                    "{bytes} more shared bytes exceeds the declared budget of {}, of which {} is held",
                    self.shared_limit,
                    self.shared_held
                ),
                source,
            ));
        }
        self.shared_held += bytes;
        self.shared_peak = self.shared_peak.max(self.shared_held);
        Ok(())
    }

    /// Releases what a frame charged, when the frame's values go out of scope.
    fn release_allocation(&mut self, bytes: u128) {
        self.allocation_held = self.allocation_held.saturating_sub(bytes);
    }

    /// Reserves one live cleanup registration.
    fn reserve_cleanup(&mut self, source: SourceRef) -> Result<(), Trap> {
        if self.cleanups_live + 1 > self.cleanup_limit {
            return Err(Trap::new(
                "RUNTIME_CLEANUP_LIMIT",
                alloc::format!(
                    "the declared cleanup budget of {} is already fully registered",
                    self.cleanup_limit
                ),
                source,
            ));
        }
        self.cleanups_live += 1;
        self.cleanups_peak = self.cleanups_peak.max(self.cleanups_live);
        Ok(())
    }

    fn release_cleanups(&mut self, count: u128) {
        self.cleanups_live = self.cleanups_live.saturating_sub(count);
    }

    /// Calls a function and writes back what it changed through a borrow.
    ///
    /// A `borrow mut` parameter names the caller's place, and the borrow rules
    /// guarantee no other alias is live for the duration of the call, so
    /// copying in and copying out is observationally the same as a reference
    /// and needs no aliasing machinery to be correct.
    fn call_with_writeback(
        &mut self,
        index: usize,
        arguments: Vec<Value>,
        operands: &[Operand],
        values: &mut [Option<Value>],
        source: SourceRef,
    ) -> Result<Value, Trap> {
        let modes: Vec<tos_ir::PassMode> = self.module.functions[index]
            .signature
            .parameters
            .iter()
            .map(|parameter| parameter.mode)
            .collect();
        let (result, finals) = self.call_capturing(index, arguments)?;
        for (position, mode) in modes.iter().enumerate() {
            if *mode != tos_ir::PassMode::MutableBorrow {
                continue;
            }
            let (Some(Operand::Value(slot)), Some(Some(value))) =
                (operands.get(position), finals.get(position))
            else {
                continue;
            };
            if let Some(target) = values.get_mut(*slot) {
                *target = Some(value.clone());
            }
        }
        let _ = source;
        Ok(result)
    }

    fn call(&mut self, index: usize, arguments: Vec<Value>) -> Result<Value, Trap> {
        Ok(self.call_capturing(index, arguments)?.0)
    }

    /// Calls a function of another module of the set.
    ///
    /// The callee executes with its own module in view and the run's single
    /// budget: fuel, depth and allocation are the entry's, because docs/41
    /// section 6 admits a call only when the callee's declared contract already
    /// fits the caller's envelope. Crossing a module boundary is therefore not
    /// a way to obtain a second budget.
    fn call_imported(
        &mut self,
        import: usize,
        name: &str,
        arguments: Vec<Value>,
        source: SourceRef,
    ) -> Result<Value, Trap> {
        let Some(declared) = self.module.imports.get(import) else {
            return Err(Trap::new(
                "RUNTIME_UNRESOLVED_IMPORT",
                "a call names an import the module does not declare",
                source,
            ));
        };
        let Some(callee) = self
            .set
            .iter()
            .find(|verified| verified.module.header.module_name == declared.module_name)
        else {
            return Err(Trap::new(
                "RUNTIME_UNRESOLVED_IMPORT",
                alloc::format!("{} is not in this run's module set", declared.module_name),
                source,
            ));
        };
        // The identity the caller was lowered against, not merely the name: a
        // set holding a different revision of the module under the same name is
        // not the module this caller was checked against.
        if !declared.module_content_id.is_empty()
            && declared.module_content_id != callee.module.header.content_id
        {
            return Err(Trap::new(
                "RUNTIME_UNRESOLVED_IMPORT",
                alloc::format!(
                    "{} in this set is {}, and the caller was lowered against {}",
                    declared.module_name,
                    callee.module.header.content_id,
                    declared.module_content_id
                ),
                source,
            ));
        }
        let Some(index) = callee.module.functions.iter().position(|function| {
            function.signature.name == name
                && function.signature.visibility == tos_ir::Visibility::Public
        }) else {
            return Err(Trap::new(
                "RUNTIME_UNRESOLVED_IMPORT",
                alloc::format!("{} exports no {name}", declared.module_name),
                source,
            ));
        };
        if callee.module.functions[index].signature.parameters.len() != arguments.len() {
            return Err(Trap::new(
                "RUNTIME_UNRESOLVED_IMPORT",
                alloc::format!(
                    "{}.{name} takes a different number of arguments",
                    declared.module_name
                ),
                source,
            ));
        }

        let caller = core::mem::replace(&mut self.module, callee.module);
        let outcome = self.call(index, arguments);
        self.module = caller;
        outcome.map_err(|mut trap| {
            if trap.site.is_none() {
                trap.site = callee
                    .module
                    .source_map
                    .get(trap.source)
                    .cloned()
                    .map(alloc::boxed::Box::new);
            }
            trap
        })
    }

    /// Calls a function and returns its result with the final state of its
    /// parameter slots, which is what a borrow writes back.
    fn call_capturing(
        &mut self,
        index: usize,
        arguments: Vec<Value>,
    ) -> Result<(Value, Vec<Option<Value>>), Trap> {
        let function = &self.module.functions[index];
        self.depth += 1;
        self.max_depth = self.max_depth.max(self.depth);
        if self.depth > self.recursion_limit {
            let source = function.source;
            self.depth -= 1;
            return Err(Trap::new(
                "RUNTIME_RECURSION_LIMIT",
                alloc::format!("the declared depth of {} is exceeded", self.recursion_limit),
                source,
            ));
        }

        // A frame's charges are its own: what it allocated and registered is
        // released when it returns, so a bounded program stays bounded however
        // many times it calls.
        let outer_allocation = core::mem::take(&mut self.frame_allocation);
        let outer_cleanups = core::mem::take(&mut self.frame_cleanups);
        let outer_sync = core::mem::take(&mut self.frame_sync);

        let mut values: Vec<Option<Value>> = alloc::vec![None; function.values.len()];
        for (slot, argument) in arguments.into_iter().enumerate() {
            if slot < values.len() {
                values[slot] = Some(argument);
            }
        }

        let mut block = 0usize;
        let mut steps = 0u128;
        let outcome = loop {
            steps += 1;
            if steps > self.fuel_limit.saturating_add(1) {
                break Err(Trap::new(
                    "RUNTIME_FUEL_EXHAUSTED",
                    "control did not leave the function within its budget",
                    function.blocks[block].source,
                ));
            }
            match self.run_block(index, block, &mut values) {
                Ok(Exit::Return(value)) => break Ok(value),
                Ok(Exit::Goto(next)) => block = next,
                Err(trap) => break Err(trap),
            }
        };
        self.depth -= 1;
        let charged = core::mem::replace(&mut self.frame_allocation, outer_allocation);
        self.release_allocation(charged);
        let registered = core::mem::replace(&mut self.frame_cleanups, outer_cleanups);
        self.release_cleanups(registered);
        // A guard cannot outlive the frame that took it (ADR-0036), so the
        // frame's guards are released with the frame.
        let guards = core::mem::replace(&mut self.frame_sync, outer_sync);
        self.sync_held = self.sync_held.saturating_sub(guards);
        outcome.map(|value| (value, values))
    }

    fn run_block(
        &mut self,
        function_index: usize,
        block_index: usize,
        values: &mut [Option<Value>],
    ) -> Result<Exit, Trap> {
        // The module outlives the engine, so a reference into it is not a
        // borrow of `self` and does not conflict with the `&mut self` a nested
        // call needs. Copying the reference out is what makes that visible to
        // the borrow checker.
        //
        // This used to clone each instruction and the terminator. The clone was
        // never needed — it was a way around a borrow that does not exist — and
        // it copied an `Instruction` for every instruction executed, which is
        // the single hottest thing an interpreter does.
        let module = self.module;
        let block = &module.functions[function_index].blocks[block_index];
        for instruction in &block.instructions {
            self.spend(instruction.source)?;
            let produced = self.evaluate(instruction, values)?;
            if let (Some(slot), Some(value)) = (instruction.result, produced) {
                if slot < values.len() {
                    values[slot] = Some(value);
                }
            }
        }
        self.terminate(&block.terminator, values, block.source)
    }

    fn terminate(
        &mut self,
        terminator: &Terminator,
        values: &mut [Option<Value>],
        source: SourceRef,
    ) -> Result<Exit, Trap> {
        match terminator {
            Terminator::Return(operand) => {
                // Every executed operation costs, terminators included: fuel is
                // the accounting for work done, and leaving a scope is work.
                self.spend(source)?;
                let value = match operand {
                    Some(operand) => self.operand(operand, values, source)?,
                    None => Value::Unit,
                };
                Ok(Exit::Return(value))
            }
            Terminator::Branch { target, .. } => {
                // A back edge is where a loop consumes fuel (docs/41 section 6).
                self.spend(source)?;
                Ok(Exit::Goto(*target))
            }
            Terminator::BranchIf {
                condition,
                true_target,
                false_target,
                ..
            } => {
                self.spend(source)?;
                let Value::Bool(taken) = self.operand(condition, values, source)? else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "a branch condition is not a bool",
                        source,
                    ));
                };
                Ok(Exit::Goto(if taken { *true_target } else { *false_target }))
            }
            Terminator::MatchEnum { subject, arms } => {
                self.spend(source)?;
                let value = self.operand(subject, values, source)?;
                let Value::Variant { index, .. } = value else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "a match subject is not a variant",
                        source,
                    ));
                };
                let Some((_, target)) = arms.iter().find(|(variant, _)| *variant == index) else {
                    // The verifier proved the map complete, so this is a
                    // representation defect rather than a program outcome.
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        alloc::format!("no arm covers variant {index}"),
                        source,
                    ));
                };
                Ok(Exit::Goto(*target))
            }
            Terminator::PropagateError { result, ok_target } => {
                self.spend(source)?;
                let value = self.operand(result, values, source)?;
                let Value::Variant { index, payload } = &value else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "`?` applied to a value that is not a Result",
                        source,
                    ));
                };
                // docs/40 section 4: `?` propagates the matching `Err` from the
                // nearest enclosing return scope, which is this function.
                if *index == 1 {
                    return Ok(Exit::Return(Value::Variant {
                        index: 1,
                        payload: payload.clone(),
                    }));
                }
                Ok(Exit::Goto(*ok_target))
            }
            Terminator::Trap(code) => Err(Trap::new(
                "RUNTIME_TRAP",
                alloc::format!("the program reached {code}"),
                source,
            )),
        }
    }

    fn evaluate(
        &mut self,
        instruction: &Instruction,
        values: &mut [Option<Value>],
    ) -> Result<Option<Value>, Trap> {
        let op = &instruction.op;
        let source = instruction.source;
        let produced = match op {
            Op::Const(constant) => Some(self.constant(*constant, source)?),
            Op::Aggregate { operands, .. } => {
                // Reserve before building: docs/41 section 6 checks a
                // reservation before the thing it pays for happens, so a value
                // that would not fit is never constructed at all.
                let charged = self.reserve_allocation(operands.len(), source)?;
                self.frame_allocation += charged;
                let mut elements = Vec::new();
                for operand in operands {
                    elements.push(self.operand(operand, values, source)?);
                }
                Some(Value::Aggregate(elements))
            }
            Op::Variant {
                index, operands, ..
            } => {
                let charged = self.reserve_allocation(operands.len(), source)?;
                self.frame_allocation += charged;
                let mut payload = Vec::new();
                for operand in operands {
                    payload.push(self.operand(operand, values, source)?);
                }
                Some(Value::Variant {
                    index: *index,
                    payload,
                })
            }
            // A read copies the value at a place; a move takes it. Both observe
            // the same location, and the verifier already proved that a moved
            // place is not read again on the same path.
            Op::Read { place } | Op::Move { place } | Op::Borrow { place, .. } => {
                Some(self.read_place(place, values, source)?)
            }
            Op::Write { place, value } => {
                let value = self.operand(value, values, source)?;
                self.write_place(place, value, values, source)?;
                None
            }
            Op::Drop { .. } => None,
            Op::Binary { op, left, right } => {
                let left = self.operand(left, values, source)?;
                let right = self.operand(right, values, source)?;
                Some(binary(*op, left, right, source)?)
            }
            Op::Unary { op, operand } => {
                let operand = self.operand(operand, values, source)?;
                Some(unary(*op, operand, source)?)
            }
            Op::Widen { operand, to } => {
                let operand = self.operand(operand, values, source)?;
                let Value::Int(_, magnitude) = operand else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "a widening conversion applied to a non-integer",
                        source,
                    ));
                };
                Some(Value::Int(*to, magnitude))
            }
            Op::Call { target, operands } => {
                let mut arguments = Vec::new();
                for operand in operands {
                    arguments.push(self.operand(operand, values, source)?);
                }
                match target {
                    CallTarget::Local(index) => {
                        Some(self.call_with_writeback(*index, arguments, operands, values, source)?)
                    }
                    CallTarget::Imported { import, name } => {
                        Some(self.call_imported(*import, name, arguments, source)?)
                    }
                    // Two different things share this target, and the
                    // instruction says which. A predeclared name is a
                    // conversion the language performs itself; the same shape
                    // carrying an accepted interface path is an operation the
                    // language does not perform at all, and the difference is
                    // exactly the field the verifier checked.
                    CallTarget::Predeclared(name) => match &instruction.unsafe_interface {
                        None => Some(self.predeclared(name, arguments, source)?),
                        Some(interface) => Some(self.reach(interface, name, arguments, source)?),
                    },
                }
            }
            Op::Closure { body, captures } => {
                let mut held = Vec::new();
                for capture in captures {
                    held.push(self.operand(capture, values, source)?);
                }
                Some(Value::Closure {
                    body: *body,
                    captures: held,
                })
            }
            Op::CallValue { callee, operands } => {
                let callee = self.operand(callee, values, source)?;
                let Value::Closure { body, captures } = callee else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "a value call applied to something that is not a closure",
                        source,
                    ));
                };
                // The body declares its own parameters first, then its
                // captures, which is the order the lowerer built it in.
                let mut arguments = Vec::new();
                for operand in operands {
                    arguments.push(self.operand(operand, values, source)?);
                }
                arguments.extend(captures);
                Some(self.call(body, arguments)?)
            }
            Op::Spawn { body, captures } => {
                let mut held = Vec::new();
                for capture in captures {
                    held.push(self.operand(capture, values, source)?);
                }
                self.tasks_started += 1;
                if self.tasks_started > self.task_limit {
                    return Err(Trap::new(
                        "RUNTIME_TASK_LIMIT",
                        alloc::format!("the declared task budget of {} is spent", self.task_limit),
                        source,
                    ));
                }
                Some(Value::Task {
                    body: *body,
                    captures: held,
                    cancelled: false,
                })
            }
            Op::Join { task } | Op::Await { task } => {
                let handle = self.operand(task, values, source)?;
                let Value::Task {
                    body,
                    captures,
                    cancelled,
                } = handle
                else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "a join applied to something that is not a task",
                        source,
                    ));
                };
                // `Cancelled` and `Completed` are the two outcomes docs/41
                // section 2 defines, and joining consumes the handle either way.
                Some(if cancelled {
                    Value::Variant {
                        index: 1,
                        payload: Vec::new(),
                    }
                } else {
                    Value::Variant {
                        index: 0,
                        payload: alloc::vec![self.call(body, captures)?],
                    }
                })
            }
            // Bootstrap serializes (docs/43 section 7), so a lock cannot block
            // and there is no contention to model. What the engine can prove
            // here is the accounting: docs/41 section 6 makes `sync` the
            // maximum live synchronization objects and guards, and a guard may
            // not be returned, stored in an aggregate or cross a boundary
            // (ADR-0036), so its lifetime is inside the frame that took it.
            // Charging at acquisition and releasing when that frame returns is
            // therefore exact at frame granularity rather than an estimate.
            Op::Lock { object, .. } => {
                let value = self.operand(object, values, source)?;
                self.reserve_sync(source)?;
                self.frame_sync += 1;
                Some(value)
            }
            // `share` consumes its argument and produces the same value behind
            // a `Shared` handle. Bootstrap has one context, so the sharing is
            // observable in the accounting rather than in aliasing: what the
            // engine can prove here is that the module stayed inside the
            // `shared` budget it declared.
            Op::Share { operand } => {
                let value = self.operand(operand, values, source)?;
                self.reserve_shared(value_cells(&value), source)?;
                Some(value)
            }
            Op::Cancel { task } => {
                // docs/41 section 2: an idempotent cooperative request that
                // consumes no ownership. The parent still has to join.
                if let Operand::Value(slot) = task {
                    if let Some(Some(Value::Task { cancelled, .. })) = values.get_mut(*slot) {
                        *cancelled = true;
                    }
                }
                None
            }
            Op::RegisterCleanup { .. } => {
                // ADR-0035: registering reads, borrows and moves nothing. It is
                // what the `cleanup` limit counts, and docs/41 section 6 makes
                // that a live count, so it is charged here and released where
                // the cleanups run.
                self.reserve_cleanup(source)?;
                self.frame_cleanups += 1;
                None
            }
            Op::RunCleanups { calls } => {
                for call in calls {
                    let mut arguments = Vec::new();
                    for capture in &call.captures {
                        arguments.push(self.operand(capture, values, source)?);
                    }
                    // ADR-0035: a cleanup acts on the scope it runs in, so what
                    // it leaves is what the next one and the scope observe.
                    self.call_with_writeback(call.body, arguments, &call.captures, values, source)?;
                }
                None
            }
            // An operation of an accepted interface schema, performed on the
            // capability an import was bound to (ADR-0060, ADR-0061).
            //
            // The capability is not an operand and never was: the instruction
            // names *which import*, and what that import was bound to is a
            // property of this run rather than of the module. So the same
            // artifact, run twice under different grants, reaches different
            // objects without a byte of it changing — which is what makes a
            // module a description of what it needs rather than of what it was
            // given.
            Op::Capability {
                import,
                right,
                operands,
            } => {
                let Some(interface) = self
                    .module
                    .capability_imports
                    .get(*import)
                    .map(|import| import.interface.as_str())
                else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "an operation names a capability import this module does not declare",
                        source,
                    ));
                };
                // Every request was answered before the run started, so this
                // cannot be absent for a module of the entry's own set; it can
                // for a *cross-module* call, whose imports are that module's
                // and are not this run's. Refusing says so rather than reaching
                // for the wrong module's authority.
                let Some(held) = self.imports.get(*import).cloned() else {
                    return Err(Trap::new(
                        "RUNTIME_CAPABILITY_DENIED",
                        alloc::format!("{interface} was requested and not granted"),
                        source,
                    ));
                };
                // ADR-0056: the capability first, then the operation's values.
                let mut arguments = alloc::vec![held];
                for operand in operands {
                    arguments.push(self.operand(operand, values, source)?);
                }
                Some(self.reach(interface, right, arguments, source)?)
            }
            Op::Atomic { .. } | Op::Resource { .. } => {
                return Err(Trap::new(
                    "RUNTIME_OPERATION_NOT_IMPLEMENTED",
                    "this reference engine does not yet execute that operation family",
                    source,
                ))
            }
        };
        Ok(produced)
    }

    fn constant(&self, index: usize, source: SourceRef) -> Result<Value, Trap> {
        let Some(constant) = self.module.constants.get(index) else {
            return Err(Trap::new(
                "RUNTIME_TYPE_CONFUSION",
                "a constant index is outside the table",
                source,
            ));
        };
        Ok(match constant {
            Constant::Unit => Value::Unit,
            Constant::Bool(value) => Value::Bool(*value),
            Constant::Int(kind, value) => Value::Int(*kind, *value),
            Constant::Size(value) => Value::Size(*value),
            Constant::Duration(value) => Value::Duration(*value),
            Constant::Text(value) => Value::Text(value.clone()),
            Constant::Bytes(value) => Value::Bytes(value.clone()),
        })
    }

    fn operand(
        &self,
        operand: &Operand,
        values: &[Option<Value>],
        source: SourceRef,
    ) -> Result<Value, Trap> {
        match operand {
            Operand::Value(index) => match values.get(*index).and_then(|slot| slot.clone()) {
                Some(value) => Ok(value),
                None => Err(Trap::new(
                    "RUNTIME_UNINITIALIZED_VALUE",
                    alloc::format!("value {index} is read before it is defined"),
                    source,
                )),
            },
            Operand::Constant(index) => self.constant(*index, source),
        }
    }

    fn read_place(
        &self,
        place: &Place,
        values: &[Option<Value>],
        source: SourceRef,
    ) -> Result<Value, Trap> {
        let mut current = match values.get(place.root).and_then(|slot| slot.clone()) {
            Some(value) => value,
            None => {
                return Err(Trap::new(
                    "RUNTIME_UNINITIALIZED_VALUE",
                    alloc::format!("value {} is read before it is defined", place.root),
                    source,
                ))
            }
        };
        for step in &place.path {
            let step = self.resolve_step(step, values, source)?;
            current = step_into(current, &step, source)?;
        }
        Ok(current)
    }

    /// Replaces a computed index with the constant it evaluates to.
    ///
    /// A dynamic index names a value of type `size`; reading it here is what
    /// turns a place the checker analysed conservatively into the one location
    /// the program actually touches.
    fn resolve_step(
        &self,
        step: &PlaceStep,
        values: &[Option<Value>],
        source: SourceRef,
    ) -> Result<PlaceStep, Trap> {
        let PlaceStep::DynamicIndex(value) = step else {
            return Ok(step.clone());
        };
        let index = match values.get(*value).and_then(|slot| slot.clone()) {
            Some(Value::Size(index)) => index,
            Some(Value::Int(_, index)) if index >= 0 => index as u128,
            Some(_) => {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "an index is not a size",
                    source,
                ))
            }
            None => {
                return Err(Trap::new(
                    "RUNTIME_UNINITIALIZED_VALUE",
                    alloc::format!("index value {value} is read before it is defined"),
                    source,
                ))
            }
        };
        let Ok(index) = u64::try_from(index) else {
            return Err(Trap::new(
                "RUNTIME_INDEX_OUT_OF_RANGE",
                "an index does not fit an element position",
                source,
            ));
        };
        Ok(PlaceStep::Index(Some(index)))
    }

    fn write_place(
        &self,
        place: &Place,
        value: Value,
        values: &mut [Option<Value>],
        source: SourceRef,
    ) -> Result<(), Trap> {
        let path: Vec<PlaceStep> = place
            .path
            .iter()
            .map(|step| self.resolve_step(step, values, source))
            .collect::<Result<_, _>>()?;
        if path.is_empty() {
            if let Some(slot) = values.get_mut(place.root) {
                *slot = Some(value);
            }
            return Ok(());
        }
        let Some(Some(root)) = values.get_mut(place.root) else {
            return Err(Trap::new(
                "RUNTIME_UNINITIALIZED_VALUE",
                alloc::format!("value {} is written before it is defined", place.root),
                source,
            ));
        };
        write_into(root, &path, value, source)
    }

    /// The predeclared V1 operations (docs/39 section 2).
    /// Leaves for an operation of an accepted interface schema, and comes back.
    ///
    /// The fuel for it was charged before this ran, like every other
    /// instruction, which is `SYSTEM_INTERFACE_V1` §6's rule that a module
    /// cannot exceed its declared budget by leaving the process. Nothing else
    /// about the engine's accounting moves: no allocation is reserved, because
    /// no value of this run's is constructed here, and no call depth is taken,
    /// because nothing of this module is entered.
    ///
    /// **This is the whole of the boundary.** What comes back is a value the
    /// engine did not compute and does not check, and that is the half of
    /// ADR-0060's determinism rule that gives: the call happened here, in this
    /// order, provably; what it answered is the world's business.
    fn reach(
        &mut self,
        interface: &str,
        operation: &str,
        arguments: Vec<Value>,
        source: SourceRef,
    ) -> Result<Value, Trap> {
        self.system.reach(Reach {
            interface,
            operation,
            arguments: &arguments,
            source,
        })
    }

    fn predeclared(
        &self,
        name: &str,
        arguments: Vec<Value>,
        source: SourceRef,
    ) -> Result<Value, Trap> {
        if let Some(target) = name.strip_prefix("to_") {
            let Some(kind) = IntKind::parse(target) else {
                return Err(Trap::new(
                    "RUNTIME_OPERATION_NOT_IMPLEMENTED",
                    alloc::format!("unknown checked conversion {name}"),
                    source,
                ));
            };
            let Some(Value::Int(_, magnitude)) = arguments.first().cloned() else {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "a checked conversion applied to a non-integer",
                    source,
                ));
            };
            // docs/40 section 3: a checked narrowing is `Result<T,
            // ConversionError>`, never a silent truncation.
            return Ok(match fits(kind, magnitude) {
                true => Value::Variant {
                    index: 0,
                    payload: alloc::vec![Value::Int(kind, magnitude)],
                },
                false => Value::Variant {
                    index: 1,
                    payload: alloc::vec![Value::Unit],
                },
            });
        }
        let wrapping = match name {
            "wrapping_add" => Some(BinaryOp::Add),
            "wrapping_sub" => Some(BinaryOp::Subtract),
            "wrapping_mul" => Some(BinaryOp::Multiply),
            _ => None,
        };
        if let Some(op) = wrapping {
            let (Some(Value::Int(kind, left)), Some(Value::Int(_, right))) =
                (arguments.first().cloned(), arguments.get(1).cloned())
            else {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "wrapping arithmetic applied to a non-integer",
                    source,
                ));
            };
            let raw = match op {
                BinaryOp::Add => left.wrapping_add(right),
                BinaryOp::Subtract => left.wrapping_sub(right),
                _ => left.wrapping_mul(right),
            };
            return Ok(Value::Int(kind, wrap(kind, raw)));
        }
        Err(Trap::new(
            "RUNTIME_OPERATION_NOT_IMPLEMENTED",
            alloc::format!("{name} is not a predeclared V1 operation this engine runs"),
            source,
        ))
    }
}

fn step_into(value: Value, step: &PlaceStep, source: SourceRef) -> Result<Value, Trap> {
    match (value, step) {
        (Value::Aggregate(elements), PlaceStep::Field(index)) => elements
            .get(*index)
            .cloned()
            .ok_or_else(|| Trap::new("RUNTIME_TYPE_CONFUSION", "field out of range", source)),
        (Value::Variant { payload, .. }, PlaceStep::Field(index)) => payload
            .get(*index)
            .cloned()
            .ok_or_else(|| Trap::new("RUNTIME_TYPE_CONFUSION", "payload out of range", source)),
        (Value::Aggregate(elements), PlaceStep::Index(Some(index))) => elements
            .get(*index as usize)
            .cloned()
            .ok_or_else(|| Trap::new("RUNTIME_INDEX_OUT_OF_RANGE", "index out of range", source)),
        (Value::Bytes(bytes), PlaceStep::Index(Some(index))) => bytes
            .get(*index as usize)
            .map(|byte| Value::Int(IntKind::U8, *byte as i128))
            .ok_or_else(|| Trap::new("RUNTIME_INDEX_OUT_OF_RANGE", "index out of range", source)),
        (_, PlaceStep::Index(None) | PlaceStep::DynamicIndex(_)) => Err(Trap::new(
            "RUNTIME_TYPE_CONFUSION",
            "an index step reached execution without a value",
            source,
        )),
        _ => Err(Trap::new(
            "RUNTIME_TYPE_CONFUSION",
            "a place step does not apply to this value",
            source,
        )),
    }
}

fn write_into(
    target: &mut Value,
    path: &[PlaceStep],
    value: Value,
    source: SourceRef,
) -> Result<(), Trap> {
    let Some((step, rest)) = path.split_first() else {
        *target = value;
        return Ok(());
    };
    let slot = match (target, step) {
        (Value::Aggregate(elements), PlaceStep::Field(index)) => elements.get_mut(*index),
        (Value::Variant { payload, .. }, PlaceStep::Field(index)) => payload.get_mut(*index),
        (Value::Aggregate(elements), PlaceStep::Index(Some(index))) => {
            elements.get_mut(*index as usize)
        }
        _ => {
            return Err(Trap::new(
                "RUNTIME_TYPE_CONFUSION",
                "a place step does not apply to this value",
                source,
            ))
        }
    };
    let Some(slot) = slot else {
        return Err(Trap::new(
            "RUNTIME_INDEX_OUT_OF_RANGE",
            "a written place is out of range",
            source,
        ));
    };
    write_into(slot, rest, value, source)
}

/// Whether a magnitude is representable in a TOS integer type.
fn fits(kind: IntKind, magnitude: i128) -> bool {
    let (width, signed) = kind.shape();
    if signed {
        let bound = 1i128 << (width - 1);
        magnitude >= -bound && magnitude < bound
    } else {
        magnitude >= 0 && magnitude < (1i128 << width)
    }
}

/// Reduces a magnitude into a TOS integer type, for explicit wrapping contracts.
fn wrap(kind: IntKind, magnitude: i128) -> i128 {
    let (width, signed) = kind.shape();
    let modulus = 1i128 << width;
    let reduced = magnitude.rem_euclid(modulus);
    if signed && reduced >= modulus / 2 {
        reduced - modulus
    } else {
        reduced
    }
}

fn binary(op: BinaryOp, left: Value, right: Value, source: SourceRef) -> Result<Value, Trap> {
    if op.is_comparison() {
        return compare(op, left, right, source);
    }
    if let (Value::Size(left), Value::Size(right)) = (&left, &right) {
        return size_arithmetic(op, *left, *right, source);
    }
    let (Value::Int(kind, left), Value::Int(_, right)) = (&left, &right) else {
        return Err(Trap::new(
            "RUNTIME_TYPE_CONFUSION",
            "arithmetic applied to a non-integer",
            source,
        ));
    };
    let (kind, left, right) = (*kind, *left, *right);
    let (width, _) = kind.shape();
    let raw = match op {
        BinaryOp::Add => left.checked_add(right),
        BinaryOp::Subtract => left.checked_sub(right),
        BinaryOp::Multiply => left.checked_mul(right),
        BinaryOp::Divide | BinaryOp::Remainder => {
            if right == 0 {
                return Err(Trap::new(
                    "RUNTIME_DIVISION_BY_ZERO",
                    "division or remainder by zero",
                    source,
                ));
            }
            if op == BinaryOp::Divide {
                left.checked_div(right)
            } else {
                left.checked_rem(right)
            }
        }
        BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
            // docs/40 section 3: a shift count is nonnegative and strictly
            // smaller than the width.
            if right < 0 || right >= i128::from(width) {
                return Err(Trap::new(
                    "RUNTIME_INVALID_SHIFT",
                    alloc::format!("shift count {right} is not below the width {width}"),
                    source,
                ));
            }
            if op == BinaryOp::ShiftLeft {
                left.checked_shl(right as u32)
            } else {
                left.checked_shr(right as u32)
            }
        }
        BinaryOp::BitAnd => Some(left & right),
        BinaryOp::BitOr => Some(left | right),
        BinaryOp::BitXor => Some(left ^ right),
        _ => None,
    };
    let Some(raw) = raw else {
        return Err(Trap::new(
            "RUNTIME_ARITHMETIC_OVERFLOW",
            "a checked operation left the representable range",
            source,
        ));
    };
    // The host's width is not the program's: the result must fit the type the
    // source declared, or the operation overflowed whatever the host could do.
    if !fits(kind, raw) {
        return Err(Trap::new(
            "RUNTIME_ARITHMETIC_OVERFLOW",
            alloc::format!("{raw} does not fit {}", kind.spelled()),
            source,
        ));
    }
    Ok(Value::Int(kind, raw))
}

/// The reference ABI width `size` arithmetic is checked in.
///
/// docs/40 section 3 says `size` arithmetic is checked in the target ABI and
/// that portable source must not assume its width. The reference engine has to
/// pick one to be a semantic oracle at all, and it says so here rather than
/// inheriting whatever the host happens to use.
const SIZE_WIDTH_BITS: u32 = 64;

fn size_arithmetic(
    op: BinaryOp,
    left: u128,
    right: u128,
    source: SourceRef,
) -> Result<Value, Trap> {
    let raw = match op {
        BinaryOp::Add => left.checked_add(right),
        BinaryOp::Subtract => left.checked_sub(right),
        BinaryOp::Multiply => left.checked_mul(right),
        BinaryOp::Divide | BinaryOp::Remainder => {
            if right == 0 {
                return Err(Trap::new(
                    "RUNTIME_DIVISION_BY_ZERO",
                    "division or remainder by zero",
                    source,
                ));
            }
            if op == BinaryOp::Divide {
                left.checked_div(right)
            } else {
                left.checked_rem(right)
            }
        }
        BinaryOp::BitAnd => Some(left & right),
        BinaryOp::BitOr => Some(left | right),
        BinaryOp::BitXor => Some(left ^ right),
        BinaryOp::ShiftLeft | BinaryOp::ShiftRight => {
            if right >= u128::from(SIZE_WIDTH_BITS) {
                return Err(Trap::new(
                    "RUNTIME_INVALID_SHIFT",
                    "shift count is not below the size width",
                    source,
                ));
            }
            if op == BinaryOp::ShiftLeft {
                left.checked_shl(right as u32)
            } else {
                left.checked_shr(right as u32)
            }
        }
        _ => None,
    };
    let Some(raw) = raw else {
        return Err(Trap::new(
            "RUNTIME_ARITHMETIC_OVERFLOW",
            "a checked size operation left the representable range",
            source,
        ));
    };
    if raw >= (1u128 << SIZE_WIDTH_BITS) {
        return Err(Trap::new(
            "RUNTIME_ARITHMETIC_OVERFLOW",
            "a checked size operation left the target ABI width",
            source,
        ));
    }
    Ok(Value::Size(raw))
}

fn compare(op: BinaryOp, left: Value, right: Value, source: SourceRef) -> Result<Value, Trap> {
    if let (Value::Bool(left), Value::Bool(right)) = (&left, &right) {
        return Ok(Value::Bool(match op {
            BinaryOp::LogicalAnd => *left && *right,
            BinaryOp::LogicalOr => *left || *right,
            BinaryOp::Equal => left == right,
            BinaryOp::NotEqual => left != right,
            _ => {
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "an ordering comparison applied to bool",
                    source,
                ))
            }
        }));
    }
    let ordering = match (&left, &right) {
        (Value::Int(_, left), Value::Int(_, right)) => left.cmp(right),
        (Value::Size(left), Value::Size(right)) => left.cmp(right),
        (Value::Duration(left), Value::Duration(right)) => left.cmp(right),
        (Value::Text(left), Value::Text(right)) => left.cmp(right),
        (Value::Bytes(left), Value::Bytes(right)) => left.cmp(right),
        _ => {
            return Ok(Value::Bool(match op {
                BinaryOp::Equal => left == right,
                BinaryOp::NotEqual => left != right,
                _ => {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "an ordering comparison applied to values that are not ordered",
                        source,
                    ))
                }
            }))
        }
    };
    Ok(Value::Bool(match op {
        BinaryOp::Equal => ordering.is_eq(),
        BinaryOp::NotEqual => ordering.is_ne(),
        BinaryOp::Less => ordering.is_lt(),
        BinaryOp::LessOrEqual => ordering.is_le(),
        BinaryOp::Greater => ordering.is_gt(),
        BinaryOp::GreaterOrEqual => ordering.is_ge(),
        _ => {
            return Err(Trap::new(
                "RUNTIME_TYPE_CONFUSION",
                "a logical operator applied to non-bool operands",
                source,
            ))
        }
    }))
}

fn unary(op: UnaryOp, operand: Value, source: SourceRef) -> Result<Value, Trap> {
    match (op, operand) {
        (UnaryOp::Not, Value::Bool(value)) => Ok(Value::Bool(!value)),
        (UnaryOp::Negate, Value::Int(kind, magnitude)) => {
            let (_, signed) = kind.shape();
            if !signed {
                // docs/40 section 3 rejects `-x` for `uN` statically; reaching
                // here means the IR claims something the language forbids.
                return Err(Trap::new(
                    "RUNTIME_TYPE_CONFUSION",
                    "negation of an unsigned value",
                    source,
                ));
            }
            let Some(negated) = magnitude.checked_neg() else {
                return Err(Trap::new(
                    "RUNTIME_ARITHMETIC_OVERFLOW",
                    "negation left the representable range",
                    source,
                ));
            };
            if !fits(kind, negated) {
                return Err(Trap::new(
                    "RUNTIME_ARITHMETIC_OVERFLOW",
                    "negation left the representable range",
                    source,
                ));
            }
            Ok(Value::Int(kind, negated))
        }
        _ => Err(Trap::new(
            "RUNTIME_TYPE_CONFUSION",
            "a unary operator does not apply to this value",
            source,
        )),
    }
}

/// The source-map entry a trap points at, for a runtime diagnostic.
///
/// docs/43 section 6: a runtime failure names the exact source it came from.
pub fn trap_source<'a>(module: &'a Module, trap: &'a Trap) -> Option<&'a tos_ir::SourceMapEntry> {
    // A trap that crossed a module boundary carries its own resolved entry;
    // only a trap raised in `module` may be resolved against `module`'s table.
    trap.site
        .as_deref()
        .or_else(|| module.source_map.get(trap.source))
}

/// How many cells a value occupies in the engine's declared cost model.
///
/// The same model `allocation` uses: one cell per scalar, and an aggregate
/// costs its parts. Using a second model for `shared` would make two declared
/// budgets incomparable to each other and to the module that declares them.
fn value_cells(value: &Value) -> usize {
    match value {
        Value::Aggregate(parts) => parts.iter().map(value_cells).sum::<usize>().max(1),
        Value::Variant { payload, .. } => payload.iter().map(value_cells).sum::<usize>().max(1),
        Value::Bytes(bytes) => bytes.len().div_ceil(16).max(1),
        Value::Text(text) => text.len().div_ceil(16).max(1),
        _ => 1,
    }
}

/// A record of what a run consumed, for the resource accounting docs/41
/// section 6 requires an engine to keep.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Accounting {
    pub fuel_used: u128,
    pub fuel_limit: u128,
    pub max_call_depth: u128,
    pub recursion_limit: u128,
    pub tasks_started: u128,
    pub task_limit: u128,
    pub allocation_peak: u128,
    pub allocation_limit: u128,
    pub cleanups_peak: u128,
    pub cleanup_limit: u128,
    pub workers_reserved: u128,
    pub worker_limit: u128,
    pub shared_peak: u128,
    pub shared_limit: u128,
    pub sync_peak: u128,
    pub sync_limit: u128,
}

impl Accounting {
    pub fn of(module: &Module, outcome: &Outcome) -> Accounting {
        Accounting {
            fuel_used: outcome.fuel_used,
            fuel_limit: module.header.resource_envelope.fuel,
            max_call_depth: outcome.max_call_depth,
            recursion_limit: module.header.resource_envelope.recursion,
            tasks_started: outcome.tasks_started,
            task_limit: module.header.resource_envelope.tasks,
            allocation_peak: outcome.allocation_peak,
            allocation_limit: module.header.resource_envelope.allocation,
            cleanups_peak: outcome.cleanups_peak,
            cleanup_limit: module.header.resource_envelope.cleanup,
            // Bootstrap serializes, so one context is reserved for the run.
            workers_reserved: 1,
            worker_limit: module.header.resource_envelope.workers,
            shared_peak: outcome.shared_peak,
            shared_limit: module.header.resource_envelope.shared,
            sync_peak: outcome.sync_peak,
            sync_limit: module.header.resource_envelope.sync,
        }
    }
}

/// The entry points a module exports, for a driver that has to choose one.
pub fn exported_entries(module: &Module) -> BTreeMap<String, usize> {
    module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| function.signature.visibility == tos_ir::Visibility::Public)
        .map(|(index, function)| (function.signature.name.clone(), index))
        .collect()
}
