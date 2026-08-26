// SPDX-License-Identifier: GPL-3.0-or-later
//! What a compact verified module image costs, measured (ADR-0070 section 6).
//!
//! **Measurement only.** Nothing here is switched into the production engine,
//! and the container this writes is an experimental `v0` the engine never
//! executes. ADR-0070 is Proposed and claims no number; this is where the
//! numbers it needs come from.
//!
//! Three modes, each in its own process, because the arena's frontier never
//! falls: a measurement that followed another would inherit its high-water mark
//! and report it as its own.
//!
//! - `--encode PATH` lowers the ceiling fixture through the production frontend,
//!   measures the live `tos_ir::Module`, the current canonical stream and the
//!   image, and writes the image to `PATH`.
//! - `--verify PATH` reads those bytes as untrusted input, parses them with the
//!   verifier-owned parser, runs the existing semantic verifier over what came
//!   out, and checks the one invariant that makes the rest meaningful: the
//!   semantic module digest of the parsed module equals the digest of the module
//!   that was encoded.
//! - `--negatives PATH` puts malformed, truncated, oversized, non-canonical,
//!   unknown-version, unknown-tag and wrong-digest inputs through the same
//!   parser, and sweeps single-byte mutations that are *resealed* with a valid
//!   digest — the hostile case, where the attacker controls the bytes and the
//!   digest both, and only the payload parser stands between them and a module.

mod image;

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use tos_core::{lower_module_in_set, ModuleContext, Parser, SourceReader};
use tos_ir::Module;
use tos_runtime::{GlobalHeap, RuntimeMemoryGrant, GRANT_VERSION};
use tos_verifier::{Limits, ResolutionSnapshot};

use image::ImageError;

/// The region every measurement runs in, the shape a nucleus grant has.
const ARENA_BYTES: usize = 1024 * 1024 * 1024;

/// The published ceiling for one normalized source unit (docs/44 section 2).
const SOURCE_CEILING: usize = 256 * 1024;

static ADOPTED: AtomicBool = AtomicBool::new(false);

struct MeasuredHeap {
    heap: GlobalHeap,
}

impl MeasuredHeap {
    fn ensure_adopted(&self) {
        if ADOPTED.swap(true, Ordering::SeqCst) {
            return;
        }
        let layout = Layout::from_size_align(ARENA_BYTES, 4096).expect("a valid region layout");
        // SAFETY: `System` is the host allocator and is unaffected by the
        // global allocator installed below it; the layout is non-zero-sized.
        let base = unsafe { std::alloc::System.alloc(layout) };
        assert!(
            !base.is_null(),
            "the measurement needs a {ARENA_BYTES}-byte region"
        );
        let grant = RuntimeMemoryGrant {
            version: GRANT_VERSION,
            base: base as usize,
            length: ARENA_BYTES,
            alignment: 4096,
            identity: 0,
        };
        // SAFETY: the region is owned by this program alone for its lifetime.
        unsafe { self.heap.adopt(&grant) }.expect("the static region is a well-formed grant");
    }
}

// SAFETY: the heap upholds the `GlobalAlloc` contract; this only adds a
// one-time adoption in front of it.
unsafe impl GlobalAlloc for MeasuredHeap {
    // SAFETY: the `GlobalAlloc` contract; this only puts a one-time adoption in front of the heap, which upholds it.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.ensure_adopted();
        // SAFETY: the heap has adopted its region.
        unsafe { self.heap.alloc(layout) }
    }

    // SAFETY: the `GlobalAlloc` contract; `pointer` was returned by `alloc` on this allocator.
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: `pointer` came from `alloc` on this allocator.
        unsafe { self.heap.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static HEAP: MeasuredHeap = MeasuredHeap {
    heap: GlobalHeap::new(),
};

fn committed() -> usize {
    HEAP.heap.usage().0
}

fn frontier() -> usize {
    HEAP.heap.usage().1
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let mode = arguments.get(1).map(String::as_str).unwrap_or("--encode");
    let path = arguments
        .get(2)
        .cloned()
        .unwrap_or_else(|| "target/ceiling.tosimg0".to_string());
    println!("TOS compact verified module image — prototype, measurement only");
    println!(
        "container: {} version {} — experimental, never executed by the engine",
        std::str::from_utf8(&image::MAGIC).expect("the magic is ASCII"),
        image::ENCODING_VERSION
    );
    println!("allocator: tos_runtime::BoundedHeap over a {ARENA_BYTES}-byte region");
    println!();
    match mode {
        "--encode" => encode_mode(&path),
        "--verify" => verify_mode(&path),
        "--negatives" => negatives_mode(&path),
        other => {
            eprintln!("unknown mode: {other}");
            eprintln!("usage: tos-image-prototype [--encode|--verify|--negatives] PATH");
            std::process::exit(2);
        }
    }
}

/// The fixture: one dependency module at the published source ceiling.
///
/// The same generator the arena bound uses, so the live-module figure here is
/// comparable to the one `STAGE3_PROCESS_GRANT.md` already published rather
/// than being a new fixture nobody can line up against it.
fn ceiling_fixture() -> String {
    let index = 1usize;
    let mut text = format!(
        "module set.m{index} version 1.0 profile bootstrap; \
         resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
         pub record Point{index} [x: i32, y: i32] \
         pub fn total{index}(point: Point{index}) -> i32 {{ return point.x + point.y; }} "
    );
    let mut filler = 0usize;
    loop {
        let chunk = format!(
            "pub record Filler{index}_{filler} [x: i32, y: i32] \
             pub fn fill{index}_{filler}(point: Filler{index}_{filler}) -> i32 \
             {{ return point.x + point.y; }} "
        );
        if text.len() + chunk.len() > SOURCE_CEILING {
            break;
        }
        text.push_str(&chunk);
        filler += 1;
    }
    text.push_str(&format!(
        "pub fn value{index}() -> i32 {{ return {index}i32; }} "
    ));
    text
}

/// Lowers the fixture and reports what the live module costs.
fn lower_fixture(text: &str) -> (Module, usize) {
    let source = SourceReader::read(text.as_bytes()).expect("the fixture is transport-valid");
    let context = ModuleContext {
        source_set: "tos-image-prototype".to_string(),
        path: "set/m1.tos".to_string(),
        content_id: tos_pipeline::content_id(source.bytes()),
        dependency_digest: tos_pipeline::list_digest(&[]),
        capability_interface_digest: tos_pipeline::list_digest(&[]),
    };
    let before = committed();
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let module = lower_module_in_set(&source, &schema, &context, &[]).expect("the fixture lowers");
    drop(schema);
    let live = committed().saturating_sub(before);
    (module, live)
}

fn encode_mode(path: &str) {
    let text = ceiling_fixture();
    println!("== the fixture ==");
    println!(
        "one module at the published source ceiling: {} B",
        text.len()
    );

    let (module, live) = lower_fixture(&text);
    println!(
        "live tos_ir::Module: {live} B ({:.2} MiB), measured through the bounded heap",
        mib(live)
    );

    // The coverage the payload actually needed, counted rather than asserted.
    let coverage = image::coverage(&module);
    println!();
    println!("== semantic surface the fixture exercises ==");
    for (name, count) in &coverage {
        let supported = image::SUPPORTED.contains(name);
        println!(
            "  {name:<32} {count:>9}  {}",
            if supported { "encoded" } else { "REFUSED" }
        );
    }
    println!(
        "  the encoder implements {} tagged variants and refuses {}",
        image::SUPPORTED.len(),
        image::UNSUPPORTED.len()
    );
    println!("  refused, and required of a production encoder:");
    for name in image::UNSUPPORTED {
        println!("    {name}");
    }

    let started = Instant::now();
    let stream = tos_ir::canonical_stream(&module);
    let stream_time = started.elapsed();
    let stream_bytes = stream.len();
    drop(stream);

    let started = Instant::now();
    let (bytes, layout) = match image::encode(&module) {
        Ok(encoded) => encoded,
        Err(error) => {
            eprintln!("the encoder refused the fixture: {error:?}");
            std::process::exit(1);
        }
    };
    let encode_time = started.elapsed();

    let digest = tos_ir::module_digest(&module);

    println!();
    println!("== the image ==");
    println!(
        "image {} B ({:.2} MiB); payload {} B; frame {} B",
        layout.image,
        mib(layout.image),
        layout.payload,
        layout.image - layout.payload
    );
    println!(
        "against the live Module   {} B / {} B = {:.2}x smaller",
        live,
        layout.image,
        live as f64 / layout.image as f64
    );
    println!(
        "against the canonical stream {} B / {} B = {:.2}x smaller",
        stream_bytes,
        layout.image,
        stream_bytes as f64 / layout.image as f64
    );

    println!();
    println!("== where the bytes are ==");
    let sections: [(&str, usize); 9] = [
        ("string table", layout.strings),
        ("header", layout.header),
        ("types", layout.types),
        ("imports", layout.imports),
        ("capability imports", layout.capability_imports),
        ("exports", layout.exports),
        ("constants", layout.constants),
        ("functions", layout.functions),
        (
            "source map",
            layout.source_map_identities + layout.source_map_entries,
        ),
    ];
    for (name, size) in sections {
        println!(
            "  {name:<20} {size:>10} B  ({:>5.1}% of payload)",
            100.0 * size as f64 / layout.payload as f64
        );
    }
    println!("  strings interned: {}", layout.string_count);

    println!();
    println!("== the source-map identity, interned ==");
    println!(
        "  {} entries over {} distinct identities",
        module.source_map.len(),
        layout.identity_count
    );
    println!(
        "  identity table {} B + entries {} B = {} B",
        layout.source_map_identities,
        layout.source_map_entries,
        layout.source_map_identities + layout.source_map_entries
    );
    println!(
        "  the same entries with identity written per entry: {} B",
        layout.source_map_inline_equivalent
    );
    let interned = layout.source_map_identities + layout.source_map_entries;
    println!(
        "  interning saves {} B ({:.2} MiB), {:.1}x on the section",
        layout.source_map_inline_equivalent - interned,
        mib(layout.source_map_inline_equivalent - interned),
        layout.source_map_inline_equivalent as f64 / interned as f64
    );

    println!();
    println!("== time ==");
    println!(
        "  canonical stream build  {:>9.2} ms",
        stream_time.as_secs_f64() * 1000.0
    );
    println!(
        "  encode                  {:>9.2} ms",
        encode_time.as_secs_f64() * 1000.0
    );

    println!();
    println!("semantic module digest: {digest}");
    println!("artifact digest:        {}", image::artifact_digest(&bytes));

    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, &bytes).expect("the image is writable");
    println!("written: {path}");
}

fn verify_mode(path: &str) {
    let limits = Limits::default();
    let bytes = std::fs::read(path).expect("the image exists — run --encode first");
    let after_read = frontier();
    println!("== the input ==");
    println!(
        "image {} B ({:.2} MiB), read as untrusted bytes",
        bytes.len(),
        mib(bytes.len())
    );
    println!("artifact digest: {}", image::artifact_digest(&bytes));
    println!(
        "arena after the bytes are in memory: committed {} B, frontier {} B ({:.2} MiB)",
        committed(),
        after_read,
        mib(after_read)
    );

    let started = Instant::now();
    let module = match image::parse(&bytes, &limits) {
        Ok(module) => module,
        Err(error) => {
            eprintln!("the parser refused a well-formed image: {error:?}");
            std::process::exit(1);
        }
    };
    let decode_time = started.elapsed();
    let after_parse = frontier();
    let materialized = committed();
    println!();
    println!("== the verifier-owned parse ==");
    println!("  decode {:>9.2} ms", decode_time.as_secs_f64() * 1000.0);
    println!(
        "  materialized Module: committed {} B ({:.2} MiB)",
        materialized,
        mib(materialized)
    );
    println!(
        "  frontier after parse: {} B ({:.2} MiB)",
        after_parse,
        mib(after_parse)
    );

    let snapshot = ResolutionSnapshot::default();
    let started = Instant::now();
    let receipt = tos_verifier::verify(&module, &snapshot, &limits);
    let verify_time = started.elapsed();
    let peak = frontier();
    let receipt = match receipt {
        Ok(receipt) => receipt,
        Err(finding) => {
            eprintln!("the verifier refused the parsed module: {finding:?}");
            std::process::exit(1);
        }
    };
    println!();
    println!("== the semantic verifier ==");
    println!("  verify {:>9.2} ms", verify_time.as_secs_f64() * 1000.0);
    println!("  receipt digest: {}", receipt.module_digest);
    println!(
        "  peak arena over read + parse + verify: {} B ({:.2} MiB)",
        peak,
        mib(peak)
    );

    // The invariant the whole measurement rests on. An image that decoded to a
    // *different* module would make every byte figure above meaningless, and a
    // receipt bound to a digest the source never produced would be a cache
    // pretending to be a verifier.
    let round_trip = tos_ir::module_digest(&module);
    println!();
    println!("== the invariant ==");
    println!("  semantic digest after encode -> parse: {round_trip}");
    assert_eq!(
        round_trip, receipt.module_digest,
        "the receipt must bind to the digest of the module the verifier saw"
    );
    println!("  the receipt binds to the module the verifier actually traversed");
    println!();
    println!("PASS: parsed image verifies, digest recorded above");
}

/// Every input the parser must refuse, and the sweep that proves it is total.
fn negatives_mode(path: &str) {
    let limits = Limits::default();
    let good = std::fs::read(path).expect("the image exists — run --encode first");
    assert!(
        image::parse(&good, &limits).is_ok(),
        "the unmodified image must parse, or the negatives prove nothing"
    );

    let mut failures = 0usize;
    let mut cases = 0usize;
    let mut check = |what: &str, bytes: Vec<u8>, expected: fn(&ImageError) -> bool| {
        cases += 1;
        match image::parse(&bytes, &limits) {
            Ok(_) => {
                failures += 1;
                println!("  {what:<34} ACCEPTED — must not be");
            }
            Err(error) => {
                let right = expected(&error);
                if !right {
                    failures += 1;
                }
                println!(
                    "  {what:<34} refused: {error:?}{}",
                    if right { "" } else { "  — WRONG REASON" }
                );
            }
        }
    };

    println!("== the frame ==");

    let mut bad = good.clone();
    bad[0] ^= 0xff;
    check("malformed: wrong magic", bad, |error| {
        matches!(error, ImageError::BadMagic)
    });

    let mut bad = good.clone();
    bad[11] = 1;
    check("unknown encoding version", bad, |error| {
        matches!(error, ImageError::UnknownVersion(1))
    });

    check(
        "truncated: shorter than a frame",
        good[..12].to_vec(),
        |error| matches!(error, ImageError::Truncated("frame")),
    );

    check(
        "truncated: body cut short",
        good[..good.len() - 64].to_vec(),
        |error| matches!(error, ImageError::Truncated(_)),
    );

    let mut bad = good.clone();
    bad[12..20].copy_from_slice(&(u64::MAX).to_be_bytes());
    check("oversized: declared length huge", bad, |error| {
        matches!(error, ImageError::Oversized { .. })
    });

    let mut bad = good.clone();
    let inflated = (image::MAX_IMAGE_BYTES as u64) - 1;
    bad[12..20].copy_from_slice(&inflated.to_be_bytes());
    check("oversized: length past the bytes", bad, |error| {
        matches!(error, ImageError::Truncated("payload"))
    });

    let mut bad = good.clone();
    bad.push(0);
    check("trailing bytes after the digest", bad, |error| {
        matches!(error, ImageError::TrailingBytes(_))
    });

    let mut bad = good.clone();
    let at = image::FRAME_HEADER + 8;
    bad[at] ^= 0x01;
    check("wrong digest: payload altered", bad, |error| {
        matches!(error, ImageError::WrongDigest)
    });

    let mut bad = good.clone();
    let last = bad.len() - 1;
    bad[last] ^= 0x01;
    check("wrong digest: digest altered", bad, |error| {
        matches!(error, ImageError::WrongDigest)
    });

    println!();
    println!("== the payload, sealed with a valid digest ==");

    // A canonical string table sorted and free of duplicates: two identical
    // entries are refused rather than tolerated.
    check(
        "non-canonical: repeated string",
        image::frame(&[0x02, 0x01, b'a', 0x01, b'a']),
        |error| matches!(error, ImageError::NonCanonicalTable("string table")),
    );

    check(
        "non-canonical: unsorted strings",
        image::frame(&[0x02, 0x01, b'b', 0x01, b'a']),
        |error| matches!(error, ImageError::NonCanonicalTable("string table")),
    );

    // 0x80 0x00 is zero written in two bytes: a second spelling of a value that
    // already has one.
    check(
        "non-canonical varint",
        image::frame(&[0x80, 0x00]),
        |error| matches!(error, ImageError::NonCanonicalVarint),
    );

    check("varint past 128 bits", image::frame(&[0xff; 24]), |error| {
        matches!(error, ImageError::VarintOverflow)
    });

    // A forged count, before any allocation is sized from it.
    check(
        "count past the declared limit",
        image::frame(&[0xff, 0xff, 0xff, 0xff, 0x0f]),
        |error| matches!(error, ImageError::CountExceedsLimit { .. }),
    );

    check(
        "count past the bytes that remain",
        image::frame(&[0x40]),
        |error| matches!(error, ImageError::Truncated("string table")),
    );

    check("truncated: empty payload", image::frame(&[]), |error| {
        matches!(error, ImageError::Truncated("varint"))
    });

    check(
        "string reference out of range",
        image::frame(&[0x00, 0x00]),
        |error| {
            matches!(
                error,
                ImageError::OutOfRange {
                    what: "string table"
                }
            )
        },
    );

    check(
        "string that is not UTF-8",
        image::frame(&[0x01, 0x01, 0xff]),
        |error| matches!(error, ImageError::BadUtf8),
    );

    // An unknown semantic tag fails closed. This is the rule that keeps the
    // prototype's partial payload coverage safe: a variant it does not
    // implement is refused, never skipped.
    let unknown_tag = unknown_type_tag(&good, &limits);
    check("unknown TypeDef tag", unknown_tag, |error| {
        matches!(
            error,
            ImageError::UnknownTag {
                family: "TypeDef",
                ..
            }
        )
    });

    println!();
    println!("== totality: resealed single-byte mutations ==");
    // The hostile case. The attacker controls the bytes, so the attacker
    // controls the digest: what stands between arbitrary input and a module
    // value is the payload parser alone. Every one of these must return, and
    // the process reaching the end of this loop is the assertion — the crate
    // builds with `panic = "abort"`, so a panic is a dead process, not a
    // caught error.
    let mut refused = 0usize;
    let mut accepted = 0usize;
    let mut state = 0x243f_6a88_85a3_08d3u64;
    let sweep = 4096;
    for _ in 0..sweep {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let at =
            image::FRAME_HEADER + (state >> 11) as usize % (good.len() - image::FRAME_HEADER - 32);
        let mut bad = good.clone();
        bad[at] ^= ((state >> 3) & 0xff) as u8;
        image::reseal(&mut bad);
        match image::parse(&bad, &limits) {
            Ok(_) => accepted += 1,
            Err(_) => refused += 1,
        }
    }
    println!("  {sweep} mutations: {refused} refused, {accepted} parsed to some module, 0 panics");
    println!("  (a mutation may leave a well-formed image of a different module; what is");
    println!("   being proved here is that the parser is total, not that every change is caught)");

    println!();
    println!("== totality: every prefix ==");
    let mut prefix_refused = 0usize;
    let step = (good.len() / 2048).max(1);
    let mut prefixes = 0usize;
    for length in (0..good.len()).step_by(step) {
        prefixes += 1;
        if image::parse(&good[..length], &limits).is_err() {
            prefix_refused += 1;
        }
    }
    println!("  {prefixes} prefixes sampled: {prefix_refused} refused, 0 panics");
    assert_eq!(
        prefix_refused, prefixes,
        "no proper prefix of an image may parse"
    );

    println!();
    if failures == 0 {
        println!("PASS: {cases} negative cases, each refused for the stated reason");
    } else {
        println!("FAIL: {failures} of {cases} negative cases did not behave");
        std::process::exit(1);
    }
}

/// The same image with one type tag replaced by a tag no reader knows.
///
/// Built by re-encoding rather than by patching a byte at a guessed offset: a
/// negative test that silently stopped hitting the case it names would be worse
/// than no test.
fn unknown_type_tag(good: &[u8], limits: &Limits) -> Vec<u8> {
    let module = image::parse(good, limits).expect("the good image parses");
    let (bytes, layout) = image::encode(&module).expect("the module re-encodes");
    let mut bad = bytes[image::FRAME_HEADER..image::FRAME_HEADER + layout.payload].to_vec();
    // The types section opens with its count; the first type's tag follows it.
    let mut at = layout.strings + layout.header;
    while bad[at] & 0x80 != 0 {
        at += 1;
    }
    at += 1;
    bad[at] = 0xfe;
    image::frame(&bad)
}
