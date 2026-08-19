// SPDX-License-Identifier: GPL-3.0-or-later
//! A module calls an operation, the engine leaves, and something answers.
//!
//! `interface_schema.rs` proves the *artifact* names the interface it reaches
//! and that a verifier refuses one that lies about it. This file is the other
//! half of ADR-0060, and the whole of ADR-0061's engine side: what happens when
//! that artifact runs, and where the authority it runs under comes from.
//!
//! The engine performs no operation of any interface and knows what none of them
//! mean. It carries a capability it cannot read, names an interface and an
//! operation, hands over the arguments, and takes back a value it does not
//! check. The host here is a recorder rather than a system — which is the point,
//! because a boundary that only works against the real system is not a boundary.
//!
//! **Authority arrives through `import capability`, not through `main`.** That
//! is ADR-0061's decision and it is visible in every module below: the entry
//! takes no parameters, and the name a request was bound to is what the
//! operation is performed on.

use std::collections::BTreeMap;

use tos_engine::{run, Handle, Reach, Refusal, Request, System, Trap, Unreachable, Value};
use tos_ir::{IntKind, Op};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

/// A module reaching the endpoint interface, with a declared fuel budget, a
/// chosen set of capability requests, and a body.
///
/// `profile full`, and it has to be: `docs/42` §3 forbids `extern` in Bootstrap.
fn module(fuel: u32, imports: &str, count: usize, body: &str) -> String {
    format!(
        "module system.test.reach version 1.0 profile full;
{imports}

resource [
    fuel: {fuel},
    stack: 4KiB,
    allocation: 1KiB,
    tasks: 1,
    workers: 1,
    sync: 0,
    shared: 0B,
    cleanup: 0,
    recursion: 4,
    imports: {count}
]

extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [endpoint];
extern fn endpoint_receive(cap: system.ipc.Endpoint) -> i64 uses [endpoint];

pub fn main() -> i64 uses [endpoint] {{
{body}
}}
"
    )
}

/// The ordinary one-request module.
fn one(fuel: u32, body: &str) -> String {
    module(
        fuel,
        "import capability system.ipc.Endpoint as endpoint;",
        1,
        body,
    )
}

fn lower(text: &str) -> tos_ir::Module {
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    let diagnostics = tos_core::Checker::check(&source, &schema);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.severity() == tos_core::Severity::Error),
        "a module reaching a declared operation checks clean: {diagnostics:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("interface-reach-test"),
            path: String::from("system/test/reach.tos"),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

/// A host that writes down what it was asked and answers from a script.
///
/// It performs nothing. That is deliberate: every property below has to hold for
/// *any* host, and one that did real work would let a passing test be a
/// statement about the system rather than about the engine.
struct Recorder {
    /// Every request the engine put, in order: interface, then binding.
    requests: Vec<(String, String)>,
    /// What this host grants, by binding. A binding it does not know is denied,
    /// which is a policy and not an omission.
    grants: BTreeMap<String, u64>,
    /// What was called, in the order it was called: interface, then operation.
    calls: Vec<(String, String)>,
    /// The capability handle each call carried, as the host reads it.
    handles: Vec<u64>,
    /// The answers, consumed in order. A host that runs out answers zero, which
    /// no test below relies on.
    answers: Vec<i64>,
}

impl Recorder {
    /// A host granting one endpoint to the binding `endpoint`.
    fn answering(answers: &[i64]) -> Recorder {
        Recorder::granting(&[("endpoint", 0x1_0000_0000)], answers)
    }

    fn granting(grants: &[(&str, u64)], answers: &[i64]) -> Recorder {
        Recorder {
            requests: Vec::new(),
            grants: grants
                .iter()
                .map(|(name, handle)| (String::from(*name), *handle))
                .collect(),
            calls: Vec::new(),
            handles: Vec::new(),
            answers: answers.iter().copied().rev().collect(),
        }
    }
}

impl System for Recorder {
    fn granted(&mut self, request: Request<'_>) -> Option<Handle> {
        self.requests
            .push((request.interface.to_string(), request.binding.to_string()));
        self.grants.get(request.binding).copied().map(Handle::new)
    }

    fn reach(&mut self, call: Reach<'_>) -> Result<Value, Trap> {
        self.calls
            .push((call.interface.to_string(), call.operation.to_string()));
        // ADR-0056: the capability is the first argument of every operation. The
        // host is the only thing in the system that may read it, and this is the
        // read — the engine carried the value here without looking.
        match call.arguments.first() {
            Some(Value::Capability(handle)) => self.handles.push(handle.get()),
            other => panic!("an operation reached without a capability first: {other:?}"),
        }
        Ok(Value::Int(
            IntKind::I64,
            i128::from(self.answers.pop().unwrap_or(0)),
        ))
    }
}

fn started(
    text: &str,
    system: &mut dyn System,
) -> Result<Result<tos_engine::Outcome, Trap>, Refusal> {
    let module = lower(text);
    let receipt = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("a module reaching an interface it imported and declared verifies");
    run(&module, &receipt, "main", Vec::new(), system)
}

fn outcome(text: &str, system: &mut dyn System) -> Result<tos_engine::Outcome, Trap> {
    started(text, system).expect("the entry takes no arguments and every request is granted")
}

#[test]
fn the_capability_is_the_import_and_is_nowhere_in_the_artifact() {
    // ADR-0061's surface, read off the IR. The operation is `Op::Capability`,
    // which names *which request* it is performed under; there is no operand
    // anywhere holding a capability, because the artifact never contains one.
    // That is the strongest form of `docs/42` §2's rule about handle
    // representation: it cannot leak from a place it does not occupy.
    let module = lower(&one(1024, "    return endpoint_send(endpoint, 8u64);"));
    let entry = module
        .functions
        .iter()
        .find(|function| function.signature.name == "main")
        .expect("the entry is in the artifact");
    assert!(
        entry.signature.parameters.is_empty(),
        "authority arrived through a parameter, not through an import"
    );

    let reaching: Vec<_> = entry
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.op {
            Op::Capability { import, right, .. } => {
                Some((*import, right.clone(), instruction.unsafe_interface.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        reaching,
        vec![(
            0,
            String::from("endpoint_send"),
            Some(String::from("system.ipc.Endpoint"))
        )]
    );
    assert_eq!(module.capability_imports[0].binding, "endpoint");
    assert_eq!(
        module.capability_imports[0].interface,
        "system.ipc.Endpoint"
    );
}

#[test]
fn the_engine_asks_for_each_request_by_name_before_anything_runs() {
    let mut host = Recorder::answering(&[-1]);
    let result = outcome(
        &one(1024, "    return endpoint_send(endpoint, 8u64);"),
        &mut host,
    )
    .expect("the run completes");

    // The request was put by interface *and* by the name the module bound it to
    // (ADR-0061), which is what a host can hold policy against.
    assert_eq!(
        host.requests,
        vec![(
            String::from("system.ipc.Endpoint"),
            String::from("endpoint")
        )]
    );
    // It left exactly once, for the interface and operation the source named.
    assert_eq!(
        host.calls,
        vec![(
            String::from("system.ipc.Endpoint"),
            String::from("endpoint_send")
        )]
    );
    // Carrying the capability that request was answered with, unchanged. The
    // engine has no operation that could have produced or altered this number.
    assert_eq!(host.handles, vec![0x1_0000_0000]);
    // And the value the module returns is the host's answer, not the engine's:
    // -1 is `E_NO_CAPABILITY`, which nothing in this module computes.
    assert_eq!(result.value, Value::Int(IntKind::I64, -1));
}

#[test]
fn two_requests_of_one_interface_reach_two_different_objects() {
    // The case that decides the key. Both imports are `system.ipc.Endpoint`, so
    // a rule matching on interface could not tell them apart and a rule matching
    // on position would swap them when the two lines are swapped. The names do
    // neither.
    // Written out rather than derived from `module`, because the effect sets
    // differ: an `extern` names exactly one capability effect — it declares one
    // operation of one interface — while the function that calls both names
    // both bindings. The interface is what has to match, and both bindings name
    // the same one.
    let pair = |first: &str, second: &str| {
        format!(
            "module system.test.reach version 1.0 profile full;
import capability system.ipc.Endpoint as {first};
import capability system.ipc.Endpoint as {second};

resource [
    fuel: 1024, stack: 4KiB, allocation: 1KiB, tasks: 1, workers: 1,
    sync: 0, shared: 0B, cleanup: 0, recursion: 4, imports: 2
]

extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [input];
extern fn endpoint_receive(cap: system.ipc.Endpoint) -> i64 uses [input];

pub fn main() -> i64 uses [input, output] {{
    let taken: i64 = endpoint_receive(input);
    return endpoint_send(output, 8u64);
}}
"
        )
    };
    let text = pair("input", "output");

    let mut host = Recorder::granting(&[("input", 0x11), ("output", 0x22)], &[0, 0]);
    outcome(&text, &mut host).expect("the run completes");
    assert_eq!(host.handles, vec![0x11, 0x22]);

    // And swapping the two source lines changes nothing about which name gets
    // which object — the property a positional key could not have.
    let swapped = pair("output", "input");
    assert_ne!(swapped, text, "the two import lines were not reordered");
    let mut reordered = Recorder::granting(&[("input", 0x11), ("output", 0x22)], &[0, 0]);
    outcome(&swapped, &mut reordered).expect("the run completes");
    assert_eq!(reordered.handles, vec![0x11, 0x22]);
}

#[test]
fn a_request_nothing_answers_is_refused_at_startup_and_names_itself() {
    // `docs/42` §2 and `SYSTEM_INTERFACE_V1` §10.3: a denied request fails at
    // startup and never reaches the call. `PROCESS_IDENTITY_V1` §7.3 asks the
    // failure to name what was denied — by the binding, which is what the source
    // calls it.
    let mut host = Recorder::granting(&[], &[0]);
    let refusal = started(
        &one(1024, "    return endpoint_send(endpoint, 8u64);"),
        &mut host,
    )
    .expect_err("a request nothing answered is a refusal");
    assert_eq!(
        refusal,
        Refusal::CapabilityDenied {
            binding: String::from("endpoint"),
            interface: String::from("system.ipc.Endpoint"),
        }
    );
    // Never reached the call, and never reached one instruction: the host was
    // asked for the grant and then nothing else happened.
    assert!(host.calls.is_empty(), "{:?}", host.calls);
}

#[test]
fn the_order_of_effects_is_the_same_and_the_values_are_not() {
    // ADR-0060's load-bearing sentence, as a test: "the order of effects is
    // deterministic and the verifier proves it. The values effects return are
    // not, and nothing may depend on their being reproducible."
    let text = one(
        1024,
        "    let sent: i64 = endpoint_send(endpoint, 8u64);\n    \
         let received: i64 = endpoint_receive(endpoint);\n    \
         return sent + received;",
    );

    let mut monday = Recorder::answering(&[1, 2]);
    let first = outcome(&text, &mut monday).expect("the run completes");
    let mut tuesday = Recorder::answering(&[100, 200]);
    let second = outcome(&text, &mut tuesday).expect("the run completes");

    // Same calls, same order, two runs of one module over one input. That is the
    // half that is a promise.
    assert_eq!(monday.calls, tuesday.calls);
    assert_eq!(
        monday.calls,
        vec![
            (
                String::from("system.ipc.Endpoint"),
                String::from("endpoint_send")
            ),
            (
                String::from("system.ipc.Endpoint"),
                String::from("endpoint_receive")
            ),
        ]
    );

    // Different answers, and therefore a different result. That is the half that
    // is not, and a module written to expect otherwise is wrong about the world
    // rather than about this contract.
    assert_eq!(first.value, Value::Int(IntKind::I64, 3));
    assert_eq!(second.value, Value::Int(IntKind::I64, 300));
    assert_eq!(first.fuel_used, second.fuel_used);
}

#[test]
fn the_call_is_charged_before_it_is_made() {
    // `SYSTEM_INTERFACE_V1` §6: an `extern` call is charged like any other call
    // before it is made, "so a module cannot exceed its declared budget by
    // leaving the process". A budget one unit short of the call has to stop the
    // module *without the host being asked anything at all* — a host that was
    // asked has already been reached, whatever is charged afterwards.
    //
    // The budget is found rather than guessed. Which instruction the call is
    // depends on what the lowerer folds, and a hard-coded number would be a test
    // of this month's constant folding.
    let body = "    let length: u64 = width(7u64);\n    return endpoint_send(endpoint, length);";
    let text = |fuel: u32| {
        one(fuel, body).replace(
            "pub fn main",
            "fn width(n: u64) -> u64 {\n    return n + 1u64;\n}\n\npub fn main",
        )
    };

    let reached = |fuel: u32| {
        let mut host = Recorder::answering(&[0]);
        let result = outcome(&text(fuel), &mut host);
        (host.calls.len(), result.err().map(|trap| trap.code))
    };

    // The smallest budget under which the module gets out at all.
    let enough = (1..64u32)
        .find(|fuel| reached(*fuel).0 > 0)
        .expect("some budget under 64 lets this module make its one call");
    // And it did real work before it: a module stopped at its first instruction
    // would prove that nothing runs without fuel, which is a different claim.
    assert!(
        enough > 3,
        "the call is the module's first work, so this proves nothing about order"
    );

    let (calls, code) = reached(enough - 1);
    assert_eq!(code, Some("RUNTIME_FUEL_EXHAUSTED"));
    assert_eq!(
        calls, 0,
        "the host was reached by a module that could not afford to reach it"
    );
}

#[test]
fn a_run_with_no_system_reaches_nothing() {
    // Not a stub that grants zero. A module that requests a capability on a run
    // with no system has requested it of something that does not exist, and the
    // only honest answer is the denial — a zero handle would be
    // indistinguishable from a grant.
    let refusal = started(
        &one(1024, "    return endpoint_send(endpoint, 8u64);"),
        &mut Unreachable,
    )
    .expect_err("there is nothing to grant");
    assert_eq!(
        refusal,
        Refusal::CapabilityDenied {
            binding: String::from("endpoint"),
            interface: String::from("system.ipc.Endpoint"),
        }
    );
}

#[test]
fn a_capability_does_not_appear_in_anything_a_reader_gets() {
    // `docs/42` §2 admits authority into provenance by interface path and keeps
    // "the concrete secret/handle representation" out. Two things could leak it
    // without anyone noticing: the value renderer, which writes the boot log, and
    // `Debug`, which writes every test failure and every diagnostic.
    let held = Value::Capability(Handle::new(0xdead_beef));
    assert_eq!(tos_pipeline::render::value(&held), "capability");
    assert_eq!(format!("{:?}", Handle::new(0xdead_beef)), "capability");
    assert!(
        !format!("{held:?}").contains("beef"),
        "a capability's handle reached a diagnostic: {held:?}"
    );
}
