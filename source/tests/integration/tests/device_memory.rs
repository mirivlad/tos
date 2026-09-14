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

// ---------------------------------------------------------------- forged IR

// The checks above are the frontend's. ADR-0081 §8 states the verifier's, and
// they are not the same obligation: a hand-written artifact never met the
// frontend that would have refused it.
//
// > The verifier independently proves the operand is exactly an MMIO region
// > kind, that a write names the mutable form, and that the enclosing artifact
// > declares a language version in which the operation exists.
//
// The decision said that and the implementation did not do it — `Op::MmioRead`
// and `Op::MmioWrite` had no verifier arm at all. Each case below starts from
// a module that verifies and damages exactly one fact, so what the refusal is
// about is the damage and not the fixture.

const WRITES: &str = "\
pub fn drive(window: MmioRegionMut) -> unit {
    mmio_write_le_u32(window, 4B, 1u64);
}";

/// The verifier's verdict on a module, as a code.
fn verdict(module: &tos_ir::Module) -> String {
    match verify(module, &ResolutionSnapshot::default(), &Limits::default()) {
        Ok(_) => String::from("accepted"),
        Err(finding) => String::from(finding.code),
    }
}

/// Finds the first device access, as `(function, block, index)`.
fn first_access(module: &tos_ir::Module) -> (usize, usize, usize) {
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, instruction) in block.instructions.iter().enumerate() {
                if matches!(instruction.op, Op::MmioRead { .. } | Op::MmioWrite { .. }) {
                    return (f, b, i);
                }
            }
        }
    }
    panic!("the fixture contains a device access");
}

/// Interns a type and returns its id.
fn interned(module: &mut tos_ir::Module, wanted: TypeDef) -> usize {
    match module.types.iter().position(|ty| *ty == wanted) {
        Some(found) => found,
        None => {
            module.types.push(wanted);
            module.types.len() - 1
        }
    }
}

/// Puts a value slot of the given type in place of one operand.
fn retype_operand(module: &mut tos_ir::Module, wanted: TypeDef, position: usize) {
    let ty = interned(module, wanted);
    let (f, b, i) = first_access(module);
    module.functions[f].values.push(ty);
    let slot = module.functions[f].values.len() - 1;
    let operand = tos_ir::Operand::Value(slot);
    match &mut module.functions[f].blocks[b].instructions[i].op {
        Op::MmioRead { region, offset, .. } => match position {
            0 => *region = operand,
            _ => *offset = operand,
        },
        Op::MmioWrite {
            region,
            offset,
            value,
            ..
        } => match position {
            0 => *region = operand,
            1 => *offset = operand,
            _ => *value = operand,
        },
        _ => panic!("not a device access"),
    }
}

#[test]
fn a_forged_access_of_an_unaccepted_width_is_refused() {
    let mut module = lower(&module("1.2", READS));
    assert_eq!(verdict(&module), "accepted");
    let (f, b, i) = first_access(&module);
    if let Op::MmioRead { width, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        *width = 3;
    }
    assert_eq!(verdict(&module), "V2021_REGION");
}

/// **The byte order is part of the form too.** No accepted operation performs a
/// big-endian device access, so an artifact claiming one names an operation the
/// language does not have — and the engine would otherwise perform it.
#[test]
fn a_forged_access_of_an_unaccepted_byte_order_is_refused() {
    let mut module = lower(&module("1.2", READS));
    let (f, b, i) = first_access(&module);
    if let Op::MmioRead { little_endian, .. } =
        &mut module.functions[f].blocks[b].instructions[i].op
    {
        *little_endian = false;
    }
    assert_eq!(verdict(&module), "V2021_REGION");
}

#[test]
fn a_forged_read_through_an_ordinary_region_is_refused() {
    let mut module = lower(&module("1.2", READS));
    let element = interned(&mut module, TypeDef::Int(tos_ir::IntKind::U8));
    retype_operand(&mut module, TypeDef::Region(element), 0);
    assert_eq!(verdict(&module), "V2021_REGION");
}

/// **A write requires the mutable form**, in the artifact and not only in
/// source: a read-only grant is read-only in the page table too.
#[test]
fn a_forged_write_through_a_read_only_mapping_is_refused() {
    let mut module = lower(&module("1.2", WRITES));
    assert_eq!(verdict(&module), "accepted");
    retype_operand(&mut module, TypeDef::MmioRegion, 0);
    assert_eq!(verdict(&module), "V2021_REGION");
}

#[test]
fn a_forged_offset_that_is_not_size_is_refused() {
    let mut module = lower(&module("1.2", READS));
    retype_operand(&mut module, TypeDef::Int(tos_ir::IntKind::I64), 1);
    assert_eq!(verdict(&module), "V2021_REGION");
}

/// ADR-0081 §7's carrier rule: a write takes exact `u64` at every width.
#[test]
fn a_forged_written_value_that_is_not_u64_is_refused() {
    let mut module = lower(&module("1.2", WRITES));
    retype_operand(&mut module, TypeDef::Int(tos_ir::IntKind::U32), 2);
    assert_eq!(verdict(&module), "V2021_REGION");
}

/// And a read answers exact `u64` at every width, so an artifact claiming the
/// transaction's width as the value's type is refused.
#[test]
fn a_forged_read_result_that_is_not_u64_is_refused() {
    let mut module = lower(&module("1.2", READS));
    let narrow = interned(&mut module, TypeDef::Int(tos_ir::IntKind::U8));
    let (f, b, i) = first_access(&module);
    let instruction = &mut module.functions[f].blocks[b].instructions[i];
    instruction.ty = narrow;
    let result = instruction.result.expect("a read defines a value");
    module.functions[f].values[result] = narrow;
    assert_eq!(verdict(&module), "V2021_REGION");
}

/// A write produces `unit`, and an artifact claiming a value from it would have
/// a value the engine never writes.
#[test]
fn a_forged_write_claiming_a_value_is_refused() {
    let mut module = lower(&module("1.2", WRITES));
    let value = interned(&mut module, TypeDef::Int(tos_ir::IntKind::U64));
    let (f, b, i) = first_access(&module);
    let instruction = &mut module.functions[f].blocks[b].instructions[i];
    instruction.ty = value;
    let result = instruction.result.expect("a write defines a unit value");
    module.functions[f].values[result] = value;
    assert_eq!(verdict(&module), "V2021_REGION");
}

/// **The declared minor is a verifier obligation.** The artifact is correct in
/// every other way; what is wrong with it is that its header claims a language
/// the operation does not belong to. No frontend produces one — `E1608` refuses
/// the source — which is the point.
#[test]
fn a_forged_access_below_its_minor_is_refused() {
    for version in ["1.0", "1.1"] {
        let mut module = lower(&module("1.2", READS));
        module.header.language_version = String::from(version);
        assert_eq!(verdict(&module), "V2021_REGION", "at {version}");
    }
}
