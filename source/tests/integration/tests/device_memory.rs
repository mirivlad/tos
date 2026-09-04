// SPDX-License-Identifier: GPL-3.0-or-later
//! Device memory as a sealed language kind (ADR-0081 §5–§9).
//!
//! What is proved here is the half that does not need hardware: the types, the
//! width-explicit accesses, the version gate, and that an MMIO access lowers to
//! its **own** verifier-visible operation rather than to an ordinary read a
//! compiler would be free to elide, coalesce or repeat.

use tos_ir::{Op, TypeDef};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

fn module(version: &str, body: &str) -> String {
    format!(
        "\
module system.test.device version {version} profile full;

resource [fuel: 65536, stack: 16KiB, allocation: 4KiB, tasks: 1, workers: 1,
          sync: 0, shared: 0B, cleanup: 0, recursion: 8, imports: 4]

{body}
"
    )
}

fn errors(text: &str) -> Vec<tos_core::Diagnostic> {
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    tos_core::Checker::check(&source, &schema)
        .into_iter()
        .filter(|d| d.severity() == tos_core::Severity::Error)
        .collect()
}

fn lower(text: &str) -> tos_ir::Module {
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    let found = tos_core::Checker::check(&source, &schema);
    assert!(
        !found
            .iter()
            .any(|d| d.severity() == tos_core::Severity::Error),
        "checks clean: {found:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("device-memory-test"),
            path: String::from("system/test/device.tos"),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

const READS: &str = "\
pub fn probe(window: MmioRegion) -> u64 {
    let status: u64 = mmio_read_u8(window, 20B);
    let queues: u64 = mmio_read_le_u16(window, 18B);
    return status + queues;
}";

#[test]
fn a_device_access_is_its_own_operation() {
    let module = lower(&module("1.2", READS));
    let ops: Vec<&Op> = module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .map(|instruction| &instruction.op)
        .collect();

    // Two source accesses, two operations — and each carries its own width, so
    // nothing downstream may widen, narrow or merge them (ADR-0081 §9).
    let observations: Vec<(u8, bool)> = ops
        .iter()
        .filter_map(|op| match op {
            Op::MmioRead {
                width,
                little_endian,
                ..
            } => Some((*width, *little_endian)),
            _ => None,
        })
        .collect();
    assert_eq!(observations, vec![(1, true), (2, true)]);

    // And not one of them is an ordinary read.
    assert!(
        !ops.iter().any(|op| matches!(op, Op::Read { .. })),
        "a device access lowered to an ordinary memory read"
    );
}

#[test]
fn the_two_forms_are_distinct_types() {
    let module = lower(&module(
        "1.2",
        "pub fn poke(window: MmioRegionMut) -> unit {\n    mmio_write_le_u32(window, 0B, 1u64);\n}",
    ));
    assert!(
        module.types.contains(&TypeDef::MmioRegionMut),
        "the writable form is not its own type"
    );
    assert!(
        !module
            .types
            .iter()
            .any(|ty| matches!(ty, TypeDef::Region(_))),
        "device memory was recorded as an ordinary region"
    );
}

/// A read-only mapping is read-only in the type. The page table enforces it
/// too, which is the other half (ADR-0081 §10) and not this test's.
#[test]
fn a_read_only_mapping_refuses_a_write() {
    let text = module(
        "1.2",
        "pub fn poke(window: MmioRegion) -> unit {\n    mmio_write_le_u32(window, 0B, 1u64);\n}",
    );
    assert!(
        errors(&text)
            .iter()
            .any(|d| d.code() == "E1215_ARGUMENT_TYPE_MISMATCH"),
        "a write through MmioRegion was accepted: {:?}",
        errors(&text)
    );
}

/// An ordinary region is not device memory, however much both end in pages.
#[test]
fn an_ordinary_region_is_not_a_device_mapping() {
    let text = module(
        "1.2",
        "pub fn probe(r: Region<u8>) -> u64 {\n    return mmio_read_u8(r, 0B);\n}",
    );
    assert!(
        errors(&text)
            .iter()
            .any(|d| d.code() == "E1215_ARGUMENT_TYPE_MISMATCH"),
        "a Region was accepted as a device mapping: {:?}",
        errors(&text)
    );
}

/// The offset is exact `size`, as every other bounded index in this language is.
#[test]
fn the_offset_is_exactly_size() {
    let text = module(
        "1.2",
        "pub fn probe(window: MmioRegion, at: u64) -> u64 {\n    return mmio_read_u8(window, at);\n}",
    );
    assert!(
        errors(&text)
            .iter()
            .any(|d| d.code() == "E1211_INDEX_TYPE_MISMATCH"),
        "a u64 offset was accepted: {:?}",
        errors(&text)
    );
}

/// A module receives the language its own header declares (ADR-0081 §6).
#[test]
fn device_memory_needs_the_minor_that_added_it() {
    for version in ["1.0", "1.1"] {
        let text = module(version, READS);
        let gated: Vec<_> = errors(&text)
            .into_iter()
            .filter(|d| d.code() == "E1608_FEATURE_REQUIRES_LANGUAGE_MINOR")
            .collect();
        assert!(!gated.is_empty(), "{version} silently acquired MMIO");
        assert_eq!(gated[0].field("requires"), Some("2"));
        assert_eq!(gated[0].field("feature"), Some("device memory"));
    }
}

/// The whole artifact, through the independent verifier.
#[test]
fn a_device_accessing_artifact_verifies() {
    let module = lower(&module("1.2", READS));
    assert_eq!(module.header.language_version, "1.2");
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("a device-accessing artifact verifies");
}
