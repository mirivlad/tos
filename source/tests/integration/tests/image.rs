// SPDX-License-Identifier: GPL-3.0-or-later
//! The module image against real modules: encode, parse, verify, and refuse.
//!
//! `tos-image`'s own tests use a fixture that names every tagged variant. This
//! one uses modules the production frontend actually produced, so that the
//! format is proved against what it will be handed rather than only against
//! what its author thought to write down.

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_image::{artifact_digest, encode, parse, reseal, ImageError};
use tos_ir::Module;
use tos_verifier::{verify, verify_image, ImageRefusal, Limits, ResolutionSnapshot};

/// The accepted ceilings as the parser's bounds — the same conversion the
/// verifier's own trusted path performs.
fn parse_limits() -> tos_image::ParseLimits {
    let limits = Limits::default();
    tos_image::ParseLimits {
        table_entries: limits.table_entries,
        modules: limits.modules,
        fields: limits.fields,
        parameters: limits.parameters,
        blocks_per_function: limits.blocks_per_function,
        instructions_per_block: limits.instructions_per_block,
        source_map_entries: limits.source_map_entries,
    }
}

const ENVELOPE: &str = "resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, \
     workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0]";

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}

fn lowered(body: &str) -> Module {
    let text = format!("module app.image version 1.0 profile bootstrap; {ENVELOPE} {body}");
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let diagnostics = Checker::check(&source, &schema);
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == tos_core::Severity::Error),
        "the fixture checks clean: {:?}",
        diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
    let context = ModuleContext {
        source_set: "tos-tests-integration".to_string(),
        path: "app/image.tos".to_string(),
        content_id: content_id(source.bytes()),
        dependency_digest: content_id(b""),
        capability_interface_digest: content_id(b""),
    };
    lower_module(&source, &schema, &context).expect("the fixture lowers")
}

/// A corpus of modules the frontend produced, each of which must survive the
/// image exactly and verify afterwards.
fn corpus() -> Vec<Module> {
    [
        "pub fn total(a: i32, b: i32) -> i32 { return a + b; }",
        "pub record Point [x: i32, y: i32] \
         pub fn sum(p: Point) -> i32 { return p.x + p.y; } \
         pub fn make(x: i32) -> i32 { let p: Point = Point(x: x, y: x + 1i32); return sum(p); }",
        "pub fn compare(a: i32, b: i32) -> bool { return a <= b && a != 0i32; }",
        "pub fn branchy(n: i32) -> i32 { if (n > 0i32) { return n; } else { return 0i32 - n; } }",
        "pub fn looping(n: i32) -> i32 { \
             let mut total: i32 = 0i32; \
             let mut at: i32 = 0i32; \
             while (at < n) { total = total + at; at = at + 1i32; } \
             return total; }",
        "pub fn mixed(a: i32, b: i32) -> i32 { \
             return (a * b) / (b + 1i32) % 7i32 + (a & b) | (a ^ b); }",
        "pub fn constants() -> i32 { return 7i32; }",
    ]
    .into_iter()
    .map(lowered)
    .collect()
}

#[test]
fn every_module_of_the_corpus_survives_the_image_exactly() {
    for module in corpus() {
        let (image, _) = encode(&module);
        let parsed = parse(&image, &parse_limits()).expect("its own image parses");
        assert_eq!(parsed, module, "{}", module.header.module_name);
        assert_eq!(
            tos_ir::module_digest(&parsed),
            tos_ir::module_digest(&module),
            "the semantic digest is unchanged"
        );
    }
}

/// The chain the ADR fixes: source, lower, untrusted encoding, verifier,
/// receipt. The receipt must bind to the digest the verifier computed from the
/// module it reconstructed, and to the same one the frontend's module has.
#[test]
fn encode_parse_verify_agrees_with_verifying_the_module_directly() {
    let limits = Limits::default();
    let snapshot = ResolutionSnapshot::default();
    for module in corpus() {
        let direct = verify(&module, &snapshot, &limits).expect("the fixture verifies");
        let (image, _) = encode(&module);
        // The one production entry: bytes in, receipt and artifact digest out.
        let verified = verify_image(&image, &snapshot, &limits).expect("the image verifies");
        assert_eq!(
            verified.receipt(),
            &direct,
            "the receipts are the same receipt"
        );
        assert_eq!(verified.module(), &module, "the module is reconstructed");
        assert_eq!(
            verified.artifact_digest(),
            &tos_hash::sha256(&image),
            "the digest is over the bytes that were parsed"
        );
        assert_eq!(
            verified.artifact_digest_text(),
            artifact_digest(&image),
            "and it is the same digest the format reports"
        );
    }
}

/// A well-formed image of a module that does not hold is refused by the
/// verifier, not by the parser — the two stages stay distinguishable.
#[test]
fn a_semantically_invalid_image_of_a_real_module_is_refused_by_the_verifier() {
    let mut module = corpus().remove(0);
    module.functions[0].blocks[0].terminator = tos_ir::Terminator::Branch {
        target: 9999,
        arguments: Vec::new(),
    };
    let (image, _) = encode(&module);
    match verify_image(&image, &ResolutionSnapshot::default(), &Limits::default()) {
        Err(ImageRefusal::Verifier(finding)) => assert!(finding.code.starts_with('V')),
        other => panic!("an invalid module was not a verifier refusal: {other:?}"),
    }
}

/// Reproducible bytes across the corpus, and a deletable cache.
#[test]
fn the_corpus_encodes_reproducibly() {
    for module in corpus() {
        let (first, _) = encode(&module);
        let (second, _) = encode(&module);
        assert_eq!(first, second);
        let parsed = parse(&first, &parse_limits()).expect("parses");
        let (again, _) = encode(&parsed);
        assert_eq!(first, again, "re-encoding a parsed module is a fixed point");
    }
}

/// Every prefix of a real image is refused, and resealed mutations return.
#[test]
fn the_parser_is_total_over_real_images() {
    let limits = parse_limits();
    for module in corpus() {
        let (image, _) = encode(&module);
        for length in 0..image.len() {
            assert!(
                parse(&image[..length], &limits).is_err(),
                "a proper prefix must not parse"
            );
        }
        let mut state = 0x9e3779b97f4a7c15u64;
        for _ in 0..512 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let span = image.len() - tos_image::FRAME_HEADER - tos_image::DIGEST_BYTES;
            let at = tos_image::FRAME_HEADER + (state >> 11) as usize % span;
            let mut bad = image.clone();
            bad[at] ^= ((state >> 3) & 0xff) as u8;
            reseal(&mut bad);
            // Whatever it returns, it returns.
            let _ = parse(&bad, &limits);
        }
    }
}

/// A module that arrives under a wrong version, wrong magic or wrong digest is
/// refused before any of it is believed.
#[test]
fn a_real_image_still_refuses_the_frame_negatives() {
    let limits = parse_limits();
    let module = &corpus()[1];
    let (good, _) = encode(module);

    let mut bad = good.clone();
    bad[0] ^= 0xff;
    assert_eq!(parse(&bad, &limits), Err(ImageError::BadMagic));

    // A version this reader does not know. It is written as a number rather
    // than as `ENCODING_VERSION + 1` so that raising the current version makes
    // this test fail loudly rather than silently testing the version in use.
    let mut bad = good.clone();
    bad[11] = 9;
    assert_eq!(
        parse(&bad, &limits),
        Err(ImageError::UnknownEncodingVersion(9))
    );

    let mut bad = good.clone();
    bad[15] = 2;
    assert_eq!(
        parse(&bad, &limits),
        Err(ImageError::UnknownSchemaVersion(2))
    );

    let mut bad = good.clone();
    let at = tos_image::FRAME_HEADER + 4;
    bad[at] ^= 0x01;
    assert_eq!(parse(&bad, &limits), Err(ImageError::WrongDigest));
}
