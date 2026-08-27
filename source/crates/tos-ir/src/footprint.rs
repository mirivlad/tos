// SPDX-License-Identifier: GPL-3.0-or-later
//! What a decoded module costs to hold.
//!
//! ADR-0071 section 7 bounds residency by **module-derived bytes**, and a bound
//! is only a bound if what it counts is an upper figure. This walks the whole
//! ownership tree of a [`Module`] and reports every heap allocation the
//! representation owns, at the capacity each allocation actually has rather
//! than at the length currently used.
//!
//! It lives here rather than beside the residency table because it is knowledge
//! about the shape of `tos-ir/v1`, and a second copy of that knowledge
//! elsewhere would be a copy to keep in agreement. Every enum family is matched
//! exhaustively and every struct is destructured by name, so a variant or a
//! field added to the schema breaks this file rather than silently costing
//! nothing.
//!
//! ## What the figure is, exactly
//!
//! ```text
//! retained_bytes(module) >= every Rust-visible heap allocation
//!                           owned by that module value
//! ```
//!
//! It includes `size_of::<Module>()` and every `Vec` and `String` capacity
//! beneath it. It does **not** include the allocator's own per-block metadata,
//! which is not a property of the module, is not portable between allocators
//! and is therefore not something a logical bound may claim to know. The
//! process's real frontier is measured separately, against a hard arena, and
//! that is where allocator overhead is accounted for.
//!
//! Nothing here is an estimate. There is no per-node constant, no rounding and
//! no assumed average: every term is a `capacity` or a `size_of`.

use alloc::string::String;
use alloc::vec::Vec;

use crate::{
    Block, CallTarget, CapabilityImport, CleanupCall, Constant, Function, Header, Import,
    Instruction, Module, Op, Operand, Parameter, Place, PlaceStep, ResourceEnvelope, Signature,
    SourceMapEntry, Terminator, TypeDef, TypeId, Variant,
};

/// Every byte a decoded module owns.
pub fn retained_bytes(module: &Module) -> usize {
    let Module {
        header,
        types,
        imports,
        capability_imports,
        exports,
        constants,
        functions,
        source_map,
    } = module;
    core::mem::size_of::<Module>()
        + header_bytes(header)
        + table(types.capacity(), types, type_bytes)
        + table(imports.capacity(), imports, import_bytes)
        + table(
            capability_imports.capacity(),
            capability_imports,
            capability_import_bytes,
        )
        + table(exports.capacity(), exports, signature_bytes)
        + table(constants.capacity(), constants, constant_bytes)
        + table(functions.capacity(), functions, function_bytes)
        + table(source_map.capacity(), source_map, source_entry_bytes)
}

/// A `Vec<T>`: its buffer at the capacity it holds, plus what each live element
/// owns beneath it.
fn table<T>(capacity: usize, elements: &[T], of: fn(&T) -> usize) -> usize {
    capacity * core::mem::size_of::<T>() + elements.iter().map(of).sum::<usize>()
}

/// The buffer behind a `Vec<T>` whose elements own nothing.
fn flat<T>(capacity: usize) -> usize {
    capacity * core::mem::size_of::<T>()
}

/// The buffer behind an owned string: capacity, not length. What is held is
/// what was allocated.
fn text_bytes(text: &String) -> usize {
    text.capacity()
}

fn optional_text_bytes(text: &Option<String>) -> usize {
    match text {
        Some(text) => text_bytes(text),
        None => 0,
    }
}

fn header_bytes(header: &Header) -> usize {
    let Header {
        schema_id,
        language_version,
        unicode_normalization_baseline,
        profile: _,
        module_name,
        source_set,
        path,
        content_id,
        dependency_digest,
        frontend_identity,
        source_map_revision,
        resource_envelope,
        capability_interface_digest,
    } = header;
    // Destructured rather than field-accessed on purpose, here and everywhere
    // below: a field added to the header stops this compiling instead of going
    // uncounted.
    let ResourceEnvelope {
        fuel: _,
        stack: _,
        allocation: _,
        tasks: _,
        workers: _,
        sync: _,
        shared: _,
        cleanup: _,
        recursion: _,
        imports: _,
    } = resource_envelope;
    text_bytes(schema_id)
        + text_bytes(language_version)
        + text_bytes(unicode_normalization_baseline)
        + text_bytes(module_name)
        + text_bytes(source_set)
        + text_bytes(path)
        + text_bytes(content_id)
        + text_bytes(dependency_digest)
        + text_bytes(frontend_identity)
        + text_bytes(source_map_revision)
        + text_bytes(capability_interface_digest)
}

fn type_bytes(definition: &TypeDef) -> usize {
    match definition {
        // Every constructor whose payload owns no allocation.
        TypeDef::Unit
        | TypeDef::Bool
        | TypeDef::Int(_)
        | TypeDef::Size
        | TypeDef::Duration
        | TypeDef::Text
        | TypeDef::Bytes
        | TypeDef::ConversionError
        | TypeDef::Event
        | TypeDef::Semaphore
        | TypeDef::Barrier
        | TypeDef::Latch
        | TypeDef::AtomicBool
        | TypeDef::AtomicU32
        | TypeDef::AtomicU64
        | TypeDef::Option(_)
        | TypeDef::Task(_)
        | TypeDef::TaskResult(_)
        | TypeDef::Shared(_)
        | TypeDef::Region(_)
        | TypeDef::DmaRegion(_)
        | TypeDef::RegionMut(_)
        | TypeDef::DmaRegionMut(_)
        | TypeDef::Mutex(_)
        | TypeDef::RwLock(_)
        | TypeDef::MutexGuard(_)
        | TypeDef::ReadGuard(_)
        | TypeDef::WriteGuard(_)
        | TypeDef::Channel(_)
        | TypeDef::Slice(_)
        | TypeDef::Result(_, _)
        | TypeDef::Array(_, _) => 0,
        TypeDef::Tuple(elements) => flat::<TypeId>(elements.capacity()),
        TypeDef::Function(parameters, _) => flat::<TypeId>(parameters.capacity()),
        TypeDef::Capability(interface) => text_bytes(interface),
        TypeDef::Nominal {
            module_content_id,
            export_name,
            kind: _,
            fields,
            variants,
        } => {
            text_bytes(module_content_id)
                + text_bytes(export_name)
                + flat::<TypeId>(fields.capacity())
                + table(variants.capacity(), variants, variant_bytes)
        }
    }
}

fn variant_bytes(variant: &Variant) -> usize {
    let Variant { name, payload } = variant;
    text_bytes(name) + flat::<TypeId>(payload.capacity())
}

fn import_bytes(import: &Import) -> usize {
    let Import {
        module_name,
        module_content_id,
        binding,
    } = import;
    text_bytes(module_name) + text_bytes(module_content_id) + text_bytes(binding)
}

fn capability_import_bytes(import: &CapabilityImport) -> usize {
    let CapabilityImport {
        interface,
        binding,
        ty: _,
    } = import;
    text_bytes(interface) + text_bytes(binding)
}

fn signature_bytes(signature: &Signature) -> usize {
    let Signature {
        name,
        visibility: _,
        is_async: _,
        parameters,
        result: _,
        effects,
    } = signature;
    text_bytes(name)
        + table(parameters.capacity(), parameters, parameter_bytes)
        + table(effects.capacity(), effects, text_bytes)
}

fn parameter_bytes(parameter: &Parameter) -> usize {
    let Parameter {
        name,
        ty: _,
        mode: _,
    } = parameter;
    text_bytes(name)
}

fn constant_bytes(constant: &Constant) -> usize {
    match constant {
        Constant::Unit
        | Constant::Bool(_)
        | Constant::Int(_, _)
        | Constant::Size(_)
        | Constant::Duration(_) => 0,
        Constant::Text(value) => text_bytes(value),
        Constant::Bytes(value) => flat::<u8>(value.capacity()),
    }
}

fn function_bytes(function: &Function) -> usize {
    let Function {
        signature,
        origin: _,
        source: _,
        stack_contribution: _,
        fuel_contribution: _,
        cleanup_contribution: _,
        values,
        blocks,
    } = function;
    signature_bytes(signature)
        + flat::<TypeId>(values.capacity())
        + table(blocks.capacity(), blocks, block_bytes)
}

fn block_bytes(block: &Block) -> usize {
    let Block {
        parameters,
        instructions,
        terminator,
        source: _,
    } = block;
    flat::<TypeId>(parameters.capacity())
        + table(instructions.capacity(), instructions, instruction_bytes)
        + terminator_bytes(terminator)
}

fn instruction_bytes(instruction: &Instruction) -> usize {
    let Instruction {
        result: _,
        ty: _,
        op,
        source: _,
        runtime_contract,
        unsafe_block: _,
        unsafe_interface,
    } = instruction;
    op_bytes(op) + optional_text_bytes(runtime_contract) + optional_text_bytes(unsafe_interface)
}

/// An operand owns nothing, and this exhaustive match is what keeps that true:
/// a variant that starts owning something stops this compiling.
fn operand_bytes(operand: &Operand) -> usize {
    match operand {
        Operand::Value(_) | Operand::Constant(_) => 0,
    }
}

fn operands_bytes(operands: &Vec<Operand>) -> usize {
    table(operands.capacity(), operands, operand_bytes)
}

fn place_step_bytes(step: &PlaceStep) -> usize {
    match step {
        PlaceStep::Field(_) | PlaceStep::Index(_) | PlaceStep::DynamicIndex(_) => 0,
    }
}

fn place_bytes(place: &Place) -> usize {
    let Place { root: _, path } = place;
    table(path.capacity(), path, place_step_bytes)
}

fn call_target_bytes(target: &CallTarget) -> usize {
    match target {
        CallTarget::Local(_) => 0,
        CallTarget::Imported { import: _, name } => text_bytes(name),
        CallTarget::Predeclared(name) => text_bytes(name),
    }
}

fn cleanup_call_bytes(call: &CleanupCall) -> usize {
    let CleanupCall { body: _, captures } = call;
    operands_bytes(captures)
}

fn op_bytes(op: &Op) -> usize {
    match op {
        Op::Const(_) => 0,
        Op::Aggregate { ty: _, operands } => operands_bytes(operands),
        Op::Variant {
            ty: _,
            index: _,
            operands,
        } => operands_bytes(operands),
        Op::Read { place } | Op::Move { place } | Op::Drop { place } => place_bytes(place),
        Op::Write { place, value } => place_bytes(place) + operand_bytes(value),
        Op::Borrow { place, kind: _ } => place_bytes(place),
        Op::Binary { op: _, left, right } => operand_bytes(left) + operand_bytes(right),
        Op::Unary { op: _, operand } => operand_bytes(operand),
        Op::Widen { operand, to: _ } => operand_bytes(operand),
        Op::Call { target, operands } => call_target_bytes(target) + operands_bytes(operands),
        Op::Spawn { body: _, captures } | Op::Closure { body: _, captures } => {
            operands_bytes(captures)
        }
        Op::CallValue { callee, operands } => operand_bytes(callee) + operands_bytes(operands),
        Op::Lock { object, mode: _ } => operand_bytes(object),
        Op::Share { operand } => operand_bytes(operand),
        Op::Join { task } | Op::Await { task } | Op::Cancel { task } => operand_bytes(task),
        Op::Atomic {
            operation: _,
            target,
            operands,
            order: _,
            failure_order: _,
        } => operand_bytes(target) + operands_bytes(operands),
        Op::Capability {
            import: _,
            further_imports,
            right,
            operands,
        } => {
            flat::<usize>(further_imports.capacity()) + text_bytes(right) + operands_bytes(operands)
        }
        Op::Resource {
            kind: _,
            amount,
            release: _,
        } => operand_bytes(amount),
        Op::RegisterCleanup { body: _ } => 0,
        Op::RunCleanups { calls } => table(calls.capacity(), calls, cleanup_call_bytes),
    }
}

fn terminator_bytes(terminator: &Terminator) -> usize {
    match terminator {
        Terminator::Return(value) => match value {
            Some(value) => operand_bytes(value),
            None => 0,
        },
        Terminator::Branch {
            target: _,
            arguments,
        } => operands_bytes(arguments),
        Terminator::BranchIf {
            condition,
            true_target: _,
            true_arguments,
            false_target: _,
            false_arguments,
        } => {
            operand_bytes(condition)
                + operands_bytes(true_arguments)
                + operands_bytes(false_arguments)
        }
        Terminator::MatchEnum { subject, arms } => {
            operand_bytes(subject) + flat::<(usize, usize)>(arms.capacity())
        }
        Terminator::PropagateError {
            result,
            ok_target: _,
        } => operand_bytes(result),
        Terminator::Trap(code) => text_bytes(code),
    }
}

fn source_entry_bytes(entry: &SourceMapEntry) -> usize {
    let SourceMapEntry {
        source_set,
        path,
        content_id,
        frontend_identity,
        language_version,
        profile: _,
        unicode_normalization_baseline,
        byte_start: _,
        byte_end: _,
        derived_from: _,
    } = entry;
    text_bytes(source_set)
        + text_bytes(path)
        + text_bytes(content_id)
        + text_bytes(frontend_identity)
        + text_bytes(language_version)
        + text_bytes(unicode_normalization_baseline)
}

/// A second traversal that sums only what is unmistakably owned.
///
/// Deliberately computed a different way from [`retained_bytes`] — lengths
/// rather than capacities, and without the module's own `size_of` — so that a
/// test can assert the bound sits above an independently derived figure rather
/// than above itself.
pub fn owned_payload_bytes(module: &Module) -> usize {
    let mut bytes = 0usize;
    let header = &module.header;
    for text in [
        &header.schema_id,
        &header.language_version,
        &header.unicode_normalization_baseline,
        &header.module_name,
        &header.source_set,
        &header.path,
        &header.content_id,
        &header.dependency_digest,
        &header.frontend_identity,
        &header.source_map_revision,
        &header.capability_interface_digest,
    ] {
        bytes += text.len();
    }
    bytes += filled(&module.types, type_bytes);
    bytes += filled(&module.imports, import_bytes);
    bytes += filled(&module.capability_imports, capability_import_bytes);
    bytes += filled(&module.exports, signature_bytes);
    bytes += filled(&module.constants, constant_bytes);
    bytes += filled(&module.functions, function_bytes);
    bytes += filled(&module.source_map, source_entry_bytes);
    bytes
}

/// The same table, counted at length instead of capacity.
fn filled<T>(elements: &[T], of: fn(&T) -> usize) -> usize {
    table(elements.len(), elements, of)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::string::String;
    use alloc::vec::Vec;

    use crate::fixtures::every_variant;
    use crate::footprint::{owned_payload_bytes, retained_bytes};
    use crate::*;

    /// The bound sits above an independently derived floor.
    ///
    /// The two figures are computed by different traversals — capacities against
    /// lengths — so this is a comparison and not a restatement. A field the
    /// accounting missed would be missing from both, which is why the exhaustive
    /// matches above are the primary defence and this is the check that the
    /// direction is right.
    #[test]
    fn the_bound_is_above_the_payload_it_bounds() {
        let module = every_variant();
        let bound = retained_bytes(&module);
        let payload = owned_payload_bytes(&module);
        assert!(
            bound >= payload,
            "the reported bound {bound} is below the payload {payload}"
        );
        assert!(payload > 0, "the all-variants module owns something");
    }

    /// Every owning field is reached.
    ///
    /// Growing an allocation anywhere in the module must grow the figure. Each
    /// case below names one place in the ownership tree and makes it bigger by a
    /// known amount; if a traversal stops short of that place, the figure does
    /// not move and the case fails. This is what covers a **struct field**
    /// addition, which an exhaustive `match` cannot catch on its own.
    /// One place in the ownership tree, and a way to make it bigger.
    type Case = (&'static str, fn(&mut Module));

    #[test]
    fn every_owned_allocation_is_reached() {
        let mut cases: Vec<Case> = Vec::new();
        cases.push(("header text", |module| {
            module.header.module_name.push_str("-longer");
        }));
        cases.push(("type table", |module| {
            module.types.push(TypeDef::Unit);
        }));
        cases.push(("nominal export name", |module| {
            for definition in &mut module.types {
                if let TypeDef::Nominal { export_name, .. } = definition {
                    export_name.push_str("-longer");
                }
            }
        }));
        cases.push(("nominal variant name", |module| {
            for definition in &mut module.types {
                if let TypeDef::Nominal { variants, .. } = definition {
                    for variant in variants.iter_mut() {
                        variant.name.push_str("-longer");
                    }
                }
            }
        }));
        cases.push(("capability interface", |module| {
            for definition in &mut module.types {
                if let TypeDef::Capability(interface) = definition {
                    interface.push_str("-longer");
                }
            }
        }));
        cases.push(("import text", |module| {
            module.imports[0].binding.push_str("-longer");
        }));
        cases.push(("capability import text", |module| {
            module.capability_imports[0].binding.push_str("-longer");
        }));
        cases.push(("export signature name", |module| {
            module.exports[0].name.push_str("-longer");
        }));
        cases.push(("export parameter name", |module| {
            module.exports[0].parameters[0].name.push_str("-longer");
        }));
        cases.push(("export effect", |module| {
            module.exports[0].effects[0].push_str("-longer");
        }));
        cases.push(("text constant", |module| {
            for constant in &mut module.constants {
                if let Constant::Text(value) = constant {
                    value.push_str("-longer");
                }
            }
        }));
        cases.push(("bytes constant", |module| {
            for constant in &mut module.constants {
                if let Constant::Bytes(value) = constant {
                    value.reserve_exact(4096);
                }
            }
        }));
        cases.push(("function values", |module| {
            module.functions[0].values.push(0);
        }));
        cases.push(("block parameters", |module| {
            module.functions[0].blocks[0].parameters.push(0);
        }));
        cases.push(("instruction runtime contract", |module| {
            for instruction in &mut module.functions[0].blocks[0].instructions {
                if let Some(contract) = &mut instruction.runtime_contract {
                    contract.push_str("-longer");
                }
            }
        }));
        cases.push(("instruction unsafe interface", |module| {
            for instruction in &mut module.functions[0].blocks[0].instructions {
                if let Some(interface) = &mut instruction.unsafe_interface {
                    interface.push_str("-longer");
                }
            }
        }));
        cases.push(("operand list", |module| {
            for instruction in &mut module.functions[0].blocks[0].instructions {
                if let Op::Aggregate { operands, .. } = &mut instruction.op {
                    operands.reserve_exact(64);
                }
            }
        }));
        cases.push(("place path", |module| {
            for instruction in &mut module.functions[0].blocks[0].instructions {
                if let Op::Move { place } = &mut instruction.op {
                    place.path.push(PlaceStep::Field(0));
                }
            }
        }));
        cases.push(("imported call name", |module| {
            for instruction in &mut module.functions[0].blocks[0].instructions {
                if let Op::Call {
                    target: CallTarget::Imported { name, .. },
                    ..
                } = &mut instruction.op
                {
                    name.push_str("-longer");
                }
            }
        }));
        cases.push(("predeclared call name", |module| {
            for instruction in &mut module.functions[0].blocks[0].instructions {
                if let Op::Call {
                    target: CallTarget::Predeclared(name),
                    ..
                } = &mut instruction.op
                {
                    name.push_str("-longer");
                }
            }
        }));
        cases.push(("capability right and further imports", |module| {
            for instruction in &mut module.functions[0].blocks[0].instructions {
                if let Op::Capability {
                    right,
                    further_imports,
                    ..
                } = &mut instruction.op
                {
                    right.push_str("-longer");
                    further_imports.push(0);
                }
            }
        }));
        cases.push(("cleanup captures", |module| {
            for instruction in &mut module.functions[0].blocks[0].instructions {
                if let Op::RunCleanups { calls } = &mut instruction.op {
                    for call in calls.iter_mut() {
                        call.captures.push(Operand::Value(0));
                    }
                }
            }
        }));
        cases.push(("branch arguments", |module| {
            for block in &mut module.functions[0].blocks {
                if let Terminator::Branch { arguments, .. } = &mut block.terminator {
                    arguments.push(Operand::Value(0));
                }
            }
        }));
        cases.push(("match arms", |module| {
            for block in &mut module.functions[0].blocks {
                if let Terminator::MatchEnum { arms, .. } = &mut block.terminator {
                    arms.push((2, 0));
                }
            }
        }));
        cases.push(("trap code", |module| {
            for block in &mut module.functions[0].blocks {
                if let Terminator::Trap(code) = &mut block.terminator {
                    code.push_str("_LONGER");
                }
            }
        }));
        cases.push(("source map text", |module| {
            module.source_map[0].path.push_str("-longer");
        }));
        cases.push(("source map table", |module| {
            let entry = module.source_map[0].clone();
            module.source_map.push(entry);
        }));

        let base = retained_bytes(&every_variant());
        for (what, grow) in cases {
            let mut module = every_variant();
            grow(&mut module);
            let grown = retained_bytes(&module);
            assert!(
                grown > base,
                "{what} grew the module but not the figure that bounds it ({base} -> {grown})"
            );
        }
    }

    /// An empty module still costs its own shape.
    #[test]
    fn the_module_itself_is_counted() {
        let module = Module {
            header: Header {
                schema_id: String::new(),
                language_version: String::new(),
                unicode_normalization_baseline: String::new(),
                profile: Profile::Bootstrap,
                module_name: String::new(),
                source_set: String::new(),
                path: String::new(),
                content_id: String::new(),
                dependency_digest: String::new(),
                frontend_identity: String::new(),
                source_map_revision: String::new(),
                resource_envelope: ResourceEnvelope::default(),
                capability_interface_digest: String::new(),
            },
            types: Vec::new(),
            imports: Vec::new(),
            capability_imports: Vec::new(),
            exports: Vec::new(),
            constants: Vec::new(),
            functions: Vec::new(),
            source_map: Vec::new(),
        };
        assert_eq!(retained_bytes(&module), core::mem::size_of::<Module>());
        assert_eq!(owned_payload_bytes(&module), 0);
    }
}
