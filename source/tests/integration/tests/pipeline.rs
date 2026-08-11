// SPDX-License-Identifier: GPL-3.0-or-later
//! The source-to-receipt gate: lowered IR is revalidated by the independent
//! verifier, and forged IR is rejected by family.
//!
//! docs/43 section 4 says the verifier does not trust the frontend. Two things
//! follow, and both are checked here:
//!
//! - frontend-produced IR passes the verifier by being valid, not by being
//!   frontend-produced. There is no trusted fast path.
//! - a module altered after lowering fails with the family that owns the rule
//!   it broke. The forged cases below start from real lowered IR and change one
//!   thing, so each one isolates a single rule.
//!
//! This test crate depends on the frontend and the verifier; neither depends on
//! the other, which is what makes the verifier's traversal independent.

use std::fs;
use std::path::{Path, PathBuf};

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_ir::{Constant, Module, Op, Terminator};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

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

/// Lowers a vector with a context whose path matches its declared module name.
fn lower_file(file: &Path) -> Option<Module> {
    let bytes = fs::read(file).expect("vector is readable");
    let source = SourceReader::read(&bytes).ok()?;
    let schema = Parser::parse_schema(&source).into_accepted()?;
    let name = schema
        .outline()
        .prefix()
        .header()
        .name()
        .iter()
        .map(|segment| segment.text(&source))
        .collect::<Vec<_>>()
        .join(".");
    let context = ModuleContext {
        source_set: String::from("tos-conformance-v1"),
        path: format!("{}.tos", name.replace('.', "/")),
        content_id: content_id(&bytes),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    lower_module(&source, &schema, &context).ok()
}

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}

/// A snapshot that declares whatever the module says it imports.
///
/// The verifier is given resolution as declared input; an empty snapshot means
/// "no snapshot supplied", which the verifier treats as not constraining
/// imports rather than as rejecting all of them.
fn snapshot() -> ResolutionSnapshot {
    ResolutionSnapshot::default()
}

#[test]
fn lowered_corpus_modules_pass_the_independent_verifier() {
    let root = corpus_root();
    let mut verified = 0usize;
    let mut failures = Vec::new();
    for directory in ["examples", "conformance/v1/accept"] {
        for file in tos_files(&root.join(directory)) {
            let Some(module) = lower_file(&file) else {
                continue;
            };
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            match verify(&module, &snapshot(), &Limits::default()) {
                Ok(receipt) => {
                    verified += 1;
                    assert_eq!(receipt.schema_id, tos_ir::SCHEMA_ID);
                    assert_eq!(receipt.verifier_identity, tos_verifier::VERIFIER_IDENTITY);
                    assert_eq!(receipt.module_digest, tos_ir::module_digest(&module));
                    assert!(receipt.source_map_digest.starts_with("sha256:"));
                }
                Err(finding) => failures.push(format!(
                    "{name}: {} at {} — {}",
                    finding.code, finding.location, finding.detail
                )),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "valid lowered IR must verify:\n{}",
        failures.join("\n")
    );
    assert!(verified > 0, "no module reached the verifier at all");
}

/// A module the frontend really produced, for the forged-IR cases to alter.
fn sample_module() -> Module {
    let text = "module app.sample version 1.0 profile bootstrap; \
         resource [fuel: 1000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub record Point [x: i32, y: i32] \
         pub fn origin() -> Point { return Point(x: 0i32, y: 0i32); } \
         pub fn total(point: Point) -> i32 { return point.x + point.y; }";
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("sample parses");
    assert!(
        Checker::check(&source, &schema).is_empty(),
        "the sample must be checked source"
    );
    let context = ModuleContext {
        source_set: String::from("tos-conformance-v1"),
        path: String::from("app/sample.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    lower_module(&source, &schema, &context).expect("sample lowers")
}

fn expect_rejection(module: &Module, code: &str) {
    match verify(module, &snapshot(), &Limits::default()) {
        Ok(_) => panic!("forged IR was accepted; expected {code}"),
        Err(finding) => assert_eq!(
            finding.code, code,
            "expected {code}, got {} — {}",
            finding.code, finding.detail
        ),
    }
}

#[test]
fn the_sample_module_verifies_before_it_is_forged() {
    verify(&sample_module(), &snapshot(), &Limits::default())
        .expect("the unmodified sample must verify");
}

#[test]
fn a_forged_schema_identity_is_rejected() {
    let mut module = sample_module();
    module.header.schema_id = String::from("tos-ir/v2");
    expect_rejection(&module, "V2002_SCHEMA");

    let mut module = sample_module();
    module.header.unicode_normalization_baseline = String::from("UCD-16.0.0/NFC");
    expect_rejection(&module, "V2002_SCHEMA");
}

#[test]
fn a_forged_source_identity_is_rejected() {
    let mut module = sample_module();
    module.header.content_id = String::from("not-a-digest");
    expect_rejection(&module, "V2003_SOURCE_IDENTITY");

    let mut module = sample_module();
    module.header.path = String::from("elsewhere/other.tos");
    expect_rejection(&module, "V2003_SOURCE_IDENTITY");
}

#[test]
fn a_table_out_of_canonical_order_is_rejected() {
    let mut module = sample_module();
    module.exports.reverse();
    expect_rejection(&module, "V2004_TABLE_ORDER");
}

#[test]
fn a_type_reference_outside_the_table_is_rejected() {
    let mut module = sample_module();
    let outside = module.types.len() + 7;
    module.types.push(tos_ir::TypeDef::Option(outside));
    expect_rejection(&module, "V2010_TYPE");
}

#[test]
fn a_control_flow_target_outside_the_function_is_rejected() {
    let mut module = sample_module();
    let function = &mut module.functions[0];
    let outside = function.blocks.len() + 3;
    function.blocks[0].terminator = Terminator::Branch {
        target: outside,
        arguments: Vec::new(),
    };
    expect_rejection(&module, "V2011_CFG");
}

#[test]
fn an_operand_outside_the_value_table_is_rejected() {
    let mut module = sample_module();
    let function = &mut module.functions[0];
    let outside = function.values.len() + 5;
    function.blocks[0].terminator = Terminator::Return(Some(tos_ir::Operand::Value(outside)));
    expect_rejection(&module, "V2011_CFG");
}

#[test]
fn a_forged_capability_import_is_rejected() {
    let mut module = sample_module();
    // Claim a capability handle whose type is an ordinary integer: exactly the
    // shape a scalar-to-authority forgery takes in IR.
    let scalar = module
        .types
        .iter()
        .position(|ty| matches!(ty, tos_ir::TypeDef::Int(_)))
        .expect("the sample has an integer type");
    module.capability_imports.push(tos_ir::CapabilityImport {
        interface: String::from("system.time.Clock"),
        binding: String::from("clock"),
        ty: scalar,
    });
    expect_rejection(&module, "V2013_CAPABILITY");
}

#[test]
fn a_capability_outside_the_declared_contract_is_rejected() {
    let mut module = sample_module();
    let ty = module.types.len();
    module.types.push(tos_ir::TypeDef::Capability(String::from(
        "system.time.Clock",
    )));
    module.capability_imports.push(tos_ir::CapabilityImport {
        interface: String::from("system.time.Clock"),
        binding: String::from("clock"),
        ty,
    });
    let declared = ResolutionSnapshot {
        modules: Default::default(),
        capability_interfaces: ["system.audit.Logger".to_string()].into_iter().collect(),
    };
    match verify(&module, &declared, &Limits::default()) {
        Ok(_) => panic!("a capability outside the declared contract was accepted"),
        Err(finding) => assert_eq!(finding.code, "V2013_CAPABILITY"),
    }
}

#[test]
fn a_value_moved_twice_on_one_path_is_rejected() {
    let mut module = sample_module();
    let function = &mut module.functions[1];
    let place = tos_ir::Place {
        root: 0,
        path: Vec::new(),
    };
    let ty = function.values[0];
    let source = function.blocks[0].source;
    for _ in 0..2 {
        let result = function.values.len();
        function.values.push(ty);
        function.blocks[0].instructions.push(tos_ir::Instruction {
            result: Some(result),
            ty,
            op: Op::Move {
                place: place.clone(),
            },
            source,
            runtime_contract: None,
            unsafe_interface: None,
        });
    }
    expect_rejection(&module, "V2020_OWNERSHIP");
}

#[test]
fn a_bootstrap_module_with_more_than_one_worker_is_rejected() {
    let mut module = sample_module();
    module.header.resource_envelope.workers = 4;
    expect_rejection(&module, "V2022_RESOURCE");
}

#[test]
fn a_bootstrap_module_that_awaits_is_rejected() {
    let mut module = sample_module();
    let function = &mut module.functions[0];
    let source = function.blocks[0].source;
    let ty = function.values[0];
    let result = function.values.len();
    function.values.push(ty);
    function.blocks[0].instructions.push(tos_ir::Instruction {
        result: Some(result),
        ty,
        op: Op::Await {
            task: tos_ir::Operand::Value(0),
        },
        source,
        runtime_contract: None,
        unsafe_interface: None,
    });
    expect_rejection(&module, "V2023_PROFILE");
}

#[test]
fn an_unconsumed_child_task_is_rejected() {
    let mut module = sample_module();
    let body = 1usize;
    let function = &mut module.functions[0];
    let source = function.blocks[0].source;
    let ty = function.values[0];
    let result = function.values.len();
    function.values.push(ty);
    function.blocks[0].instructions.push(tos_ir::Instruction {
        result: Some(result),
        ty,
        op: Op::Spawn {
            body,
            captures: Vec::new(),
        },
        source,
        runtime_contract: None,
        unsafe_interface: None,
    });
    expect_rejection(&module, "V2030_TASK_SCOPE");
}

#[test]
fn an_illegal_atomic_order_is_rejected() {
    let mut module = sample_module();
    let function = &mut module.functions[0];
    let source = function.blocks[0].source;
    let ty = function.values[0];
    let result = function.values.len();
    function.values.push(ty);
    function.blocks[0].instructions.push(tos_ir::Instruction {
        result: Some(result),
        ty,
        op: Op::Atomic {
            operation: tos_ir::AtomicOp::Load,
            target: tos_ir::Operand::Value(0),
            operands: Vec::new(),
            order: tos_ir::MemoryOrder::Release,
            failure_order: None,
        },
        source,
        runtime_contract: None,
        unsafe_interface: None,
    });
    expect_rejection(&module, "V2032_ATOMIC_ORDER");
}

#[test]
fn an_unsafe_interface_claim_is_rejected() {
    let mut module = sample_module();
    module.functions[0].blocks[0].instructions[0].unsafe_interface =
        Some(String::from("host.libc/v1"));
    expect_rejection(&module, "V2033_UNSAFE");
}

#[test]
fn a_source_map_entry_claiming_another_module_is_rejected() {
    let mut module = sample_module();
    module.source_map[0].content_id = String::from("sha256:deadbeef");
    expect_rejection(&module, "V2040_SOURCE_MAP");
}

#[test]
fn a_table_beyond_the_published_ceiling_is_rejected() {
    let mut module = sample_module();
    let ceiling = Limits {
        table_entries: 1,
        ..Limits::default()
    };
    module.constants.push(Constant::Unit);
    module.constants.push(Constant::Bool(true));
    match verify(&module, &snapshot(), &ceiling) {
        Ok(_) => panic!("a table beyond the ceiling was accepted"),
        Err(finding) => assert_eq!(finding.code, "V2001_LIMIT"),
    }
}

#[test]
fn a_receipt_binds_to_the_digest_of_the_module_that_was_checked() {
    let module = sample_module();
    let receipt = verify(&module, &snapshot(), &Limits::default()).expect("the sample verifies");

    let mut altered = module.clone();
    altered.constants.push(Constant::Bool(false));
    assert_ne!(
        receipt.module_digest,
        tos_ir::module_digest(&altered),
        "a receipt must not match a module it did not check"
    );
}
