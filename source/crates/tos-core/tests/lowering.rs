// SPDX-License-Identifier: GPL-3.0-or-later
//! The lowering gate: checked source becomes `tos-ir/v1` deterministically.
//!
//! docs/43 section 4 requires lowering to be deterministic — identical declared
//! inputs yield semantically identical ordered tables — and every operation to
//! carry a source-map entry. Both are properties of the whole corpus rather
//! than of one example, so they are checked over it.
//!
//! A vector the implemented subset does not cover reports a named gap. The gate
//! records which vectors those are, so the boundary shrinks visibly as coverage
//! grows instead of being asserted in prose.

use std::fs;
use std::path::{Path, PathBuf};

use tos_core::{lower_module, module_digest, Checker, ModuleContext, Parser, SourceReader};
use tos_ir::{Module, Op, Terminator};

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

fn context(path: &str) -> ModuleContext {
    ModuleContext {
        source_set: String::from("test-source-set"),
        path: String::from(path),
        content_id: String::from("sha256:0"),
        dependency_digest: String::from("sha256:0"),
        capability_interface_digest: String::from("sha256:0"),
    }
}

/// Lowers a vector, returning the module or the gap that stopped it.
fn lower_file(file: &Path) -> Result<Module, String> {
    let bytes = fs::read(file).expect("vector is readable");
    let source = SourceReader::read(&bytes).map_err(|error| std::format!("source: {error:?}"))?;
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .ok_or_else(|| String::from("vector does not parse"))?;
    let name = file.file_name().unwrap().to_string_lossy().into_owned();
    lower_module(&source, &schema, &context(&name))
        .map_err(|gap| std::format!("gap: {}", gap.construct))
}

#[test]
fn accepted_source_lowers_or_names_the_construct_it_cannot_lower() {
    let root = corpus_root();
    let mut lowered = 0usize;
    let mut gaps: Vec<String> = Vec::new();
    for directory in ["examples", "conformance/v1/accept"] {
        for file in tos_files(&root.join(directory)) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            match lower_file(&file) {
                Ok(module) => {
                    lowered += 1;
                    assert_eq!(module.header.schema_id, tos_ir::SCHEMA_ID);
                    assert_eq!(module.header.language_version, tos_ir::LANGUAGE_VERSION);
                    assert_eq!(
                        module.header.unicode_normalization_baseline,
                        tos_ir::UNICODE_BASELINE
                    );
                    assert!(
                        !module.functions.is_empty(),
                        "{name}: a module with functions lowered none"
                    );
                }
                Err(detail) => gaps.push(std::format!("{name}: {detail}")),
            }
        }
    }
    assert!(
        lowered > 0,
        "the lowerer produced no module at all:\n{}",
        gaps.join("\n")
    );
    // Every remaining gap names a construct rather than failing silently.
    for gap in &gaps {
        assert!(gap.contains("gap: "), "unexpected lowering failure: {gap}");
    }
}

#[test]
fn lowering_the_same_source_twice_yields_the_same_digest() {
    let root = corpus_root();
    for directory in ["examples", "conformance/v1/accept"] {
        for file in tos_files(&root.join(directory)) {
            let Ok(first) = lower_file(&file) else {
                continue;
            };
            let second = lower_file(&file).expect("a module that lowered once lowers again");
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            assert_eq!(
                module_digest(&first),
                module_digest(&second),
                "{name}: lowering is not deterministic"
            );
            assert_eq!(first, second, "{name}: lowered tables differ");
        }
    }
}

#[test]
fn every_instruction_and_block_carries_a_source_map_entry() {
    let root = corpus_root();
    for directory in ["examples", "conformance/v1/accept"] {
        for file in tos_files(&root.join(directory)) {
            let Ok(module) = lower_file(&file) else {
                continue;
            };
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            for function in &module.functions {
                assert!(
                    function.source < module.source_map.len(),
                    "{name}: a function has no source-map entry"
                );
                for block in &function.blocks {
                    assert!(
                        block.source < module.source_map.len(),
                        "{name}: a block has no source-map entry"
                    );
                    for instruction in &block.instructions {
                        assert!(
                            instruction.source < module.source_map.len(),
                            "{name}: an instruction has no source-map entry"
                        );
                        let entry = &module.source_map[instruction.source];
                        assert!(
                            entry.byte_start <= entry.byte_end,
                            "{name}: a source-map span runs backwards"
                        );
                        assert_eq!(entry.frontend_identity, tos_core::FRONTEND_IDENTITY);
                    }
                }
            }
        }
    }
}

#[test]
fn every_block_ends_in_exactly_one_terminator_with_reachable_targets() {
    let root = corpus_root();
    for directory in ["examples", "conformance/v1/accept"] {
        for file in tos_files(&root.join(directory)) {
            let Ok(module) = lower_file(&file) else {
                continue;
            };
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            for function in &module.functions {
                let count = function.blocks.len();
                for block in &function.blocks {
                    for target in terminator_targets(&block.terminator) {
                        assert!(
                            target < count,
                            "{name}: a terminator names block {target} of {count}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_lowered_operand_names_a_defined_value_or_constant() {
    let root = corpus_root();
    for directory in ["examples", "conformance/v1/accept"] {
        for file in tos_files(&root.join(directory)) {
            let Ok(module) = lower_file(&file) else {
                continue;
            };
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            for function in &module.functions {
                for block in &function.blocks {
                    for instruction in &block.instructions {
                        if let Some(result) = instruction.result {
                            assert!(
                                result < function.values.len(),
                                "{name}: an instruction defines value {result} outside the table"
                            );
                        }
                        assert!(
                            module.has_type(instruction.ty),
                            "{name}: an instruction has a type outside the table"
                        );
                        for operand in operands_of(&instruction.op) {
                            match operand {
                                tos_ir::Operand::Value(value) => assert!(
                                    value < function.values.len(),
                                    "{name}: an operand names value {value} outside the table"
                                ),
                                tos_ir::Operand::Constant(constant) => assert!(
                                    constant < module.constants.len(),
                                    "{name}: an operand names constant {constant} outside the table"
                                ),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn a_module_digest_changes_when_the_module_does() {
    let text = "module app.main version 1.0 profile bootstrap; \
         resource [fuel: 1000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub fn main() -> i32 { return 1i32; }";
    let other = text.replace("return 1i32;", "return 2i32;");

    let one = lower_text(text);
    let two = lower_text(&other);
    assert_ne!(
        module_digest(&one),
        module_digest(&two),
        "a changed constant must change the digest"
    );
    assert_eq!(module_digest(&one), module_digest(&lower_text(text)));
}

fn lower_text(text: &str) -> Module {
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("source parses");
    assert!(
        Checker::check(&source, &schema).is_empty(),
        "the lowerer takes checked source only"
    );
    lower_module(&source, &schema, &context("app/main.tos")).expect("source lowers")
}

fn terminator_targets(terminator: &Terminator) -> Vec<usize> {
    match terminator {
        Terminator::Return(_) | Terminator::Trap(_) => Vec::new(),
        Terminator::Branch { target, .. } => std::vec![*target],
        Terminator::BranchIf {
            true_target,
            false_target,
            ..
        } => std::vec![*true_target, *false_target],
        Terminator::MatchEnum { arms, .. } => arms.iter().map(|(_, target)| *target).collect(),
        Terminator::PropagateError { ok_target, .. } => std::vec![*ok_target],
    }
}

fn operands_of(op: &Op) -> Vec<tos_ir::Operand> {
    match op {
        Op::Aggregate { operands, .. } | Op::Variant { operands, .. } => operands.clone(),
        Op::Write { value, .. } => std::vec![value.clone()],
        Op::Binary { left, right, .. } => std::vec![left.clone(), right.clone()],
        Op::Unary { operand, .. } | Op::Widen { operand, .. } => std::vec![operand.clone()],
        Op::Call { operands, .. } => operands.clone(),
        Op::Spawn { captures, .. } => captures.clone(),
        Op::Join { task } | Op::Await { task } | Op::Cancel { task } => std::vec![task.clone()],
        Op::Atomic {
            target, operands, ..
        } => {
            let mut all = std::vec![target.clone()];
            all.extend(operands.iter().cloned());
            all
        }
        Op::Capability { operands, .. } => operands.clone(),
        Op::Resource { amount, .. } => std::vec![amount.clone()],
        _ => Vec::new(),
    }
}

#[test]
fn the_lowering_boundary_is_recorded() {
    // Not an assertion about coverage: a printed record of which accepted
    // vectors lower today and which construct stops the rest, so the boundary
    // is observable in the gate output rather than described in prose.
    let root = corpus_root();
    let mut lowered: Vec<String> = Vec::new();
    let mut gaps: Vec<String> = Vec::new();
    for directory in ["examples", "conformance/v1/accept"] {
        for file in tos_files(&root.join(directory)) {
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            match lower_file(&file) {
                Ok(_) => lowered.push(name),
                Err(detail) => gaps.push(std::format!("{name} — {detail}")),
            }
        }
    }
    std::println!("lowered {} vector(s):", lowered.len());
    for name in &lowered {
        std::println!("  {name}");
    }
    std::println!("{} vector(s) outside the implemented subset:", gaps.len());
    for gap in &gaps {
        std::println!("  {gap}");
    }
}
