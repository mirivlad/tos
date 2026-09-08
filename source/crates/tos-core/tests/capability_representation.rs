// SPDX-License-Identifier: GPL-3.0-or-later
//! The frontend half of ADR-0085: which capability an import may request.
//!
//! `SYSTEM_INTERFACE_V1` §4.3 separates an interface's identity from the class
//! of TOS Core values that represents it. One consequence lands entirely in
//! source checking: an interface whose representation is not `AsInterface` has
//! **no import that could be one**, because an import is typed
//! `TypeDef::Capability(interface)` and there is nowhere in
//!
//! ```tos
//! import capability platform.dma.Region as region;
//! ```
//!
//! for the element type or the mutability of a `DmaRegion<T>` to come from.

use tos_core::{Checker, Diagnostic, Parser, SourceReader};

const RESOURCE: &str = "resource [fuel: 1000, stack: 8KiB, allocation: 1KiB, tasks: 1, \
     workers: 1, sync: 0, shared: 0B, cleanup: 4, recursion: 4, imports: 1] ";

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

/// A module requesting an authority no startup import can produce.
fn importing(interface: &str) -> String {
    format!(
        "module app.representation version 1.0 profile bootstrap; \
         import capability {interface} as held; {RESOURCE} pub fn main() -> unit {{ }}"
    )
}

#[test]
fn importing_a_capability_no_import_can_produce_is_refused() {
    let text = importing("platform.dma.Region");
    let all = diagnostics(&text);
    let finding = all
        .iter()
        .find(|d| d.code() == "E1503_NONIMPORTABLE_CAPABILITY")
        .unwrap_or_else(|| panic!("{:?}", codes(&text)));
    // Both fields, because a reader has to be told *which* interface and *what*
    // about it: the representation is the reason, and naming only the interface
    // would leave "why can I not ask for this one" to be looked up.
    assert_eq!(finding.field("interface"), Some("platform.dma.Region"));
    assert_eq!(finding.field("representation"), Some("DmaRegionFamily"));
}

/// The diagnostic is about the **representation**, not about the path being a
/// platform one or a DMA one.
#[test]
fn every_ordinary_interface_is_still_importable() {
    for interface in [
        "system.ipc.Endpoint",
        "system.ipc.Reply",
        "system.memory.Authority",
        "system.process.Control",
        "system.process.LaunchPlan",
        "system.process.LaunchPlanBuilder",
        "platform.pci.Bus",
        "platform.pci.FunctionConfig",
        "platform.irq.Source",
    ] {
        let text = importing(interface);
        assert!(
            !codes(&text).contains(&"E1503_NONIMPORTABLE_CAPABILITY"),
            "{interface} is importable and must not be refused: {:?}",
            codes(&text)
        );
    }
}

/// A path no accepted schema declares is **not** this diagnostic.
///
/// Importability is a property of an accepted interface's representation, so a
/// path that names no interface has none — whatever is wrong with such a module
/// is wrong for a different reason, and answering `E1503` would tell a reader
/// the schema declares something it does not.
#[test]
fn a_path_no_schema_declares_is_not_reported_as_nonimportable() {
    let text = importing("nothing.declares.This");
    assert!(
        !codes(&text).contains(&"E1503_NONIMPORTABLE_CAPABILITY"),
        "{:?}",
        codes(&text)
    );
}

/// It is refused at **every** minor, which is what makes it not a feature gate.
///
/// TOS Core 1.3 makes a capability *position* filled by the region family
/// valid; it does not make this declaration valid, because no version of any
/// import could produce that value. A module cannot reach it by declaring a
/// later minor and cannot escape it by declaring an earlier one.
#[test]
fn no_language_minor_makes_the_declaration_valid() {
    for minor in ["1.0", "1.1", "1.2"] {
        let text = format!(
            "module app.representation version {minor} profile bootstrap; \
             import capability platform.dma.Region as held; {RESOURCE} \
             pub fn main() -> unit {{ }}"
        );
        assert!(
            codes(&text).contains(&"E1503_NONIMPORTABLE_CAPABILITY"),
            "{minor}: {:?}",
            codes(&text)
        );
    }
}

/// A module that reaches `platform.dma.Region` through the values that
/// represent it.
///
/// The region is a parameter because nothing produces one yet: `SYSTEM_ABI_V1`
/// operation 30's schema row waits on ADR-0085 §18. The capability *position*
/// is the thing under test, and a parameter fills it exactly as an
/// operation-produced value would.
fn reaching(minor: &str, argument: &str) -> String {
    format!(
        "module app.representation version {minor} profile full; \
         resource [fuel: 1000, stack: 8KiB, allocation: 1KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 4, recursion: 4, imports: 0] \
         extern fn dma_device_address(region: platform.dma.Region, offset: size) \
             -> Result<u64, i64> uses [platform.dma.Region]; \
         pub fn translate(area: DmaRegion<mut u64>) -> Result<u64, i64> \
             uses [platform.dma.Region] {{ return dma_device_address({argument}, 0B); }}"
    )
}

/// **A 1.2 module does not receive the 1.3 rule**, however capable the frontend
/// compiling it is (ADR-0085 §13, `docs/42` §1).
///
/// The diagnostic names the feature and the minor it needs rather than the form
/// it happened to see, which is what makes it readable by somebody who did not
/// know the rule existed.
#[test]
fn a_module_below_the_minor_may_not_reach_a_represented_interface() {
    for minor in ["1.0", "1.1", "1.2"] {
        let text = reaching(minor, "area");
        let all = diagnostics(&text);
        // Selected by the **feature** rather than by being the first `E1608`:
        // a 1.0 module writing a dotted effect is also short of ADR-0080's
        // minor, and that is a true and separate finding about the same line.
        let finding = all
            .iter()
            .find(|d| {
                d.code() == "E1608_FEATURE_REQUIRES_LANGUAGE_MINOR"
                    && d.field("feature") == Some("capability representation")
            })
            .unwrap_or_else(|| panic!("{minor}: {:?}", codes(&text)));
        assert_eq!(finding.field("declared"), Some(&minor[2..3]));
        assert_eq!(finding.field("requires"), Some("3"));
    }
}

/// And every module that reaches no represented interface is untouched.
///
/// §14: the amendment makes strictly more programs well-typed and rejects none
/// that were, so an ordinary 1.0 module must not acquire a diagnostic from a
/// feature it does not use.
#[test]
fn an_ordinary_module_gains_no_version_diagnostic() {
    let text = "module app.ordinary version 1.0 profile full; \
         import capability system.ipc.Endpoint as endpoint; \
         resource [fuel: 1000, stack: 8KiB, allocation: 1KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 4, recursion: 4, imports: 1] \
         extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [endpoint]; \
         pub fn main() -> i64 uses [endpoint] { return endpoint_send(endpoint, 8u64); }";
    assert_eq!(codes(text), Vec::<&str>::new());
}

/// **A 1.3 module reaches the interface through the values that represent it.**
///
/// `DmaRegion<T>` and `DmaRegion<mut T>` both satisfy a `platform.dma.Region`
/// position, for any element type: the representation is the family, and an
/// element type is not part of membership in it.
#[test]
fn a_one_three_module_may_fill_the_position_with_either_region_mode() {
    for written in [
        "DmaRegion<mut u64>",
        "DmaRegion<u64>",
        "DmaRegion<mut u8>",
        "DmaRegion<i32>",
    ] {
        let text = format!(
            "module app.representation version 1.3 profile full; \
             resource [fuel: 1000, stack: 8KiB, allocation: 1KiB, tasks: 1, workers: 1, \
             sync: 0, shared: 0B, cleanup: 4, recursion: 4, imports: 0] \
             extern fn dma_device_address(region: platform.dma.Region, offset: size) \
                 -> Result<u64, i64> uses [platform.dma.Region]; \
             pub fn translate(area: {written}) -> Result<u64, i64> \
                 uses [platform.dma.Region] {{ return dma_device_address(area, 0B); }}"
        );
        assert_eq!(codes(&text), Vec::<&str>::new(), "{written}");
    }
}

/// **Use is not consumption** (ADR-0085 §8): one binding, used as many times as
/// the module writes it.
///
/// No operation of this interface consumes its receiver, and affinity is about
/// how many *values* name the object — using one value twice creates no second
/// value. The `capability_attenuate`/`capability_release` pair has always relied
/// on this; here it is for the interface the amendment adds.
#[test]
fn one_region_binding_may_be_used_more_than_once() {
    let text = "module app.representation version 1.3 profile full; \
         resource [fuel: 1000, stack: 8KiB, allocation: 1KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 4, recursion: 4, imports: 0] \
         extern fn dma_device_address(region: platform.dma.Region, offset: size) \
             -> Result<u64, i64> uses [platform.dma.Region]; \
         extern fn capability_release(region: platform.dma.Region) -> i64 \
             uses [platform.dma.Region]; \
         pub fn translate(area: DmaRegion<mut u64>) -> i64 uses [platform.dma.Region] { \
             let first: Result<u64, i64> = dma_device_address(area, 0B); \
             let second: Result<u64, i64> = dma_device_address(area, 64B); \
             return capability_release(area); }";
    assert_eq!(codes(text), Vec::<&str>::new());
}

/// A region is not accepted at an `AsInterface` position (§16.2), and the
/// refusal is the frontend's rather than the verifier's.
#[test]
fn a_region_does_not_fill_an_ordinary_capability_position() {
    let text = "module app.representation version 1.3 profile full; \
         import capability system.ipc.Endpoint as endpoint; \
         resource [fuel: 1000, stack: 8KiB, allocation: 1KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 4, recursion: 4, imports: 1] \
         extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [endpoint]; \
         pub fn main(area: DmaRegion<mut u64>) -> i64 uses [endpoint] { \
             return endpoint_send(area, 8u64); }";
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    // It checks clean — the checker has no types with which to tell a region
    // from an ordinary local at a call site — and the **lowerer** refuses to
    // build the instruction, which is where the types are.
    assert!(Checker::check(&source, &schema)
        .iter()
        .all(|d| d.severity() != tos_core::Severity::Error));
    let gap = tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("capability-representation-test"),
            path: String::from("app/representation.tos"),
            content_id: String::from("test"),
            dependency_digest: String::from("test"),
            capability_interface_digest: String::from("test"),
        },
    )
    .expect_err("a region does not fill an ordinary capability position");
    assert!(
        gap.construct.contains("declares no operation of this name"),
        "{gap:?}"
    );
}
