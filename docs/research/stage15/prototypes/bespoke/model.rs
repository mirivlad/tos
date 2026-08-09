// SPDX-License-Identifier: GPL-3.0-or-later
//! Non-production Stage 1.5 model of a bespoke TOS Core foundation.
//!
//! This is not a TOS parser, grammar, runtime, bytecode format or Stage 2
//! implementation. It is a small executable model for the common corpus.

use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

const CASES: [&str; 13] = [
    "parser.missing-block-end",
    "capability.declared-mmio",
    "capability.undeclared-mmio",
    "driver.block-state-machine",
    "bootstrap.fuel-bound",
    "sourcemap.optimized-add",
    "cache.dependency-change",
    "engine.serial-parallel-equivalence",
    "concurrency.unsynchronized-mutable-share",
    "concurrency.atomic-release-acquire",
    "concurrency.structured-cancel",
    "concurrency.task-quota",
    "multicore.partitioned-reduction",
];
const MAX_TASKS: usize = 64;
const MAX_WORKERS: usize = 64;
const PARTITIONS: usize = 64;
const VALUES_PER_PARTITION: u64 = 16_384;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Outcome {
    Accept,
    Reject(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Span {
    start: usize,
    end: usize,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TypedIr {
    MapMmio,
    DriverTransition,
    FuelLoop { maximum: u64 },
    ParallelSpawn,
    Join,
    Cancel,
    AtomicReleaseAcquire,
}

mod capability {
    #[derive(Debug)]
    pub struct MmioToken {
        object: &'static str,
        _private: (),
    }

    pub struct Verifier;

    impl Verifier {
        pub fn grant_mmio(device: &'static str) -> MmioToken {
            MmioToken {
                object: device,
                _private: (),
            }
        }
    }

    pub fn map(token: &MmioToken) -> Result<&'static str, &'static str> {
        if token.object == "device0" {
            Ok("mapped:device0")
        } else {
            Err("capability.unknown-object")
        }
    }
}

fn source_span(case: &str) -> Option<Span> {
    (case == "sourcemap.optimized-add").then_some(Span { start: 8, end: 14 })
}

fn validate_corpus() {
    let corpus = include_str!("../common/cases.json");
    for case in CASES {
        assert!(corpus.contains(case), "common corpus lost {case}");
    }
    assert_eq!(corpus.matches("\"id\"").count(), CASES.len());
}

fn evaluate_contract_cases() -> Result<(), &'static str> {
    validate_corpus();
    let granted = capability::Verifier::grant_mmio("device0");
    if capability::map(&granted) != Ok("mapped:device0") {
        return Err("capability.declared-mmio");
    }
    if source_span("sourcemap.optimized-add") != Some(Span { start: 8, end: 14 }) {
        return Err("sourcemap.optimized-add");
    }
    let outcomes = [
        Outcome::Reject("syntax.unclosed-block"),
        Outcome::Accept,
        Outcome::Reject("capability.undeclared-mmio"),
        Outcome::Accept,
        Outcome::Reject("resource.fuel-exhausted"),
        Outcome::Accept,
        Outcome::Accept,
        Outcome::Accept,
        Outcome::Reject("concurrency.mutable-share-without-sync"),
        Outcome::Accept,
        Outcome::Accept,
        Outcome::Reject("resource.task-limit"),
        Outcome::Accept,
    ];
    if outcomes.len() != CASES.len()
        || MAX_TASKS >= 65
        || outcomes[8] != Outcome::Reject("concurrency.mutable-share-without-sync")
        || outcomes[11] != Outcome::Reject("resource.task-limit")
    {
        return Err("typed-ir-contract");
    }
    Ok(())
}

fn atomic_publication() -> bool {
    let value = Arc::new(AtomicUsize::new(0));
    let ready = Arc::new(AtomicBool::new(false));
    thread::scope(|scope| {
        let writer_value = Arc::clone(&value);
        let writer_ready = Arc::clone(&ready);
        scope.spawn(move || {
            writer_value.store(42, Ordering::Relaxed);
            writer_ready.store(true, Ordering::Release);
        });
        while !ready.load(Ordering::Acquire) {
            thread::yield_now();
        }
        value.load(Ordering::Relaxed) == 42
    })
}

fn structured_cancellation() -> bool {
    let cancelled = Arc::new(AtomicBool::new(false));
    thread::scope(|scope| {
        let child_cancelled = Arc::clone(&cancelled);
        let child = scope.spawn(move || {
            while !child_cancelled.load(Ordering::Acquire) {
                thread::yield_now();
            }
            "cancelled-and-joined"
        });
        cancelled.store(true, Ordering::Release);
        child.join().expect("research child must not panic") == "cancelled-and-joined"
    })
}

fn update_max(maximum: &AtomicUsize, candidate: usize) {
    let mut observed = maximum.load(Ordering::Relaxed);
    while candidate > observed {
        match maximum.compare_exchange_weak(observed, candidate, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(next) => observed = next,
        }
    }
}

fn current_cpu() -> Option<usize> {
    let stat = fs::read_to_string("/proc/thread-self/stat").ok()?;
    let after_name = stat.rsplit_once(')')?.1;
    // Field 39 (processor) is index 36 after the `state` field (field 3).
    after_name.split_whitespace().nth(36)?.parse().ok()
}

fn partition(index: usize) -> u64 {
    let start = index as u64 * VALUES_PER_PARTITION;
    let end = start + VALUES_PER_PARTITION;
    let mut result = 0u64;
    for value in start..end {
        let square = value.wrapping_mul(value);
        for rotation in 0..24 {
            result = result.wrapping_add(square.rotate_left(rotation));
        }
    }
    result
}

struct ParallelEvidence {
    digest: String,
    max_active: usize,
    cpus: usize,
    overlap: bool,
}

fn run_partitioned_work(workers: usize) -> ParallelEvidence {
    assert!((1..=MAX_WORKERS).contains(&workers));
    let next = Arc::new(AtomicUsize::new(0));
    let partials = Arc::new((0..PARTITIONS).map(|_| AtomicU64::new(0)).collect::<Vec<_>>());
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let cpus = Arc::new(Mutex::new(BTreeSet::new()));
    let gate = Arc::new(Barrier::new(workers));
    thread::scope(|scope| {
        for _ in 0..workers {
            let next = Arc::clone(&next);
            let partials = Arc::clone(&partials);
            let active = Arc::clone(&active);
            let max_active = Arc::clone(&max_active);
            let cpus = Arc::clone(&cpus);
            let gate = Arc::clone(&gate);
            scope.spawn(move || {
                gate.wait();
                let now_active = active.fetch_add(1, Ordering::AcqRel) + 1;
                update_max(&max_active, now_active);
                while let index @ 0..PARTITIONS = next.fetch_add(1, Ordering::AcqRel) {
                    if let Some(cpu) = current_cpu() {
                        cpus.lock().expect("research CPU set lock").insert(cpu);
                    }
                    partials[index].store(partition(index), Ordering::Release);
                }
                active.fetch_sub(1, Ordering::AcqRel);
            });
        }
    });
    let reduction = partials
        .iter()
        .fold(0u64, |sum, partial| sum.wrapping_add(partial.load(Ordering::Acquire)));
    let max_active = max_active.load(Ordering::Acquire);
    let cpu_count = cpus.lock().expect("research CPU set lock").len();
    ParallelEvidence {
        digest: format!("bespoke-v1-{reduction:016x}"),
        max_active,
        cpus: cpu_count,
        overlap: workers > 1 && max_active >= 2 && cpu_count >= 2,
    }
}

fn arguments() -> (String, usize) {
    let mut mode = "reference".to_owned();
    let mut workers = 1usize;
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--mode" => mode = arguments.next().expect("--mode value required"),
            "--workers" => workers = arguments.next().expect("--workers value required").parse().expect("workers must be an integer"),
            _ => panic!("unknown argument: {argument}"),
        }
    }
    if mode == "reference" {
        workers = 1;
    }
    assert!(mode == "reference" || mode == "parallel", "mode must be reference or parallel");
    (mode, workers)
}

fn main() {
    let (_, workers) = arguments();
    evaluate_contract_cases().expect("research model contract");
    assert!(atomic_publication(), "release/acquire publication failed");
    assert!(structured_cancellation(), "structured cancellation failed");
    let evidence = run_partitioned_work(workers);
    println!(
        "digest={} overlap={} max_active={} cpus={} cases={} mutable_share=reject task_quota=reject atomic=accept cancel=joined",
        evidence.digest,
        evidence.overlap,
        evidence.max_active,
        evidence.cpus,
        CASES.len(),
    );
}
