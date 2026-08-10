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
use tos_core::{Parser, SourceReader};

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

    // A mutated input must never parse. The only Ok case allowed would be a
    // zero-flip mutation, which cannot occur (flips >= 1), and pure garbage is
    // never valid. Any Ok here means the property failed.
    if accepted != 0 {
        eprintln!("FUZZ FAIL: {accepted} mutated/garbage inputs accepted");
        std::process::exit(1);
    }
    println!("FUZZ PASS rounds={rounds} (total parsers, no panics, no false accepts)");
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
