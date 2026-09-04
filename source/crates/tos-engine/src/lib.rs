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
//! **It executes verified IR only, and it never holds it all.** [`run_closure`]
//! is handed a [`Closure`] whose launch verified every module of the exact
//! resolved closure, one at a time, before the first instruction, and reduced
//! each to a fixed-size record committing to the exact bytes. What a frame
//! carries is a [`ClosureModuleId`] and three indices: the module it names is
//! made resident for one step and released again, so it may be evicted and
//! reloaded between any two steps of the same frame without the frame noticing
//! (ADR-0071 section 6). A reload is byte identity against the trusted record,
//! never a second run of the verifier.
//!
//! There is no set of everything and no way to reach a module except through
//! the closure's own membership, so a call that names something outside it has
//! no identifier to be found under.
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
    BinaryOp, CallTarget, CapabilitySource, Constant, Instruction, IntKind, Module, Op, Operand,
    Place, PlaceStep, SourceRef, Terminator, UnaryOp,
};
use tos_residency::{
    Failure, ModuleProvider, Residency, VerifiedClosureManifest, VerifiedModuleRecord,
};

/// The identity a running frame carries, minted only by the verified closure's
/// own manifest.
pub use tos_residency::ClosureModuleId;

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
    /// The closure's entry module could not be made resident before the first
    /// instruction. Carries the residency's own account of why.
    EntryNotResident(Failure),
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
/// The capability an operand names, or a trap.
fn capability_of(value: &Value, source: usize) -> Result<Handle, Trap> {
    match value {
        Value::Capability(handle) => Ok(*handle),
        _ => Err(Trap::new(
            "RUNTIME_TYPE_CONFUSION",
            String::from("a device access names something that is not a mapping"),
            source,
        )),
    }
}

/// The unsigned number an operand names, or a trap.
fn number_of(value: &Value, source: usize) -> Result<u64, Trap> {
    match value {
        Value::Int(_, number) if *number >= 0 => Ok(*number as u64),
        _ => Err(Trap::new(
            "RUNTIME_TYPE_CONFUSION",
            String::from("a device access names an offset that is not an unsigned number"),
            source,
        )),
    }
}

/// One device access the host is asked to perform (ADR-0081 §7).
#[derive(Clone, Copy, Debug)]
pub struct Observe {
    /// The mapping, as the capability the module holds.
    pub region: Handle,
    /// Where in that mapping, in bytes.
    pub offset: u64,
    /// How many bytes move, which is also the alignment the offset must satisfy.
    pub width: u8,
    /// Whether the device's bytes are little-endian. Carried rather than
    /// assumed, so a big-endian target is a different value here.
    pub little_endian: bool,
    /// The value to write, or `None` for a read.
    pub value: Option<u64>,
}

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

    /// Performs **one** hardware access of the declared width (ADR-0081 §9).
    ///
    /// Called once per source-level MMIO operation, and the host performs
    /// exactly one device transaction for it: not elided, not coalesced with
    /// another, not repeated speculatively, not widened or narrowed, and not
    /// reordered against another such access. Two reads in source are two
    /// device observations.
    ///
    /// The engine supplies the capability, a byte offset and the transaction's
    /// shape. **It never supplies an address**, and never learns one: which
    /// bytes a mapping covers is the host's, exactly as which frames a region
    /// covers is.
    ///
    /// A read answers with the value; a write answers with [`Value::Unit`]. A
    /// refusal — an out-of-range offset, a stale mapping, a width the mapping
    /// cannot serve — is a [`Trap`], because unlike an interface operation
    /// there is no status a module could have handled: the access did not
    /// happen and there is no value to stand for one that did.
    fn observe(&mut self, access: Observe) -> Result<Value, Trap>;

    /// Marks the instant before one TOS Core call, for an external observer.
    ///
    /// **The seam of ADR-0066 milestone 6b, and it exists only when this crate
    /// is built with `measurement-marks`.** The engine that ships has no
    /// observation point at all; with the feature, these are called immediately
    /// around the execution of one `Op::Call` to a local function, which is what
    /// `IPC_V1` §8's denominator is a call *of*.
    ///
    /// The marks flow through the system the caller handed the engine rather
    /// than through a global, for the reason `run_set` gives about everything
    /// else: what a run can reach is decided at the call site. Both default to
    /// nothing, so a host that is not being measured says nothing by saying
    /// nothing.
    ///
    /// An observation that could influence the call would not be one: these take
    /// no argument, return nothing, and the engine ignores whatever they do.
    #[cfg(feature = "measurement-marks")]
    fn mark_before_call(&mut self) {}

    /// Marks the instant after it.
    #[cfg(feature = "measurement-marks")]
    fn mark_after_call(&mut self) {}
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

    fn observe(&mut self, _access: Observe) -> Result<Value, Trap> {
        Err(Trap::new(
            "RUNTIME_DEVICE_UNREACHABLE",
            String::from("a device access was made on a run with no device to reach"),
            0,
        ))
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

/// The verified closure a run executes inside.
///
/// **The engine's only way to reach a module.** There is no set to index, no
/// table of everything, and no second path: a frame names a
/// [`ClosureModuleId`], and the module behind it is fetched here, for one step,
/// and released again. Between two steps of the same frame the module it is
/// running in may have been evicted and reloaded, and nothing in the frame
/// notices — which is the property ADR-0071 section 6 states and this shape
/// makes unavoidable rather than remembered.
///
/// Everything it holds is authority the launch already established: the trusted
/// records, the closure's membership, and a provider that answers with bytes and
/// nothing else. A module the manifest does not contain has no identifier, so
/// asking for one is not refused here — it cannot be spelled.
pub struct Closure<'a> {
    residency: &'a mut Residency,
    provider: &'a dyn ModuleProvider,
    records: &'a [VerifiedModuleRecord],
    manifest: &'a VerifiedClosureManifest,
}

impl<'a> Closure<'a> {
    /// Binds a bounded resident set to the closure its launch verified.
    pub fn new(
        residency: &'a mut Residency,
        provider: &'a dyn ModuleProvider,
        records: &'a [VerifiedModuleRecord],
        manifest: &'a VerifiedClosureManifest,
    ) -> Closure<'a> {
        Closure {
            residency,
            provider,
            records,
            manifest,
        }
    }

    /// Where the run starts: the entry module and the index of its entry
    /// function, both fixed at launch.
    pub fn entry(&self) -> (ClosureModuleId, usize) {
        self.manifest.entry()
    }

    /// Makes a module resident, evicting whatever the declared bounds require.
    ///
    /// A reload is byte identity against the trusted record — the semantic
    /// verifier does not run a second time — and everything the module derives
    /// is rebuilt before it is admitted.
    fn ensure(&mut self, id: ClosureModuleId) -> Result<(), Failure> {
        self.residency
            .ensure(id, self.provider, self.records, self.manifest)
    }

    /// The resident module. Only ever called immediately after [`Closure::ensure`],
    /// and the borrow it returns ends before the frame stack is touched.
    fn module_of(&self, id: ClosureModuleId) -> Option<&Module> {
        self.residency.module_of(id)
    }

    /// Which module a resident caller's import slot names.
    fn import_of(&self, id: ClosureModuleId, slot: usize) -> Option<ClosureModuleId> {
        self.residency.import_of(id, slot)
    }

    /// Which **public** function of a resident module an export name reaches.
    fn export_of(&self, id: ClosureModuleId, name: &str) -> Option<usize> {
        self.residency.export_of(id, name)
    }

    /// What the run cost the resident set.
    pub fn traffic(&self) -> tos_residency::Traffic {
        self.residency.traffic()
    }

    /// What is resident now, by component.
    pub fn ledger(&self) -> tos_residency::Ledger {
        self.residency.ledger()
    }
}

/// A residency failure, as the trap a running program sees.
///
/// A frame that cannot reach its own module is not a program error the module
/// could have avoided, so it traps with the identity and the check that refused
/// rather than being silently retried or resolved some other way.
fn residency_trap(failure: &Failure, source: SourceRef) -> Trap {
    let detail = match failure {
        Failure::Missing(module) => {
            alloc::format!("the provider has no image for closure module {module}")
        }
        Failure::ArtifactDigest { module } => alloc::format!(
            "the image for closure module {module} is not the one this launch verified"
        ),
        Failure::Parser { module, error } => {
            alloc::format!("the image for closure module {module} did not parse: {error:?}")
        }
        Failure::Verifier { module, .. } => {
            alloc::format!("closure module {module} was refused by the verifier")
        }
        Failure::WrongModule { module } => alloc::format!(
            "closure module {module} declares an import the verified closure does not contain"
        ),
        Failure::NoEntryFunction { module } => {
            alloc::format!("closure module {module} exports no such entry function")
        }
        Failure::OverResidencyBound { module, bytes } => alloc::format!(
            "closure module {module} needs {bytes} resident bytes, past the declared bound"
        ),
    };
    Trap::new("RUNTIME_MODULE_UNAVAILABLE", detail, source)
}

/// Runs the entry function of a verified closure.
///
/// A cross-module call is resolved against the closure's own membership and
/// nothing else: the engine never loads, searches for or fabricates a module,
/// and a name that is not a member has no identifier to be found under.
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
pub fn run_closure(
    closure: &mut Closure<'_>,
    arguments: Vec<Value>,
    system: &mut dyn System,
) -> Result<Result<Outcome, Trap>, Refusal> {
    let (entry_module, index) = closure.entry();

    // The entry module before anything else, because everything below is read
    // out of it. Nothing else is made resident here: the closure was verified
    // at launch, one module at a time, and a run that touched every module
    // before its first instruction would be holding the whole closure again.
    if let Err(failure) = closure.ensure(entry_module) {
        return Err(Refusal::EntryNotResident(failure));
    }

    // One borrow, and everything the run needs from the entry module is copied
    // out of it before the borrow ends: the arity, the envelope, and the
    // capability requests. From here the module may be evicted like any other.
    let (expected, envelope, requests) = {
        let module = closure
            .module_of(entry_module)
            .expect("the entry module was just made resident");
        let Some(function) = module.functions.get(index) else {
            return Err(Refusal::NoSuchEntry(alloc::format!("function {index}")));
        };
        let requests: Vec<(String, String)> = module
            .capability_imports
            .iter()
            .map(|request| (request.interface.clone(), request.binding.clone()))
            .collect();
        (
            function.signature.parameters.len(),
            module.header.resource_envelope.clone(),
            requests,
        )
    };
    if expected != arguments.len() {
        return Err(Refusal::EntryArity {
            expected,
            actual: arguments.len(),
        });
    }

    // Every request answered before the first instruction, or none of them run.
    // A module that got as far as a call before discovering it holds nothing
    // would have already done work under an assumption that was false.
    let mut imports = Vec::with_capacity(requests.len());
    for (position, (interface, binding)) in requests.iter().enumerate() {
        let held = system.granted(Request {
            interface,
            binding,
            position,
        });
        match held {
            Some(handle) => imports.push(Value::Capability(handle)),
            None => {
                return Err(Refusal::CapabilityDenied {
                    binding: binding.clone(),
                    interface: interface.clone(),
                })
            }
        }
    }

    let envelope = &envelope;
    let mut engine = Engine {
        system,
        imports,
        fuel_limit: envelope.fuel,
        recursion_limit: envelope.recursion.max(1),
        fuel_used: 0,
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
    };
    // docs/41 section 6: a reservation is checked before the thing it pays for
    // happens. A module that declares no worker cannot run one instruction.
    if let Err(trap) = engine.reserve_worker() {
        return Ok(Err(trap));
    }
    let outcome = engine.execute(closure, entry_module, index, arguments);
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

struct Engine<'system> {
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
}

/// What one activation has charged, released exactly when it returns.
///
/// These used to be engine fields that a call saved, zeroed and restored. That
/// is the shape of bug this removes rather than fixes: a charge belongs to the
/// activation that made it, so it lives on the frame and is released when the
/// frame is popped, whether it returns or traps.
#[derive(Clone, Copy, Debug, Default)]
struct Charges {
    allocation: u128,
    cleanups: u128,
    sync: u128,
}

/// One activation, owning everything it needs.
///
/// **Nothing here borrows a module.** Not a `&Module`, not a `&Function`, not a
/// `&Block`, `&Instruction`, `&Operand` or `&Place`, and no slice into any of
/// them. What the frame holds is a module identity and three indices, so the
/// module it names may be released and read again between any two steps
/// (ADR-0071 §6). Give this type a lifetime parameter and the property is gone.
struct Frame {
    /// Which module this activation is running in. A **stable identity**, not a
    /// pointer and not a position in anything that could be reordered: the
    /// module it names may be evicted and reloaded between any two steps of this
    /// frame, and the identity still resolves to the same verified module
    /// afterwards.
    module: ClosureModuleId,
    function: usize,
    block: usize,
    /// The next instruction of `block` to execute. A resumed frame continues
    /// here, which is what makes a call a transition rather than a recursion.
    instruction: usize,
    values: Vec<Option<Value>>,
    charges: Charges,
    /// Block entries in this activation, for the escape guard the old loop kept
    /// per call.
    steps: u128,
    /// What this frame is waiting for, when it is suspended in a call.
    pending: Option<Pending>,
}

/// What a suspended frame does with the value its callee returns.
///
/// Every field is owned. A continuation that held the caller's operands by
/// reference would be holding a slice into a module across a call, which is
/// exactly what may not survive a step.
enum Pending {
    /// An ordinary call: write back what a `borrow mut` parameter changed, then
    /// store the result.
    Call {
        result: Option<usize>,
        /// `(callee parameter slot, caller value slot)` for each `borrow mut`
        /// parameter whose caller operand names a value. Computed before the
        /// call, from the callee's declared modes and the caller's operands.
        writeback: Vec<(usize, usize)>,
        /// Whether this call is inside an ADR-0066 measurement interval.
        marks: bool,
    },
    /// A join or await: the result is the task's outcome, wrapped.
    Join { result: Option<usize> },
    /// A scope's cleanups, in order. ADR-0035 makes what one leaves visible to
    /// the next, so the write-back is applied before the next is entered.
    Cleanups {
        plans: Vec<CleanupPlan>,
        at: usize,
        writeback: Vec<(usize, usize)>,
        source: SourceRef,
    },
}

/// One deferred cleanup, copied out of the instruction that named it.
struct CleanupPlan {
    body: usize,
    captures: Vec<Operand>,
}

/// Which function an activation is about to enter.
///
/// An imported target carries an owned name and is resolved by the driver,
/// outside the step, because resolving it is what may change what is resident.
enum Target {
    /// A function of the frame's own module.
    Local(usize),
    Imported(ImportedCall),
}

/// A cross-module call, as the driver receives it.
///
/// One owned value rather than three parameters, because the three belong
/// together: the slot names the import, the name names the export, and the
/// operands are there because a cross-module write-back plan cannot be computed
/// where the call is written. It needs the **callee's** declared parameter
/// modes, and those are in a module the caller does not hold — so the operands
/// travel to where the callee is resolved.
struct ImportedCall {
    slot: usize,
    name: String,
    operands: Vec<Operand>,
}

/// What evaluating one instruction produced.
enum Evaluated {
    Value(Option<Value>),
    Enter {
        target: Target,
        arguments: Vec<Value>,
        pending: Pending,
    },
}

/// What one frame's step decided.
enum Transition {
    Enter {
        target: Target,
        arguments: Vec<Value>,
    },
    Leave(Value),
}

/// The invariant this engine is built around, asserted where it can be.
///
/// A type that borrows something cannot be `'static`. So requiring `'static` of
/// the frame and of every continuation is a compile-time statement that none of
/// them holds a reference into a module — give any of them a lifetime parameter
/// and this stops compiling. It is not a substitute for reading the types, but
/// it is a wall that a future edit runs into rather than walks past.
const _: () = {
    const fn owns_nothing_borrowed<T: 'static>() {}
    owns_nothing_borrowed::<Frame>();
    owns_nothing_borrowed::<Pending>();
    owns_nothing_borrowed::<CleanupPlan>();
    owns_nothing_borrowed::<Target>();
    owns_nothing_borrowed::<ImportedCall>();
    owns_nothing_borrowed::<Evaluated>();
    owns_nothing_borrowed::<Transition>();
    owns_nothing_borrowed::<Charges>();
    owns_nothing_borrowed::<Value>();
};

/// Which of a callee's parameters write back into which of the caller's slots.
///
/// A `borrow mut` parameter names the caller's place, and the borrow rules
/// guarantee no other alias is live for the duration of the call, so copying in
/// and copying out is observationally the same as a reference and needs no
/// aliasing machinery to be correct. The plan is computed before the call, from
/// the callee's declared modes and the caller's operands, so the continuation
/// carries indices rather than a slice into an instruction.
///
/// Only `Operand::Value` writes back, which is the semantics this replaces: a
/// constant operand names no place to write to.
fn writeback_plan(module: &Module, function: usize, operands: &[Operand]) -> Vec<(usize, usize)> {
    let Some(body) = module.functions.get(function) else {
        return Vec::new();
    };
    let mut plan = Vec::new();
    for (position, parameter) in body.signature.parameters.iter().enumerate() {
        if parameter.mode != tos_ir::PassMode::MutableBorrow {
            continue;
        }
        if let Some(Operand::Value(slot)) = operands.get(position) {
            plan.push((position, *slot));
        }
    }
    plan
}

/// Copies a callee's final parameter slots into the caller's places.
fn write_back(caller: &mut Frame, plan: &[(usize, usize)], callee: &[Option<Value>]) {
    for (position, slot) in plan {
        if let (Some(Some(value)), Some(target)) =
            (callee.get(*position), caller.values.get_mut(*slot))
        {
            *target = Some(value.clone());
        }
    }
}

/// Stores a call's result in the caller's slot, when the instruction named one.
fn store(caller: &mut Frame, result: Option<usize>, value: Value) {
    if let Some(slot) = result {
        if slot < caller.values.len() {
            caller.values[slot] = Some(value);
        }
    }
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

impl Engine<'_> {
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

    /// Runs the whole program as an explicit stack of activations.
    ///
    /// One loop, and a module borrowed only for the length of a single step.
    /// Between steps nothing holds a reference into a module, which is what
    /// makes a frame survivable across an eviction (ADR-0071 §6): a call is a
    /// transition on this stack rather than a recursion on the host's, so the
    /// depth a TOS program may reach is the depth it declares and not the depth
    /// the host happens to have.
    fn execute(
        &mut self,
        closure: &mut Closure<'_>,
        entry_module: ClosureModuleId,
        entry_function: usize,
        arguments: Vec<Value>,
    ) -> Result<Value, Trap> {
        let mut frames: Vec<Frame> = Vec::new();
        match self.enter(
            closure,
            &mut frames,
            entry_module,
            entry_function,
            arguments,
        ) {
            Ok(()) => {}
            Err(trap) => return Err(self.unwind(closure, &mut frames, trap)),
        }

        loop {
            let home = frames
                .last()
                .expect("the loop ends when the stack empties")
                .module;

            // The step's module is made resident, borrowed, stepped, and the
            // borrow released — in that order, every time round. It is the last
            // one that matters: nothing below this block holds a reference into
            // a module, so what happens next is free to evict it.
            let transition = {
                if let Err(failure) = closure.ensure(home) {
                    let trap = residency_trap(&failure, self.site_of(closure, &frames));
                    return Err(self.unwind(closure, &mut frames, trap));
                }
                let module: &Module = closure
                    .module_of(home)
                    .expect("the frame's module was just made resident");
                let frame = frames.last_mut().expect("just observed");
                self.step(module, frame)
            };
            let transition = match transition {
                Ok(transition) => transition,
                Err(trap) => return Err(self.unwind(closure, &mut frames, trap)),
            };

            match transition {
                Transition::Enter { target, arguments } => {
                    let entered = match target {
                        Target::Local(index) => {
                            self.enter(closure, &mut frames, home, index, arguments)
                        }
                        Target::Imported(call) => {
                            self.enter_imported(closure, &mut frames, home, &call, arguments)
                        }
                    };
                    if let Err(trap) = entered {
                        return Err(self.unwind(closure, &mut frames, trap));
                    }
                }
                Transition::Leave(value) => {
                    let frame = frames.pop().expect("a frame was running");
                    self.release_frame(&frame);
                    let Some(caller) = frames.last_mut() else {
                        return Ok(value);
                    };
                    let taken = caller.pending.take();
                    let resumed = match taken {
                        Some(pending) => {
                            self.resume(closure, &mut frames, pending, frame.values, value)
                        }
                        // A frame with no caller continuation is the entry, and
                        // the entry has no caller.
                        None => Ok(()),
                    };
                    if let Err(trap) = resumed {
                        return Err(self.unwind(closure, &mut frames, trap));
                    }
                }
            }
        }
    }

    /// The source span of the instruction the innermost frame is at.
    ///
    /// Used only to give a residency refusal a place in the program. It makes
    /// the frame's module resident to read it, which is allowed: the frame holds
    /// a stable identity, and reading a span through it is the same operation as
    /// running an instruction through it. If even that fails there is no span to
    /// report and the trap carries the module's own identity instead.
    fn site_of(&self, closure: &mut Closure<'_>, frames: &[Frame]) -> SourceRef {
        let Some(frame) = frames.last() else {
            return 0;
        };
        if closure.ensure(frame.module).is_err() {
            return 0;
        }
        closure
            .module_of(frame.module)
            .and_then(|module| module.functions.get(frame.function))
            .and_then(|function| function.blocks.get(frame.block))
            .map(|block| block.source)
            .unwrap_or(0)
    }

    /// Pushes an activation, charging depth exactly where the recursive form
    /// charged it.
    fn enter(
        &mut self,
        closure: &mut Closure<'_>,
        frames: &mut Vec<Frame>,
        module: ClosureModuleId,
        function: usize,
        arguments: Vec<Value>,
    ) -> Result<(), Trap> {
        // Two numbers out of the callee, and then the borrow is done: how many
        // value slots the activation needs and where its declaration is. The
        // frame that gets pushed below holds neither the module nor anything
        // reached through it.
        let (slots, source) = {
            if let Err(failure) = closure.ensure(module) {
                let site = self.site_of(closure, frames);
                return Err(residency_trap(&failure, site));
            }
            let body = closure
                .module_of(module)
                .expect("the callee was just made resident")
                .functions
                .get(function)
                .ok_or_else(|| {
                    Trap::new(
                        "RUNTIME_UNRESOLVED_IMPORT",
                        "a call names a function the module does not define",
                        0,
                    )
                })?;
            (body.values.len(), body.source)
        };
        let depth = frames.len() as u128 + 1;
        self.max_depth = self.max_depth.max(depth);
        if depth > self.recursion_limit {
            return Err(Trap::new(
                "RUNTIME_RECURSION_LIMIT",
                alloc::format!("the declared depth of {} is exceeded", self.recursion_limit),
                source,
            ));
        }
        let mut values: Vec<Option<Value>> = alloc::vec![None; slots];
        for (slot, argument) in arguments.into_iter().enumerate() {
            if slot < values.len() {
                values[slot] = Some(argument);
            }
        }
        frames.push(Frame {
            module,
            function,
            block: 0,
            instruction: 0,
            values,
            charges: Charges::default(),
            steps: 1,
            pending: None,
        });
        Ok(())
    }

    /// Resolves a cross-module call against the verified closure and pushes it.
    ///
    /// The whole path, and nothing beside it:
    ///
    /// ```text
    /// caller identity -> resident import slot -> callee identity
    ///                 -> make the callee resident
    ///                 -> its public export index -> function index
    /// ```
    ///
    /// There is no search over the closure at any step. The import slot is
    /// resident state the caller's own verified artifact produced, resolved
    /// against trusted membership when the caller was loaded; the export index
    /// holds public functions only, so a private one is not refused here but
    /// absent. Making the callee resident may evict the caller, and that is
    /// expected: nothing below holds a reference into it.
    ///
    /// The callee executes with the run's single budget: fuel, depth and
    /// allocation are the entry's, because docs/41 section 6 admits a call only
    /// when the callee's declared contract already fits the caller's envelope.
    /// Crossing a module boundary is not a way to obtain a second budget.
    fn enter_imported(
        &mut self,
        closure: &mut Closure<'_>,
        frames: &mut Vec<Frame>,
        home: ClosureModuleId,
        call: &ImportedCall,
        arguments: Vec<Value>,
    ) -> Result<(), Trap> {
        let ImportedCall {
            slot,
            name,
            operands,
        } = call;
        // The call site, out of the caller, before anything can evict it.
        if let Err(failure) = closure.ensure(home) {
            return Err(residency_trap(&failure, 0));
        }
        let source = frames
            .last()
            .and_then(|frame| {
                let module = closure.module_of(frame.module)?;
                let block = module
                    .functions
                    .get(frame.function)?
                    .blocks
                    .get(frame.block)?;
                block
                    .instructions
                    .get(frame.instruction.saturating_sub(1))
                    .map(|instruction| instruction.source)
            })
            .unwrap_or(0);

        let Some(callee) = closure.import_of(home, *slot) else {
            return Err(Trap::new(
                "RUNTIME_UNRESOLVED_IMPORT",
                "a call names an import the module does not declare",
                source,
            ));
        };

        // From here the caller may go. What survives is `home`, an identity.
        if let Err(failure) = closure.ensure(callee) {
            return Err(residency_trap(&failure, source));
        }
        let Some(index) = closure.export_of(callee, name.as_str()) else {
            return Err(Trap::new(
                "RUNTIME_UNRESOLVED_IMPORT",
                alloc::format!("closure module {} exports no {name}", callee.position()),
                source,
            ));
        };
        // The write-back plan, now that the callee's declared modes are in reach.
        // It is index pairs and nothing else, so it survives both modules being
        // evicted before the callee returns.
        let plan = closure
            .module_of(callee)
            .map(|module| writeback_plan(module, index, operands))
            .unwrap_or_default();
        if let Some(caller) = frames.last_mut() {
            if let Some(Pending::Call { writeback, .. }) = &mut caller.pending {
                *writeback = plan;
            }
        }

        let arity = closure
            .module_of(callee)
            .and_then(|module| module.functions.get(index))
            .map(|function| function.signature.parameters.len());
        if arity != Some(arguments.len()) {
            return Err(Trap::new(
                "RUNTIME_UNRESOLVED_IMPORT",
                alloc::format!(
                    "closure module {}.{name} takes a different number of arguments",
                    callee.position()
                ),
                source,
            ));
        }
        self.enter(closure, frames, callee, index, arguments)
    }

    /// Applies a caller's continuation to what its callee returned.
    fn resume(
        &mut self,
        closure: &mut Closure<'_>,
        frames: &mut Vec<Frame>,
        pending: Pending,
        callee_values: Vec<Option<Value>>,
        value: Value,
    ) -> Result<(), Trap> {
        match pending {
            Pending::Call {
                result,
                writeback,
                marks,
            } => {
                let caller = frames.last_mut().expect("a caller was observed");
                write_back(caller, &writeback, &callee_values);
                store(caller, result, value);
                let _ = marks;
                #[cfg(feature = "measurement-marks")]
                if marks {
                    self.system.mark_after_call();
                }
                Ok(())
            }
            Pending::Join { result } => {
                let caller = frames.last_mut().expect("a caller was observed");
                store(
                    caller,
                    result,
                    Value::Variant {
                        index: 0,
                        payload: alloc::vec![value],
                    },
                );
                Ok(())
            }
            Pending::Cleanups {
                plans,
                at,
                writeback,
                source,
            } => {
                {
                    let caller = frames.last_mut().expect("a caller was observed");
                    write_back(caller, &writeback, &callee_values);
                }
                self.run_cleanup(closure, frames, plans, at + 1, source)
            }
        }
    }

    /// Enters the next cleanup of a scope, or finishes the sequence.
    ///
    /// ADR-0035 runs them in the order the instruction gives, and what one
    /// leaves is what the next observes — which is why the write-back above
    /// happens before this is called.
    fn run_cleanup(
        &mut self,
        closure: &mut Closure<'_>,
        frames: &mut Vec<Frame>,
        plans: Vec<CleanupPlan>,
        at: usize,
        source: SourceRef,
    ) -> Result<(), Trap> {
        if at >= plans.len() {
            return Ok(());
        }
        let home = frames.last().expect("a caller was observed").module;
        // The scope's module, again by identity: a cleanup chain crosses as many
        // eviction points as it has bodies, and each one starts by asking for
        // the module rather than by keeping it.
        if let Err(failure) = closure.ensure(home) {
            return Err(residency_trap(&failure, source));
        }
        let body = plans[at].body;
        let (arguments, writeback) = {
            let module: &Module = closure
                .module_of(home)
                .expect("the scope's module was just made resident");
            let plan = &plans[at];
            let caller = frames.last().expect("a caller was observed");
            let mut arguments = Vec::new();
            for capture in &plan.captures {
                arguments.push(self.operand(module, capture, &caller.values, source)?);
            }
            (arguments, writeback_plan(module, body, &plan.captures))
        };
        {
            let caller = frames.last_mut().expect("a caller was observed");
            caller.pending = Some(Pending::Cleanups {
                plans,
                at,
                writeback,
                source,
            });
        }
        self.enter(closure, frames, home, body, arguments)
    }

    /// Releases exactly what one activation charged.
    fn release_frame(&mut self, frame: &Frame) {
        self.release_allocation(frame.charges.allocation);
        self.release_cleanups(frame.charges.cleanups);
        // A guard cannot outlive the frame that took it (ADR-0036).
        self.sync_held = self.sync_held.saturating_sub(frame.charges.sync);
    }

    /// Unwinds the stack for a trap, releasing each frame's charges and
    /// resolving the site where the recursive form resolved it.
    fn unwind(
        &mut self,
        closure: &mut Closure<'_>,
        frames: &mut Vec<Frame>,
        mut trap: Trap,
    ) -> Trap {
        while let Some(frame) = frames.pop() {
            self.release_frame(&frame);
            if trap.site.is_none() {
                // **Every** trap resolves its own site, at the innermost frame
                // that still has one. It has to: nothing downstream holds the
                // module any more, so a trap that left the engine carrying only
                // an index into a source map nobody can reach would be a trap
                // with no place in the program.
                //
                // The frame's module may have been evicted long ago. It is asked
                // for again here, by the identity the frame still holds — an
                // unwind cannot assume anything is resident, and a source map is
                // read through the same door as everything else. A module that
                // cannot be reached leaves the trap without a site rather than
                // turning a trap into a second failure.
                if closure.ensure(frame.module).is_ok() {
                    trap.site = closure
                        .module_of(frame.module)
                        .and_then(|module| module.source_map.get(trap.source))
                        .cloned()
                        .map(alloc::boxed::Box::new);
                }
            }
            if let Some(caller) = frames.last_mut() {
                let pending = caller.pending.take();
                #[cfg(feature = "measurement-marks")]
                if let Some(Pending::Call { marks: true, .. }) = pending {
                    self.system.mark_after_call();
                }
                let _ = pending;
            }
        }
        trap
    }

    /// Runs one frame until it transfers control.
    ///
    /// The module is a parameter and is dropped by the caller before anything
    /// touches the stack. Everything this reads out of it is copied into owned
    /// data before the frame suspends.
    fn step(&mut self, module: &Module, frame: &mut Frame) -> Result<Transition, Trap> {
        loop {
            let function = &module.functions[frame.function];
            let block = &function.blocks[frame.block];
            while frame.instruction < block.instructions.len() {
                let at = frame.instruction;
                let instruction = &block.instructions[at];
                self.spend(instruction.source)?;
                match self.evaluate(
                    module,
                    frame.module,
                    instruction,
                    &mut frame.values,
                    &mut frame.charges,
                )? {
                    Evaluated::Value(produced) => {
                        if let (Some(slot), Some(value)) = (instruction.result, produced) {
                            if slot < frame.values.len() {
                                frame.values[slot] = Some(value);
                            }
                        }
                        frame.instruction = at + 1;
                    }
                    Evaluated::Enter {
                        target,
                        arguments,
                        pending,
                    } => {
                        // Resume past the call, so the frame that comes back
                        // does not run it again.
                        frame.instruction = at + 1;
                        frame.pending = Some(pending);
                        return Ok(Transition::Enter { target, arguments });
                    }
                }
            }
            match self.terminate(module, &block.terminator, &mut frame.values, block.source)? {
                Exit::Return(value) => return Ok(Transition::Leave(value)),
                Exit::Goto(next) => {
                    frame.block = next;
                    frame.instruction = 0;
                    frame.steps += 1;
                    if frame.steps > self.fuel_limit.saturating_add(1) {
                        return Err(Trap::new(
                            "RUNTIME_FUEL_EXHAUSTED",
                            "control did not leave the function within its budget",
                            function.blocks[next].source,
                        ));
                    }
                }
            }
        }
    }

    fn terminate(
        &mut self,
        module: &Module,
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
                    Some(operand) => self.operand(module, operand, values, source)?,
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
                let Value::Bool(taken) = self.operand(module, condition, values, source)? else {
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
                let value = self.operand(module, subject, values, source)?;
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
                let value = self.operand(module, result, values, source)?;
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
        module: &Module,
        home: ClosureModuleId,
        instruction: &Instruction,
        values: &mut [Option<Value>],
        charges: &mut Charges,
    ) -> Result<Evaluated, Trap> {
        let op = &instruction.op;
        let source = instruction.source;
        let _ = home;
        let produced = match op {
            Op::Const(constant) => Some(self.constant(module, *constant, source)?),
            // **An observation, not a load** (ADR-0081 §9). The engine does not
            // perform it and does not know an address: it hands the host the
            // capability, the offset and the shape of the transaction, and the
            // host performs exactly one hardware access of that width. Every
            // rule about not eliding, coalescing, repeating or reordering is a
            // property of this being one call per source operation.
            Op::MmioRead {
                region,
                offset,
                width,
                little_endian,
            } => {
                let region = self.operand(module, region, values, source)?;
                let offset = self.operand(module, offset, values, source)?;
                Some(self.system.observe(Observe {
                    region: capability_of(&region, source)?,
                    offset: number_of(&offset, source)?,
                    width: *width,
                    little_endian: *little_endian,
                    value: None,
                })?)
            }
            Op::MmioWrite {
                region,
                offset,
                value,
                width,
                little_endian,
            } => {
                let region = self.operand(module, region, values, source)?;
                let offset = self.operand(module, offset, values, source)?;
                let value = self.operand(module, value, values, source)?;
                self.system.observe(Observe {
                    region: capability_of(&region, source)?,
                    offset: number_of(&offset, source)?,
                    width: *width,
                    little_endian: *little_endian,
                    value: Some(number_of(&value, source)?),
                })?;
                Some(Value::Unit)
            }
            Op::Aggregate { operands, .. } => {
                // Reserve before building: docs/41 section 6 checks a
                // reservation before the thing it pays for happens, so a value
                // that would not fit is never constructed at all.
                let charged = self.reserve_allocation(operands.len(), source)?;
                charges.allocation += charged;
                let mut elements = Vec::new();
                for operand in operands {
                    elements.push(self.operand(module, operand, values, source)?);
                }
                Some(Value::Aggregate(elements))
            }
            Op::Variant {
                index, operands, ..
            } => {
                let charged = self.reserve_allocation(operands.len(), source)?;
                charges.allocation += charged;
                let mut payload = Vec::new();
                for operand in operands {
                    payload.push(self.operand(module, operand, values, source)?);
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
                let value = self.operand(module, value, values, source)?;
                self.write_place(place, value, values, source)?;
                None
            }
            Op::Drop { .. } => None,
            Op::Binary { op, left, right } => {
                let left = self.operand(module, left, values, source)?;
                let right = self.operand(module, right, values, source)?;
                Some(binary(*op, left, right, source)?)
            }
            Op::Unary { op, operand } => {
                let operand = self.operand(module, operand, values, source)?;
                Some(unary(*op, operand, source)?)
            }
            Op::Widen { operand, to } => {
                let operand = self.operand(module, operand, values, source)?;
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
                match target {
                    // ADR-0066 milestone 6b puts its marks here, and the
                    // boundary is exactly this expression. Between them:
                    // reading the arguments from the caller's slots, the
                    // depth increment and its recursion check, the callee's
                    // value slots being made and the arguments moved in, the
                    // fuel-counted body, the release of what the frame charged
                    // — and the mutable-borrow writeback. That is a call and
                    // its inevitable accounting: what `IPC_V1` §8 names,
                    // without anything a run does once. The interval is
                    // unchanged by the frame machine: `mark_before_call` still
                    // fires before the arguments are read, and
                    // `mark_after_call` fires when the continuation completes,
                    // which is after the writeback and on the trap path too.
                    CallTarget::Local(index) => {
                        #[cfg(feature = "measurement-marks")]
                        self.system.mark_before_call();
                        let arguments = match self.arguments(module, operands, values, source) {
                            Ok(arguments) => arguments,
                            Err(trap) => {
                                #[cfg(feature = "measurement-marks")]
                                self.system.mark_after_call();
                                return Err(trap);
                            }
                        };
                        return Ok(Evaluated::Enter {
                            target: Target::Local(*index),
                            arguments,
                            pending: Pending::Call {
                                result: instruction.result,
                                writeback: writeback_plan(module, *index, operands),
                                marks: true,
                            },
                        });
                    }
                    CallTarget::Imported { import, name } => {
                        let arguments = self.arguments(module, operands, values, source)?;
                        return Ok(Evaluated::Enter {
                            target: Target::Imported(ImportedCall {
                                slot: *import,
                                name: name.clone(),
                                operands: operands.clone(),
                            }),
                            arguments,
                            pending: Pending::Call {
                                result: instruction.result,
                                writeback: Vec::new(),
                                marks: false,
                            },
                        });
                    }
                    // Two different things share this target, and the
                    // instruction says which. A predeclared name is a
                    // conversion the language performs itself; the same shape
                    // carrying an accepted interface path is an operation the
                    // language does not perform at all, and the difference is
                    // exactly the field the verifier checked.
                    CallTarget::Predeclared(name) => {
                        let arguments = self.arguments(module, operands, values, source)?;
                        match &instruction.unsafe_interface {
                            None => Some(self.predeclared(name, arguments, source)?),
                            Some(interface) => {
                                Some(self.reach(interface, name, arguments, source)?)
                            }
                        }
                    }
                }
            }
            Op::Closure { body, captures } => {
                let mut held = Vec::new();
                for capture in captures {
                    held.push(self.operand(module, capture, values, source)?);
                }
                Some(Value::Closure {
                    body: *body,
                    captures: held,
                })
            }
            Op::CallValue { callee, operands } => {
                let callee = self.operand(module, callee, values, source)?;
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
                    arguments.push(self.operand(module, operand, values, source)?);
                }
                arguments.extend(captures);
                return Ok(Evaluated::Enter {
                    target: Target::Local(body),
                    arguments,
                    pending: Pending::Call {
                        result: instruction.result,
                        writeback: Vec::new(),
                        marks: false,
                    },
                });
            }
            Op::Spawn { body, captures } => {
                let mut held = Vec::new();
                for capture in captures {
                    held.push(self.operand(module, capture, values, source)?);
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
                let handle = self.operand(module, task, values, source)?;
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
                if cancelled {
                    Some(Value::Variant {
                        index: 1,
                        payload: Vec::new(),
                    })
                } else {
                    // A task that was not cancelled runs here, serialized, and
                    // its result is wrapped by the continuation.
                    return Ok(Evaluated::Enter {
                        target: Target::Local(body),
                        arguments: captures,
                        pending: Pending::Join {
                            result: instruction.result,
                        },
                    });
                }
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
                let value = self.operand(module, object, values, source)?;
                self.reserve_sync(source)?;
                charges.sync += 1;
                Some(value)
            }
            // `share` consumes its argument and produces the same value behind
            // a `Shared` handle. Bootstrap has one context, so the sharing is
            // observable in the accounting rather than in aliasing: what the
            // engine can prove here is that the module stayed inside the
            // `shared` budget it declared.
            Op::Share { operand } => {
                let value = self.operand(module, operand, values, source)?;
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
                charges.cleanups += 1;
                None
            }
            Op::RunCleanups { calls } => {
                // ADR-0035: a cleanup acts on the scope it runs in, so what it
                // leaves is what the next one and the scope observe. The list
                // is copied out of the instruction — a continuation holding
                // `&CleanupCall` would be holding a slice into a module across
                // a call.
                if calls.is_empty() {
                    None
                } else {
                    let plans: Vec<CleanupPlan> = calls
                        .iter()
                        .map(|call| CleanupPlan {
                            body: call.body,
                            captures: call.captures.clone(),
                        })
                        .collect();
                    let first = &plans[0];
                    let mut arguments = Vec::new();
                    for capture in &first.captures {
                        arguments.push(self.operand(module, capture, values, source)?);
                    }
                    let writeback = writeback_plan(module, first.body, &first.captures);
                    let body = first.body;
                    return Ok(Evaluated::Enter {
                        target: Target::Local(body),
                        arguments,
                        pending: Pending::Cleanups {
                            plans,
                            at: 0,
                            writeback,
                            source,
                        },
                    });
                }
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
                capabilities,
                right,
                operands,
            } => {
                // ADR-0056, ADR-0063 and ADR-0078: the capabilities first, in
                // the order the schema declares them, then the operation's
                // values. Each position says where its capability comes from,
                // and both cases end in the same place — a `Value::Capability`
                // the engine carries without reading.
                let mut arguments = Vec::with_capacity(capabilities.len() + operands.len());
                let mut first = None;
                for source_of in capabilities {
                    let (interface, held) = match source_of {
                        CapabilitySource::Import(index) => {
                            let Some(declared) = module
                                .capability_imports
                                .get(*index)
                                .map(|import| import.interface.as_str())
                            else {
                                return Err(Trap::new(
                                    "RUNTIME_TYPE_CONFUSION",
                                    "an operation names a capability import this module                                      does not declare",
                                    source,
                                ));
                            };
                            // Every request was answered before the run started,
                            // so this cannot be absent for a module of the
                            // entry's own set; it can for a *cross-module* call,
                            // whose imports are that module's and are not this
                            // run's. Refusing says so rather than reaching for
                            // the wrong module's authority.
                            let Some(held) = self.imports.get(*index).cloned() else {
                                return Err(Trap::new(
                                    "RUNTIME_CAPABILITY_DENIED",
                                    alloc::format!("{declared} was requested and not granted"),
                                    source,
                                ));
                            };
                            (declared, held)
                        }
                        // A capability the module holds as a value, because an
                        // operation produced it. The engine has carried it since
                        // it arrived without looking at it, and does not look
                        // now: what it checks is that it *is* one, because a
                        // scalar in a capability position is a program the
                        // verifier should have refused.
                        CapabilitySource::Value(operand) => {
                            let value = self.operand(module, operand, values, source)?;
                            if !matches!(value, Value::Capability(_)) {
                                return Err(Trap::new(
                                    "RUNTIME_TYPE_CONFUSION",
                                    "a capability position is filled by a value that is not                                      a capability",
                                    source,
                                ));
                            }
                            // The interface is the instruction's own, which the
                            // verifier proved is this value's type.
                            let declared = instruction.unsafe_interface.as_deref().unwrap_or("");
                            (declared, value)
                        }
                    };
                    if first.is_none() {
                        first = Some(interface);
                    }
                    arguments.push(held);
                }
                let Some(interface) = first else {
                    return Err(Trap::new(
                        "RUNTIME_TYPE_CONFUSION",
                        "an operation names no capability at all",
                        source,
                    ));
                };
                for operand in operands {
                    arguments.push(self.operand(module, operand, values, source)?);
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
        Ok(Evaluated::Value(produced))
    }

    fn constant(&self, module: &Module, index: usize, source: SourceRef) -> Result<Value, Trap> {
        let Some(constant) = module.constants.get(index) else {
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

    fn arguments(
        &self,
        module: &Module,
        operands: &[Operand],
        values: &[Option<Value>],
        source: SourceRef,
    ) -> Result<Vec<Value>, Trap> {
        operands
            .iter()
            .map(|operand| self.operand(module, operand, values, source))
            .collect()
    }

    fn operand(
        &self,
        module: &Module,
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
            Operand::Constant(index) => self.constant(module, *index, source),
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
        Accounting::under(&module.header.resource_envelope, outcome)
    }

    /// The same, from the declared envelope alone.
    ///
    /// A bounded run releases the module it started in — the envelope is copied
    /// out before the first instruction and the module may be evicted at any
    /// point after — so the accounting has to be expressible without it.
    pub fn under(envelope: &tos_ir::ResourceEnvelope, outcome: &Outcome) -> Accounting {
        Accounting {
            fuel_used: outcome.fuel_used,
            fuel_limit: envelope.fuel,
            max_call_depth: outcome.max_call_depth,
            recursion_limit: envelope.recursion,
            tasks_started: outcome.tasks_started,
            task_limit: envelope.tasks,
            allocation_peak: outcome.allocation_peak,
            allocation_limit: envelope.allocation,
            cleanups_peak: outcome.cleanups_peak,
            cleanup_limit: envelope.cleanup,
            // Bootstrap serializes, so one context is reserved for the run.
            workers_reserved: 1,
            worker_limit: envelope.workers,
            shared_peak: outcome.shared_peak,
            shared_limit: envelope.shared,
            sync_peak: outcome.sync_peak,
            sync_limit: envelope.sync,
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
