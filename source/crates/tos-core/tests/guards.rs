// SPDX-License-Identifier: GPL-3.0-or-later
//! Guard lifetimes (ADR-0036), positive and negative.
//!
//! Each case is one rule of ADR-0036 section 5, checked through the whole
//! frontend rather than through the guard slice alone: a rule that only holds
//! when the slice is called directly is not a rule the language enforces.

use tos_core::{Checker, Diagnostic, Parser, SourceReader};

const BOOTSTRAP: &str = "module app.guards version 1.0 profile bootstrap; \
     resource [fuel: 1000, stack: 8KiB, allocation: 1KiB, tasks: 1, workers: 1, \
     sync: 2, shared: 0B, cleanup: 4, recursion: 4, imports: 0] ";

const FULL: &str = "module app.guards version 1.0 profile full; \
     resource [fuel: 1000, stack: 8KiB, allocation: 1KiB, tasks: 2, workers: 1, \
     sync: 2, shared: 0B, cleanup: 4, recursion: 4, imports: 0] ";

fn diagnostics(text: &str) -> Vec<Diagnostic> {
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .unwrap_or_else(|| panic!("the fixture must parse: {text}"));
    Checker::check(&source, &schema)
}

/// Every `E1402` finding's `operation` field, in report order.
fn guard_operations(text: &str) -> Vec<String> {
    diagnostics(text)
        .iter()
        .filter(|diagnostic| diagnostic.code() == "E1402_INVALID_GUARD_LIFETIME")
        .map(|diagnostic| {
            diagnostic
                .field("operation")
                .expect("every guard finding names its operation")
                .to_string()
        })
        .collect()
}

fn codes(text: &str) -> Vec<&'static str> {
    diagnostics(text).iter().map(Diagnostic::code).collect()
}

// ------------------------------------------------------------------ positives

#[test]
fn a_mutex_guard_taken_and_released_within_a_block_is_accepted() {
    let text =
        format!("{BOOTSTRAP} pub fn main(lock: Mutex<i32>) -> unit {{ let guard = lock.lock(); }}");
    assert_eq!(
        guard_operations(&text),
        Vec::<String>::new(),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn two_read_guards_of_one_rwlock_are_accepted() {
    let text = format!(
        "{BOOTSTRAP} pub fn main(lock: RwLock<i32>) -> unit {{ \
         let first = lock.read(); let second = lock.read(); }}"
    );
    assert_eq!(
        guard_operations(&text),
        Vec::<String>::new(),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn moving_a_guard_into_another_binding_is_not_a_release_and_is_accepted() {
    // ADR-0036 section 4: a move transfers the guard and the release obligation
    // with it. Releasing on every move would release at the first hand-off.
    let text = format!(
        "{BOOTSTRAP} pub fn main(lock: Mutex<i32>) -> unit {{ \
         let taken = lock.lock(); let held = taken; }}"
    );
    assert_eq!(
        guard_operations(&text),
        Vec::<String>::new(),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn passing_a_guard_to_a_helper_is_accepted() {
    let text = format!(
        "{BOOTSTRAP} fn use_guard(guard: MutexGuard<i32>) -> unit {{ }} \
         pub fn main(lock: Mutex<i32>) -> unit {{ use_guard(lock.lock()); }}"
    );
    assert_eq!(
        guard_operations(&text),
        Vec::<String>::new(),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn a_lock_operation_on_something_that_is_not_a_lock_yields_no_guard() {
    // ADR-0035 forbids inferring a guard from a spelling. A record with a field
    // called `lock` is not a synchronization object, and nothing here treats it
    // as one.
    let text = format!(
        "{BOOTSTRAP} pub record Door [lock: i32] \
         pub fn main(door: Door) -> i32 {{ return door.lock; }}"
    );
    assert_eq!(
        guard_operations(&text),
        Vec::<String>::new(),
        "{:?}",
        codes(&text)
    );
}

// ------------------------------------------------------------------ negatives

#[test]
fn returning_a_guard_is_reported() {
    let text = format!(
        "{BOOTSTRAP} pub fn main(lock: Mutex<i32>) -> MutexGuard<i32> {{ \
         let guard = lock.lock(); return guard; }}"
    );
    assert_eq!(guard_operations(&text), vec!["returned"]);
}

#[test]
fn returning_a_lock_operation_result_directly_is_reported() {
    let text = format!(
        "{BOOTSTRAP} pub fn main(lock: Mutex<i32>) -> MutexGuard<i32> {{ return lock.lock(); }}"
    );
    assert_eq!(guard_operations(&text), vec!["returned"]);
}

#[test]
fn placing_a_guard_into_a_record_is_reported() {
    let text = format!(
        "{BOOTSTRAP} pub record Holder [guard: MutexGuard<i32>] \
         pub fn main(lock: Mutex<i32>) -> unit {{ \
         let guard = lock.lock(); let held = Holder(guard: guard); }}"
    );
    assert_eq!(guard_operations(&text), vec!["aggregate"]);
}

#[test]
fn placing_a_guard_into_a_tuple_is_reported() {
    let text = format!(
        "{BOOTSTRAP} pub fn main(lock: Mutex<i32>) -> unit {{ \
         let guard = lock.lock(); let pair = (guard, 1i32); }}"
    );
    assert_eq!(guard_operations(&text), vec!["aggregate"]);
}

#[test]
fn capturing_a_guard_into_a_task_is_reported_as_a_guard_finding() {
    // ADR-0036 section 5 precedence: this is E1402 with operation=task_boundary
    // and NOT E1304_INVALID_TASK_CAPTURE. The capture codes keep their meaning
    // for every other non-transferable value.
    let text = format!(
        "{FULL} pub fn main(lock: Mutex<i32>) -> unit {{ \
         let guard = lock.lock(); let child = spawn async {{ let inner = guard; }}; }}"
    );
    let operations = guard_operations(&text);
    assert_eq!(operations, vec!["task_boundary"]);
    assert!(
        !codes(&text).contains(&"E1304_INVALID_TASK_CAPTURE"),
        "the capture code must not also fire: {:?}",
        codes(&text)
    );
}

#[test]
fn holding_a_guard_across_an_await_is_reported() {
    let text = format!(
        "{FULL} pub fn main(lock: Mutex<i32>, work: Task<i32>) -> unit {{ \
         let guard = lock.lock(); let done = await work; }}"
    );
    assert!(
        guard_operations(&text).contains(&String::from("held_across_await")),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn moving_the_lock_while_its_guard_is_live_is_reported() {
    let text = format!(
        "{BOOTSTRAP} fn consume(lock: Mutex<i32>) -> unit {{ }} \
         pub fn main(lock: Mutex<i32>) -> unit {{ \
         let guard = lock.lock(); consume(lock); }}"
    );
    assert_eq!(guard_operations(&text), vec!["lock_outlived"]);
}

#[test]
fn constructing_a_guard_is_a_forged_nonconstructible_type() {
    // ADR-0036 section 1: there is no constructor syntax for a guard, so
    // writing one is the nonconstructible-type error of ADR-0039.
    let text = format!("{BOOTSTRAP} pub fn main() -> unit {{ let guard = MutexGuard(0i32); }}");
    assert!(
        codes(&text).contains(&"E1213_NONCONSTRUCTIBLE_TYPE"),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn a_guard_constructor_with_the_wrong_arity_is_an_arity_finding() {
    let text = format!("{BOOTSTRAP} pub fn main(guard: ReadGuard<i32, i32>) -> unit {{ }}");
    assert!(
        codes(&text).contains(&"E1204_TYPE_ARGUMENT_ARITY"),
        "{:?}",
        codes(&text)
    );
}

// ------------------------------------------------------------------ structure

#[test]
fn every_guard_finding_names_its_guard_and_where_it_was_acquired() {
    let text = format!(
        "{BOOTSTRAP} pub fn main(lock: RwLock<i32>) -> WriteGuard<i32> {{ \
         let guard = lock.write(); return guard; }}"
    );
    let all = diagnostics(&text);
    let finding = all
        .iter()
        .find(|diagnostic| diagnostic.code() == "E1402_INVALID_GUARD_LIFETIME")
        .expect("a guard finding");
    assert_eq!(finding.field("operation"), Some("returned"));
    assert_eq!(finding.field("guard"), Some("WriteGuard"));
    // A lifetime finding that does not say where the lifetime started cannot be
    // acted on, so the acquisition offset is required.
    assert!(finding.field("acquired_at").is_some());
    assert_eq!(finding.stage(), tos_core::Stage::Type);
    assert_eq!(finding.severity(), tos_core::Severity::Error);
}
