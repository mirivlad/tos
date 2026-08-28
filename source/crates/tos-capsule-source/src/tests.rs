// SPDX-License-Identifier: GPL-3.0-or-later
//! What a capsule-backed source set offers, and what it costs to ask.

use super::*;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use tos_capsule::build::{Builder, FileSpec};
use tos_capsule::parse;
use tos_pipeline::{Run, Silent, Unreachable};

/// A module of the fixture set, as canonical source.
fn module(name: &str, imports: &str, body: &str) -> String {
    let mut text = String::from("module ");
    text.push_str(name);
    text.push_str(" version 1.0 profile bootstrap; ");
    text.push_str(imports);
    text.push_str(
        " resource [fuel: 10000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 4] ",
    );
    text.push_str(body);
    text
}

/// A capsule carrying a two-module program, a version marker and a notice.
///
/// The boot path is the canonical one the format requires, and the entry is
/// stored there: a capsule whose boot file is not source is not a capsule this
/// system boots.
fn fixture() -> Vec<u8> {
    let math = module(
        "system.lib.math",
        "",
        "pub fn double(value: i32) -> i32 { return value * 2i32; }",
    );
    let init = module(
        "system.boot.init",
        "import system.lib.math as math;",
        "pub fn main() -> i32 { return math.double(21i32); }",
    );
    let mut builder = Builder::new();
    builder.add(FileSpec::new("/system/boot/init.tos", init.as_bytes()));
    builder.add(FileSpec::new("/system/lib/math.tos", math.as_bytes()));
    // Not source, and offered as none: a version marker never claimed to be a
    // module.
    builder.add(FileSpec::new("/system/version", b"0.2.1\n"));
    builder.set_licence_notice(b"notice\n".to_vec());
    builder.build().expect("the fixture capsule builds")
}

#[test]
fn only_the_source_files_of_a_capsule_are_offered_as_a_set() {
    let bytes = fixture();
    let capsule = parse(&bytes).expect("the fixture capsule parses");
    let provider = CapsuleSourceProvider::over(capsule);

    let catalog = provider.catalog();
    let paths: Vec<&str> = catalog.iter().map(|entry| entry.path).collect();
    assert_eq!(
        paths,
        vec!["system/boot/init.tos", "system/lib/math.tos"],
        "every .tos file, module-root relative, and nothing else"
    );
}

#[test]
fn a_unit_is_a_window_into_the_capsule_rather_than_a_copy() {
    let bytes = fixture();
    let capsule = parse(&bytes).expect("the fixture capsule parses");
    let provider = CapsuleSourceProvider::over(capsule);

    let entry = provider.catalog()[0];
    let snapshot = provider.source(entry.id).expect("the entry has source");
    let unit = snapshot.bytes();

    let base = bytes.as_ptr() as usize;
    let at = unit.as_ptr() as usize;
    assert!(
        at >= base && at + unit.len() <= base + bytes.len(),
        "the unit lies inside the capsule payload, so nothing was copied"
    );
    assert!(
        unit.starts_with(b"module system.boot.init"),
        "and it is the source the capsule stores"
    );
}

#[test]
fn a_capsule_entry_that_is_not_source_has_no_bytes_to_offer() {
    let bytes = fixture();
    let capsule = parse(&bytes).expect("the fixture capsule parses");
    let provider = CapsuleSourceProvider::over(capsule);

    // The path table is sorted by path bytes, so the version marker sits after
    // the two source files; it is addressed here directly rather than through
    // the catalog, which never offered it.
    let position = (0..capsule.path_table_count() as usize)
        .find(|index| {
            capsule
                .file_at(*index)
                .is_some_and(|file| file.name == b"/system/version")
        })
        .expect("the fixture carries a version marker");
    assert!(
        provider.source(SourceEntryId::at(position)).is_none(),
        "an entry the set does not offer cannot be materialized through it"
    );
    assert!(
        provider
            .source(SourceEntryId::at(capsule.path_table_count() as usize))
            .is_none(),
        "and neither can a position the capsule does not have"
    );
}

/// A capsule is a source set a build can actually be run over (ADR-0073, B).
///
/// The whole reference path, from a capsule's own bytes: the provider offers
/// the set, resolution closes the closure over it, the build encodes images,
/// the admission verifies them and the run executes. What this establishes is
/// the **provider and the algorithm** — that a capsule-backed source set builds
/// and runs. What a capsule can hold, and whether a build worker can hand its
/// output to another process, are different claims and are not made here.
#[test]
fn a_capsule_backed_source_set_builds_verifies_and_runs() {
    let bytes = fixture();
    let capsule = parse(&bytes).expect("the fixture capsule parses");
    let provider = CapsuleSourceProvider::over(capsule);

    let built = tos_pipeline::build_from_provider(
        &provider,
        "tos-capsule-source-tests",
        "system/boot/init.tos",
        &mut Silent,
    )
    .expect("the capsule contains the entry");
    let tos_pipeline::Build::Ready(built) = built else {
        panic!("the capsule's source closure builds");
    };
    assert_eq!(built.modules(), 2, "the entry and the module it imports");

    let admitted = tos_pipeline::admit(*built, "main", &mut Silent, tos_pipeline::HOST_RESIDENCY);
    let tos_pipeline::Preparation::Ready(mut prepared) = admitted else {
        panic!("the built closure is admitted");
    };
    let run = tos_pipeline::run_prepared(&mut prepared, Vec::new(), &mut Unreachable);
    let Run::Completed(completion) = run else {
        panic!("the capsule's program runs: {run:?}");
    };
    assert_eq!(
        completion.value,
        tos_pipeline::Value::Int(tos_pipeline::IntKind::I32, 42)
    );
}
