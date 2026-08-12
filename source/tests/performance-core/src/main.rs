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

/// The engine decomposition fixtures.
///
/// Each runs the *same* loop the same number of times and differs only in what
/// the body does, so the cost of one semantic component is the difference
/// between a fixture and the empty one. They are built from the same shapes the
/// million-operation benchmark uses — not invented to make a tidy table — and
/// they run on the production engine, not a second interpreter written to
/// measure the first.
const DECOMPOSITION_ROUNDS: usize = 200_000;

fn decomposition_fixture(kind: &str) -> Option<String> {
    // The allocation budget is generous because a frame holds what it builds
    // until it returns, and the aggregate fixture builds four values per
    // iteration inside one frame. That is the engine's accounting working as
    // docs/41 requires, not a leak, and shrinking the budget would measure the
    // trap instead of the construction.
    let head = "module system.boot.init version 1.0 profile bootstrap; \
         resource [fuel: 100000000, stack: 64KiB, allocation: 1GiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] ";
    let rounds = DECOMPOSITION_ROUNDS;
    let body = match kind {
        // Dispatch, the loop compare, the conditional branch, the back edge and
        // the fuel charged for each. Everything else is measured against this.
        "empty" => String::new(),
        // Four more integer additions per iteration.
        "arithmetic" => String::from(
            "total = total + 1i64; total = total + 2i64; \
             total = total + 3i64; total = total + 4i64; ",
        ),
        // Four more comparisons, each consumed by a value rather than a branch.
        "comparison" => String::from(
            "let a = current < 5i64; let b = current > 5i64; \
             let c = current <= 5i64; let d = current >= 5i64; ",
        ),
        // Four more conditional branches.
        "branch" => String::from(
            "if (current > 0i64) { total = total + 1i64; } \
             if (current > 1i64) { total = total + 1i64; } \
             if (current > 2i64) { total = total + 1i64; } \
             if (current > 3i64) { total = total + 1i64; } ",
        ),
        // Four more local reads and writes, no arithmetic beyond the copy.
        "locals" => String::from("let p = current; let q = p; let r = q; let s = r; "),
        // Four more calls and returns, with the frame work each implies.
        "call" => String::from(
            "total = total + one(); total = total + one(); \
             total = total + one(); total = total + one(); ",
        ),
        // Four more aggregate constructions: Value building and the allocation
        // accounting that docs/41 requires before the effect.
        "aggregate" => String::from(
            "let g = Pair(x: current, y: current); let h = Pair(x: current, y: current); \
             let i = Pair(x: current, y: current); let j = Pair(x: current, y: current); ",
        ),
        _ => return None,
    };
    Some(format!(
        "{head} pub record Pair [x: i64, y: i64] \
         pub fn one() -> i64 {{ return 1i64; }} \
         pub fn main() -> i64 {{ \
         let mut total = 0i64; let mut current = 0i64; \
         while (current < {rounds}i64) {{ {body}current = current + 1i64; }} \
         return total; }}"
    ))
}

/// Prints one measured fixture verbatim and stops.
///
/// The reference half of the pair is taken by booting these exact bytes as a
/// capsule's canonical boot module. Emitting them from the harness that
/// measures them natively is what makes the two halves the same fixture rather
/// than two fixtures that resemble each other.
fn emit_fixture(kind: &str) -> Option<String> {
    let canonical = canonical_module(256 * 1024);
    match kind {
        "frontend" => Some(canonical),
        "execute" => Some(million_operation_module()),
        "reject" => Some(quota_exceeding_module(&canonical)),
        other => decomposition_fixture(other),
    }
}

/// Times each decomposition fixture natively, as the denominator of a ratio.
fn report_decomposition() {
    println!();
    println!("engine decomposition, median us over {DECOMPOSITION_ROUNDS} iterations");
    println!("(each component is four more operations per iteration than `empty`)");
    for kind in [
        "empty",
        "arithmetic",
        "comparison",
        "branch",
        "locals",
        "call",
        "aggregate",
    ] {
        let text = decomposition_fixture(kind).expect("a known fixture");
        let samples = measure(|| {
            run_benchmark(&text);
        });
        println!("  {kind:<11} {}", percentile(&samples, 50));
    }
}

fn main() {
    if let Some(kind) = argument("--emit-fixture") {
        match emit_fixture(&kind) {
            Some(text) => {
                print!("{text}");
                return;
            }
            None => {
                eprintln!("usage: --emit-fixture frontend|execute|reject");
                std::process::exit(2);
            }
        }
    }
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

    if argument("--decompose").is_some() {
        report_decomposition();
        return;
    }
    if argument("--stages").is_some() {
        report_stages(&module_text);
    }

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
        path: String::from("system/boot/init.tos"),
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

/// Times each frontend stage separately, so the cost has a location.
///
/// A total tells you the frontend is slow. It does not tell you which stage to
/// look at, and guessing which one is the mistake that produced a refuted
/// hypothesis once already.
fn report_stages(text: &str) {
    let read = measure(|| {
        SourceReader::read(text.as_bytes()).expect("transport-valid");
    });
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid");
    let parse = measure(|| {
        Parser::parse_schema(&source)
            .into_accepted()
            .expect("parses");
    });
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("parses");
    let check = measure(|| {
        let found = Checker::check(&source, &schema);
        assert!(found.is_empty());
    });
    let context = ModuleContext {
        source_set: String::from("tos-performance"),
        path: String::from("system/boot/init.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let lower = measure(|| {
        lower_module(&source, &schema, &context).expect("lowers");
    });
    let module = lower_module(&source, &schema, &context).expect("lowers");
    let verify = measure(|| {
        verify(&module, &ResolutionSnapshot::default(), &Limits::default()).expect("verifies");
    });
    // The receipt binds to the module's complete digest (docs/43 section 5), so
    // verification necessarily hashes the whole module. Timing it separately
    // says how much of `verify` is the checking and how much is the binding.
    let digest = measure(|| {
        let _ = tos_ir::module_digest(&module);
    });
    println!();
    println!("frontend stages, median us (the same fixture, one stage at a time)");
    for (name, samples) in [
        ("read", &read),
        ("parse", &parse),
        ("check", &check),
        ("lower", &lower),
        ("verify", &verify),
        ("  of which digest", &digest),
    ] {
        println!("  {name:<7} {}", percentile(samples, 50));
    }
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
        path: String::from("system/boot/init.tos"),
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
        path: String::from("system/boot/init.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = lower_module(&source, &schema, &context).expect("benchmark lowers");
    let receipt = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("benchmark verifies");
    run(&module, &receipt, "main", vec![])
        .expect("the entry exists")
        .expect("the benchmark does not trap")
        .value
}

/// A canonical module of about `bytes` bytes, built from ordinary declarations.
fn canonical_module(bytes: usize) -> String {
    let mut text = String::from(
        "module system.boot.init version 1.0 profile bootstrap; \
         resource [fuel: 100000000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub fn main() -> i32 { return 0i32; } ",
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
        "module system.boot.init version 1.0 profile bootstrap; \
         resource [fuel: 100000000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub fn main() -> i64 { \
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
