// SPDX-License-Identifier: GPL-3.0-or-later
//! The Stage 2 measurement harness for the docs/35 budgets.
//!
//! It measures the two metrics docs/35 assigns to the bootstrap profile:
//!
//! - parse, type-check, lower and verify a 256 KiB canonical module;
//! - execute the standard one-million-operation integer/control-flow benchmark.
//!
//! and the quota-rejection behaviour that bounds how badly a rejected input may
//! cost compared with an accepted one.
//!
//! **Which platform a record came from is an input, never an inference.** The
//! harness is told with `--profile native` or `--profile reference` and records
//! what it was told. It never decides for itself that the machine it happens to
//! be running on is the reference platform: choosing the platform after seeing
//! the number is the thing ADR-0040 exists to prevent.
//!
//! The execution budget is a ratio. ADR-0040 section 2 reads docs/35's "host
//! reference interpreter time under the same semantic implementation" as the
//! native-host run of this same engine at this same commit, so the ratio is
//! `reference / native` and both halves are retained. `--baseline <us>` supplies
//! the native p95 when computing the reference-side record, so the quotient is
//! never presented without the measurement it came from.
//!
//! docs/35 fixes the sampling: three warmups then twenty-one samples, with
//! median, p95 and p99 retained.

use std::time::Instant;

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_engine::{run, Value};
use tos_ir::IntKind;
use tos_verifier::{verify, Limits, ResolutionSnapshot};

const WARMUPS: usize = 3;
const SAMPLES: usize = 21;

fn main() {
    let profile = argument("--profile").unwrap_or_else(|| String::from("native"));
    let baseline: Option<u128> = argument("--baseline").and_then(|value| value.parse().ok());
    if profile != "native" && profile != "reference" {
        eprintln!("usage: --profile native|reference [--baseline <native p95 us>]");
        std::process::exit(2);
    }

    println!("TOS Core Stage 2 measurement harness");
    println!("profile: {profile} (declared, not inferred — ADR-0040)");
    println!(
        "evidence level: {}",
        if profile == "reference" {
            "the record is reference-platform only if it was actually taken there"
        } else {
            "P1 native-host baseline; it is the denominator of the ratio, not a gate"
        }
    );
    println!("sampling: {WARMUPS} warmups, {SAMPLES} samples, median/p95/p99 in microseconds");
    println!();

    let module_text = canonical_module(256 * 1024);
    println!(
        "fixture: canonical module of {} bytes, content {}",
        module_text.len(),
        content_id(module_text.as_bytes())
    );
    if let Err(reason) = explain(&module_text) {
        println!("fixture does not reach a receipt: {reason}");
        std::process::exit(1);
    }
    let frontend = measure(|| {
        let reached = frontend_to_receipt(&module_text);
        assert!(reached, "the canonical fixture must reach a receipt");
    });
    report("parse + check + lower + verify, 256 KiB module", &frontend);
    println!("  docs/35 budget: 500 ms p95 on the reference platform");

    let benchmark = million_operation_module();
    let execution = measure(|| {
        let value = run_benchmark(&benchmark);
        assert_eq!(value, Value::Int(IntKind::I64, 1_000_000));
    });
    report(
        "one-million-operation integer/control-flow benchmark",
        &execution,
    );
    let execution_p95 = percentile(&execution, 95);
    println!("  docs/35 budget: within 10x a host reference interpreter");
    match (profile.as_str(), baseline) {
        ("reference", Some(native)) => println!(
            "  reference/native p95 ratio: {:.3} (docs/35 budget: at most 10.000), native baseline {native} us",
            execution_p95 as f64 / native.max(1) as f64
        ),
        ("reference", None) => println!(
            "  ratio not computed: pass --baseline <native p95 us> from a native run of this same commit"
        ),
        _ => println!(
            "  this is the native baseline; pass --baseline {execution_p95} to the reference-profile run to obtain the ratio"
        ),
    }

    // The comparison only means something against a comparable input: docs/35
    // bounds a rejection by the *accepted-input* budget, so the rejected
    // fixture is the same 256 KiB module with one quota key it may not declare.
    let rejected = quota_exceeding_module(&module_text);
    let rejection = measure(|| {
        let accepted = frontend_to_receipt(&rejected);
        assert!(!accepted, "the quota fixture must be rejected");
    });
    report("reject a quota-exceeding module", &rejection);
    let accepted_p95 = percentile(&frontend, 95);
    let rejected_p95 = percentile(&rejection, 95);
    println!(
        "  rejection/acceptance p95 ratio: {:.3} (docs/35 budget: at most 2.000)",
        rejected_p95 as f64 / accepted_p95.max(1) as f64
    );

    println!();
    if profile == "native" {
        println!("This is the native half of the pair. Closing the docs/35 Stage 2 gate");
        println!("needs the same procedure under the ADR-0040 reference profile, with");
        println!("both halves retained as raw samples.");
    } else {
        println!("Reference-profile record. It closes the docs/35 Stage 2 gate only if it");
        println!("was taken under the ADR-0040 profile and both halves are retained.");
    }
}

/// Reads a declared argument. Nothing is inferred from the environment.
fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(found) = args.next() {
        if found == name {
            return args.next();
        }
    }
    None
}

/// Says why the path stopped, so a fixture defect is never read as a slow path.
fn explain(text: &str) -> Result<(), String> {
    let source = SourceReader::read(text.as_bytes()).map_err(|e| format!("source: {e:?}"))?;
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .ok_or_else(|| String::from("does not parse"))?;
    let diagnostics = Checker::check(&source, &schema);
    if !diagnostics.is_empty() {
        return Err(format!("checker: {}", diagnostics[0].code()));
    }
    let context = ModuleContext {
        source_set: String::from("tos-performance"),
        path: String::from("app/bench.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = lower_module(&source, &schema, &context)
        .map_err(|gap| format!("gap: {}", gap.construct))?;
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .map(|_| ())
        .map_err(|finding| format!("verifier: {} — {}", finding.code, finding.detail))
}

/// Runs the whole production path and says whether it produced a receipt.
fn frontend_to_receipt(text: &str) -> bool {
    let Ok(source) = SourceReader::read(text.as_bytes()) else {
        return false;
    };
    let Some(schema) = Parser::parse_schema(&source).into_accepted() else {
        return false;
    };
    if !Checker::check(&source, &schema).is_empty() {
        return false;
    }
    let context = ModuleContext {
        source_set: String::from("tos-performance"),
        path: String::from("app/bench.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let Ok(module) = lower_module(&source, &schema, &context) else {
        return false;
    };
    verify(&module, &ResolutionSnapshot::default(), &Limits::default()).is_ok()
}

fn run_benchmark(text: &str) -> Value {
    let source = SourceReader::read(text.as_bytes()).expect("benchmark source is valid");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("benchmark parses");
    assert!(Checker::check(&source, &schema).is_empty());
    let context = ModuleContext {
        source_set: String::from("tos-performance"),
        path: String::from("app/bench.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = lower_module(&source, &schema, &context).expect("benchmark lowers");
    let receipt = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("benchmark verifies");
    run(&module, &receipt, "bench", vec![])
        .expect("the entry exists")
        .expect("the benchmark does not trap")
        .value
}

/// A canonical module of about `bytes` bytes, built from ordinary declarations.
fn canonical_module(bytes: usize) -> String {
    let mut text = String::from(
        "module app.bench version 1.0 profile bootstrap; \
         resource [fuel: 100000000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] ",
    );
    let mut index = 0usize;
    loop {
        let chunk = format!(
            "pub record Point{index} [x: i32, y: i32] \
             pub fn total{index}(point: Point{index}) -> i32 {{ return point.x + point.y; }} "
        );
        // docs/44 section 2 caps a normalized source unit at 256 KiB, so the
        // fixture fills that budget without crossing it.
        if text.len() + chunk.len() > bytes {
            break;
        }
        text.push_str(&chunk);
        index += 1;
    }
    text
}

/// The standard one-million-operation integer and control-flow benchmark.
fn million_operation_module() -> String {
    String::from(
        "module app.bench version 1.0 profile bootstrap; \
         resource [fuel: 100000000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub fn bench() -> i64 { \
         let mut total = 0i64; let mut current = 0i64; \
         while (current < 1000000i64) { total = total + 1i64; current = current + 1i64; } \
         return total; }",
    )
}

/// The canonical fixture with one resource key it may not declare.
///
/// Same size, same declarations, one difference: what is measured is the cost
/// of rejecting an input of the size that was accepted, which is what docs/35
/// bounds.
fn quota_exceeding_module(accepted: &str) -> String {
    accepted.replacen("imports: 0]", "imports: 0, bandwidth: 4]", 1)
}

fn measure(mut work: impl FnMut()) -> Vec<u128> {
    for _ in 0..WARMUPS {
        work();
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let started = Instant::now();
        work();
        samples.push(started.elapsed().as_micros());
    }
    samples.sort_unstable();
    samples
}

fn percentile(sorted: &[u128], percent: usize) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    // Nearest-rank, which needs no interpolation and is exact for 21 samples.
    let rank = (percent * sorted.len()).div_ceil(100).max(1);
    sorted[rank - 1]
}

fn report(name: &str, samples: &[u128]) {
    println!(
        "{name}\n  median {} us, p95 {} us, p99 {} us, min {} us, max {} us",
        percentile(samples, 50),
        percentile(samples, 95),
        percentile(samples, 99),
        samples.first().copied().unwrap_or(0),
        samples.last().copied().unwrap_or(0),
    );
    print!("  raw samples (us):");
    for sample in samples {
        print!(" {sample}");
    }
    println!();
}

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}
