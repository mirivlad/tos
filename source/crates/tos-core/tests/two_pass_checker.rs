// SPDX-License-Identifier: GPL-3.0-or-later
//! The set-wide check run in phases, against the same check run whole.
//!
//! A two-pass checker holds less: the first pass keeps what a set is resolved
//! *from* and the second reads each module's qualified uses again and drops
//! them. What it must not change is anything a caller can observe — the
//! diagnostic codes, their fields, their spans, their derived positions and the
//! order they arrive in — because the order across the groups is part of what
//! `check_module_summaries` reports and a caller assembling the phases could
//! silently get it wrong.
//!
//! So every fixture here is run both ways and compared exactly, error cases
//! included. A one-pass run is the oracle.

use tos_core::{
    check_module_cycles, check_module_membership, check_module_summaries, check_qualified_types_of,
    resolve_set, Diagnostic, ModuleEntry, ModuleSummary, Parser, SourceReader,
};

/// One unit of a fixture set.
struct Unit {
    path: &'static str,
    text: String,
}

fn unit(path: &'static str, text: String) -> Unit {
    Unit { path, text }
}

const ENVELOPE: &str = "resource [fuel: 10000, stack: 64KiB, allocation: 4KiB, tasks: 1, \
     workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 4] ";

fn module(name: &str, imports: &str, body: &str) -> String {
    format!("module {name} version 1.0 profile bootstrap; {imports} {ENVELOPE} {body}")
}

/// The whole check, over summaries that carry their own qualified uses.
fn one_pass(units: &[Unit]) -> Vec<Diagnostic> {
    let mut summaries = Vec::new();
    for unit in units {
        let source = SourceReader::read(unit.text.as_bytes()).expect("the fixture reads");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("the fixture parses");
        summaries.push(ModuleEntry::new(unit.path, &source, &schema).summarize());
    }
    check_module_summaries(&summaries)
}

/// The same check in phases: membership, then one module's uses at a time from
/// the source again, then cycles.
fn two_pass(units: &[Unit]) -> Vec<Diagnostic> {
    let mut summaries: Vec<ModuleSummary> = Vec::new();
    for unit in units {
        let source = SourceReader::read(unit.text.as_bytes()).expect("the fixture reads");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("the fixture parses");
        summaries.push(ModuleEntry::new(unit.path, &source, &schema).summarize_membership());
    }
    let resolution = resolve_set(&summaries);
    let mut diagnostics = check_module_membership(&summaries, &resolution);
    for (position, unit) in units.iter().enumerate() {
        let source = SourceReader::read(unit.text.as_bytes()).expect("the fixture reads again");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("the fixture parses again");
        let uses = ModuleEntry::new(unit.path, &source, &schema).qualified_uses();
        check_qualified_types_of(
            &summaries[position],
            &uses,
            &summaries,
            &resolution,
            &mut diagnostics,
        );
    }
    diagnostics.extend(check_module_cycles(&summaries, &resolution));
    diagnostics
}

/// Compared as a caller sees them: code, severity, stage, span, positions and
/// every field, in order.
fn rendered(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| format!("{diagnostic:?}"))
        .collect()
}

fn agree(units: &[Unit]) {
    let whole = one_pass(units);
    let phased = two_pass(units);
    assert_eq!(
        rendered(&phased),
        rendered(&whole),
        "the phases must report what the whole check reports, in the same order"
    );
}

#[test]
fn a_set_with_nothing_wrong_produces_the_same_nothing() {
    agree(&[
        unit(
            "system/lib/math.tos",
            module(
                "system.lib.math",
                "",
                "pub record Point [x: i32, y: i32] \
                 pub fn double(value: i32) -> i32 { return value * 2i32; }",
            ),
        ),
        unit(
            "system/boot/init.tos",
            module(
                "system.boot.init",
                "import system.lib.math as math;",
                "pub fn main(point: math.Point) -> i32 { return math.double(point.x); }",
            ),
        ),
    ]);
}

#[test]
fn a_qualified_name_the_target_does_not_declare_is_reported_the_same_way() {
    agree(&[
        unit(
            "system/lib/math.tos",
            module("system.lib.math", "", "pub record Point [x: i32, y: i32] "),
        ),
        unit(
            "system/boot/init.tos",
            module(
                "system.boot.init",
                "import system.lib.math as math;",
                "pub fn main(reading: math.Reading) -> i32 { return 1i32; }",
            ),
        ),
    ]);
}

#[test]
fn a_path_that_does_not_derive_the_name_is_reported_the_same_way() {
    agree(&[unit(
        "system/boot/elsewhere.tos",
        module(
            "system.boot.init",
            "",
            "pub fn main() -> i32 { return 1i32; }",
        ),
    )]);
}

#[test]
fn an_import_that_resolves_to_nothing_is_reported_the_same_way() {
    agree(&[unit(
        "system/boot/init.tos",
        module(
            "system.boot.init",
            "import system.lib.absent as gone;",
            "pub fn main() -> i32 { return 1i32; }",
        ),
    )]);
}

#[test]
fn a_cycle_is_reported_the_same_way_and_in_the_same_place() {
    agree(&[
        unit(
            "system/lib/one.tos",
            module(
                "system.lib.one",
                "import system.lib.two as two;",
                "pub fn a() -> i32 { return 1i32; }",
            ),
        ),
        unit(
            "system/lib/two.tos",
            module(
                "system.lib.two",
                "import system.lib.one as one;",
                "pub fn b() -> i32 { return 2i32; }",
            ),
        ),
    ]);
}

/// Every failure at once, which is what fixes the **order between groups**.
///
/// A path mismatch, an unresolvable import, an unknown qualified type and a
/// cycle in one set: if the phases emitted them grouped differently — cycles
/// before qualified types, say — this is the test that says so.
#[test]
fn a_set_wrong_in_four_ways_reports_them_in_the_same_order() {
    agree(&[
        unit(
            "system/lib/wrong-path.tos",
            module(
                "system.lib.one",
                "import system.lib.two as two;",
                "pub record Held [x: i32] pub fn a() -> i32 { return 1i32; }",
            ),
        ),
        unit(
            "system/lib/two.tos",
            module(
                "system.lib.two",
                "import system.lib.one as one; import system.lib.absent as gone;",
                "pub fn b(held: one.Missing) -> i32 { return 2i32; }",
            ),
        ),
    ]);
}

/// Many uses across many modules, so ordering within the qualified-type group
/// is exercised rather than assumed.
#[test]
fn many_qualified_uses_across_many_modules_agree() {
    let mut units = Vec::new();
    units.push(unit(
        "system/lib/m0.tos",
        module(
            "system.lib.m0",
            "",
            "pub record R0 [x: i32] pub record R1 [x: i32] pub fn v() -> i32 { return 0i32; }",
        ),
    ));
    for index in 1..6 {
        let mut body = String::new();
        for use_site in 0..4 {
            // Half of them name a type the target declares and half do not, so
            // the group holds both diagnostics and silence, interleaved.
            let name = if use_site % 2 == 0 { "R0" } else { "Absent" };
            body.push_str(&format!(
                "pub fn use{use_site}(value: base.{name}) -> i32 {{ return value.x; }} "
            ));
        }
        units.push(unit(
            match index {
                1 => "system/lib/m1.tos",
                2 => "system/lib/m2.tos",
                3 => "system/lib/m3.tos",
                4 => "system/lib/m4.tos",
                _ => "system/lib/m5.tos",
            },
            module(
                match index {
                    1 => "system.lib.m1",
                    2 => "system.lib.m2",
                    3 => "system.lib.m3",
                    4 => "system.lib.m4",
                    _ => "system.lib.m5",
                },
                "import system.lib.m0 as base;",
                &body,
            ),
        ));
    }
    agree(&units);
}
