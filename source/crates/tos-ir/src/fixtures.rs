// SPDX-License-Identifier: GPL-3.0-or-later
//! The all-variants module.
//!
//! One fixture, owned by the crate that owns the schema. A copy of it in each
//! crate that needs one would be several fixtures to keep complete, and the
//! whole point of this module is completeness: every tagged variant of
//! `tos-ir/v1` appears in it, so a traversal that loses one — an encoder, a
//! parser, a digest, an accounting walk — fails a test instead of a receipt.
//!
//! Behind a feature and off in every production build. It is data, not
//! behaviour: it constructs a module and nothing else.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::*;

/// A module that uses **every** tagged variant `tos-ir/v1` has.
///
/// Not a plausible program — it is not meant to verify — but a complete one in
/// the only sense that matters here: if a variant exists in the schema, it is in
/// this module, so a round trip that loses one fails a test rather than a
/// receipt.
pub fn every_variant() -> Module {
    let types = vec![
        TypeDef::Unit,
        TypeDef::Bool,
        TypeDef::Int(IntKind::I8),
        TypeDef::Int(IntKind::I16),
        TypeDef::Int(IntKind::I32),
        TypeDef::Int(IntKind::I64),
        TypeDef::Int(IntKind::U8),
        TypeDef::Int(IntKind::U16),
        TypeDef::Int(IntKind::U32),
        TypeDef::Int(IntKind::U64),
        TypeDef::Size,
        TypeDef::Duration,
        TypeDef::Text,
        TypeDef::Bytes,
        TypeDef::ConversionError,
        TypeDef::Event,
        TypeDef::Semaphore,
        TypeDef::Barrier,
        TypeDef::Latch,
        TypeDef::AtomicBool,
        TypeDef::AtomicU32,
        TypeDef::AtomicU64,
        TypeDef::Option(0),
        TypeDef::Task(1),
        TypeDef::TaskResult(2),
        TypeDef::Shared(3),
        TypeDef::Region(4),
        TypeDef::DmaRegion(5),
        TypeDef::RegionMut(6),
        TypeDef::DmaRegionMut(7),
        TypeDef::Mutex(8),
        TypeDef::RwLock(9),
        TypeDef::MutexGuard(10),
        TypeDef::ReadGuard(11),
        TypeDef::WriteGuard(12),
        TypeDef::Channel(13),
        TypeDef::Slice(14),
        TypeDef::Result(0, 1),
        TypeDef::Array(2, u64::MAX),
        TypeDef::Tuple(vec![0, 1, 2]),
        TypeDef::Function(vec![0, 1], 2),
        TypeDef::Capability(String::from("system.time.Clock")),
        TypeDef::Nominal {
            module_content_id: String::from("sha256:nominal"),
            export_name: String::from("Point"),
            kind: NominalKind::Record,
            fields: vec![2, 2],
            variants: Vec::new(),
        },
        TypeDef::Nominal {
            module_content_id: String::from("sha256:nominal"),
            export_name: String::from("Shape"),
            kind: NominalKind::Enum,
            fields: Vec::new(),
            variants: vec![
                Variant {
                    name: String::from("Round"),
                    payload: vec![2],
                },
                Variant {
                    name: String::from("Square"),
                    payload: Vec::new(),
                },
            ],
        },
    ];

    let constants = vec![
        Constant::Unit,
        Constant::Bool(true),
        Constant::Bool(false),
        Constant::Int(IntKind::I64, i128::MIN),
        Constant::Int(IntKind::U64, i128::MAX),
        Constant::Size(u128::MAX),
        Constant::Duration(0),
        Constant::Text(String::from("a text constant")),
        Constant::Bytes(vec![0, 1, 254, 255]),
    ];

    let places = [
        Place {
            root: 0,
            path: Vec::new(),
        },
        Place {
            root: 1,
            path: vec![
                PlaceStep::Field(3),
                PlaceStep::Index(Some(u64::MAX)),
                PlaceStep::Index(None),
                PlaceStep::DynamicIndex(2),
            ],
        },
    ];

    let mut operations = vec![
        Op::Const(0),
        Op::Aggregate {
            ty: 39,
            operands: vec![Operand::Value(0), Operand::Constant(1)],
        },
        Op::Variant {
            ty: 43,
            index: 1,
            operands: Vec::new(),
        },
        Op::Read {
            place: places[0].clone(),
        },
        Op::Move {
            place: places[1].clone(),
        },
        Op::Write {
            place: places[1].clone(),
            value: Operand::Constant(0),
        },
        Op::Borrow {
            place: places[0].clone(),
            kind: BorrowKind::Shared,
        },
        Op::Borrow {
            place: places[0].clone(),
            kind: BorrowKind::Mutable,
        },
        Op::Drop {
            place: places[0].clone(),
        },
        Op::Unary {
            op: UnaryOp::Negate,
            operand: Operand::Value(0),
        },
        Op::Unary {
            op: UnaryOp::Not,
            operand: Operand::Value(0),
        },
        Op::Call {
            target: CallTarget::Local(0),
            operands: Vec::new(),
        },
        Op::Call {
            target: CallTarget::Imported {
                import: 0,
                name: String::from("origin"),
            },
            operands: vec![Operand::Value(1)],
        },
        Op::Call {
            target: CallTarget::Predeclared(String::from("checked_add")),
            operands: Vec::new(),
        },
        Op::Spawn {
            body: 0,
            captures: vec![Operand::Value(0)],
        },
        Op::Join {
            task: Operand::Value(0),
        },
        Op::Await {
            task: Operand::Value(0),
        },
        Op::Cancel {
            task: Operand::Value(0),
        },
        Op::Capability {
            // Both sources, so a fixture that exercises "every variant" covers
            // the runtime-value case as well as the import case (ADR-0078).
            capabilities: vec![
                crate::CapabilitySource::Import(0),
                crate::CapabilitySource::Value(Operand::Value(0)),
            ],
            right: String::from("read"),
            operands: vec![Operand::Constant(0)],
        },
        Op::RegisterCleanup { body: 0 },
        Op::RunCleanups {
            calls: vec![CleanupCall {
                body: 0,
                captures: vec![Operand::Value(0)],
            }],
        },
        Op::Closure {
            body: 0,
            captures: Vec::new(),
        },
        Op::CallValue {
            callee: Operand::Value(0),
            operands: vec![Operand::Constant(0)],
        },
        Op::Share {
            operand: Operand::Value(0),
        },
    ];
    for op in [
        BinaryOp::Add,
        BinaryOp::Subtract,
        BinaryOp::Multiply,
        BinaryOp::Divide,
        BinaryOp::Remainder,
        BinaryOp::ShiftLeft,
        BinaryOp::ShiftRight,
        BinaryOp::BitAnd,
        BinaryOp::BitOr,
        BinaryOp::BitXor,
        BinaryOp::Equal,
        BinaryOp::NotEqual,
        BinaryOp::Less,
        BinaryOp::LessOrEqual,
        BinaryOp::Greater,
        BinaryOp::GreaterOrEqual,
        BinaryOp::LogicalAnd,
        BinaryOp::LogicalOr,
    ] {
        operations.push(Op::Binary {
            op,
            left: Operand::Value(0),
            right: Operand::Constant(0),
        });
    }
    for to in [
        IntKind::I8,
        IntKind::I16,
        IntKind::I32,
        IntKind::I64,
        IntKind::U8,
        IntKind::U16,
        IntKind::U32,
        IntKind::U64,
    ] {
        operations.push(Op::Widen {
            operand: Operand::Value(0),
            to,
        });
    }
    for mode in [LockMode::Mutex, LockMode::Read, LockMode::Write] {
        operations.push(Op::Lock {
            object: Operand::Value(0),
            mode,
        });
    }
    for kind in [
        ResourceKind::Fuel,
        ResourceKind::Stack,
        ResourceKind::Allocation,
        ResourceKind::Task,
        ResourceKind::Worker,
        ResourceKind::Sync,
        ResourceKind::Shared,
        ResourceKind::Cleanup,
        ResourceKind::Recursion,
    ] {
        operations.push(Op::Resource {
            kind,
            amount: Operand::Constant(0),
            release: kind == ResourceKind::Fuel,
        });
    }
    let orders = [
        MemoryOrder::Relaxed,
        MemoryOrder::Acquire,
        MemoryOrder::Release,
        MemoryOrder::AcqRel,
        MemoryOrder::SeqCst,
    ];
    for operation in [
        AtomicOp::Load,
        AtomicOp::Store,
        AtomicOp::Swap,
        AtomicOp::FetchAdd,
        AtomicOp::FetchSub,
        AtomicOp::FetchAnd,
        AtomicOp::FetchOr,
        AtomicOp::FetchXor,
        AtomicOp::CompareExchange,
    ] {
        for (at, order) in orders.iter().enumerate() {
            operations.push(Op::Atomic {
                operation,
                target: Operand::Value(0),
                operands: vec![Operand::Constant(0)],
                order: *order,
                failure_order: (at % 2 == 0).then_some(orders[(at + 1) % orders.len()]),
            });
        }
    }

    let instructions: Vec<Instruction> = operations
        .into_iter()
        .enumerate()
        .map(|(at, op)| Instruction {
            result: (at % 3 != 0).then_some(at),
            ty: at % types.len(),
            op,
            source: at % 3,
            runtime_contract: (at % 4 == 0).then(|| String::from("tos-runtime/task/v1")),
            unsafe_block: at % 5 == 0,
            unsafe_interface: (at % 7 == 0).then(|| String::from("accepted.ffi/v1")),
        })
        .collect();

    let terminators = vec![
        Terminator::Return(None),
        Terminator::Return(Some(Operand::Value(0))),
        Terminator::Branch {
            target: 0,
            arguments: vec![Operand::Value(0)],
        },
        Terminator::BranchIf {
            condition: Operand::Value(0),
            true_target: 0,
            true_arguments: vec![Operand::Constant(0)],
            false_target: 1,
            false_arguments: Vec::new(),
        },
        Terminator::MatchEnum {
            subject: Operand::Value(0),
            arms: vec![(0, 1), (1, 2)],
        },
        Terminator::PropagateError {
            result: Operand::Value(0),
            ok_target: 3,
        },
        Terminator::Trap(String::from("E9999_TRAP")),
    ];

    let blocks: Vec<Block> = terminators
        .into_iter()
        .enumerate()
        .map(|(at, terminator)| Block {
            parameters: vec![at % types.len()],
            instructions: if at == 0 {
                instructions.clone()
            } else {
                Vec::new()
            },
            terminator,
            source: at % 3,
        })
        .collect();

    let signature = |name: &str, visibility, mode| Signature {
        name: String::from(name),
        visibility,
        is_async: matches!(visibility, Visibility::Public),
        parameters: vec![Parameter {
            name: String::from("argument"),
            ty: 4,
            mode,
        }],
        result: 4,
        effects: vec![String::from("system.time/v1")],
    };

    let functions = vec![
        Function {
            signature: signature("declared", Visibility::Public, PassMode::Owned),
            origin: FunctionOrigin::Declared,
            source: 0,
            stack_contribution: u128::MAX,
            fuel_contribution: 0,
            cleanup_contribution: 7,
            values: vec![4, 4, 4],
            blocks,
        },
        Function {
            signature: signature("lowered", Visibility::Private, PassMode::SharedBorrow),
            origin: FunctionOrigin::LoweredBody,
            source: 1,
            stack_contribution: 1,
            fuel_contribution: 2,
            cleanup_contribution: 3,
            values: Vec::new(),
            blocks: Vec::new(),
        },
        Function {
            signature: signature("borrowing", Visibility::Private, PassMode::MutableBorrow),
            origin: FunctionOrigin::LoweredBody,
            source: 2,
            stack_contribution: 0,
            fuel_contribution: 0,
            cleanup_contribution: 0,
            values: Vec::new(),
            blocks: Vec::new(),
        },
    ];

    let entry = |profile, byte_start, derived_from| SourceMapEntry {
        source_set: String::from("tos-image-tests"),
        path: String::from("set/every.tos"),
        content_id: String::from("sha256:content"),
        frontend_identity: String::from("tos-core-reference/0.1.0"),
        language_version: String::from(crate::LANGUAGE_VERSION),
        profile,
        unicode_normalization_baseline: String::from(crate::UNICODE_BASELINE),
        byte_start,
        byte_end: byte_start + 4,
        derived_from,
    };

    Module {
        header: Header {
            schema_id: String::from(crate::SCHEMA_ID),
            language_version: String::from(crate::LANGUAGE_VERSION),
            unicode_normalization_baseline: String::from(crate::UNICODE_BASELINE),
            profile: Profile::Full,
            module_name: String::from("set.every"),
            source_set: String::from("tos-image-tests"),
            path: String::from("set/every.tos"),
            content_id: String::from("sha256:content"),
            dependency_digest: String::from("sha256:dependencies"),
            frontend_identity: String::from("tos-core-reference/0.1.0"),
            source_map_revision: String::from(crate::SOURCE_MAP_REVISION),
            resource_envelope: ResourceEnvelope {
                fuel: u128::MAX,
                stack: 1,
                allocation: 2,
                tasks: 3,
                workers: 4,
                sync: 5,
                shared: 6,
                cleanup: 7,
                recursion: 8,
                imports: 9,
            },
            capability_interface_digest: String::from("sha256:capabilities"),
        },
        types,
        imports: vec![Import {
            module_name: String::from("set.other"),
            module_content_id: String::from("sha256:other"),
            binding: String::from("other"),
        }],
        capability_imports: vec![CapabilityImport {
            interface: String::from("system.time.Clock"),
            binding: String::from("clock"),
            ty: 41,
        }],
        exports: vec![signature("declared", Visibility::Public, PassMode::Owned)],
        constants,
        functions,
        source_map: vec![
            entry(Profile::Bootstrap, 0, None),
            entry(Profile::Full, 8, Some(0)),
            entry(Profile::Full, 16, None),
        ],
    }
}
