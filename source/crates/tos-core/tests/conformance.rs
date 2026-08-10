// SPDX-License-Identifier: GPL-3.0-or-later
//! Parser acceptance and rejection gate over the accepted conformance corpus.
//!
//! The corpus in `docs/language` is the accepted evidence for TOS Core V1
//! (docs/44). This gate binds the parser to it directly instead of restating
//! expectations in the test file:
//!
//! - every canonical example and every `accept/` vector parses with no
//!   diagnostic at all;
//! - every `reject/` vector whose recorded code belongs to the source reader,
//!   lexer or parser produces exactly that code;
//! - every `reject/` vector whose recorded code belongs to a later stage parses
//!   cleanly, because rejecting it here would mean the parser is enforcing
//!   semantics it does not own.
//!
//! The third rule is the one that catches a corpus defect: a vector that cannot
//! reach the stage it targets is not evidence for that stage.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tos_core::{Checker, Parser, SourceReader};

fn corpus_root() -> PathBuf {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../docs/language"
    ))
    .to_path_buf()
}

fn tos_files(directory: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(directory)
        .expect("corpus directory is readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "tos"))
        .collect();
    files.sort();
    files
}

/// Reads the expected primary code of each `reject/` vector out of the accepted
/// expectations table.
fn recorded_reject_codes() -> BTreeMap<String, String> {
    let text = fs::read_to_string(corpus_root().join("conformance/v1/EXPECTATIONS.md"))
        .expect("expectations are readable");
    let mut codes = BTreeMap::new();
    for line in text.lines() {
        if !line.starts_with("| R") {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let Some(input) = cells.get(2) else { continue };
        let Some(name) = input
            .strip_prefix("`reject/")
            .and_then(|rest| rest.strip_suffix(".tos`"))
        else {
            continue;
        };
        let expected = cells.get(4).copied().unwrap_or_default();
        let code = expected
            .split('`')
            .nth(1)
            .unwrap_or_default()
            .trim()
            .to_string();
        assert!(
            !code.is_empty(),
            "reject/{name}.tos records no diagnostic code"
        );
        codes.insert(std::format!("{name}.tos"), code);
    }
    assert!(!codes.is_empty(), "expectations table was not parsed");
    codes
}

/// Whether a code belongs to a stage the parser owns.
fn is_frontend_code(code: &str) -> bool {
    code.starts_with("E10") || code.starts_with("E11")
}

/// Later-stage codes the checker already implements.
///
/// A vector recording one of these must now be rejected rather than merely
/// parse; the list grows as each check lands, so a check cannot be implemented
/// without its corpus evidence starting to bind.
const IMPLEMENTED_CHECKS: [&str; 5] = [
    "E1202_UNKNOWN_VALUE_NAME",
    "E1205_DUPLICATE_RECORD_FIELD",
    "E1700_RESOURCE_DECLARATION_REQUIRED",
    "E1703_DUPLICATE_RESOURCE_DECLARATION",
    "E1704_UNKNOWN_RESOURCE_LIMIT",
];

/// Parses and then checks a vector, returning the first diagnostic code.
fn check_report(path: &Path) -> Option<String> {
    let bytes = fs::read(path).expect("vector is readable");
    let source = SourceReader::read(&bytes).ok()?;
    let outcome = Parser::parse_schema(&source);
    let schema = outcome.into_accepted()?;
    let diagnostics = Checker::check(&source, &schema);
    diagnostics
        .first()
        .map(|diagnostic| diagnostic.code().to_string())
}

fn parse_report(path: &Path) -> Result<Option<String>, String> {
    let bytes = fs::read(path).expect("vector is readable");
    let source = match SourceReader::read(&bytes) {
        Ok(source) => source,
        Err(error) => return Ok(Some(error.code().symbol().to_string())),
    };
    let outcome = Parser::parse_schema(&source);
    match outcome.diagnostics().first() {
        None => Ok(None),
        Some(diagnostic) => {
            let detail = std::format!(
                "{} at {:?} (line {}, column {})",
                diagnostic.code(),
                diagnostic.span().text(&source),
                diagnostic.start().line(),
                diagnostic.start().column()
            );
            Err(detail)
        }
    }
}

#[test]
fn canonical_examples_and_accepted_vectors_parse_cleanly() {
    let root = corpus_root();
    let mut failures = Vec::new();
    for directory in ["examples", "conformance/v1/accept"] {
        for file in tos_files(&root.join(directory)) {
            let name = file.strip_prefix(&root).unwrap().display().to_string();
            match parse_report(&file) {
                Ok(None) => {}
                Ok(Some(code)) => failures.push(std::format!("{name}: source error {code}")),
                Err(detail) => failures.push(std::format!("{name}: {detail}")),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "canonical source must parse:\n{}",
        failures.join("\n")
    );
}

#[test]
fn rejected_vectors_fail_at_the_stage_their_expectation_records() {
    let root = corpus_root();
    let expected = recorded_reject_codes();
    let mut failures = Vec::new();
    for file in tos_files(&root.join("conformance/v1/reject")) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let Some(code) = expected.get(&name) else {
            failures.push(std::format!("{name}: no recorded expectation"));
            continue;
        };
        let observed = match parse_report(&file) {
            Ok(None) => None,
            Ok(Some(source_code)) => Some(source_code),
            Err(detail) => Some(detail),
        };
        if is_frontend_code(code) {
            match observed {
                Some(detail) if detail.starts_with(code) => {}
                Some(detail) => {
                    failures.push(std::format!("{name}: expected {code}, got {detail}"))
                }
                None => failures.push(std::format!("{name}: expected {code}, but it parsed")),
            }
            continue;
        }
        // The vector targets a later stage, so the parser must let it through.
        if let Some(detail) = observed {
            failures.push(std::format!(
                "{name}: targets {code} but cannot reach that stage — {detail}"
            ));
            continue;
        }
        if !IMPLEMENTED_CHECKS.contains(&code.as_str()) {
            continue;
        }
        match check_report(&file) {
            Some(observed) if observed == *code => {}
            Some(observed) => failures.push(std::format!(
                "{name}: expected {code}, checker gave {observed}"
            )),
            None => failures.push(std::format!(
                "{name}: expected {code}, but it checked clean"
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "reject corpus disagrees:\n{}",
        failures.join("\n")
    );
}
