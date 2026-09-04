// SPDX-License-Identifier: GPL-3.0-or-later
//! What the format has to prove before anything executes what it holds.

use super::*;
use alloc::vec;

/// The all-variants module, from the crate that owns the schema.
///
/// Not restated here: a fixture the format tests kept to themselves would be a
/// second list of what `tos-ir/v1` contains, and two lists is one too many.
use tos_ir::fixtures::every_variant;

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
            TypeDef::MmioRegion => "TypeDef::MmioRegion",
            TypeDef::MmioRegionMut => "TypeDef::MmioRegionMut",
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
                    Op::MmioRead { .. } => "Op::MmioRead",
                    Op::MmioWrite { .. } => "Op::MmioWrite",
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
/// A source map that walks backwards, ends before it starts and reaches the
/// ceiling still round-trips exactly.
///
/// Spans are written as signed steps (encoding version 2), which is what makes
/// them cheap — and a step encoding is only admissible if it is **total**. A
/// map that jumps backwards between entries, an entry whose end precedes its
/// start, and offsets far enough apart to need several varint bytes are the
/// three cases the arithmetic has to survive; none of them is something the
/// frontend is expected to produce, and all of them are something an image may
/// contain.
#[test]
fn a_source_map_that_walks_backwards_survives_the_round_trip() {
    let mut module = every_variant();
    let template = module
        .source_map
        .first()
        .cloned()
        .expect("the fixture has a source map");
    let spans: [(usize, usize); 6] = [
        (1_000, 1_010),
        (12, 20),           // backwards from the entry before it
        (262_143, 262_144), // at the published source ceiling
        (500, 400),         // an end before its own start
        (0, 0),             // empty, at the origin
        (262_144, 0),       // the longest backwards step the ceiling admits
    ];
    module.source_map = spans
        .iter()
        .map(|(start, end)| SourceMapEntry {
            byte_start: *start,
            byte_end: *end,
            derived_from: None,
            ..template.clone()
        })
        .collect();

    let (image, _) = encode(&module);
    let parsed = parse(&image, &limits()).expect("the image parses");
    assert_eq!(
        parsed.source_map, module.source_map,
        "every span comes back as itself, whichever way it stepped"
    );
    assert_eq!(parsed, module, "and so does the rest of the module");
}

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
