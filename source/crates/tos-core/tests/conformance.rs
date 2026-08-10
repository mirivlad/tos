// SPDX-License-Identifier: GPL-3.0-or-later
//! Parser acceptance and rejection gate over the accepted conformance corpus.
//!
//! The corpus in `docs/language` is the accepted evidence for TOS Core V1
//! (docs/44). This gate binds the parser to it directly instead of restating
//! expectations in the test file:
//!
//! - every canonical example and every `accept/` vector parses **and checks**
//!   with no diagnostic at all;
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

use tos_core::SourceUnit as SourceReaderUnit;
use tos_core::{check_source_set, Checker, ModuleEntry, Parser, SourceReader};

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
const IMPLEMENTED_CHECKS: [&str; 16] = [
    "E1202_UNKNOWN_VALUE_NAME",
    "E1211_INDEX_TYPE_MISMATCH",
    "E1210_INTEGER_TYPE_MISMATCH",
    "E1212_INVALID_AS_CONVERSION",
    "E1222_RETURN_TYPE_MISMATCH",
    "E1220_NONEXHAUSTIVE_MATCH",
    "E1203_UNKNOWN_TYPE_NAME",
    "E1204_TYPE_ARGUMENT_ARITY",
    "E1206_MISSING_RECORD_FIELD",
    "E1207_UNKNOWN_RECORD_FIELD",
    "E1702_PROFILE_NOT_SUPPORTED",
    "E1221_MISSING_RETURN",
    "E1205_DUPLICATE_RECORD_FIELD",
    "E1700_RESOURCE_DECLARATION_REQUIRED",
    "E1703_DUPLICATE_RESOURCE_DECLARATION",
    "E1704_UNKNOWN_RESOURCE_LIMIT",
];

/// Modules a vector needs in its source set, by canonical path.
///
/// A vector that imports another module cannot be resolved alone: the qualified
/// names it writes are decided by the module its binding names.
fn companion_paths(root: &Path, source_text: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in source_text.lines() {
        let Some(rest) = line.trim().strip_prefix("import ") else {
            continue;
        };
        let name = rest
            .split([' ', ';'])
            .next()
            .unwrap_or_default()
            .trim_end_matches(';');
        if name.is_empty() {
            continue;
        }
        let leaf = name.rsplit('.').next().unwrap_or_default();
        for directory in ["conformance/v1/accept", "conformance/v1/reject", "examples"] {
            let candidate = root
                .join(directory)
                .join(std::format!("{}.tos", leaf.replace('_', "-")));
            if candidate.is_file() {
                paths.push(candidate);
            }
        }
    }
    paths
}

/// Parses and then checks a vector, returning the first diagnostic code.
fn check_report(path: &Path) -> Option<String> {
    let bytes = fs::read(path).expect("vector is readable");
    let source = SourceReader::read(&bytes).ok()?;
    let outcome = Parser::parse_schema(&source);
    let schema = outcome.into_accepted()?;

    let root = corpus_root();
    let text = std::string::String::from_utf8_lossy(&bytes).into_owned();
    let companions = companion_paths(&root, &text);
    if companions.is_empty() {
        return Checker::check(&source, &schema)
            .first()
            .map(|diagnostic| diagnostic.code().to_string());
    }

    // Resolve the vector together with the modules it imports.
    let mut loaded = Vec::new();
    for companion in companions {
        let companion_bytes = fs::read(&companion).expect("companion is readable");
        let Ok(companion_source) = SourceReader::read(&companion_bytes) else {
            continue;
        };
        let Some(companion_schema) = Parser::parse_schema(&companion_source).into_accepted() else {
            continue;
        };
        loaded.push((companion, companion_source, companion_schema));
    }
    let mut entries = std::vec![ModuleEntry::new(
        &canonical_path(&source, &schema),
        &source,
        &schema,
    )];
    for (_, companion_source, companion_schema) in &loaded {
        entries.push(ModuleEntry::new(
            &canonical_path(companion_source, companion_schema),
            companion_source,
            companion_schema,
        ));
    }
    check_source_set(&entries)
        .into_iter()
        .find(|diagnostic| diagnostic.code() != "E1603_MODULE_PATH_MISMATCH")
        .map(|diagnostic| diagnostic.code().to_string())
}

/// The path a module's declared name maps to, so corpus files resolve without
/// living at their canonical repository locations.
fn canonical_path(source: &SourceReaderUnit, schema: &tos_core::Schema) -> String {
    let name = schema
        .outline()
        .prefix()
        .header()
        .name()
        .iter()
        .map(|segment| segment.text(source))
        .collect::<Vec<_>>()
        .join(".");
    std::format!("{}.tos", name.replace('.', "/"))
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
                Ok(Some(code)) => {
                    failures.push(std::format!("{name}: source error {code}"));
                    continue;
                }
                Err(detail) => {
                    failures.push(std::format!("{name}: {detail}"));
                    continue;
                }
            }
            // Accepted source must also survive every implemented check, not
            // merely parse: a checker that rejects canonical source is wrong.
            if let Some(code) = check_report(&file) {
                failures.push(std::format!("{name}: checker gave {code}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "canonical source must parse and check:\n{}",
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
