// SPDX-License-Identifier: GPL-3.0-or-later
//! Non-production Stage 1.5 adapted-Rust profile experiment.
//!
//! This experiment neither accepts TOS source nor selects Rust. It isolates the
//! restrictions and runtime contracts an adapted Rust foundation would require.

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

mod capability {
    #[derive(Debug)]
    pub struct MmioCapability {
        grant: &'static str,
    }

    pub struct Verifier;

    impl Verifier {
        pub fn grant_mmio(device: &'static str) -> MmioCapability {
            MmioCapability { grant: device }
        }
    }

    pub fn map(capability: &MmioCapability) -> Result<&'static str, &'static str> {
        if capability.grant == "device0" {
            Ok("mapped:device0")
        } else {
            Err("capability.unknown-object")
        }
    }
}

#[derive(Debug)]
struct TaskBudget {
    maximum: usize,
    used: usize,
}

impl TaskBudget {
    fn new(maximum: usize) -> Self {
        Self { maximum, used: 0 }
    }

    fn reserve(&mut self) -> Result<(), &'static str> {
        if self.used == self.maximum {
            return Err("resource.task-limit");
        }
        self.used += 1;
        Ok(())
    }
}

fn cache_identity(source_digest: &[u8], dependency_digest: &[u8]) -> u64 {
    source_digest
        .iter()
        .chain(dependency_digest)
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn validate_profile_contract() -> Result<(), &'static str> {
    let corpus = include_str!("../common/cases.json");
    if CASES.iter().any(|case| !corpus.contains(case)) || corpus.matches("\"id\"").count() != CASES.len() {
        return Err("common-corpus-mismatch");
    }
    let capability = capability::Verifier::grant_mmio("device0");
    if capability::map(&capability) != Ok("mapped:device0") {
        return Err("capability.declared-mmio");
    }
    if cache_identity(b"source-a", b"dependency-a") == cache_identity(b"source-a", b"dependency-b") {
        return Err("cache.dependency-change");
    }
    let mut budget = TaskBudget::new(MAX_TASKS);
    for _ in 0..MAX_TASKS {
        budget.reserve()?;
    }
    if budget.reserve() != Err("resource.task-limit") {
        return Err("resource.task-limit");
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
        digest: format!("stage15-common-v1-{reduction:016x}"),
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
    validate_profile_contract().expect("profile contract");
    assert!(atomic_publication(), "release/acquire publication failed");
    assert!(structured_cancellation(), "structured cancellation failed");
    let evidence = run_partitioned_work(workers);
    println!(
        "digest={} overlap={} max_active={} cpus={} cases={} task_quota=reject atomic=accept cancel=joined",
        evidence.digest,
        evidence.overlap,
        evidence.max_active,
        evidence.cpus,
        CASES.len(),
    );
}
