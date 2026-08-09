// SPDX-License-Identifier: GPL-3.0-or-later
//! Research-only native measurement of the exact Stage 1 validation sequence.
//!
//! This binary deliberately calls the production capsule parser twice for each
//! sample and performs the second-pass canonical boot-file lookup. It does not
//! emulate firmware, capsule I/O, BootInfo construction or serial delivery.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use tos_capsule::test_crypto_baseline::{verify as verify_parser_crypto, CryptoAccounting};
use tos_capsule::{parse, Capsule, BOOT_PATH};
use tos_hash::sha256;

enum Mode {
    Full,
    Crypto,
}

struct Args {
    capsule: PathBuf,
    output: PathBuf,
    warmups: usize,
    samples: usize,
    mode: Mode,
}

fn usage() -> ! {
    eprintln!(
        "usage: tos-stage1-performance --capsule FILE --out FILE [--mode full|crypto] [--warmups N] [--samples N]"
    );
    std::process::exit(2);
}

fn parse_positive(value: Option<&String>, option: &str) -> usize {
    value
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|&number| number > 0)
        .unwrap_or_else(|| {
            eprintln!("{option} requires a positive integer");
            usage()
        })
}

fn args() -> Args {
    let values: Vec<String> = std::env::args().skip(1).collect();
    let mut capsule = None;
    let mut output = None;
    let mut warmups = 3;
    let mut samples = 21;
    let mut mode = Mode::Full;
    let mut index = 0;
    while index < values.len() {
        match values[index].as_str() {
            "--capsule" => {
                capsule = values.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--out" => {
                output = values.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            "--warmups" => {
                warmups = parse_positive(values.get(index + 1), "--warmups");
                index += 2;
            }
            "--samples" => {
                samples = parse_positive(values.get(index + 1), "--samples");
                index += 2;
            }
            "--mode" => {
                mode = match values.get(index + 1).map(String::as_str) {
                    Some("full") => Mode::Full,
                    Some("crypto") => Mode::Crypto,
                    _ => {
                        eprintln!("--mode must be full or crypto");
                        usage();
                    }
                };
                index += 2;
            }
            "-h" | "--help" => usage(),
            other => {
                eprintln!("unknown option: {other}");
                usage();
            }
        }
    }
    Args {
        capsule: capsule.unwrap_or_else(|| usage()),
        output: output.unwrap_or_else(|| usage()),
        warmups,
        samples,
        mode,
    }
}

fn validate_twice_and_lookup(bytes: &[u8]) -> Result<(), String> {
    // The loader computes the plain capsule digest for BootInfo before its
    // parser validation. The nucleus recomputes it before its own parser
    // validation, comparing the explicit ABI mirror rather than a cache.
    let loader_capsule_digest = sha256(bytes);
    // First pass models the loader's fresh capsule validation. Dropping the
    // parsed view before the second call ensures no parsed object/digest is
    // carried across the boundary.
    {
        let first = parse(bytes).map_err(|error| format!("first validation: {error:?}"))?;
        let _first_files = first.file_count();
    }

    let nucleus_capsule_digest = sha256(bytes);
    if nucleus_capsule_digest != loader_capsule_digest {
        return Err("nucleus validation: BootInfo capsule digest mismatch".to_string());
    }

    // Second pass models the nucleus's independent validation and canonical
    // boot-text lookup. `parse` recomputes whole/per-file/detached digests.
    let second = parse(bytes).map_err(|error| format!("second validation: {error:?}"))?;
    let boot = second
        .boot_file()
        .ok_or_else(|| "second validation: canonical boot file is absent".to_string())?;
    if boot.name != BOOT_PATH {
        return Err("second validation: canonical lookup returned a wrong path".to_string());
    }
    Ok(())
}

fn sum_accounting(parser: CryptoAccounting, capsule_bytes: usize) -> CryptoAccounting {
    CryptoAccounting {
        // Two parser passes plus the loader/nucleus plain-capsule SHA-256
        // mirror pair. The mirror pair is an existing required ABI operation.
        bytes_hashed: parser.bytes_hashed * 2 + (capsule_bytes as u64) * 2,
        hash_invocations: parser.hash_invocations * 2 + 2,
        file_hashes: parser.file_hashes * 2,
        detached_identity_hashes: parser.detached_identity_hashes * 2,
        whole_capsule_hashes: parser.whole_capsule_hashes * 2 + 2,
    }
}

fn validate_unavoidable_crypto_twice(
    bytes: &[u8],
    capsule: &Capsule<'_>,
) -> Result<CryptoAccounting, String> {
    let loader_capsule_digest = sha256(bytes);
    let first =
        verify_parser_crypto(capsule).map_err(|error| format!("first crypto pass: {error:?}"))?;
    let nucleus_capsule_digest = sha256(bytes);
    if nucleus_capsule_digest != loader_capsule_digest {
        return Err("crypto baseline: BootInfo capsule digest mismatch".to_string());
    }
    let second =
        verify_parser_crypto(capsule).map_err(|error| format!("second crypto pass: {error:?}"))?;
    if second != first {
        return Err("crypto baseline: pass accounting differs".to_string());
    }
    Ok(sum_accounting(first, bytes.len()))
}

fn record_full(
    output: &mut fs::File,
    phase: &str,
    index: usize,
    bytes: &[u8],
) -> Result<(), String> {
    let started = Instant::now();
    validate_twice_and_lookup(bytes)?;
    let duration_ns = started.elapsed().as_nanos();
    writeln!(
        output,
        "{{\"duration_ns\":{duration_ns},\"index\":{index},\"lookup\":\"/system/boot/init.tos\",\"mode\":\"full\",\"phase\":\"{phase}\",\"validations\":2}}"
    )
    .map_err(|error| format!("write sample: {error}"))
}

fn record_crypto(
    output: &mut fs::File,
    phase: &str,
    index: usize,
    bytes: &[u8],
    capsule: &Capsule<'_>,
) -> Result<(), String> {
    let started = Instant::now();
    let accounting = validate_unavoidable_crypto_twice(bytes, capsule)?;
    let duration_ns = started.elapsed().as_nanos();
    writeln!(
        output,
        "{{\"crypto_bytes_per_boot\":{},\"crypto_hashes_per_boot\":{},\"duration_ns\":{duration_ns},\"index\":{index},\"mode\":\"unavoidable_crypto\",\"phase\":\"{phase}\",\"validations\":2}}",
        accounting.bytes_hashed,
        accounting.hash_invocations,
    )
    .map_err(|error| format!("write sample: {error}"))
}

fn main() {
    let args = args();
    let bytes = fs::read(&args.capsule).unwrap_or_else(|error| {
        eprintln!("read capsule {}: {error}", args.capsule.display());
        std::process::exit(2);
    });
    let mut output = fs::File::create(&args.output).unwrap_or_else(|error| {
        eprintln!("create output {}: {error}", args.output.display());
        std::process::exit(2);
    });
    let crypto_capsule = match args.mode {
        Mode::Full => None,
        // Setup establishes only a structural borrowed view outside the timer.
        // Its computed hashes are dropped; each timed crypto pass starts fresh.
        Mode::Crypto => Some(parse(&bytes).unwrap_or_else(|error| {
            eprintln!("crypto setup parse: {error:?}");
            std::process::exit(1);
        })),
    };
    for (phase, count) in [("warmup", args.warmups), ("measurement", args.samples)] {
        for index in 1..=count {
            let result = match crypto_capsule.as_ref() {
                Some(capsule) => record_crypto(&mut output, phase, index, &bytes, capsule),
                None => record_full(&mut output, phase, index, &bytes),
            };
            result.unwrap_or_else(|error| {
                eprintln!("native {phase} {index}: {error}");
                std::process::exit(1);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tos_capsule::build::{Builder, FileSpec};
    use tos_capsule::SRC_KIND_DETACHED;

    #[test]
    fn production_parser_runs_twice_then_finds_canonical_boot_file() {
        let mut builder = Builder::new();
        builder.source_identity_kind = SRC_KIND_DETACHED;
        builder.add(FileSpec::new("/system/boot/init.tos", b"# boot\n"));
        builder.add(FileSpec::new("/system/version", b"0.2.1\n"));
        let bytes = builder.build().expect("build fixture");
        validate_twice_and_lookup(&bytes).expect("two production validations and lookup");
    }

    #[test]
    fn crypto_baseline_rehashes_one_parser_pass_without_cached_results() {
        let mut builder = Builder::new();
        builder.source_identity_kind = SRC_KIND_DETACHED;
        builder.add(FileSpec::new("/system/boot/init.tos", b"# boot\n"));
        builder.add(FileSpec::new("/system/version", b"0.2.1\n"));
        let bytes = builder.build().expect("build fixture");
        let capsule = parse(&bytes).expect("parse fixture");
        let first = verify_parser_crypto(&capsule).expect("first crypto replay");
        let second = verify_parser_crypto(&capsule).expect("second crypto replay");
        assert_eq!(first, second);
        assert_eq!(first.file_hashes, 2);
        assert_eq!(first.detached_identity_hashes, 1);
        assert_eq!(first.whole_capsule_hashes, 1);
        assert_eq!(first.hash_invocations, 4);
        assert_eq!(
            first.bytes_hashed,
            7 + 6 + 11 + 2 * (4 + 32) + 36 + bytes.len() as u64
        );
    }
}
