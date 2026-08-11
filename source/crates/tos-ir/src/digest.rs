// SPDX-License-Identifier: GPL-3.0-or-later
//! The module digest a verified-module receipt binds to (docs/43 sections 4–6).
//!
//! docs/43 deliberately does not freeze an on-disk byte encoding, so this is
//! not one. It is a canonical digest over the schema's logical sections in the
//! order docs/43 section 2 fixes, computed the same way by anyone holding the
//! same module value. Two modules with the same digest have the same semantic
//! content; a forged or altered table changes it.
//!
//! The encoding fed to the hash is length-prefixed at every variable-length
//! position, so no two distinct modules can produce the same byte stream by
//! moving a boundary — the ambiguity that makes naive concatenation unsafe.

use std::string::String;
use std::vec::Vec;

use crate::{
    AtomicOp, BinaryOp, Block, BorrowKind, CallTarget, Constant, Function, Import, Instruction,
    Module, Op, Operand, Place, PlaceStep, ResourceKind, Signature, SourceMapEntry, Terminator,
    TypeDef, UnaryOp, Variant,
};

/// The digest of a module, as `sha256:<hex>`.
pub fn module_digest(module: &Module) -> String {
    let mut writer = Writer::default();
    write_module(&mut writer, module);
    let digest = tos_hash::sha256(&writer.bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    std::format!(
        "sha256:{}",
        core::str::from_utf8(&hex).expect("hex output is ASCII")
    )
}

#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    /// A discriminant, which also separates otherwise adjacent fields.
    fn tag(&mut self, tag: u8) {
        self.bytes.push(tag);
    }

    fn number(&mut self, value: u128) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn signed(&mut self, value: i128) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn text(&mut self, value: &str) {
        self.number(value.len() as u128);
        self.bytes.extend_from_slice(value.as_bytes());
    }

    fn blob(&mut self, value: &[u8]) {
        self.number(value.len() as u128);
        self.bytes.extend_from_slice(value);
    }

    fn count(&mut self, value: usize) {
        self.number(value as u128);
    }

    fn flag(&mut self, value: bool) {
        self.bytes.push(u8::from(value));
    }
}

fn write_module(out: &mut Writer, module: &Module) {
    let header = &module.header;
    out.text(&header.schema_id);
    out.text(&header.language_version);
    out.text(&header.unicode_normalization_baseline);
    out.text(header.profile.spelled());
    out.text(&header.module_name);
    out.text(&header.source_set);
    out.text(&header.path);
    out.text(&header.content_id);
    out.text(&header.dependency_digest);
    out.text(&header.frontend_identity);
    out.text(&header.source_map_revision);
    let envelope = &header.resource_envelope;
    for limit in [
        envelope.fuel,
        envelope.stack,
        envelope.allocation,
        envelope.tasks,
        envelope.workers,
        envelope.sync,
        envelope.shared,
        envelope.cleanup,
        envelope.recursion,
        envelope.imports,
    ] {
        out.number(limit);
    }
    out.text(&header.capability_interface_digest);

    out.count(module.types.len());
    for definition in &module.types {
        write_type(out, definition);
    }
    out.count(module.imports.len());
    for import in &module.imports {
        write_import(out, import);
    }
    out.count(module.capability_imports.len());
    for import in &module.capability_imports {
        out.text(&import.interface);
        out.text(&import.binding);
        out.count(import.ty);
    }
    out.count(module.exports.len());
    for signature in &module.exports {
        write_signature(out, signature);
    }
    out.count(module.constants.len());
    for constant in &module.constants {
        write_constant(out, constant);
    }
    out.count(module.functions.len());
    for function in &module.functions {
        write_function(out, function);
    }
    out.count(module.source_map.len());
    for entry in &module.source_map {
        write_source_entry(out, entry);
    }
}

fn write_type(out: &mut Writer, definition: &TypeDef) {
    match definition {
        TypeDef::Unit => out.tag(0),
        TypeDef::Bool => out.tag(1),
        TypeDef::Int(kind) => {
            out.tag(2);
            out.text(kind.spelled());
        }
        TypeDef::Size => out.tag(3),
        TypeDef::Duration => out.tag(4),
        TypeDef::Text => out.tag(5),
        TypeDef::Bytes => out.tag(6),
        TypeDef::ConversionError => out.tag(7),
        TypeDef::Event => out.tag(8),
        TypeDef::Semaphore => out.tag(9),
        TypeDef::Barrier => out.tag(10),
        TypeDef::Latch => out.tag(11),
        TypeDef::AtomicBool => out.tag(12),
        TypeDef::AtomicU32 => out.tag(13),
        TypeDef::AtomicU64 => out.tag(14),
        TypeDef::Option(inner) => {
            out.tag(15);
            out.count(*inner);
        }
        TypeDef::Task(inner) => {
            out.tag(16);
            out.count(*inner);
        }
        TypeDef::TaskResult(inner) => {
            out.tag(17);
            out.count(*inner);
        }
        TypeDef::Shared(inner) => {
            out.tag(18);
            out.count(*inner);
        }
        TypeDef::Region(inner) => {
            out.tag(19);
            out.count(*inner);
        }
        TypeDef::DmaRegion(inner) => {
            out.tag(20);
            out.count(*inner);
        }
        TypeDef::Mutex(inner) => {
            out.tag(21);
            out.count(*inner);
        }
        TypeDef::RwLock(inner) => {
            out.tag(22);
            out.count(*inner);
        }
        TypeDef::Channel(inner) => {
            out.tag(23);
            out.count(*inner);
        }
        TypeDef::Slice(inner) => {
            out.tag(24);
            out.count(*inner);
        }
        TypeDef::Result(ok, error) => {
            out.tag(25);
            out.count(*ok);
            out.count(*error);
        }
        TypeDef::Array(element, length) => {
            out.tag(26);
            out.count(*element);
            out.number(*length as u128);
        }
        TypeDef::Tuple(elements) => {
            out.tag(27);
            out.count(elements.len());
            for element in elements {
                out.count(*element);
            }
        }
        TypeDef::Function(parameters, result) => {
            out.tag(28);
            out.count(parameters.len());
            for parameter in parameters {
                out.count(*parameter);
            }
            out.count(*result);
        }
        TypeDef::Capability(interface) => {
            out.tag(29);
            out.text(interface);
        }
        TypeDef::Nominal {
            module_content_id,
            export_name,
            kind,
            fields,
            variants,
        } => {
            out.tag(30);
            out.text(module_content_id);
            out.text(export_name);
            out.tag(match kind {
                crate::NominalKind::Record => 0,
                crate::NominalKind::Enum => 1,
            });
            out.count(fields.len());
            for field in fields {
                out.count(*field);
            }
            out.count(variants.len());
            for variant in variants {
                write_variant(out, variant);
            }
        }
    }
}

fn write_variant(out: &mut Writer, variant: &Variant) {
    out.text(&variant.name);
    out.count(variant.payload.len());
    for payload in &variant.payload {
        out.count(*payload);
    }
}

fn write_import(out: &mut Writer, import: &Import) {
    out.text(&import.module_name);
    out.text(&import.module_content_id);
    out.text(&import.binding);
}

fn write_signature(out: &mut Writer, signature: &Signature) {
    out.text(&signature.name);
    out.tag(match signature.visibility {
        crate::Visibility::Private => 0,
        crate::Visibility::Public => 1,
    });
    out.flag(signature.is_async);
    out.count(signature.parameters.len());
    for parameter in &signature.parameters {
        out.text(&parameter.name);
        out.count(parameter.ty);
        out.tag(match parameter.mode {
            crate::PassMode::Owned => 0,
            crate::PassMode::SharedBorrow => 1,
            crate::PassMode::MutableBorrow => 2,
        });
    }
    out.count(signature.result);
    out.count(signature.effects.len());
    for effect in &signature.effects {
        out.text(effect);
    }
}

fn write_constant(out: &mut Writer, constant: &Constant) {
    match constant {
        Constant::Unit => out.tag(0),
        Constant::Bool(value) => {
            out.tag(1);
            out.flag(*value);
        }
        Constant::Int(kind, value) => {
            out.tag(2);
            out.text(kind.spelled());
            out.signed(*value);
        }
        Constant::Size(value) => {
            out.tag(3);
            out.number(*value);
        }
        Constant::Duration(value) => {
            out.tag(4);
            out.number(*value);
        }
        Constant::Text(value) => {
            out.tag(5);
            out.text(value);
        }
        Constant::Bytes(value) => {
            out.tag(6);
            out.blob(value);
        }
    }
}

fn write_function(out: &mut Writer, function: &Function) {
    write_signature(out, &function.signature);
    out.tag(match function.origin {
        crate::FunctionOrigin::Declared => 0,
        crate::FunctionOrigin::LoweredBody => 1,
    });
    out.count(function.source);
    out.number(function.stack_contribution);
    out.number(function.fuel_contribution);
    out.number(function.cleanup_contribution);
    out.count(function.values.len());
    for ty in &function.values {
        out.count(*ty);
    }
    out.count(function.blocks.len());
    for block in &function.blocks {
        write_block(out, block);
    }
}

fn write_block(out: &mut Writer, block: &Block) {
    out.count(block.parameters.len());
    for parameter in &block.parameters {
        out.count(*parameter);
    }
    out.count(block.instructions.len());
    for instruction in &block.instructions {
        write_instruction(out, instruction);
    }
    write_terminator(out, &block.terminator);
    out.count(block.source);
}

fn write_instruction(out: &mut Writer, instruction: &Instruction) {
    match instruction.result {
        Some(value) => {
            out.tag(1);
            out.count(value);
        }
        None => out.tag(0),
    }
    out.count(instruction.ty);
    write_op(out, &instruction.op);
    out.count(instruction.source);
    out.flag(instruction.unsafe_block);
    write_optional_text(out, instruction.runtime_contract.as_deref());
    write_optional_text(out, instruction.unsafe_interface.as_deref());
}

fn write_optional_text(out: &mut Writer, value: Option<&str>) {
    match value {
        Some(text) => {
            out.tag(1);
            out.text(text);
        }
        None => out.tag(0),
    }
}

fn write_operand(out: &mut Writer, operand: &Operand) {
    match operand {
        Operand::Value(value) => {
            out.tag(0);
            out.count(*value);
        }
        Operand::Constant(constant) => {
            out.tag(1);
            out.count(*constant);
        }
    }
}

fn write_operands(out: &mut Writer, operands: &[Operand]) {
    out.count(operands.len());
    for operand in operands {
        write_operand(out, operand);
    }
}

fn write_place(out: &mut Writer, place: &Place) {
    out.count(place.root);
    out.count(place.path.len());
    for step in &place.path {
        match step {
            PlaceStep::Field(index) => {
                out.tag(0);
                out.count(*index);
            }
            PlaceStep::Index(Some(index)) => {
                out.tag(1);
                out.number(*index as u128);
            }
            PlaceStep::Index(None) => out.tag(2),
            PlaceStep::DynamicIndex(value) => {
                out.tag(3);
                out.count(*value);
            }
        }
    }
}

fn write_op(out: &mut Writer, op: &Op) {
    match op {
        Op::Const(constant) => {
            out.tag(0);
            out.count(*constant);
        }
        Op::Aggregate { ty, operands } => {
            out.tag(1);
            out.count(*ty);
            write_operands(out, operands);
        }
        Op::Variant {
            ty,
            index,
            operands,
        } => {
            out.tag(2);
            out.count(*ty);
            out.count(*index);
            write_operands(out, operands);
        }
        Op::Read { place } => {
            out.tag(3);
            write_place(out, place);
        }
        Op::Move { place } => {
            out.tag(4);
            write_place(out, place);
        }
        Op::Write { place, value } => {
            out.tag(5);
            write_place(out, place);
            write_operand(out, value);
        }
        Op::Borrow { place, kind } => {
            out.tag(6);
            write_place(out, place);
            out.tag(match kind {
                BorrowKind::Shared => 0,
                BorrowKind::Mutable => 1,
            });
        }
        Op::Drop { place } => {
            out.tag(7);
            write_place(out, place);
        }
        Op::Binary { op, left, right } => {
            out.tag(8);
            out.tag(binary_tag(*op));
            write_operand(out, left);
            write_operand(out, right);
        }
        Op::Unary { op, operand } => {
            out.tag(9);
            out.tag(match op {
                UnaryOp::Negate => 0,
                UnaryOp::Not => 1,
            });
            write_operand(out, operand);
        }
        Op::Widen { operand, to } => {
            out.tag(10);
            write_operand(out, operand);
            out.text(to.spelled());
        }
        Op::Call { target, operands } => {
            out.tag(11);
            match target {
                CallTarget::Local(index) => {
                    out.tag(0);
                    out.count(*index);
                }
                CallTarget::Imported { import, name } => {
                    out.tag(1);
                    out.count(*import);
                    out.text(name);
                }
                CallTarget::Predeclared(name) => {
                    out.tag(2);
                    out.text(name);
                }
            }
            write_operands(out, operands);
        }
        Op::Spawn { body, captures } => {
            out.tag(12);
            out.count(*body);
            write_operands(out, captures);
        }
        Op::Join { task } => {
            out.tag(13);
            write_operand(out, task);
        }
        Op::Await { task } => {
            out.tag(14);
            write_operand(out, task);
        }
        Op::Cancel { task } => {
            out.tag(15);
            write_operand(out, task);
        }
        Op::Atomic {
            operation,
            target,
            operands,
            order,
            failure_order,
        } => {
            out.tag(16);
            out.tag(atomic_tag(*operation));
            write_operand(out, target);
            write_operands(out, operands);
            out.text(order.spelled());
            match failure_order {
                Some(order) => {
                    out.tag(1);
                    out.text(order.spelled());
                }
                None => out.tag(0),
            }
        }
        Op::Capability {
            import,
            right,
            operands,
        } => {
            out.tag(17);
            out.count(*import);
            out.text(right);
            write_operands(out, operands);
        }
        Op::Resource {
            kind,
            amount,
            release,
        } => {
            out.tag(18);
            out.tag(resource_tag(*kind));
            write_operand(out, amount);
            out.flag(*release);
        }
        Op::RegisterCleanup { body } => {
            out.tag(19);
            out.count(*body);
        }
        Op::RunCleanups { calls } => {
            out.tag(20);
            out.count(calls.len());
            for call in calls {
                out.count(call.body);
                write_operands(out, &call.captures);
            }
        }
        Op::Closure { body, captures } => {
            out.tag(21);
            out.count(*body);
            write_operands(out, captures);
        }
        Op::CallValue { callee, operands } => {
            out.tag(22);
            write_operand(out, callee);
            write_operands(out, operands);
        }
    }
}

fn binary_tag(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Add => 0,
        BinaryOp::Subtract => 1,
        BinaryOp::Multiply => 2,
        BinaryOp::Divide => 3,
        BinaryOp::Remainder => 4,
        BinaryOp::ShiftLeft => 5,
        BinaryOp::ShiftRight => 6,
        BinaryOp::BitAnd => 7,
        BinaryOp::BitOr => 8,
        BinaryOp::BitXor => 9,
        BinaryOp::Equal => 10,
        BinaryOp::NotEqual => 11,
        BinaryOp::Less => 12,
        BinaryOp::LessOrEqual => 13,
        BinaryOp::Greater => 14,
        BinaryOp::GreaterOrEqual => 15,
        BinaryOp::LogicalAnd => 16,
        BinaryOp::LogicalOr => 17,
    }
}

fn atomic_tag(op: AtomicOp) -> u8 {
    match op {
        AtomicOp::Load => 0,
        AtomicOp::Store => 1,
        AtomicOp::Swap => 2,
        AtomicOp::FetchAdd => 3,
        AtomicOp::FetchSub => 4,
        AtomicOp::FetchAnd => 5,
        AtomicOp::FetchOr => 6,
        AtomicOp::FetchXor => 7,
        AtomicOp::CompareExchange => 8,
    }
}

fn resource_tag(kind: ResourceKind) -> u8 {
    match kind {
        ResourceKind::Fuel => 0,
        ResourceKind::Stack => 1,
        ResourceKind::Allocation => 2,
        ResourceKind::Task => 3,
        ResourceKind::Worker => 4,
        ResourceKind::Sync => 5,
        ResourceKind::Shared => 6,
        ResourceKind::Cleanup => 7,
        ResourceKind::Recursion => 8,
    }
}

fn write_terminator(out: &mut Writer, terminator: &Terminator) {
    match terminator {
        Terminator::Return(value) => {
            out.tag(0);
            match value {
                Some(operand) => {
                    out.tag(1);
                    write_operand(out, operand);
                }
                None => out.tag(0),
            }
        }
        Terminator::Branch { target, arguments } => {
            out.tag(1);
            out.count(*target);
            write_operands(out, arguments);
        }
        Terminator::BranchIf {
            condition,
            true_target,
            true_arguments,
            false_target,
            false_arguments,
        } => {
            out.tag(2);
            write_operand(out, condition);
            out.count(*true_target);
            write_operands(out, true_arguments);
            out.count(*false_target);
            write_operands(out, false_arguments);
        }
        Terminator::MatchEnum { subject, arms } => {
            out.tag(3);
            write_operand(out, subject);
            out.count(arms.len());
            for (variant, target) in arms {
                out.count(*variant);
                out.count(*target);
            }
        }
        Terminator::PropagateError { result, ok_target } => {
            out.tag(4);
            write_operand(out, result);
            out.count(*ok_target);
        }
        Terminator::Trap(code) => {
            out.tag(5);
            out.text(code);
        }
    }
}

fn write_source_entry(out: &mut Writer, entry: &SourceMapEntry) {
    out.text(&entry.source_set);
    out.text(&entry.path);
    out.text(&entry.content_id);
    out.text(&entry.frontend_identity);
    out.text(&entry.language_version);
    out.text(entry.profile.spelled());
    out.text(&entry.unicode_normalization_baseline);
    out.count(entry.byte_start);
    out.count(entry.byte_end);
    match entry.derived_from {
        Some(parent) => {
            out.tag(1);
            out.count(parent);
        }
        None => out.tag(0),
    }
}
