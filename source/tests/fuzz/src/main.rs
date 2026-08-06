// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic mutation fuzz target for the capsule parser.
//!
//! Property under test: `tos_capsule::parse` is total and bounded — for ANY
//! byte string it must return `Ok` or `Err`, never panic, never loop, and
//! never accept a mutated input as valid. The generator is a fixed-seed LCG,
//! so runs are reproducible. Exits 0 on pass, 1 on failure.

use tos_capsule::parse;

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

    // A mutated input must never parse. The only Ok case allowed would be a
    // zero-flip mutation, which cannot occur (flips >= 1), and pure garbage is
    // never valid. Any Ok here means the property failed.
    if accepted != 0 {
        eprintln!("FUZZ FAIL: {accepted} mutated/garbage inputs accepted");
        std::process::exit(1);
    }
    println!("FUZZ PASS rounds={rounds} (total parser, no panics, no false accepts)");
}
