// SPDX-License-Identifier: GPL-3.0-or-later
//! How much of the accepted corpus reaches an executed result.
//!
//! `Gap` makes the lowerer's boundary visible instead of implied, and a boundary
//! nobody measures is a boundary nobody knows the size of. This runs every
//! accepted conformance vector through the whole reference path and records
//! exactly where each one stops.
//!
//! **This measures corpus coverage, not language coverage.** A corpus contains
//! what someone put in it; the contract contains what docs/39–44 accept. This
//! test is a regression gate on the first and is not evidence of the second —
//! confusing them is exactly how a lowering gap survived an earlier closure
//! audit. `crates/tos-pipeline/tests/patterns.rs` exercises the grammar's
//! pattern families directly, for that reason.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tos_pipeline::{execute, PipelineStage, Request, Run, Silent};

/// Accepted vectors that reach the verifier and the engine today.
///
/// A ratchet: it may rise, and a fall means a construct that used to reach IR
/// no longer does.
const AT_LEAST_VERIFIED: usize = 35;

fn accept_dir() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR")))
        .join("../../../docs/language/conformance/v1/accept")
}

#[test]
fn the_accepted_corpus_lowering_boundary_is_measured_not_implied() {
    let mut stops: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut gaps: BTreeMap<String, usize> = BTreeMap::new();
    let mut verified = 0usize;
    let mut total = 0usize;

    let mut files: Vec<PathBuf> = fs::read_dir(accept_dir())
        .expect("the accepted corpus is readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "tos"))
        .collect();
    files.sort();

    for file in files {
        total += 1;
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let text = fs::read(&file).expect("readable");
        // A vector that imports another module needs that module in the set.
        // This driver runs one module, so resolution would correctly report the
        // import as missing — a fact about the driver, not about the vector.
        if String::from_utf8_lossy(&text)
            .lines()
            .any(|line| line.trim_start().starts_with("import ") && !line.contains("capability"))
        {
            total -= 1;
            continue;
        }
        // The corpus states a module's own path in its header; the resolution
        // stage needs the two to agree, so the declared name decides the path.
        let declared = String::from_utf8_lossy(&text)
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("module ")
                    .map(|rest| rest.split_whitespace().next().unwrap_or("").to_string())
            })
            .unwrap_or_default();
        let path = format!("{}.tos", declared.replace('.', "/"));
        let request = Request {
            source_set: "tos-conformance",
            path: &path,
            bytes: &text,
            entry: "main",
        };
        let run = execute(&request, Vec::new(), &mut Silent);
        let stage = match &run {
            Run::Completed(_) => {
                verified += 1;
                "completed"
            }
            Run::Refused(_) | Run::Trapped { .. } => {
                // It reached the engine, so it lowered and verified.
                verified += 1;
                "executed"
            }
            Run::NotLowered(gap) => {
                *gaps.entry(gap.construct.to_string()).or_default() += 1;
                "not-lowered"
            }
            Run::Unverified(_) => "unverified",
            Run::Diagnosed { stage, .. } => match stage {
                PipelineStage::Parse => "parse",
                PipelineStage::Check => "check",
                _ => "resolve",
            },
            Run::SourceRejected { .. } => "read",
        };
        stops.entry(stage.to_string()).or_default().push(name);
    }

    println!("accepted corpus: {total} vectors");
    for (stage, names) in &stops {
        println!("  {stage:<12} {}", names.len());
    }
    if !gaps.is_empty() {
        println!("lowering gaps by construct:");
        for (construct, count) in &gaps {
            println!("  {construct:<40} {count}");
        }
    }

    assert!(
        verified >= AT_LEAST_VERIFIED,
        "coverage fell: {verified} vectors reached the verifier, expected at least {AT_LEAST_VERIFIED}"
    );
    // A vector that the *frontend* rejects is a corpus defect, not a lowering
    // limit: the corpus says these are accepted programs.
    let rejected: Vec<&String> = ["read", "parse", "check", "resolve"]
        .iter()
        .filter_map(|stage| stops.get(*stage))
        .flatten()
        .collect();
    assert!(
        rejected.is_empty(),
        "accepted vectors the frontend rejected: {rejected:?}"
    );
}
