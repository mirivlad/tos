// SPDX-License-Identifier: GPL-3.0-or-later
//! The supervisor's state machine, driven against a scripted system.
//!
//! **The state machine is not here.** It is in
//! `tests/vectors/supervision/init.tos`, and the policy it acts on is in
//! `tests/vectors/supervision/services.tos`. What is here is a host that
//! answers operations and writes down what crossed — so that the restart
//! window's boundaries can be put at exact ticks, which a real boot cannot do,
//! and so that a wrong answer is visible as a wrong *decision* rather than as a
//! number that happens to differ.
//!
//! The boundaries proved below are the ones a restart budget actually has:
//!
//! - a failure **inside** the window, with budget left — restart;
//! - the failure that **fills** the budget — terminal `FAILED`;
//! - a failure **past** the window — the old one expires, so the budget is not
//!   filled and the service restarts instead;
//! - and an event after the latch — no restart, ever.
//!
//! Each is one tick apart from the next, because a window is a `<` and an
//! implementation that wrote `<=` would pass a test whose ticks were far apart.

use tos_engine::{Handle, Reach, Request, System, Trap, Value};
use tos_ir::IntKind;

const ROOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/vectors/supervision"
);

fn unit(name: &str) -> (String, Vec<u8>) {
    let path = match name {
        "services.tos" => String::from("system/policy/services.tos"),
        _ => String::from("system/boot/init.tos"),
    };
    (
        path,
        std::fs::read(format!("{ROOT}/{name}")).expect("the vector is in the repository"),
    )
}

/// What the supervisor did, as the host saw it.
#[derive(Default)]
struct Watched {
    /// Every journal record the supervisor wrote, in order.
    journal: Vec<String>,
    /// How many children it created.
    created: usize,
}

/// A system that answers the supervisor and ends its children on a script.
///
/// It contains **no policy**: it does not decide what to restart, when to stop,
/// or what depends on what. What it decides is when a child ends and at which
/// tick, which is the nucleus's half of the story, and it answers every
/// operation the way the ABI does.
struct Scripted {
    /// The tick each successive ending is reported at, consumed in order. A
    /// script that runs out ends nothing more, which is how a run stops.
    ticks: Vec<u64>,
    /// Children created but not yet ended, oldest first: instance identities.
    live: Vec<u64>,
    next_instance: u64,
    next_handle: u64,
    watched: Watched,
}

impl Scripted {
    fn new(ticks: &[u64]) -> Scripted {
        Scripted {
            ticks: ticks.iter().copied().rev().collect(),
            live: Vec::new(),
            next_instance: 0,
            next_handle: 0,
            watched: Watched::default(),
        }
    }

    fn capability(&mut self) -> Value {
        self.next_handle += 1;
        Value::Capability(Handle::new(0x1000 + self.next_handle))
    }
}

impl System for Scripted {
    /// No device is reachable on this run, and saying so is the only honest
    /// answer: a device access here has reached hardware that does not exist.
    fn observe(
        &mut self,
        _access: tos_engine::Observe,
    ) -> Result<tos_engine::Value, tos_engine::Trap> {
        Err(tos_engine::Trap::new(
            "RUNTIME_DEVICE_UNREACHABLE",
            String::from("a device access was made on a run with no device to reach"),
            0,
        ))
    }

    fn granted(&mut self, request: Request<'_>) -> Option<Handle> {
        Some(Handle::new(match request.binding {
            "process" => 0x10,
            "memory" => 0x11,
            "journal" => 0x12,
            _ => return None,
        }))
    }

    fn reach(&mut self, call: Reach<'_>) -> Result<Value, Trap> {
        let text = call.arguments.iter().find_map(|value| match value {
            Value::Text(text) => Some(text.clone()),
            _ => None,
        });
        let ok = |produced: Value| {
            Ok(Value::Variant {
                index: 0,
                payload: vec![produced],
            })
        };
        match call.operation {
            "endpoint_send_text" => {
                self.watched
                    .journal
                    .push(text.expect("a journal record carries its text"));
                Ok(Value::Int(IntKind::I64, 0))
            }
            "endpoint_receive" | "endow_for_launch" | "capability_release" => {
                Ok(Value::Int(IntKind::I64, 0))
            }
            "launch_plan_create" | "launch_plan_seal" => {
                let produced = self.capability();
                ok(produced)
            }
            "process_create_funded" => {
                self.next_instance += 1;
                let instance = self.next_instance;
                self.live.push(instance);
                self.watched.created += 1;
                let control = self.capability();
                ok(Value::Aggregate(vec![
                    control,
                    Value::Int(IntKind::U64, i128::from(instance)),
                ]))
            }
            "process_wait_child" => {
                // A child ends at the next scripted tick. When the script is
                // spent, the wait can gain no member and is cancelled — which
                // is what ADR-0059's liveness rule does to a wait like this.
                let (Some(tick), Some(instance)) = (self.ticks.pop(), self.live.first().copied())
                else {
                    return Ok(Value::Variant {
                        index: 1,
                        payload: vec![Value::Int(IntKind::I64, -5)],
                    });
                };
                self.live.remove(0);
                let number = |value: u64| Value::Int(IntKind::U64, i128::from(value));
                let none = Value::Variant {
                    index: 0,
                    payload: vec![],
                };
                ok(Value::Aggregate(vec![
                    number(instance),
                    number(1),
                    // `ENDING_EXITED`: the child reached its own end.
                    number(1),
                    Value::Variant {
                        index: 1,
                        payload: vec![number(0)],
                    },
                    none.clone(),
                    none,
                    number(instance),
                    number(tick),
                ]))
            }
            other => Err(Trap::new(
                "RUNTIME_OPERATION_NOT_IMPLEMENTED",
                format!("the script answers no {other}"),
                call.source,
            )),
        }
    }
}

/// Runs the supervisor over the real policy, ending children at these ticks.
fn supervise(ticks: &[u64]) -> (i64, Watched) {
    let sources: Vec<(String, Vec<u8>)> = ["services.tos", "init.tos"].map(unit).into();
    let units: Vec<tos_pipeline::Unit<'_>> = sources
        .iter()
        .map(|(path, bytes)| tos_pipeline::Unit {
            path: path.as_str(),
            bytes: bytes.as_slice(),
        })
        .collect();
    let request = tos_pipeline::SetRequest {
        source_set: "supervision-test",
        units: &units,
        entry_path: "system/boot/init.tos",
        entry: "main",
    };
    let mut trace = Silent;
    let mut system = Scripted::new(ticks);
    let prepared = tos_pipeline::prepare_from_source(
        &request,
        &mut trace,
        tos_pipeline::ResidencyLimits {
            modules: 4,
            bytes: 64 * 1024 * 1024,
        },
    )
    .expect("the supervision set prepares");
    let tos_pipeline::Preparation::Ready(mut prepared) = prepared else {
        panic!("the supervision set was refused");
    };
    let outcome = prepared
        .run(Vec::new(), &mut system)
        .expect("the entry takes no arguments and every request is granted")
        .expect("the supervisor completes");
    let Value::Int(_, produced) = outcome.value else {
        panic!("the supervisor returns an i64");
    };
    (produced as i64, system.watched)
}

struct Silent;

impl tos_pipeline::Trace for Silent {
    fn entering(&mut self, _stage: tos_pipeline::PipelineStage) {}
}

/// How many times a decision appears in the journal.
fn decisions(watched: &Watched, decision: &str) -> usize {
    watched
        .journal
        .iter()
        .filter(|record| *record == decision)
        .count()
}

/// The window a policy states, as the exact number the boundary turns on.
///
/// A window is a `<`. An implementation that wrote `<=` would keep a failure
/// that is exactly a window old, and the two scripts below differ by one tick
/// across that line — so the pair fails for that mistake and for no other.
const WINDOW: u64 = 1_000_000;

#[test]
fn a_failure_exactly_a_window_old_has_expired() {
    let (value, watched) = supervise(&[0, 1, 2, WINDOW]);
    assert_eq!(
        decisions(&watched, "error.supervisor.state.failed"),
        0,
        "a failure exactly a window old was kept: {:?}",
        watched.journal
    );
    assert_eq!(
        decisions(&watched, "error.supervisor.policy.budget-exhausted"),
        0
    );
    // 1000 + created x10 + latched x100 + blocked.
    assert_eq!(value, 1071);
}

#[test]
fn a_failure_one_tick_inside_the_window_still_counts() {
    let (value, watched) = supervise(&[0, 1, 2, WINDOW - 1]);
    assert_eq!(
        decisions(&watched, "error.supervisor.state.failed"),
        1,
        "a failure one tick inside the window was dropped: {:?}",
        watched.journal
    );
    assert_eq!(
        decisions(&watched, "error.supervisor.policy.budget-exhausted"),
        1
    );
    assert_eq!(value, 1161);
}

#[test]
fn a_failure_with_budget_left_restarts() {
    let (_, watched) = supervise(&[500]);
    assert_eq!(decisions(&watched, "info.supervisor.observed.ending"), 1);
    assert_eq!(
        decisions(&watched, "warn.supervisor.inferred.own-failure"),
        1
    );
    assert_eq!(
        decisions(&watched, "info.supervisor.policy.restart-permitted"),
        1
    );
    assert_eq!(
        decisions(&watched, "error.supervisor.policy.budget-exhausted"),
        0
    );
}

#[test]
fn a_window_narrower_than_the_gap_never_accumulates() {
    // Service 2's window is one tick and every gap is larger, so each of its
    // failures is alone in its window however many there are. Ten endings and
    // it is still being restarted, while the two wide-window services latched.
    //
    // This is the test that fails if failures are counted rather than dated.
    let (_, watched) = supervise(&[500, 501, 502, 503, 504, 505, 506, 507, 508, 509]);
    assert_eq!(decisions(&watched, "error.supervisor.state.failed"), 2);
    let latched_at = watched
        .journal
        .iter()
        .rposition(|record| record == "error.supervisor.state.failed")
        .expect("something latched");
    assert!(
        watched.journal[latched_at..]
            .iter()
            .any(|record| record == "info.supervisor.result.created"),
        "nothing was restarted after the last latch: {:?}",
        watched.journal
    );
}

#[test]
fn blocked_is_a_statement_about_now_and_is_left_when_the_dependency_runs() {
    // Service 0 requires service 2 and is considered **before** it, so in the
    // first round its dependency is not running and it is BLOCKED. It is not
    // ended, not failed, and not skipped: a later round finds the dependency
    // running and starts it.
    let (_, watched) = supervise(&[500, 501, 502, 503, 504, 505, 506, 507, 508, 509]);
    assert_eq!(
        watched.journal.iter().take(3).collect::<Vec<_>>(),
        vec![
            "warn.supervisor.policy.dependency-unavailable",
            "system/boot/worker.tos",
            "warn.supervisor.state.blocked",
        ]
    );
    // Blocking consumed no restart budget: nothing about that service failed,
    // so no failure was inferred before it eventually started.
    let first_failure = watched
        .journal
        .iter()
        .position(|record| record == "warn.supervisor.inferred.own-failure")
        .expect("a failure was inferred later");
    assert!(2 < first_failure);
    assert!(
        watched.journal[..first_failure]
            .iter()
            .filter(|record| *record == "info.supervisor.result.created")
            .count()
            >= 2
    );
}

#[test]
fn a_latched_service_is_not_started_by_an_event_that_would_have_started_it() {
    let (_, watched) = supervise(&[500, 501, 502, 503, 504, 505, 506, 507, 508, 509]);
    let last_latch = watched
        .journal
        .iter()
        .rposition(|record| record == "error.supervisor.state.failed")
        .expect("something latched");
    let after = &watched.journal[last_latch..];
    assert!(
        after
            .iter()
            .filter(|record| *record == "warn.supervisor.policy.latched-no-start")
            .count()
            >= 2,
        "a latched service was never asked again: {after:?}"
    );
    // And the dependency of the latched service goes on coming back — service 2
    // is restarted repeatedly after the latch — without un-latching anything.
    // Terminal dominates dependency recovery.
    assert!(after
        .iter()
        .any(|record| record == "info.supervisor.result.created"));
    assert_eq!(
        after
            .iter()
            .filter(|record| *record == "error.supervisor.state.failed")
            .count(),
        1,
        "a service latched twice, so the latch is not a latch"
    );
}

#[test]
fn the_policy_decides_what_exists_and_the_supervisor_reads_it() {
    // Nothing ends. The supervisor starts what the policy names and blocks what
    // the policy says depends on something not yet running — three services,
    // because the policy says three, and the supervisor's text names none.
    let (_, watched) = supervise(&[]);
    // Two in the first round, and the blocked one in the second — because by
    // then its dependency is running. BLOCKED was a statement about that
    // moment, and nothing but the moment changed.
    assert_eq!(watched.created, 3);
    assert_eq!(decisions(&watched, "warn.supervisor.state.blocked"), 1);
    assert!(watched
        .journal
        .contains(&String::from("system/boot/worker.tos")));
    // With nothing running, the wait can gain no member: the supervisor is told
    // so rather than spinning.
    assert!(decisions(&watched, "info.supervisor.observed.no-ending") > 0);
}

#[test]
fn the_decisions_appear_in_the_order_the_machine_makes_them() {
    let (_, watched) = supervise(&[500]);
    let first: Vec<&str> = watched.journal.iter().map(String::as_str).take(7).collect();
    assert_eq!(
        first,
        vec![
            "warn.supervisor.policy.dependency-unavailable",
            "system/boot/worker.tos",
            "warn.supervisor.state.blocked",
            "info.supervisor.policy.start-permitted",
            "system/boot/worker.tos",
            "info.supervisor.action.create",
            "info.supervisor.result.created",
        ]
    );
    // And an ending is observed before anything is inferred from it, which is
    // the difference between a nucleus fact and a supervisor conclusion.
    let observed = watched
        .journal
        .iter()
        .position(|record| record == "info.supervisor.observed.ending")
        .expect("an ending was observed");
    let inferred = watched
        .journal
        .iter()
        .position(|record| record == "warn.supervisor.inferred.own-failure")
        .expect("a failure was inferred");
    assert!(observed < inferred);
}

#[test]
fn the_supervisor_dates_failures_by_the_field_the_record_declares() {
    // `ChildEnding` is matched on by field: the supervisor compares
    // `child_instance` to decide which service ended and reads `ended_tick` to
    // date the failure. A host that built the record's fields in another order
    // would make it date failures by an instance identity, and the window would
    // stop meaning anything.
    //
    // Two runs differing only in the tick field differ in their outcome, which
    // is what says the tick is what was read.
    let (early, _) = supervise(&[0, 1, 2, WINDOW - 1]);
    let (late, _) = supervise(&[0, 1, 2, WINDOW]);
    assert_ne!(early, late);
}
