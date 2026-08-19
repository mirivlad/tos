// SPDX-License-Identifier: GPL-3.0-or-later
//! A module calls an operation, the engine leaves, and something answers.
//!
//! `interface_schema.rs` proves the *artifact* names the interface it reaches
//! and that a verifier refuses one that lies about it. This file is the other
//! half of ADR-0060: what happens when that artifact runs.
//!
//! The engine performs no operation of any interface and knows what none of
//! them mean. It carries a capability it cannot read, names an interface and an
//! operation, hands over the arguments, and takes back a value it does not
//! check. Everything below is a property of that boundary, and the host here is
//! a recorder rather than a system — which is the point, because a boundary that
//! only works against the real system is not a boundary.

use tos_engine::{run, Handle, Reach, System, Trap, Unreachable, Value};
use tos_ir::IntKind;
use tos_verifier::{verify, Limits, ResolutionSnapshot};

/// A module reaching the endpoint interface, with a declared fuel budget and a
/// body the caller chooses.
///
/// `profile full`, and it has to be: `docs/42` §3 forbids `extern` in Bootstrap.
fn module(fuel: u32, body: &str) -> String {
    format!(
        "module system.test.reach version 1.0 profile full;
import capability system.ipc.Endpoint as endpoint;

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
    imports: 1
]

extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [endpoint];
extern fn endpoint_receive(cap: system.ipc.Endpoint) -> i64 uses [endpoint];

pub fn main(cap: system.ipc.Endpoint) -> i64 uses [endpoint] {{
{body}
}}
"
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
/// It performs nothing. That is deliberate: every property below has to hold
/// for *any* host, and one that did real work would let a passing test be a
/// statement about the system rather than about the engine.
struct Recorder {
    /// What was called, in the order it was called: interface, then operation.
    calls: Vec<(String, String)>,
    /// The capability handle each call carried, as the host reads it.
    handles: Vec<u64>,
    /// The answers, consumed in order. A host that runs out answers zero,
    /// which no test below relies on.
    answers: Vec<i64>,
}

impl Recorder {
    fn answering(answers: &[i64]) -> Recorder {
        Recorder {
            calls: Vec::new(),
            handles: Vec::new(),
            answers: answers.iter().copied().rev().collect(),
        }
    }
}

impl System for Recorder {
    fn reach(&mut self, call: Reach<'_>) -> Result<Value, Trap> {
        self.calls
            .push((call.interface.to_string(), call.operation.to_string()));
        // ADR-0056: the capability is the first argument of every operation.
        // The host is the only thing in the system that may read it, and this
        // is the read — the engine carried the value here without looking.
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

fn outcome(
    text: &str,
    handle: u64,
    system: &mut dyn System,
) -> Result<tos_engine::Outcome, tos_engine::Trap> {
    let module = lower(text);
    let receipt = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("a module reaching an interface it imported and declared verifies");
    run(
        &module,
        &receipt,
        "main",
        vec![Value::Capability(Handle::new(handle))],
        system,
    )
    .expect("the entry takes exactly the one capability this run supplies")
}

#[test]
fn the_engine_leaves_for_the_interface_and_comes_back_with_what_it_was_told() {
    let mut host = Recorder::answering(&[-1]);
    let result = outcome(
        &module(1024, "    return endpoint_send(cap, 8u64);"),
        0x1_0000_0000,
        &mut host,
    )
    .expect("the run completes");

    // It left exactly once, for the interface and operation the source named.
    assert_eq!(
        host.calls,
        vec![(
            String::from("system.ipc.Endpoint"),
            String::from("endpoint_send")
        )]
    );
    // Carrying the capability the run was given, unchanged. The engine has no
    // operation that could have produced or altered this number.
    assert_eq!(host.handles, vec![0x1_0000_0000]);

    // And the value the module returns is the host's answer, not the engine's:
    // -1 is `E_NO_CAPABILITY`, which nothing in this module computes.
    assert_eq!(result.value, Value::Int(IntKind::I64, -1));
}

#[test]
fn the_order_of_effects_is_the_same_and_the_values_are_not() {
    // ADR-0060's load-bearing sentence, as a test: "the order of effects is
    // deterministic and the verifier proves it. The values effects return are
    // not, and nothing may depend on their being reproducible."
    let text = module(
        1024,
        "    let sent: i64 = endpoint_send(cap, 8u64);\n    \
         let received: i64 = endpoint_receive(cap);\n    \
         return sent + received;",
    );

    let mut monday = Recorder::answering(&[1, 2]);
    let first = outcome(&text, 0x7, &mut monday).expect("the run completes");
    let mut tuesday = Recorder::answering(&[100, 200]);
    let second = outcome(&text, 0x7, &mut tuesday).expect("the run completes");

    // Same calls, same order, two runs of one module over one input. That is
    // the half that is a promise.
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

    // Different answers, and therefore a different result. That is the half
    // that is not, and a module written to expect otherwise is wrong about the
    // world rather than about this contract.
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
    // depends on what the lowerer folds, and a hard-coded number would be a
    // test of this month's constant folding.
    let body = "    let length: u64 = width(7u64);\n    return endpoint_send(cap, length);";
    let text = |fuel: u32| {
        module(fuel, body).replace(
            "pub fn main",
            "fn width(n: u64) -> u64 {\n    return n + 1u64;\n}\n\npub fn main",
        )
    };

    let reached = |fuel: u32| {
        let mut host = Recorder::answering(&[0]);
        let result = outcome(&text(fuel), 0x7, &mut host);
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
    // Not a stub that returns zero. A module that reaches an interface on a run
    // that has no system has reached something that does not exist, and the
    // only honest answer is to say so — a zero would be indistinguishable from
    // an operation that succeeded.
    let trap = outcome(
        &module(1024, "    return endpoint_send(cap, 8u64);"),
        0x7,
        &mut Unreachable,
    )
    .expect_err("there is nothing to reach");
    assert_eq!(trap.code, "RUNTIME_INTERFACE_UNREACHABLE");
    assert!(trap.detail.contains("system.ipc.Endpoint"), "{trap:?}");
}

#[test]
fn a_capability_does_not_appear_in_anything_a_reader_gets() {
    // `docs/42` §2 admits authority into provenance by interface path and keeps
    // "the concrete secret/handle representation" out. Two things could leak it
    // without anyone noticing: the value renderer, which writes the boot log,
    // and `Debug`, which writes every test failure and every diagnostic.
    let held = Value::Capability(Handle::new(0xdead_beef));
    assert_eq!(tos_pipeline::render::value(&held), "capability");
    assert_eq!(format!("{:?}", Handle::new(0xdead_beef)), "capability");
    assert!(
        !format!("{held:?}").contains("beef"),
        "a capability's handle reached a diagnostic: {held:?}"
    );
}
