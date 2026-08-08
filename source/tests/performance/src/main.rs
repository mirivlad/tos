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

use tos_capsule::{parse, BOOT_PATH};

struct Args {
    capsule: PathBuf,
    output: PathBuf,
    warmups: usize,
    samples: usize,
}

fn usage() -> ! {
    eprintln!(
        "usage: tos-stage1-performance --capsule FILE --out FILE [--warmups N] [--samples N]"
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
    }
}

fn validate_twice_and_lookup(bytes: &[u8]) -> Result<(), String> {
    // First pass models the loader's fresh capsule validation. Dropping the
    // parsed view before the second call ensures no parsed object/digest is
    // carried across the boundary.
    {
        let first = parse(bytes).map_err(|error| format!("first validation: {error:?}"))?;
        let _first_files = first.file_count();
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

fn record(output: &mut fs::File, phase: &str, index: usize, bytes: &[u8]) -> Result<(), String> {
    let started = Instant::now();
    validate_twice_and_lookup(bytes)?;
    let duration_ns = started.elapsed().as_nanos();
    writeln!(
        output,
        "{{\"duration_ns\":{duration_ns},\"index\":{index},\"lookup\":\"/system/boot/init.tos\",\"phase\":\"{phase}\",\"validations\":2}}"
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
    for index in 1..=args.warmups {
        record(&mut output, "warmup", index, &bytes).unwrap_or_else(|error| {
            eprintln!("native warm-up {index}: {error}");
            std::process::exit(1);
        });
    }
    for index in 1..=args.samples {
        record(&mut output, "measurement", index, &bytes).unwrap_or_else(|error| {
            eprintln!("native measurement {index}: {error}");
            std::process::exit(1);
        });
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
}
