// SPDX-License-Identifier: GPL-3.0-or-later
//! Region modes, `share`, and what each of the four facts costs (ADR-0037).

use tos_core::{Checker, Diagnostic, Parser, SourceReader};

const PRELUDE: &str = "module app.regions version 1.0 profile bootstrap; \
     resource [fuel: 1000, stack: 8KiB, allocation: 1KiB, tasks: 2, workers: 1, \
     sync: 0, shared: 4KiB, cleanup: 4, recursion: 4, imports: 0] ";

fn diagnostics(text: &str) -> Vec<Diagnostic> {
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .unwrap_or_else(|| panic!("the fixture must parse: {text}"));
    Checker::check(&source, &schema)
}

fn codes(text: &str) -> Vec<&'static str> {
    diagnostics(text).iter().map(Diagnostic::code).collect()
}

#[test]
fn a_mutably_granted_region_is_written_with_mut_inside_the_type_argument() {
    let text = format!("{PRELUDE} pub fn main(area: Region<mut i32>) -> unit {{ }}");
    assert_eq!(codes(&text), Vec::<&str>::new());
}

#[test]
fn mut_is_not_a_general_type_qualifier() {
    // ADR-0037 section 1: `mut` inside a type argument is admitted for exactly
    // two constructors. Anywhere else it is not a type qualifier at all.
    let text = format!("{PRELUDE} pub fn main(value: Option<mut i32>) -> unit {{ }}");
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid");
    assert!(
        Parser::parse_schema(&source).has_errors(),
        "`mut` outside a region type argument must not parse"
    );
}

#[test]
fn sharing_an_immutable_region_is_accepted_and_yields_a_shared_handle() {
    let text = format!(
        "{PRELUDE} pub fn main(area: Region<i32>) -> Shared<Region<i32>> {{ return share(area); }}"
    );
    assert_eq!(codes(&text), Vec::<&str>::new());
}

#[test]
fn sharing_a_mutable_region_is_an_argument_type_mismatch() {
    let text = format!(
        "{PRELUDE} pub fn main(area: Region<mut i32>) -> unit {{ let handle = share(area); }}"
    );
    let all = diagnostics(&text);
    let finding = all
        .iter()
        .find(|d| d.code() == "E1215_ARGUMENT_TYPE_MISMATCH")
        .unwrap_or_else(|| panic!("{:?}", codes(&text)));
    assert_eq!(finding.field("callee"), Some("share"));
    // The written form is spelled back: a diagnostic naming a type nobody typed
    // sends the reader looking for it.
    assert_eq!(finding.field("actual"), Some("Region<mut i32>"));
}

#[test]
fn sharing_a_dma_region_is_an_argument_type_mismatch_in_either_mode() {
    for written in ["DmaRegion<i32>", "DmaRegion<mut i32>"] {
        let text = format!(
            "{PRELUDE} pub fn main(area: {written}) -> unit {{ let handle = share(area); }}"
        );
        assert!(
            codes(&text).contains(&"E1215_ARGUMENT_TYPE_MISMATCH"),
            "{written}: {:?}",
            codes(&text)
        );
    }
}

#[test]
fn sharing_something_holding_a_guard_is_refused() {
    // Transitive immutability: `T` and everything reachable from it must hold
    // no mutable region, no mutable borrow and no guard.
    let text = format!(
        "{PRELUDE} pub fn main(guard: MutexGuard<i32>) -> unit {{ let handle = share(guard); }}"
    );
    assert!(
        codes(&text).contains(&"E1215_ARGUMENT_TYPE_MISMATCH"),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn using_a_region_after_share_consumed_it_is_a_use_after_move() {
    let text = format!(
        "{PRELUDE} fn take(area: Region<i32>) -> unit {{ }} \
         pub fn main(area: Region<i32>) -> unit {{ let handle = share(area); take(area); }}"
    );
    assert!(
        codes(&text).contains(&"E1301_USE_AFTER_MOVE"),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn writing_through_an_immutably_granted_region_is_refused() {
    let text = format!("{PRELUDE} pub fn main(area: Region<i32>) -> unit {{ area[0] = 1i32; }}");
    let all = diagnostics(&text);
    let finding = all
        .iter()
        .find(|d| d.code() == "E1201_ASSIGN_TO_IMMUTABLE")
        .unwrap_or_else(|| panic!("{:?}", codes(&text)));
    assert_eq!(finding.field("reason"), Some("immutably granted region"));
}

#[test]
fn a_mut_binding_cannot_launder_an_immutable_grant() {
    // The region's declared mode decides, not the binding form. A `let mut`
    // handle may be rebound; nothing may be written through it.
    let text = format!(
        "{PRELUDE} pub fn main(given: Region<i32>) -> unit {{ \
         let mut area = given; area[0] = 1i32; }}"
    );
    assert!(
        codes(&text).contains(&"E1201_ASSIGN_TO_IMMUTABLE"),
        "{:?}",
        codes(&text)
    );
}

#[test]
fn writing_through_a_mutably_granted_region_is_accepted() {
    let text = format!(
        "{PRELUDE} pub fn main(borrow mut area: Region<mut i32>) -> unit {{ area[0] = 1i32; }}"
    );
    assert_eq!(codes(&text), Vec::<&str>::new());
}

#[test]
fn capturing_a_non_transferable_region_into_a_task_names_the_mode() {
    for (written, reason) in [
        ("Region<mut i32>", "mutable region"),
        ("DmaRegion<i32>", "DMA region"),
        ("DmaRegion<mut i32>", "DMA region"),
    ] {
        let text = format!(
            "{PRELUDE} pub fn main(area: {written}) -> unit {{ \
             let child = spawn parallel {{ let inner = area; }}; let done = join child; }}"
        );
        let all = diagnostics(&text);
        let finding = all
            .iter()
            .find(|d| d.code() == "E1304_INVALID_TASK_CAPTURE")
            .unwrap_or_else(|| panic!("{written}: {:?}", codes(&text)));
        assert_eq!(finding.field("reason"), Some(reason), "{written}");
    }
}

#[test]
fn moving_an_immutable_region_into_one_task_is_accepted() {
    let text = format!(
        "{PRELUDE} pub fn main(area: Region<i32>) -> unit {{ \
         let child = spawn parallel {{ let inner = area; }}; let done = join child; }}"
    );
    assert_eq!(codes(&text), Vec::<&str>::new());
}
