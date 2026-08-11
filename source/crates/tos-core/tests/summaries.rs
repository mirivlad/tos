// SPDX-License-Identifier: GPL-3.0-or-later
//! Set-wide resolution over derived summaries.
//!
//! The property that matters is not that summaries are smaller. It is that they
//! decide *exactly* what parse trees decided: a resolution architecture that
//! changed a verdict would have traded a memory bound for a semantic change.

use tos_core::{
    check_module_set, check_module_summaries, ModuleEntry, ModuleSummary, Parser, SourceReader,
    SourceUnit,
};

/// The module header, which ends the prefix line. Imports follow it, then the
/// resource declaration — the order docs/39 fixes for a module prefix.
const HEADER: &str = "version 1.0 profile bootstrap;";
const RESOURCE: &str = " resource [fuel: 100, stack: 8KiB, allocation: 1KiB, tasks: 1, \
     workers: 1, sync: 0, shared: 0B, cleanup: 0, recursion: 2, imports: 4] ";

fn unit(text: &str) -> SourceUnit {
    SourceReader::read(text.as_bytes()).expect("transport-valid source")
}

/// Resolves a set both ways and asserts the verdicts are the same.
fn both_ways(modules: &[(&str, String)]) -> Vec<&'static str> {
    let sources: Vec<SourceUnit> = modules.iter().map(|(_, text)| unit(text)).collect();
    let schemas: Vec<_> = sources
        .iter()
        .map(|source| {
            Parser::parse_schema(source)
                .into_accepted()
                .expect("the fixture parses")
        })
        .collect();
    let entries: Vec<ModuleEntry> = modules
        .iter()
        .enumerate()
        .map(|(index, (path, _))| ModuleEntry::new(path, &sources[index], &schemas[index]))
        .collect();

    let over_trees: Vec<&'static str> = check_module_set(&entries)
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect();

    // The same set, resolved after every tree has been dropped.
    let summaries: Vec<ModuleSummary> = entries.iter().map(ModuleEntry::summarize).collect();
    drop(entries);
    drop(schemas);
    let over_summaries: Vec<&'static str> = check_module_summaries(&summaries)
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect();

    assert_eq!(
        over_trees, over_summaries,
        "resolving over summaries must reach the same verdict as resolving over trees"
    );
    over_trees
}

#[test]
fn a_resolvable_set_is_accepted_by_both_readings() {
    let modules = vec![
        (
            "app/base.tos",
            format!("module app.base {HEADER}{RESOURCE} pub record Reading [value: i32]"),
        ),
        (
            "app/user.tos",
            format!(
                "module app.user {HEADER} import app.base as base;{RESOURCE} \
                 pub fn read(entry: base.Reading) -> i32 {{ return entry.value; }}"
            ),
        ),
    ];
    assert_eq!(both_ways(&modules), Vec::<&str>::new());
}

#[test]
fn a_qualified_name_the_target_does_not_declare_is_found_by_both_readings() {
    let modules = vec![
        (
            "app/base.tos",
            format!("module app.base {HEADER}{RESOURCE} pub record Reading [value: i32]"),
        ),
        (
            "app/user.tos",
            format!(
                "module app.user {HEADER} import app.base as base;{RESOURCE} \
                 pub fn read(entry: base.Missing) -> i32 {{ return 0i32; }}"
            ),
        ),
    ];
    assert_eq!(both_ways(&modules), vec!["E1203_UNKNOWN_TYPE_NAME"]);
}

#[test]
fn a_missing_import_a_path_mismatch_and_a_cycle_are_found_by_both_readings() {
    let modules = vec![
        (
            "app/left.tos",
            format!("module app.left {HEADER} import app.right as right;{RESOURCE}"),
        ),
        (
            "app/right.tos",
            format!("module app.right {HEADER} import app.left as left;{RESOURCE}"),
        ),
        (
            "app/stray.tos",
            format!("module app.elsewhere {HEADER} import app.absent as absent;{RESOURCE}"),
        ),
    ];
    let codes = both_ways(&modules);
    assert!(codes.contains(&"E1606_IMPORT_CYCLE"), "{codes:?}");
    assert!(codes.contains(&"E1603_MODULE_PATH_MISMATCH"), "{codes:?}");
    assert!(codes.contains(&"E1604_IMPORT_NOT_FOUND"), "{codes:?}");
}

#[test]
fn an_ambiguous_name_is_found_by_both_readings() {
    // ADR-0038 root precedence settles roots and only roots; one root declaring
    // a name twice has nothing ordering it.
    let text = format!("module app.base {HEADER}{RESOURCE} pub record Reading [value: i32]");
    let first = unit(&text);
    let second = unit(&text);
    let user = unit(&format!(
        "module app.user {HEADER} import app.base as base;{RESOURCE} \
         pub fn read(entry: base.Reading) -> i32 {{ return entry.value; }}"
    ));
    let schemas: Vec<_> = [&first, &second, &user]
        .into_iter()
        .map(|source| {
            Parser::parse_schema(source)
                .into_accepted()
                .expect("parses")
        })
        .collect();
    let entries = vec![
        ModuleEntry::new("app/base.tos", &first, &schemas[0]),
        ModuleEntry::new("app/base.tos", &second, &schemas[1]),
        ModuleEntry::new("app/user.tos", &user, &schemas[2]),
    ];
    let over_trees: Vec<&'static str> = check_module_set(&entries)
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect();
    let summaries: Vec<ModuleSummary> = entries.iter().map(ModuleEntry::summarize).collect();
    let over_summaries: Vec<&'static str> = check_module_summaries(&summaries)
        .iter()
        .map(|diagnostic| diagnostic.code())
        .collect();
    assert_eq!(over_trees, over_summaries);
    assert!(
        over_trees.contains(&"E1605_AMBIGUOUS_IMPORT"),
        "{over_trees:?}"
    );
}

#[test]
fn a_summary_is_bound_to_the_source_it_was_derived_from() {
    // A derived artifact trusted without being bound to its input is a way to
    // make the source stop being the truth. A resolver handed a summary it did
    // not derive has to be able to ask.
    let text = format!("module app.base {HEADER}{RESOURCE} pub record Reading [value: i32]");
    let source = unit(&text);
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("parses");
    let summary = ModuleEntry::new("app/base.tos", &source, &schema).summarize();

    assert!(summary.describes(source.bytes()));
    assert!(summary.content_id.starts_with("sha256:"));

    let changed = format!("module app.base {HEADER}{RESOURCE} pub record Reading [value: i64]");
    let other = unit(&changed);
    assert!(
        !summary.describes(other.bytes()),
        "a summary must not describe source it was not derived from"
    );
}

#[test]
fn a_summary_is_regenerable_and_deleting_it_costs_only_time() {
    let text = format!("module app.base {HEADER}{RESOURCE} pub record Reading [value: i32]");
    let source = unit(&text);
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("parses");
    let entry = ModuleEntry::new("app/base.tos", &source, &schema);
    let first = entry.summarize();
    let second = entry.summarize();
    assert_eq!(first.content_id, second.content_id);
    assert_eq!(first.name, second.name);
    assert_eq!(first.declared_types, second.declared_types);
}

#[test]
fn a_summary_holds_the_interface_and_not_the_body() {
    // The bound this architecture buys comes from exactly this: a module with a
    // large body summarizes to the same size as one with a small body.
    let interface = "pub record Reading [value: i32] ";
    let small = format!(
        "module app.base {HEADER}{RESOURCE} {interface} pub fn f() -> i32 {{ return 0i32; }}"
    );
    let mut body = String::new();
    for index in 0..200 {
        body.push_str(&format!(
            "pub fn g{index}() -> i32 {{ return {index}i32; }} "
        ));
    }
    let large = format!("module app.base {HEADER}{RESOURCE} {interface} {body}");
    assert!(
        large.len() > small.len() * 8,
        "the fixture must differ in body size"
    );

    let summarize = |text: &str| {
        let source = unit(text);
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("parses");
        let summary = ModuleEntry::new("app/base.tos", &source, &schema).summarize();
        (
            summary.declared_types.len(),
            summary.imports.len(),
            summary.qualified_uses.len(),
        )
    };
    assert_eq!(summarize(&small), summarize(&large));
}
