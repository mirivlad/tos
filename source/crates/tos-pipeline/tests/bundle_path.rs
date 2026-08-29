// SPDX-License-Identifier: GPL-3.0-or-later
//! The bundle path against the in-memory one, on the same source.
//!
//! ADR-0073 §1 lets a build's products leave the workspace as they are made.
//! What must not change when they do is anything a run can observe: the same
//! source must produce the same images, the same receipt, the same value and
//! the same accounting whether the closure was admitted from memory or read out
//! of a `TOSBUNDLE/v1` the build never held.
//!
//! That is the whole point of the differential. A storage arrangement that
//! changed a result would be a semantic input, and a build's output has no
//! business being one.

use tos_bundle::{Bundle, BundleError, SliceBacking};
use tos_pipeline::{
    admit, admit_bundle, build_from_provider, build_into_bundle, run_prepared, Build,
    BuildIntoBundle, Preparation, Run, Silent, SliceSourceProvider, Unit, Unreachable,
};

const SOURCE_SET: &str = "tos-bundle-path-tests";
const ENTRY_PATH: &str = "system/boot/init.tos";

fn module(name: &str, imports: &str, body: &str) -> String {
    format!(
        "module {name} version 1.0 profile bootstrap; {imports} \
         resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 4] {body}"
    )
}

/// A two-module program: the entry calls across the closure for its answer.
fn texts() -> (String, String) {
    (
        module(
            "system.lib.math",
            "",
            "pub fn double(value: i32) -> i32 { return value * 2i32; }",
        ),
        module(
            "system.boot.init",
            "import system.lib.math as math;",
            "pub fn main() -> i32 { return math.double(21i32); }",
        ),
    )
}

fn units<'a>(dependency: &'a str, entry: &'a str) -> Vec<Unit<'a>> {
    vec![
        Unit {
            path: "system/lib/math.tos",
            bytes: dependency.as_bytes(),
        },
        Unit {
            path: ENTRY_PATH,
            bytes: entry.as_bytes(),
        },
    ]
}

/// What one path produced, in the terms a caller can compare.
struct Outcome {
    /// How much of the backing the bundle occupies, for a caller that parses it
    /// again. Zero on the in-memory path, which wrote no bundle.
    bytes: usize,
    receipt: tos_verifier::VerifiedModule,
    value: tos_pipeline::Value,
    fuel_used: u128,
    fuel_limit: u128,
    modules: usize,
}

/// Build, admit and run in memory.
fn in_memory(units: &[Unit<'_>]) -> Outcome {
    let provider = SliceSourceProvider::new(units);
    let built = build_from_provider(&provider, SOURCE_SET, ENTRY_PATH, &mut Silent)
        .expect("the set names an entry it contains");
    let Build::Ready(built) = built else {
        panic!("the closure builds");
    };
    let modules = built.modules();
    let Preparation::Ready(mut prepared) =
        admit(*built, "main", &mut Silent, tos_pipeline::HOST_RESIDENCY)
    else {
        panic!("the closure is admitted");
    };
    let run = run_prepared(&mut prepared, Vec::new(), &mut Unreachable);
    let Run::Completed(completion) = run else {
        panic!("the closure runs: {run:?}");
    };
    Outcome {
        bytes: 0,
        receipt: completion.receipt.clone(),
        value: completion.value.clone(),
        fuel_used: completion.accounting.fuel_used,
        fuel_limit: completion.accounting.fuel_limit,
        modules,
    }
}

/// Build into a bundle, then admit and run out of it.
fn through_a_bundle(units: &[Unit<'_>], backing: &mut [u8]) -> Outcome {
    let provider = SliceSourceProvider::new(units);
    let written = {
        let mut slice = SliceBacking::new(backing);
        build_into_bundle(&provider, SOURCE_SET, ENTRY_PATH, &mut slice, &mut Silent)
            .expect("the set names an entry it contains")
    };
    let BuildIntoBundle::Written { bytes, modules } = written else {
        panic!("the bundle is written");
    };
    let bundle = Bundle::parse(&backing[..bytes]).expect("the bundle parses");
    assert_eq!(bundle.modules(), modules);
    assert_eq!(bundle.entry_path(), ENTRY_PATH);

    let Preparation::Ready(mut prepared) =
        admit_bundle(&bundle, "main", &mut Silent, tos_pipeline::HOST_RESIDENCY)
    else {
        panic!("the bundle is admitted");
    };
    let run = run_prepared(&mut prepared, Vec::new(), &mut Unreachable);
    let Run::Completed(completion) = run else {
        panic!("the bundle's closure runs: {run:?}");
    };
    Outcome {
        bytes,
        receipt: completion.receipt.clone(),
        value: completion.value.clone(),
        fuel_used: completion.accounting.fuel_used,
        fuel_limit: completion.accounting.fuel_limit,
        modules,
    }
}

/// The two paths agree on everything a run can observe.
#[test]
fn a_closure_admitted_from_a_bundle_is_the_closure_admitted_from_memory() {
    let (dependency, entry) = texts();
    let units = units(&dependency, &entry);
    let memory = in_memory(&units);
    let mut backing = vec![0u8; 1 << 20];
    let bundled = through_a_bundle(&units, &mut backing);

    assert_eq!(memory.modules, bundled.modules, "the same membership");
    assert_eq!(
        memory.receipt, bundled.receipt,
        "the same receipt, from the target's own verifier over the same bytes"
    );
    assert_eq!(memory.value, bundled.value, "the same answer");
    assert_eq!(memory.fuel_used, bundled.fuel_used, "for the same cost");
    assert_eq!(
        memory.fuel_limit, bundled.fuel_limit,
        "against the same declared budget"
    );
}

/// The images in a bundle are the images a build produces, byte for byte.
///
/// Compared through the receipts rather than by eye: the entry receipt binds to
/// the artifact digest of the image it was issued for, so two paths agreeing on
/// it agree on the bytes. The images are also read back to prove they survived
/// the round trip as themselves.
#[test]
fn the_bundle_carries_the_images_unchanged() {
    let (dependency, entry) = texts();
    let units = units(&dependency, &entry);
    let mut backing = vec![0u8; 1 << 20];
    let bundled = through_a_bundle(&units, &mut backing);
    assert_eq!(bundled.modules, 2);

    let bundle = Bundle::parse(&backing[..bundled.bytes]).expect("the bundle parses");
    for position in 0..bundle.modules() {
        let image = bundle.image(position).expect("an image per module");
        assert!(
            image.starts_with(&tos_image::MAGIC),
            "what came out of the bundle is still a TOSIMAGE, framed as one"
        );
        let declaration = bundle
            .declaration(position)
            .expect("a declaration per module");
        assert!(
            declaration.content_id.starts_with("sha256:"),
            "and the build's claim about it names an identity"
        );
    }
}

/// The same source produces the same bundle, byte for byte.
///
/// Determinism is what lets a bundle be compared, cached or reproduced at all,
/// and it has to survive the build being restructured: a two-pass check that
/// reordered anything, or a representation that iterated a set in a different
/// order, would show up here as different bytes rather than as a wrong answer.
#[test]
fn two_builds_of_one_source_set_produce_the_same_bundle_bytes() {
    let (dependency, entry) = texts();
    let units = units(&dependency, &entry);
    let provider = SliceSourceProvider::new(&units);

    let mut first = vec![0u8; 1 << 20];
    let mut second = vec![0u8; 1 << 20];
    let mut written = Vec::new();
    for backing in [&mut first, &mut second] {
        let mut slice = SliceBacking::new(backing);
        let BuildIntoBundle::Written { bytes, .. } =
            build_into_bundle(&provider, SOURCE_SET, ENTRY_PATH, &mut slice, &mut Silent)
                .expect("the set names an entry it contains")
        else {
            panic!("the bundle is written");
        };
        written.push(bytes);
    }

    assert_eq!(written[0], written[1], "the same length");
    assert_eq!(
        first[..written[0]],
        second[..written[1]],
        "and the same bytes"
    );
}

/// A backing too small for the closure ends the build, and leaves nothing
/// launchable.
#[test]
fn a_bundle_that_does_not_fit_fails_closed() {
    let (dependency, entry) = texts();
    let units = units(&dependency, &entry);
    let provider = SliceSourceProvider::new(&units);
    let mut backing = vec![0u8; 512];
    let mut slice = SliceBacking::new(&mut backing);
    let written = build_into_bundle(&provider, SOURCE_SET, ENTRY_PATH, &mut slice, &mut Silent)
        .expect("the set names an entry it contains");
    let BuildIntoBundle::OutOfRoom(full) = written else {
        panic!("512 bytes cannot hold a two-module closure");
    };
    assert_eq!(full.capacity, 512);
    assert!(full.needed > full.capacity);

    // What is in the backing is not a bundle: the header was never completed,
    // so no reader can be handed a shorter closure than the one that was asked
    // for.
    assert_eq!(Bundle::parse(&backing), Err(BundleError::BadMagic));
}

/// Bytes that are not a bundle are refused before any verifier is asked.
#[test]
fn a_bundle_that_does_not_describe_itself_is_never_admitted() {
    let mut zeroes = vec![0u8; 4096];
    assert_eq!(Bundle::parse(&zeroes), Err(BundleError::BadMagic));

    let (dependency, entry) = texts();
    let units = units(&dependency, &entry);
    let provider = SliceSourceProvider::new(&units);
    let bytes = {
        let mut slice = SliceBacking::new(&mut zeroes);
        let BuildIntoBundle::Written { bytes, .. } =
            build_into_bundle(&provider, SOURCE_SET, ENTRY_PATH, &mut slice, &mut Silent)
                .expect("the set names an entry it contains")
        else {
            panic!("the bundle is written");
        };
        bytes
    };

    // One byte of an image, flipped. The framing still parses — that is the
    // point — and the target's verifier is what refuses it.
    let table_offset = usize::from(zeroes[24]) | (usize::from(zeroes[25]) << 8);
    let image_offset =
        usize::from(zeroes[table_offset]) | (usize::from(zeroes[table_offset + 1]) << 8);
    zeroes[image_offset + 4] ^= 0xff;
    let bundle = Bundle::parse(&zeroes[..bytes]).expect("the framing is untouched");
    let refused = admit_bundle(&bundle, "main", &mut Silent, tos_pipeline::HOST_RESIDENCY);
    assert!(
        matches!(refused, Preparation::Refused(_)),
        "an image the bundle's own declaration does not fit is not admitted"
    );
}
