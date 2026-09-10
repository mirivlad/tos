// SPDX-License-Identifier: GPL-3.0-or-later
//! DMA publication and consumption, end to end (ADR-0086 §16).
//!
//! The two operations are a language surface, an IR form, a container tag and a
//! host boundary, and the conformance set is written across all four because a
//! claim proved in one of them is not proved in the others. So this file has
//! three kinds of case, and keeps them apart:
//!
//! - **acceptance**, from canonical source: all four combinations of direction
//!   and mutability lower, verify, and survive a round trip;
//! - **frontend refusal**, from canonical source: an operand that is not a DMA
//!   region is refused where it is written;
//! - **forged-artifact refusal**, from source that is lowered and then damaged.
//!   A verifier that only meets what this frontend emits proves nothing about a
//!   frontend somebody else wrote, so every operand type ADR-0086 §15 names is
//!   forged in turn — including the ones no source could produce.
//!
//! **What none of it demonstrates** is that x86-64 needed the ordering point.
//! It cannot: the architecture does not permit the executions the primitive
//! exists to forbid, so no run on this profile can distinguish a correct
//! implementation from an absent one. That evidence is semantic and structural
//! — the operation survives the pipeline, arrives at the host exactly once, and
//! lowers to the instructions ADR-0086 §11 accepts, which
//! `scripts/tests/check-dma-ordering-backend.sh` reads out of the built image.

use tos_ir::{DmaSyncDirection, Module, Op, Operand, TypeDef};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

/// A 1.4 module that publishes, writes, consumes and reads one region.
///
/// The region is a parameter: what the cases below need is a value of the
/// family with a real type in the artifact's own table, and how it was obtained
/// is ADR-0084's business rather than this ADR's.
const MODULE: &str = "\
module system.test.ordering version 1.4 profile full;

resource [
    fuel: 1024,
    stack: 4KiB,
    allocation: 1KiB,
    tasks: 1,
    workers: 1,
    sync: 0,
    shared: 0B,
    cleanup: 0,
    recursion: 4,
    imports: 1
]

pub fn ring(region: DmaRegion<mut u8>) -> u8 {
    region[0B] = 1u8;
    dma_publish(region);
    region[1B] = 2u8;
    dma_publish(region);
    dma_consume(region);
    return region[0B];
}
";

/// One module of the given version, around the given item.
fn source_of(version: &str, body: &str) -> String {
    format!(
        "module system.test.ordering version {version} profile full;\n\
         \n\
         resource [\n\
             fuel: 1024,\n\
             stack: 4KiB,\n\
             allocation: 1KiB,\n\
             tasks: 1,\n\
             workers: 1,\n\
             sync: 0,\n\
             shared: 0B,\n\
             cleanup: 0,\n\
             recursion: 4,\n\
             imports: 1\n\
         ]\n\
         \n\
         {body}\n"
    )
}

/// Type-checks one module and returns its diagnostics.
fn check(text: &str) -> Vec<tos_core::Diagnostic> {
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    tos_core::Checker::check(&source, &schema)
}

fn errors(text: &str) -> Vec<tos_core::Diagnostic> {
    check(text)
        .into_iter()
        .filter(|d| d.severity() == tos_core::Severity::Error)
        .collect()
}

fn lower(text: &str) -> Module {
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    let diagnostics = tos_core::Checker::check(&source, &schema);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.severity() == tos_core::Severity::Error),
        "the module checks clean: {diagnostics:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("dma-ordering-test"),
            path: String::from("system/test/ordering.tos"),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

fn accepts(module: &Module) {
    verify(module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("the artifact verifies");
}

fn refuse(module: &Module) -> tos_verifier::Finding {
    verify(module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("the artifact is refused")
}

/// Every `DmaSync` in a module, as (function, block, instruction).
fn sync_sites(module: &Module) -> Vec<(usize, usize, usize)> {
    let mut found = Vec::new();
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, instruction) in block.instructions.iter().enumerate() {
                if matches!(instruction.op, Op::DmaSync { .. }) {
                    found.push((f, b, i));
                }
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

/// §16 — all four combinations of direction and mutability are accepted.
///
/// **Both mutabilities, deliberately** (ADR-0086 §3). Mutability is current CPU
/// write authority and a `DmaSync` is a visibility boundary rather than a
/// write, so requiring `mut` would be a type rule with no obligation behind it.
#[test]
fn both_directions_over_both_mutabilities_are_accepted() {
    for (direction, mutability) in [
        ("dma_publish", "DmaRegion<u8>"),
        ("dma_publish", "DmaRegion<mut u8>"),
        ("dma_consume", "DmaRegion<u8>"),
        ("dma_consume", "DmaRegion<mut u8>"),
    ] {
        let text = source_of(
            "1.4",
            &format!(
                "pub fn edge(region: {mutability}) -> i64 {{ {direction}(region); return 0i64; }}"
            ),
        );
        assert!(errors(&text).is_empty(), "{direction} over {mutability}");
        accepts(&lower(&text));
    }
}

/// §16 — the element type is irrelevant to synchronisation.
///
/// Operation 30 produces only `DmaRegion<mut u8>` today (ADR-0085 §18), and the
/// ordering rule is deliberately not written to that: it is over the closed
/// family, so a region of any accessible element synchronises.
#[test]
fn every_element_type_of_the_family_synchronises() {
    for element in ["u8", "u16", "u32", "u64", "i8", "i32", "i64", "bool"] {
        let text = source_of(
            "1.4",
            &format!(
                "pub fn edge(region: DmaRegion<mut {element}>) -> i64 \
                 {{ dma_publish(region); dma_consume(region); return 0i64; }}"
            ),
        );
        assert!(errors(&text).is_empty(), "element {element}");
        accepts(&lower(&text));
    }
}

/// §16 — the operation consumes no ownership, so the region outlives it.
///
/// This is the property a driver's whole use depends on: a ring is published
/// once per batch, for the life of the queue. An operation that took ownership
/// would be usable once, which is not an ordering primitive at all.
#[test]
fn the_region_is_usable_after_both_operations() {
    let module = lower(MODULE);
    accepts(&module);
    let sites = sync_sites(&module);
    assert_eq!(sites.len(), 3, "three ordering points, as written");
    // The source writes, publishes, writes, publishes, consumes and then reads
    // — so a region that had been consumed by the first publish could not have
    // reached the last read, and the module would not have lowered.
    let (f, b, _) = sites[0];
    let after = &module.functions[f].blocks[b].instructions;
    assert!(
        after
            .iter()
            .any(|instruction| matches!(instruction.op, Op::Write { .. })),
        "a write follows the first publish"
    );
}

/// §16 — the two directions are different programs, and the artifact says so.
#[test]
fn publish_and_consume_differ_in_digest_and_in_bytes() {
    let publish = lower(&source_of(
        "1.4",
        "pub fn edge(region: DmaRegion<mut u8>) -> i64 { dma_publish(region); return 0i64; }",
    ));
    let consume = lower(&source_of(
        "1.4",
        "pub fn edge(region: DmaRegion<mut u8>) -> i64 { dma_consume(region); return 0i64; }",
    ));
    assert_ne!(
        tos_ir::digest::module_digest(&publish),
        tos_ir::digest::module_digest(&consume),
        "the direction is part of a module's identity"
    );
    let (publish_bytes, _) = tos_image::encode(&publish);
    let (consume_bytes, _) = tos_image::encode(&consume);
    assert_ne!(publish_bytes, consume_bytes);
    assert_eq!(
        publish_bytes.len(),
        consume_bytes.len(),
        "they differ in the discriminator and in nothing else"
    );
}

/// §14 — the operation is written with tag **40**, and the direction follows it
/// as a closed discriminator.
///
/// Read out of the bytes rather than trusted: a renumbering that was consistent
/// between writer and parser would round-trip and rehash the same, so the tag
/// itself has to be looked at.
#[test]
fn the_operation_is_encoded_as_tag_forty() {
    for (direction, discriminator) in [("dma_publish", 0u8), ("dma_consume", 1u8)] {
        let module = lower(&source_of(
            "1.4",
            &format!("pub fn edge(region: DmaRegion<mut u8>) -> i64 {{ {direction}(region); return 0i64; }}"),
        ));
        let (bytes, _) = tos_image::encode(&module);
        let at = bytes
            .windows(2)
            .position(|pair| pair[0] == 40)
            .expect("the operation tag is in the stream");
        assert!(
            bytes[at..]
                .iter()
                .take(8)
                .any(|byte| *byte == discriminator),
            "the direction discriminator follows the tag"
        );
    }
}

/// §16 — a round trip through the container preserves both directions.
#[test]
fn both_directions_survive_encode_and_decode() {
    let module = lower(MODULE);
    let (bytes, _) = tos_image::encode(&module);
    let parsed = tos_image::parse(
        &bytes,
        &tos_image::ParseLimits {
            table_entries: 65_536,
            modules: 256,
            fields: 1024,
            parameters: 128,
            blocks_per_function: 4096,
            instructions_per_block: 65_536,
            source_map_entries: 262_144,
        },
    )
    .expect("the image parses");
    let directions: Vec<DmaSyncDirection> = sync_sites(&parsed)
        .into_iter()
        .map(
            |(f, b, i)| match parsed.functions[f].blocks[b].instructions[i].op {
                Op::DmaSync { direction, .. } => direction,
                _ => unreachable!("the site is a DmaSync"),
            },
        )
        .collect();
    assert_eq!(
        directions,
        alloc::vec![
            DmaSyncDirection::Publish,
            DmaSyncDirection::Publish,
            DmaSyncDirection::Consume
        ]
    );
    assert_eq!(
        tos_ir::digest::module_digest(&parsed),
        tos_ir::digest::module_digest(&module),
        "decoding recovers the module, not something like it"
    );
}

// ---------------------------------------------------------------------------
// Frontend refusal
// ---------------------------------------------------------------------------

/// §16 — an operand that is not a DMA region is refused where it is written.
#[test]
fn a_wrong_operand_family_is_refused_in_source() {
    for wrong in [
        "region: Region<mut u8>",
        "region: MmioRegionMut",
        "region: MmioRegion",
        "region: u64",
        "region: bool",
    ] {
        let text = source_of(
            "1.4",
            &format!("pub fn edge({wrong}) -> i64 {{ dma_publish(region); return 0i64; }}"),
        );
        let found = errors(&text);
        assert!(
            found
                .iter()
                .any(|d| d.code() == "E1215_ARGUMENT_TYPE_MISMATCH"),
            "{wrong}: {found:?}"
        );
    }
}

/// §16 — a 1.3 module writing either operation is refused by the feature gate.
///
/// **A module receives the language its header declared** (`docs/42` §1). The
/// same body is accepted at 1.4 and refused at 1.3, which is the whole of what
/// the gate is for.
#[test]
fn a_1_3_module_using_the_feature_is_refused() {
    for direction in ["dma_publish", "dma_consume"] {
        let text = source_of(
            "1.3",
            &format!("pub fn edge(region: DmaRegion<mut u8>) -> i64 {{ {direction}(region); return 0i64; }}"),
        );
        let gated: Vec<_> = errors(&text)
            .into_iter()
            .filter(|d| d.code() == "E1608_FEATURE_REQUIRES_LANGUAGE_MINOR")
            .collect();
        assert_eq!(gated.len(), 1, "{direction}");
        assert_eq!(gated[0].field("feature"), Some("DMA ordering"));
        assert_eq!(gated[0].field("declared"), Some("3"));
        assert_eq!(gated[0].field("requires"), Some("4"));
    }
}

/// §16 — the gate reaches a call however deeply it is nested.
#[test]
fn the_feature_gate_reaches_a_nested_call() {
    let text = source_of(
        "1.3",
        "pub fn edge(region: DmaRegion<mut u8>, flag: bool) -> i64 \
         { if (flag) { while (flag) { dma_consume(region); } } return 0i64; }",
    );
    assert!(errors(&text)
        .iter()
        .any(|d| d.code() == "E1608_FEATURE_REQUIRES_LANGUAGE_MINOR"));
}

// ---------------------------------------------------------------------------
// Forged-artifact refusal
// ---------------------------------------------------------------------------

/// Points the first `DmaSync`'s operand at a value whose type is forged to `ty`.
fn forge_operand_type(module: &mut Module, ty: TypeDef) {
    let (f, b, i) = sync_sites(module)[0];
    module.types.push(ty);
    let forged = module.types.len() - 1;
    let value = module.functions[f].values.len();
    module.functions[f].values.push(forged);
    if let Op::DmaSync { region, .. } = &mut module.functions[f].blocks[b].instructions[i].op {
        *region = Operand::Value(value);
    }
}

/// §15 obligation 3 and 4 — **every other type refuses**, including the ones no
/// source could write.
///
/// The operand's type is read out of the artifact's own table, so the forgery
/// is done there: a value of the forged type, and the operation pointed at it.
/// A verifier that asked the producer what the type was would pass all of
/// these.
#[test]
fn a_forged_operand_of_any_other_type_is_refused() {
    for ty in [
        TypeDef::Unit,
        TypeDef::Bool,
        TypeDef::Int(tos_ir::IntKind::U64),
        TypeDef::Size,
        TypeDef::Region(0),
        TypeDef::RegionMut(0),
        TypeDef::MmioRegion,
        TypeDef::MmioRegionMut,
        TypeDef::Capability(String::from("platform.dma.Region")),
        TypeDef::Shared(0),
        TypeDef::Task(0),
        TypeDef::Mutex(0),
        TypeDef::MutexGuard(0),
        TypeDef::Tuple(alloc::vec![0]),
        TypeDef::Slice(0),
    ] {
        let mut module = lower(MODULE);
        let named = format!("{ty:?}");
        forge_operand_type(&mut module, ty);
        let finding = refuse(&module);
        assert_eq!(finding.code, "V2021_REGION", "{named}");
        assert!(
            finding.detail.contains("not a DmaRegion"),
            "{named}: {finding:?}"
        );
    }
}

/// §15 obligation 6 — the operation produces `unit`.
///
/// An artifact claiming a value from it would have a value the engine never
/// writes, and every later use of that value would be reading something that
/// was never produced.
#[test]
fn a_forged_result_type_is_refused() {
    let mut module = lower(MODULE);
    let (f, b, i) = sync_sites(&module)[0];
    module.types.push(TypeDef::Int(tos_ir::IntKind::U64));
    let forged = module.types.len() - 1;
    module.functions[f].blocks[b].instructions[i].ty = forged;
    if let Some(result) = module.functions[f].blocks[b].instructions[i].result {
        module.functions[f].values[result] = forged;
    }
    let finding = refuse(&module);
    assert_eq!(finding.code, "V2021_REGION");
    assert!(finding.detail.contains("other than unit"), "{finding:?}");
}

/// §16 — a **forged 1.3 artifact** carrying the operation is refused by the
/// verifier, independently of the frontend gate that would have caught it.
///
/// The two refusals are separate obligations: a hand-written artifact never met
/// the frontend at all.
#[test]
fn a_forged_1_3_artifact_carrying_the_operation_is_refused() {
    let mut module = lower(MODULE);
    module.header.language_version = String::from("1.3");
    let finding = refuse(&module);
    assert_eq!(finding.code, "V2021_REGION");
    assert!(
        finding.detail.contains("declared language version 1.3"),
        "{finding:?}"
    );
}

/// §15 — the version is checked **last**, so a defect that would be there at
/// any minor is reported as itself rather than as a version problem.
#[test]
fn a_wrong_operand_in_a_1_3_artifact_reports_the_operand() {
    let mut module = lower(MODULE);
    module.header.language_version = String::from("1.3");
    forge_operand_type(&mut module, TypeDef::MmioRegionMut);
    let finding = refuse(&module);
    assert!(
        finding.detail.contains("not a DmaRegion"),
        "the operand is named, not the version: {finding:?}"
    );
}

/// §16 — a direction the container does not know is refused, never defaulted.
#[test]
fn a_malformed_direction_is_refused_by_the_parser() {
    let module = lower(&source_of(
        "1.4",
        "pub fn edge(region: DmaRegion<mut u8>) -> i64 { dma_publish(region); return 0i64; }",
    ));
    let (bytes, _) = tos_image::encode(&module);
    let at = bytes
        .windows(2)
        .position(|pair| pair[0] == 40)
        .expect("the operation tag is in the stream");
    // The discriminator is the byte after the tag's operand. Sweep the few
    // bytes that follow and damage the first `0`, which is the Publish value.
    let mut damaged = bytes.clone();
    let discriminator = (at + 1..at + 6)
        .find(|index| damaged[*index] == 0)
        .expect("the Publish discriminator follows the tag");
    damaged[discriminator] = 7;
    tos_image::reseal(&mut damaged);
    let error = tos_image::parse(
        &damaged,
        &tos_image::ParseLimits {
            table_entries: 65_536,
            modules: 256,
            fields: 1024,
            parameters: 128,
            blocks_per_function: 4096,
            instructions_per_block: 65_536,
            source_map_entries: 262_144,
        },
    )
    .expect_err("an unknown discriminator is refused");
    assert!(format!("{error:?}").contains("DmaSync") || format!("{error:?}").contains("Unknown"));
}

// ---------------------------------------------------------------------------
// Ordering litmus vectors
// ---------------------------------------------------------------------------

/// The three canonical sequences of ADR-0086 §16, as **artifact order**.
///
/// **This is semantic and compiler evidence, and it is not weak-memory
/// falsification.** x86-64 does not permit the executions the primitive
/// forbids, so no run on this profile can distinguish a correct implementation
/// from an absent one. What can be checked — and is what the operations are
/// for — is that the ordering point survives the whole pipeline and lands
/// between the accesses the source put it between, in the artifact a backend
/// will read.
#[test]
fn the_ordering_vectors_lower_in_the_order_they_are_written() {
    let module = lower(&source_of(
        "1.4",
        "pub fn queue(region: DmaRegion<mut u8>, notify: MmioRegionMut) -> i64 {\n\
         \x20   region[0B] = 7u8;\n\
         \x20   dma_publish(region);\n\
         \x20   region[1B] = 1u8;\n\
         \x20   dma_publish(region);\n\
         \x20   mmio_write_le_u16(notify, 0B, 3u64);\n\
         \x20   dma_consume(region);\n\
         \x20   let used: u8 = region[1B];\n\
         \x20   return 0i64;\n\
         }",
    ));
    accepts(&module);
    let function = module
        .functions
        .iter()
        .find(|function| function.signature.name.ends_with("queue"))
        .expect("the function is in the artifact");
    let mut shape = Vec::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            let name = match &instruction.op {
                Op::Write { .. } => "write",
                Op::Read { .. } => "read",
                Op::MmioWrite { .. } => "notify",
                Op::DmaSync {
                    direction: DmaSyncDirection::Publish,
                    ..
                } => "publish",
                Op::DmaSync {
                    direction: DmaSyncDirection::Consume,
                    ..
                } => "consume",
                _ => continue,
            };
            shape.push(name);
        }
    }
    assert_eq!(
        shape,
        alloc::vec![
            // descriptor writes -> publish -> ownership marker write
            "write", "publish", "write", //
            // ownership marker write -> publish -> MMIO notification
            "publish", "notify", //
            // completion -> consume -> DMA data reads
            "consume", "read",
        ]
    );
}

/// The standalone form, which revision 2 of the ADR deliberately accepted.
///
/// Nothing precedes the consume that a device observation could anchor an
/// ordering on. It is legal because `dma_consume` is a synchronisation point in
/// its own right — which is exactly why the x86-64 backend pays for an
/// execution barrier rather than a compiler barrier (ADR-0086 §7, §11).
#[test]
fn a_standalone_consume_before_a_status_read_is_accepted() {
    let module = lower(&source_of(
        "1.4",
        "pub fn poll(region: DmaRegion<mut u8>) -> u8 \
         { dma_consume(region); return region[0B]; }",
    ));
    accepts(&module);
    assert_eq!(sync_sites(&module).len(), 1);
}

extern crate alloc;
