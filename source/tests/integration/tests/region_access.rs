// SPDX-License-Identifier: GPL-3.0-or-later
//! Indexed access to a granted region (ADR-0081 §2).
//!
//! **Existing V1 semantics, implemented — not a new API.** `docs/44`'s
//! `E1211_INDEX_TYPE_MISMATCH` has covered "an array, slice or region index"
//! since V1; ADR-0037 §7 requires a positive vector writing through a
//! `Region<mut T>`; `docs/43` §2 gives the region/DMA family "typed grant,
//! rights, checked range/alignment, no physical-address exposure". What was
//! missing was the implementation.
//!
//! What is proved here: the four granted modes read and write as ADR-0037's
//! table says they may, the index is exact `size`, the element type must have a
//! representation the language already fixes, and a whole artifact survives the
//! independent verifier.

use tos_verifier::{verify, Limits, ResolutionSnapshot};

fn module(body: &str) -> String {
    format!(
        "\
module system.test.region version 1.0 profile full;

resource [fuel: 65536, stack: 16KiB, allocation: 4KiB, tasks: 1, workers: 1,
          sync: 0, shared: 0B, cleanup: 0, recursion: 8, imports: 4]

{body}
"
    )
}

fn diagnostics(text: &str) -> Vec<tos_core::Diagnostic> {
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    tos_core::Checker::check(&source, &schema)
}

fn errors(text: &str) -> Vec<tos_core::Diagnostic> {
    diagnostics(text)
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
            source_set: String::from("region-access-test"),
            path: String::from("system/test/region.tos"),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

/// ADR-0037 §2's table, as source that must be accepted or refused.
#[test]
fn the_four_granted_modes_read_and_write_as_the_table_says() {
    // Readable in every mode.
    for ty in [
        "Region<i32>",
        "Region<mut i32>",
        "DmaRegion<i32>",
        "DmaRegion<mut i32>",
    ] {
        let text = module(&format!(
            "pub fn get(r: {ty}) -> i32 {{\n    return r[0B];\n}}"
        ));
        assert!(
            errors(&text).is_empty(),
            "{ty} is not readable: {:?}",
            errors(&text)
        );
    }

    // Writable only in the mutably granted modes.
    for ty in ["Region<mut i32>", "DmaRegion<mut i32>"] {
        let text = module(&format!(
            "pub fn put(r: {ty}) -> i32 {{\n    r[1B] = 7i32;\n    return r[1B];\n}}"
        ));
        assert!(
            errors(&text).is_empty(),
            "{ty} is not writable: {:?}",
            errors(&text)
        );
    }
    for ty in ["Region<i32>", "DmaRegion<i32>"] {
        let text = module(&format!(
            "pub fn put(r: {ty}) -> i32 {{\n    r[1B] = 7i32;\n    return r[1B];\n}}"
        ));
        assert!(
            errors(&text)
                .iter()
                .any(|d| d.code() == "E1201_ASSIGN_TO_IMMUTABLE"),
            "{ty} accepted a write: {:?}",
            errors(&text)
        );
    }
}

/// `docs/40` §3: the index is exact `size`, with an integer literal
/// contextually typed as one.
#[test]
fn the_index_is_exactly_size() {
    let text = module("pub fn get(r: Region<i32>, at: u64) -> i32 {\n    return r[at];\n}");
    assert!(
        errors(&text)
            .iter()
            .any(|d| d.code() == "E1211_INDEX_TYPE_MISMATCH"),
        "a u64 index was accepted: {:?}",
        errors(&text)
    );
}

/// ADR-0081 §3: only element types whose representation the language already
/// fixes. A record's field order is the frontend's choice and must not become a
/// published binary format by being indexable in shared or device memory.
#[test]
fn only_fixed_representation_elements_are_accessible() {
    let text = module(
        "record Pair [a: i32, b: i32]\n\
         pub fn get(r: Region<Pair>) -> i32 {\n    return r[0B].a;\n}",
    );
    assert!(
        !errors(&text).is_empty(),
        "an aggregate element was silently given a layout"
    );
}

/// The whole path, over the artifact rather than the frontend's word.
#[test]
fn a_region_accessing_module_verifies() {
    let module = lower(&module(
        "pub fn copy_one(from: Region<i32>, into: Region<mut i32>) -> i32 {\n\
        \x20   let value: i32 = from[0B];\n\
        \x20   into[0B] = value;\n\
        \x20   return into[0B];\n\
         }",
    ));
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("a region-accessing artifact verifies");
}

/// ADR-0037's ownership rules are untouched: indexing does not make a region an
/// array, and a mutable region still may not be shared.
#[test]
fn indexing_does_not_weaken_the_ownership_model() {
    let text = module(
        "pub fn leak(r: Region<mut i32>) -> i32 {\n    let s = share(r);\n    return 0i32;\n}",
    );
    assert!(
        !errors(&text).is_empty(),
        "a mutable region became shareable once it was indexable"
    );
}

/// An indexed access carries the **element's** type, not the region's.
///
/// It did not. `r[0B]` on a `Region<i32>` lowered to an `Op::Read` whose
/// declared type was `Region<i32>` — the container it read out of — and nothing
/// caught it: region access is unreachable at runtime until an operation
/// produces a region, and the verifier did not project a place.
///
/// It is load-bearing once a region is reachable, because a host serving an
/// indexed access takes the **element width** from that type. There is nothing
/// else it could take it from: the extent is the host's and the index is a
/// position rather than an offset.
#[test]
fn an_indexed_access_is_typed_as_the_element() {
    use tos_ir::{Op, PlaceStep, TypeDef};
    let module = lower(&module(
        "pub fn get(r: Region<i32>, d: DmaRegion<mut u8>, at: size) -> i32 {\n\
         \x20   d[0B] = 3u8;\n\
         \x20   let dynamic: u8 = d[at];\n\
         \x20   return r[0B];\n\
         }",
    ));
    let function = &module.functions[0];
    let mut indexed = 0usize;
    for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
        let (place, claimed) = match &instruction.op {
            Op::Read { place } => (place, instruction.ty),
            Op::Write { place, value } => {
                let ty = match value {
                    tos_ir::Operand::Value(id) => function.values[*id],
                    tos_ir::Operand::Constant(id) => match &module.constants[*id] {
                        tos_ir::Constant::Int(kind, _) => module
                            .types
                            .iter()
                            .position(|ty| *ty == TypeDef::Int(*kind))
                            .expect("the constant's type is in the table"),
                        other => panic!("unexpected constant {other:?}"),
                    },
                };
                (place, ty)
            }
            _ => continue,
        };
        if !place
            .path
            .iter()
            .any(|step| matches!(step, PlaceStep::Index(_) | PlaceStep::DynamicIndex(_)))
        {
            continue;
        }
        indexed += 1;
        let root = module.type_of(function.values[place.root]);
        let element = match root {
            Some(TypeDef::Region(inner))
            | Some(TypeDef::RegionMut(inner))
            | Some(TypeDef::DmaRegion(inner))
            | Some(TypeDef::DmaRegionMut(inner)) => *inner,
            other => panic!("an indexed root is not a region: {other:?}"),
        };
        assert_eq!(
            claimed,
            element,
            "an indexed access claims {:?} where the element is {:?}",
            module.type_of(claimed),
            module.type_of(element)
        );
    }
    // A literal position, a computed one, and a write: three, so the assertion
    // above is about something rather than vacuously true of an empty loop.
    assert_eq!(indexed, 3);
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("correctly typed region access verifies");
}

/// And an artifact that claims a wider element than the region has is refused.
///
/// This is the forgery the check exists for: a `u64` read through a
/// `DmaRegion<u8>` would ask a host to move eight bytes for a one-byte element,
/// and every bound it then checked would be checking the wrong number. No
/// frontend emits it, which is why the verifier refuses it rather than trusting
/// the declared type.
#[test]
fn an_indexed_access_claiming_the_wrong_element_width_is_refused() {
    use tos_ir::{Op, PlaceStep, TypeDef};
    let mut module = lower(&module(
        "pub fn get(d: DmaRegion<mut u8>) -> u8 {\n    return d[0B];\n}",
    ));
    let wider = module
        .types
        .iter()
        .position(|ty| *ty == TypeDef::Int(tos_ir::IntKind::U64))
        .unwrap_or_else(|| {
            module.types.push(TypeDef::Int(tos_ir::IntKind::U64));
            module.types.len() - 1
        });
    let function = &mut module.functions[0];
    for instruction in function
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
    {
        let Op::Read { place } = &instruction.op else {
            continue;
        };
        if !place
            .path
            .iter()
            .any(|step| matches!(step, PlaceStep::Index(_) | PlaceStep::DynamicIndex(_)))
        {
            continue;
        }
        instruction.ty = wider;
        if let Some(result) = instruction.result {
            function.values[result] = wider;
        }
    }
    let finding = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("a read claiming the wrong element width is refused");
    assert_eq!(finding.code, "V2010_TYPE");
    assert!(finding.detail.contains("element type"), "{finding:?}");
}
