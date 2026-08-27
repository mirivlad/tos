// SPDX-License-Identifier: GPL-3.0-or-later
//! What the format has to prove before anything executes what it holds.

use super::*;
use alloc::vec;

/// The accepted V1 ceilings, restated here so the tests do not reach for the
/// verifier either.
fn limits() -> ParseLimits {
    ParseLimits {
        table_entries: 65_536,
        modules: 256,
        fields: 1024,
        parameters: 128,
        blocks_per_function: 4096,
        instructions_per_block: 65_536,
        source_map_entries: 262_144,
    }
}

/// A module that uses **every** tagged variant `tos-ir/v1` has.
///
/// Not a plausible program — it is not meant to verify — but a complete one in
/// the only sense that matters here: if a variant exists in the schema, it is in
/// this module, so a round trip that loses one fails a test rather than a
/// receipt.
fn every_variant() -> Module {
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
            import: 0,
            further_imports: vec![0, 0],
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
        language_version: String::from(tos_ir::LANGUAGE_VERSION),
        profile,
        unicode_normalization_baseline: String::from(tos_ir::UNICODE_BASELINE),
        byte_start,
        byte_end: byte_start + 4,
        derived_from,
    };

    Module {
        header: Header {
            schema_id: String::from(tos_ir::SCHEMA_ID),
            language_version: String::from(tos_ir::LANGUAGE_VERSION),
            unicode_normalization_baseline: String::from(tos_ir::UNICODE_BASELINE),
            profile: Profile::Full,
            module_name: String::from("set.every"),
            source_set: String::from("tos-image-tests"),
            path: String::from("set/every.tos"),
            content_id: String::from("sha256:content"),
            dependency_digest: String::from("sha256:dependencies"),
            frontend_identity: String::from("tos-core-reference/0.1.0"),
            source_map_revision: String::from(tos_ir::SOURCE_MAP_REVISION),
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

/// The name of every tagged variant present in a module.
///
/// Used to say what a fixture covers by counting rather than by claiming.
fn variants_present(module: &Module) -> BTreeSet<&'static str> {
    let mut seen = BTreeSet::new();
    for definition in &module.types {
        seen.insert(match definition {
            TypeDef::Unit => "TypeDef::Unit",
            TypeDef::Bool => "TypeDef::Bool",
            TypeDef::Int(_) => "TypeDef::Int",
            TypeDef::Size => "TypeDef::Size",
            TypeDef::Duration => "TypeDef::Duration",
            TypeDef::Text => "TypeDef::Text",
            TypeDef::Bytes => "TypeDef::Bytes",
            TypeDef::ConversionError => "TypeDef::ConversionError",
            TypeDef::Event => "TypeDef::Event",
            TypeDef::Semaphore => "TypeDef::Semaphore",
            TypeDef::Barrier => "TypeDef::Barrier",
            TypeDef::Latch => "TypeDef::Latch",
            TypeDef::AtomicBool => "TypeDef::AtomicBool",
            TypeDef::AtomicU32 => "TypeDef::AtomicU32",
            TypeDef::AtomicU64 => "TypeDef::AtomicU64",
            TypeDef::Option(_) => "TypeDef::Option",
            TypeDef::Task(_) => "TypeDef::Task",
            TypeDef::TaskResult(_) => "TypeDef::TaskResult",
            TypeDef::Shared(_) => "TypeDef::Shared",
            TypeDef::Region(_) => "TypeDef::Region",
            TypeDef::DmaRegion(_) => "TypeDef::DmaRegion",
            TypeDef::RegionMut(_) => "TypeDef::RegionMut",
            TypeDef::DmaRegionMut(_) => "TypeDef::DmaRegionMut",
            TypeDef::Mutex(_) => "TypeDef::Mutex",
            TypeDef::RwLock(_) => "TypeDef::RwLock",
            TypeDef::MutexGuard(_) => "TypeDef::MutexGuard",
            TypeDef::ReadGuard(_) => "TypeDef::ReadGuard",
            TypeDef::WriteGuard(_) => "TypeDef::WriteGuard",
            TypeDef::Channel(_) => "TypeDef::Channel",
            TypeDef::Slice(_) => "TypeDef::Slice",
            TypeDef::Result(_, _) => "TypeDef::Result",
            TypeDef::Array(_, _) => "TypeDef::Array",
            TypeDef::Tuple(_) => "TypeDef::Tuple",
            TypeDef::Function(_, _) => "TypeDef::Function",
            TypeDef::Capability(_) => "TypeDef::Capability",
            TypeDef::Nominal { .. } => "TypeDef::Nominal",
        });
    }
    for constant in &module.constants {
        seen.insert(match constant {
            Constant::Unit => "Constant::Unit",
            Constant::Bool(_) => "Constant::Bool",
            Constant::Int(_, _) => "Constant::Int",
            Constant::Size(_) => "Constant::Size",
            Constant::Duration(_) => "Constant::Duration",
            Constant::Text(_) => "Constant::Text",
            Constant::Bytes(_) => "Constant::Bytes",
        });
    }
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                seen.insert(match &instruction.op {
                    Op::Const(_) => "Op::Const",
                    Op::Aggregate { .. } => "Op::Aggregate",
                    Op::Variant { .. } => "Op::Variant",
                    Op::Read { .. } => "Op::Read",
                    Op::Move { .. } => "Op::Move",
                    Op::Write { .. } => "Op::Write",
                    Op::Borrow { .. } => "Op::Borrow",
                    Op::Drop { .. } => "Op::Drop",
                    Op::Binary { .. } => "Op::Binary",
                    Op::Unary { .. } => "Op::Unary",
                    Op::Widen { .. } => "Op::Widen",
                    Op::Call { .. } => "Op::Call",
                    Op::Spawn { .. } => "Op::Spawn",
                    Op::Closure { .. } => "Op::Closure",
                    Op::CallValue { .. } => "Op::CallValue",
                    Op::Lock { .. } => "Op::Lock",
                    Op::Share { .. } => "Op::Share",
                    Op::Join { .. } => "Op::Join",
                    Op::Await { .. } => "Op::Await",
                    Op::Cancel { .. } => "Op::Cancel",
                    Op::Atomic { .. } => "Op::Atomic",
                    Op::Capability { .. } => "Op::Capability",
                    Op::Resource { .. } => "Op::Resource",
                    Op::RegisterCleanup { .. } => "Op::RegisterCleanup",
                    Op::RunCleanups { .. } => "Op::RunCleanups",
                });
            }
            seen.insert(match &block.terminator {
                Terminator::Return(_) => "Terminator::Return",
                Terminator::Branch { .. } => "Terminator::Branch",
                Terminator::BranchIf { .. } => "Terminator::BranchIf",
                Terminator::MatchEnum { .. } => "Terminator::MatchEnum",
                Terminator::PropagateError { .. } => "Terminator::PropagateError",
                Terminator::Trap(_) => "Terminator::Trap",
            });
        }
    }
    seen
}

/// The whole schema, and the fixture covers it.
///
/// 36 type constructors, 25 operations, 6 terminators, 7 constants. Counted
/// against the fixture rather than asserted about the encoder, so a variant
/// added to `tos-ir` and forgotten here fails this test before it reaches a
/// format that cannot write it.
#[test]
fn the_fixture_uses_every_tagged_variant() {
    let present = variants_present(&every_variant());
    let types = present
        .iter()
        .filter(|name| name.starts_with("TypeDef::"))
        .count();
    let operations = present
        .iter()
        .filter(|name| name.starts_with("Op::"))
        .count();
    let terminators = present
        .iter()
        .filter(|name| name.starts_with("Terminator::"))
        .count();
    let constants = present
        .iter()
        .filter(|name| name.starts_with("Constant::"))
        .count();
    assert_eq!(types, 36, "every TypeDef constructor");
    assert_eq!(operations, 25, "every Op");
    assert_eq!(terminators, 6, "every Terminator");
    assert_eq!(constants, 7, "every Constant");
}

/// The invariant every byte figure and every receipt rests on.
#[test]
fn a_module_survives_encode_and_parse_exactly() {
    let module = every_variant();
    let (image, _) = encode(&module);
    let parsed = parse(&image, &limits()).expect("its own image parses");
    assert_eq!(parsed, module, "the module is reconstructed exactly");
    assert_eq!(
        tos_ir::module_digest(&parsed),
        tos_ir::module_digest(&module),
        "the semantic digest is unchanged"
    );
}

/// Reproducible bytes: the same module always encodes the same way, and a round
/// trip is a fixed point.
#[test]
fn encoding_is_reproducible() {
    let module = every_variant();
    let (first, layout) = encode(&module);
    let (second, again) = encode(&module);
    assert_eq!(first, second, "the same module encodes to the same bytes");
    assert_eq!(layout, again);

    let parsed = parse(&first, &limits()).expect("parses");
    let (third, _) = encode(&parsed);
    assert_eq!(first, third, "re-encoding a parsed module is a fixed point");
    assert_eq!(artifact_digest(&first), artifact_digest(&third));
}

/// A cache is deletable and regenerable: nothing about an image is a source of
/// truth, so throwing one away and making it again costs speed and nothing else.
#[test]
fn an_image_regenerates_identically() {
    let module = every_variant();
    let (image, _) = encode(&module);
    let digest = artifact_digest(&image);
    let semantic = tos_ir::module_digest(&module);
    drop(image);

    let (regenerated, _) = encode(&module);
    assert_eq!(artifact_digest(&regenerated), digest);
    let parsed = parse(&regenerated, &limits()).expect("parses");
    assert_eq!(tos_ir::module_digest(&parsed), semantic);
}

/// The frame's own refusals.
#[test]
fn the_frame_refuses_what_it_should() {
    let (good, _) = encode(&every_variant());
    let limits = limits();

    let mut bad = good.clone();
    bad[0] ^= 0xff;
    assert_eq!(parse(&bad, &limits), Err(ImageError::BadMagic));

    let mut bad = good.clone();
    bad[11] = 9;
    assert_eq!(
        parse(&bad, &limits),
        Err(ImageError::UnknownEncodingVersion(9))
    );

    let mut bad = good.clone();
    bad[15] = 9;
    assert_eq!(
        parse(&bad, &limits),
        Err(ImageError::UnknownSchemaVersion(9))
    );

    assert_eq!(
        parse(&good[..12], &limits),
        Err(ImageError::Truncated("frame"))
    );

    assert!(matches!(
        parse(&good[..good.len() - 64], &limits),
        Err(ImageError::Truncated(_))
    ));

    let mut bad = good.clone();
    bad[16..24].copy_from_slice(&u64::MAX.to_be_bytes());
    assert!(matches!(
        parse(&bad, &limits),
        Err(ImageError::Oversized { .. })
    ));

    let mut bad = good.clone();
    bad[16..24].copy_from_slice(&((MAX_IMAGE_BYTES as u64) - 1).to_be_bytes());
    assert_eq!(parse(&bad, &limits), Err(ImageError::Truncated("payload")));

    let mut bad = good.clone();
    bad.push(0);
    assert!(matches!(
        parse(&bad, &limits),
        Err(ImageError::TrailingBytes(_))
    ));

    let mut bad = good.clone();
    bad[FRAME_HEADER + 8] ^= 0x01;
    assert_eq!(parse(&bad, &limits), Err(ImageError::WrongDigest));

    let mut bad = good.clone();
    let last = bad.len() - 1;
    bad[last] ^= 0x01;
    assert_eq!(parse(&bad, &limits), Err(ImageError::WrongDigest));
}

/// The payload's refusals, each sealed with a **valid** artifact digest.
///
/// An attacker who controls the bytes controls the digest too, so a digest is
/// integrity and never authenticity: the payload parser has to stand on its own.
#[test]
fn the_payload_refuses_what_it_should() {
    let limits = limits();
    type Case = (&'static str, Vec<u8>, fn(&ImageError) -> bool);
    let cases: &[Case] = &[
        ("repeated string", vec![0x02, 0x01, b'a', 0x01, b'a'], |e| {
            matches!(e, ImageError::NonCanonicalTable("string table"))
        }),
        (
            "unsorted strings",
            vec![0x02, 0x01, b'b', 0x01, b'a'],
            |e| matches!(e, ImageError::NonCanonicalTable("string table")),
        ),
        ("non-canonical varint", vec![0x80, 0x00], |e| {
            matches!(e, ImageError::NonCanonicalVarint)
        }),
        ("varint past 128 bits", vec![0xff; 24], |e| {
            matches!(e, ImageError::VarintOverflow)
        }),
        (
            "count past the limit",
            vec![0xff, 0xff, 0xff, 0xff, 0x0f],
            |e| matches!(e, ImageError::CountExceedsLimit { .. }),
        ),
        ("count past the bytes", vec![0x40], |e| {
            matches!(e, ImageError::Truncated("string table"))
        }),
        ("empty payload", Vec::new(), |e| {
            matches!(e, ImageError::Truncated("varint"))
        }),
        ("string out of range", vec![0x00, 0x00], |e| {
            matches!(
                e,
                ImageError::OutOfRange {
                    what: "string table"
                }
            )
        }),
        ("not UTF-8", vec![0x01, 0x01, 0xff], |e| {
            matches!(e, ImageError::BadUtf8)
        }),
    ];
    for (what, payload, expected) in cases {
        let image = frame(payload);
        match parse(&image, &limits) {
            Ok(_) => panic!("{what} was accepted"),
            Err(error) => assert!(expected(&error), "{what}: {error:?}"),
        }
    }
}

/// An unknown tag fails closed, in every family that has one.
#[test]
fn an_unknown_tag_fails_closed() {
    let limits = limits();
    let module = every_variant();
    let (image, layout) = encode(&module);
    let payload = &image[FRAME_HEADER..FRAME_HEADER + layout.payload];

    // The types section opens with its count; the first type's tag follows.
    let mut at = layout.strings + layout.header;
    while payload[at] & 0x80 != 0 {
        at += 1;
    }
    at += 1;
    let mut bad = payload.to_vec();
    bad[at] = 0xfe;
    match parse(&frame(&bad), &limits) {
        Err(ImageError::UnknownTag { family, tag }) => {
            assert_eq!(family, "TypeDef");
            assert_eq!(tag, 0xfe);
        }
        other => panic!("an unknown TypeDef tag was not refused: {other:?}"),
    }
}

/// Totality: the parser returns for every input, and every prefix is refused.
#[test]
fn the_parser_is_total() {
    let limits = limits();
    let (good, _) = encode(&every_variant());

    for length in 0..good.len() {
        assert!(
            parse(&good[..length], &limits).is_err(),
            "a proper prefix of an image must not parse: {length}"
        );
    }

    // The hostile case: bytes changed *and* resealed, so what is being tested is
    // the payload parser rather than the digest. A mutation may leave a
    // well-formed image of a different module — deciding whether that module is
    // admissible is the verifier's job — so what this proves is that the parser
    // returns, never that every change is caught.
    let mut state = 0x243f_6a88_85a3_08d3u64;
    let mut refused = 0usize;
    let mut accepted = 0usize;
    for _ in 0..8192 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let at = FRAME_HEADER + (state >> 11) as usize % (good.len() - FRAME_HEADER - DIGEST_BYTES);
        let mut bad = good.clone();
        bad[at] ^= ((state >> 3) & 0xff) as u8;
        reseal(&mut bad);
        match parse(&bad, &limits) {
            Ok(_) => accepted += 1,
            Err(_) => refused += 1,
        }
    }
    assert_eq!(refused + accepted, 8192);

    // And arbitrary bytes, framed and unframed.
    for length in [0usize, 1, 7, 8, 23, 24, 25, 64, 1024] {
        let noise: Vec<u8> = (0..length).map(|at| (at * 37 + 11) as u8).collect();
        let _ = parse(&noise, &limits);
        let _ = parse(&frame(&noise), &limits);
    }
}

/// A count is bounded before anything is allocated from it.
#[test]
fn a_forged_count_allocates_nothing() {
    let limits = limits();
    // A string-table count of nearly four million with two bytes behind it.
    let payload = vec![0xff, 0xff, 0xff, 0x01, 0x00];
    match parse(&frame(&payload), &limits) {
        Err(ImageError::Truncated("string table")) => {}
        other => panic!("a forged count was not bounded by the bytes: {other:?}"),
    }
}
