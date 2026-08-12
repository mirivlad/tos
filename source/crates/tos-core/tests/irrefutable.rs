// SPDX-License-Identifier: GPL-3.0-or-later
//! `let` and `for` bind unconditionally, so their patterns must not fail
//! (ADR-0046).

use tos_core::{Checker, Diagnostic, Parser, SourceReader};

const PRELUDE: &str = "module app.patterns version 1.0 profile bootstrap; \
     resource [fuel: 1000, stack: 8KiB, allocation: 1KiB, tasks: 1, workers: 1, \
     sync: 0, shared: 0B, cleanup: 4, recursion: 4, imports: 0] ";

fn diagnostics(text: &str) -> Vec<Diagnostic> {
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .unwrap_or_else(|| panic!("the fixture must parse: {text}"));
    Checker::check(&source, &schema)
}

fn codes(text: &str) -> Vec<&'static str> {
    diagnostics(text).iter().map(Diagnostic::code).collect()
}

#[test]
fn an_irrefutable_tuple_let_is_accepted() {
    let text = format!(
        "{PRELUDE} pub fn main() -> i32 {{ let pair = (1i32, 2i32); \
         let (a, b) = pair; return a + b; }}"
    );
    assert_eq!(codes(&text), Vec::<&str>::new());
}

#[test]
fn a_nested_irrefutable_let_is_accepted() {
    let text = format!(
        "{PRELUDE} pub fn main() -> i32 {{ let nested = (1i32, (2i32, 3i32)); \
         let (head, (left, right)) = nested; return head + left + right; }}"
    );
    assert_eq!(codes(&text), Vec::<&str>::new());
}

#[test]
fn a_wildcard_let_is_irrefutable() {
    let text = format!(
        "{PRELUDE} pub fn main() -> i32 {{ let pair = (1i32, 2i32); \
         let (_, b) = pair; return b; }}"
    );
    assert_eq!(codes(&text), Vec::<&str>::new());
}

#[test]
fn a_refutable_enum_pattern_in_let_is_refused() {
    let text = format!(
        "{PRELUDE} pub enum Mode [Fast, Slow] \
         pub fn main(mode: Mode) -> unit {{ let Fast = mode; }}"
    );
    let all = diagnostics(&text);
    let finding = all
        .iter()
        .find(|d| d.code() == "E1223_REFUTABLE_PATTERN")
        .unwrap_or_else(|| panic!("{:?}", codes(&text)));
    assert_eq!(finding.field("context"), Some("let"));
    assert!(finding.field("reason").is_some());
}

#[test]
fn a_sole_variant_pattern_in_let_is_irrefutable() {
    // Refutability is about whether the type has another variant to be, not
    // about the pattern's shape.
    let text = format!(
        "{PRELUDE} pub enum Only [Just] \
         pub fn main(value: Only) -> unit {{ let Just = value; }}"
    );
    assert!(
        !codes(&text).contains(&"E1223_REFUTABLE_PATTERN"),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn a_refutable_component_makes_the_whole_tuple_pattern_refutable() {
    let text = format!(
        "{PRELUDE} pub enum Mode [Fast, Slow] \
         pub fn main(pair: (Mode, i32)) -> unit {{ let (Fast, count) = pair; }}"
    );
    assert!(
        codes(&text).contains(&"E1223_REFUTABLE_PATTERN"),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn a_refutable_pattern_in_match_stays_accepted() {
    // `match` has arms to fall through to; that is the whole difference.
    let text = format!(
        "{PRELUDE} pub enum Mode [Fast, Slow] \
         pub fn main(mode: Mode) -> unit {{ match (mode) {{ Fast => {{ }} Slow => {{ }} }} }}"
    );
    assert!(
        !codes(&text).contains(&"E1223_REFUTABLE_PATTERN"),
        "{:?}",
        codes(&text)
    );
}
