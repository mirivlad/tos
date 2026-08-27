// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic mutation fuzz targets for the binary and textual parsers.
//!
//! Property under test: each parser is total and bounded — for ANY byte string
//! it must return a result, never panic, and never loop. The capsule parser
//! must additionally never accept a mutated or garbage input. The TOS Core
//! frontend must additionally always terminate its recovery loop and produce a
//! result that is either a clean tree or at least one diagnostic, never both
//! empty. The generator is a fixed-seed PRNG, so runs are reproducible.
//! Exits 0 on pass, 1 on failure.

use tos_capsule::parse;
use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_image::{encode, reseal};
use tos_ir::Module;
use tos_verifier::{verify, Limits, ResolutionSnapshot};

const BASE: &[u8] = include_bytes!("../../vectors/capsule-v1/valid-001.bin");

/// xorshift64* PRNG, fixed seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
}

fn main() {
    let mut rng = Rng(0x544f_5300_0000_0001); // "TOS..." seed
    let rounds: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(50_000);

    let mut accepted = 0usize;
    for _ in 0..rounds {
        let len = (rng.next() as usize) % (BASE.len() * 2 + 1);
        let mut buf = vec![0u8; len];
        // 50%: seed from BASE (mutation), 50%: pure garbage.
        if len > 0 && len <= BASE.len() && rng.next() & 1 == 0 {
            buf.copy_from_slice(&BASE[..len]);
            let flips = 1 + (rng.next() % 8) as usize;
            for _ in 0..flips {
                let at = (rng.next() as usize) % len.max(1);
                buf[at] = (rng.next() & 0xff) as u8;
            }
        } else {
            for b in buf.iter_mut() {
                *b = (rng.next() & 0xff) as u8;
            }
        }

        match parse(&buf) {
            Ok(_) => accepted += 1,
            Err(_) => {}
        }
    }

    fuzz_tos_core(&mut rng, rounds);
    fuzz_forged_ir(&mut rng, rounds);
    fuzz_module_image(&mut rng, rounds);

    // A mutated input must never parse. The only Ok case allowed would be a
    // zero-flip mutation, which cannot occur (flips >= 1), and pure garbage is
    // never valid. Any Ok here means the property failed.
    if accepted != 0 {
        eprintln!("FUZZ FAIL: {accepted} mutated/garbage inputs accepted");
        std::process::exit(1);
    }
    println!("FUZZ PASS rounds={rounds} (total parsers and verifier, no panics, no false accepts)");
}

/// Canonical source the textual mutation rounds start from.
const TOS_BASE: &[u8] =
    include_bytes!("../../../../docs/language/conformance/v1/accept/control-heads.tos");

/// Drives the TOS Core source reader and parser over mutated and random bytes.
///
/// AGENTS.md section 8 requires a parser to be total over arbitrary bytes and
/// to report structured errors rather than panic. Recovery makes termination a
/// property worth asserting directly: every synchronization step must consume a
/// token or reach end of source, so no input may leave the parser looping.
fn fuzz_tos_core(rng: &mut Rng, rounds: usize) {
    for _ in 0..rounds {
        let len = (rng.next() as usize) % (TOS_BASE.len() * 2 + 1);
        let mut buf = vec![0u8; len];
        if len > 0 && len <= TOS_BASE.len() && rng.next() & 1 == 0 {
            buf.copy_from_slice(&TOS_BASE[..len]);
            let flips = 1 + (rng.next() % 8) as usize;
            for _ in 0..flips {
                let at = (rng.next() as usize) % len.max(1);
                buf[at] = (rng.next() & 0xff) as u8;
            }
        } else {
            for b in buf.iter_mut() {
                *b = (rng.next() & 0xff) as u8;
            }
        }

        let Ok(source) = SourceReader::read(&buf) else {
            continue;
        };
        let outcome = Parser::parse_schema(&source);
        if outcome.has_errors() {
            if outcome.diagnostics().is_empty() {
                eprintln!("FUZZ FAIL: error outcome with no diagnostic");
                std::process::exit(1);
            }
            continue;
        }
        if outcome.into_accepted().is_none() {
            eprintln!("FUZZ FAIL: clean outcome produced no tree");
            std::process::exit(1);
        }
    }
}

/// Drives the module-image parser over mutated and random bytes.
///
/// The parser sits in the verifier path and reads untrusted input, so the same
/// property applies as to every other parser here: total over arbitrary bytes,
/// bounded in what it allocates, structured errors rather than panics.
///
/// Two kinds of input, and the second is the one that matters. Bytes that are
/// merely corrupted fail the artifact digest and never reach the payload
/// reader. Bytes that are corrupted **and resealed** do reach it — which is the
/// real case, because an attacker who can write the bytes can write the digest.
fn fuzz_module_image(rng: &mut Rng, rounds: usize) {
    let module = image_base();
    let (base, _) = encode(&module);
    let limits = Limits::default();
    let mut accepted = 0usize;
    for _ in 0..rounds {
        let len = (rng.next() as usize) % (base.len() + 1);
        let mut buf = vec![0u8; len];
        let seeded = len > 0 && len <= base.len() && rng.next() & 1 == 0;
        if seeded {
            buf.copy_from_slice(&base[..len]);
            let flips = 1 + (rng.next() % 8) as usize;
            for _ in 0..flips {
                let at = (rng.next() as usize) % len.max(1);
                buf[at] = (rng.next() & 0xff) as u8;
            }
            // Half of the seeded rounds are resealed, so the payload reader is
            // reached rather than short-circuited by the digest.
            if rng.next() & 1 == 0 {
                reseal(&mut buf);
            }
        } else {
            for b in buf.iter_mut() {
                *b = (rng.next() & 0xff) as u8;
            }
        }
        if tos_image::parse(&buf, &limits).is_ok() {
            accepted += 1;
        }
    }
    // Accepting is allowed: a resealed mutation can be a well-formed image of a
    // different module, and deciding whether that module is admissible is the
    // verifier's job. What is being asserted is that every round returned.
    println!("  image fuzz: {rounds} rounds, {accepted} parsed to some module, no panics");
}

/// A real lowered module for the image rounds to start from.
fn image_base() -> Module {
    let source = SourceReader::read(IR_BASE.as_bytes()).expect("the base parses");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the base parses");
    let context = ModuleContext {
        source_set: String::from("tos-tests-fuzz"),
        path: String::from("app/fuzz.tos"),
        content_id: String::from("sha256:00"),
        dependency_digest: String::from("sha256:00"),
        capability_interface_digest: String::from("sha256:00"),
    };
    lower_module(&source, &schema, &context).expect("the base lowers")
}

/// Source the forged-IR rounds start from, so mutations begin from real IR.
const IR_BASE: &str = "module app.fuzz version 1.0 profile bootstrap; \
     resource [fuel: 1000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
     sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] \
     pub record Point [x: i32, y: i32] \
     pub fn origin() -> Point { return Point(x: 0i32, y: 0i32); } \
     pub fn total(point: Point) -> i32 { return point.x + point.y; }";

/// Drives the independent verifier over structurally forged IR.
///
/// docs/44 section 3 requires evidence that malformed or forged IR never
/// panics. Bytes cannot express that here — docs/43 section 1 freezes no
/// encoding — so the mutations are structural: indices are moved out of range,
/// identities are replaced, tables are reordered and instructions are dropped,
/// which is exactly what a forged in-memory object looks like.
///
/// The property is not "everything is rejected": some mutations produce IR that
/// is still valid. The property is that the verifier always *answers* — a
/// receipt or one deterministic finding — and that a receipt it grants names
/// the digest of the module it actually saw.
fn fuzz_forged_ir(rng: &mut Rng, rounds: usize) {
    let Some(base) = build_base_module() else {
        eprintln!("FUZZ FAIL: the forged-IR base module did not build");
        std::process::exit(1);
    };
    let snapshot = ResolutionSnapshot::default();
    let limits = Limits::default();
    // The unmutated module must verify, or every later round measures nothing.
    if verify(&base, &snapshot, &limits).is_err() {
        eprintln!("FUZZ FAIL: the forged-IR base module does not verify");
        std::process::exit(1);
    }

    for _ in 0..rounds {
        let mut module = base.clone();
        let mutations = 1 + (rng.next() % 4) as usize;
        for _ in 0..mutations {
            forge(rng, &mut module);
        }
        match verify(&module, &snapshot, &limits) {
            Ok(receipt) => {
                // A receipt is only ever for the module the verifier saw.
                if receipt.module_digest != tos_ir::module_digest(&module) {
                    eprintln!("FUZZ FAIL: a receipt names a different module");
                    std::process::exit(1);
                }
            }
            Err(finding) => {
                if finding.code.is_empty() {
                    eprintln!("FUZZ FAIL: a finding carries no code");
                    std::process::exit(1);
                }
            }
        }
    }
}

fn build_base_module() -> Option<Module> {
    let source = SourceReader::read(IR_BASE.as_bytes()).ok()?;
    let schema = Parser::parse_schema(&source).into_accepted()?;
    if !Checker::check(&source, &schema).is_empty() {
        return None;
    }
    let context = ModuleContext {
        source_set: String::from("tos-fuzz"),
        path: String::from("app/fuzz.tos"),
        content_id: String::from("sha256:0000"),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    lower_module(&source, &schema, &context).ok()
}

/// Applies one structural forgery to a module.
fn forge(rng: &mut Rng, module: &mut Module) {
    let choice = rng.next() % 12;
    let wild = |rng: &mut Rng, bound: usize| (rng.next() as usize) % (bound * 4 + 7);
    match choice {
        0 => module.header.schema_id = String::from("tos-ir/forged"),
        1 => module.header.content_id = String::from("forged"),
        2 => module.header.path = String::from("elsewhere/other.tos"),
        3 => module.header.resource_envelope.workers = 1 + u128::from(rng.next() % 8),
        4 => module.exports.reverse(),
        5 => module.functions.reverse(),
        6 => {
            let bound = module.types.len();
            module.types.push(tos_ir::TypeDef::Option(wild(rng, bound)));
        }
        7 => {
            if let Some(entry) = module.source_map.first_mut() {
                entry.content_id = String::from("forged");
            }
        }
        8 => {
            let count = module.functions.len();
            if count > 0 {
                let at = (rng.next() as usize) % count;
                let blocks = module.functions[at].blocks.len();
                if blocks > 0 {
                    let block = (rng.next() as usize) % blocks;
                    let target = wild(rng, blocks);
                    module.functions[at].blocks[block].terminator = tos_ir::Terminator::Branch {
                        target,
                        arguments: Vec::new(),
                    };
                }
            }
        }
        9 => {
            let count = module.functions.len();
            if count > 0 {
                let at = (rng.next() as usize) % count;
                let values = module.functions[at].values.len();
                let blocks = module.functions[at].blocks.len();
                if blocks > 0 {
                    let block = (rng.next() as usize) % blocks;
                    let operand = tos_ir::Operand::Value(wild(rng, values));
                    module.functions[at].blocks[block].terminator =
                        tos_ir::Terminator::Return(Some(operand));
                }
            }
        }
        10 => {
            let count = module.functions.len();
            if count > 0 {
                let at = (rng.next() as usize) % count;
                let blocks = module.functions[at].blocks.len();
                if blocks > 0 {
                    let block = (rng.next() as usize) % blocks;
                    module.functions[at].blocks[block].instructions.pop();
                }
            }
        }
        _ => {
            let count = module.functions.len();
            if count > 0 {
                let at = (rng.next() as usize) % count;
                let blocks = module.functions[at].blocks.len();
                if blocks > 0 {
                    let block = (rng.next() as usize) % blocks;
                    if let Some(instruction) =
                        module.functions[at].blocks[block].instructions.first_mut()
                    {
                        instruction.source = wild(rng, 4);
                        instruction.unsafe_interface = Some(String::from("host.forged/v1"));
                    }
                }
            }
        }
    }
}
