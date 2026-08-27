// SPDX-License-Identifier: GPL-3.0-or-later
//! The properties ADR-0071 fixes, held by construction and checked here.

use super::*;
use alloc::string::String;
use alloc::vec;

use tos_ir::*;

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

/// A module that exports one function and optionally imports another module.
fn module(name: &str, content: &str, imports: &[(&str, &str)]) -> Module {
    // docs/42 §1 derives the expected module name from the path, and
    // `V2003_SOURCE_IDENTITY` checks it, so the fixture derives the path from
    // the name rather than inventing one.
    let path = alloc::format!("{}.tos", name.replace('.', "/"));
    Module {
        header: Header {
            schema_id: String::from(tos_ir::SCHEMA_ID),
            language_version: String::from(tos_ir::LANGUAGE_VERSION),
            unicode_normalization_baseline: String::from(tos_ir::UNICODE_BASELINE),
            profile: Profile::Bootstrap,
            module_name: String::from(name),
            source_set: String::from("tos-residency-tests"),
            path: path.clone(),
            content_id: String::from(content),
            dependency_digest: String::from("sha256:dependencies"),
            frontend_identity: String::from("tos-core-reference/0.1.0"),
            source_map_revision: String::from(tos_ir::SOURCE_MAP_REVISION),
            resource_envelope: ResourceEnvelope {
                fuel: 1000,
                stack: 1024,
                imports: imports.len() as u128,
                ..ResourceEnvelope::default()
            },
            capability_interface_digest: String::from("sha256:capabilities"),
        },
        types: vec![TypeDef::Int(IntKind::I32)],
        imports: imports
            .iter()
            .map(|(name, content)| Import {
                module_name: String::from(*name),
                module_content_id: String::from(*content),
                binding: String::from("dependency"),
            })
            .collect(),
        capability_imports: Vec::new(),
        exports: Vec::new(),
        constants: vec![Constant::Int(IntKind::I32, 7)],
        functions: vec![Function {
            signature: Signature {
                name: String::from("answer"),
                visibility: Visibility::Public,
                is_async: false,
                parameters: Vec::new(),
                result: 0,
                effects: Vec::new(),
            },
            origin: FunctionOrigin::Declared,
            source: 0,
            stack_contribution: 0,
            fuel_contribution: 0,
            cleanup_contribution: 0,
            values: vec![0],
            blocks: vec![Block {
                parameters: Vec::new(),
                instructions: vec![Instruction {
                    result: Some(0),
                    ty: 0,
                    op: Op::Const(0),
                    source: 0,
                    runtime_contract: None,
                    unsafe_block: false,
                    unsafe_interface: None,
                }],
                terminator: Terminator::Return(Some(Operand::Value(0))),
                source: 0,
            }],
        }],
        source_map: vec![SourceMapEntry {
            source_set: String::from("tos-residency-tests"),
            path: path.clone(),
            content_id: String::from(content),
            frontend_identity: String::from("tos-core-reference/0.1.0"),
            language_version: String::from(tos_ir::LANGUAGE_VERSION),
            profile: Profile::Bootstrap,
            unicode_normalization_baseline: String::from(tos_ir::UNICODE_BASELINE),
            byte_start: 0,
            byte_end: 4,
            derived_from: None,
        }],
    }
}

/// A closure of images, held in memory for the test.
struct Store {
    images: Vec<ImageSnapshot>,
    resolutions: Vec<ResolutionSnapshot>,
}

impl ClosureSource for Store {
    fn count(&self) -> usize {
        self.images.len()
    }

    fn image(&self, position: usize) -> Option<ImageSnapshot> {
        self.images.get(position).cloned()
    }
}

impl ModuleProvider for Store {
    fn image(&self, id: ClosureModuleId) -> Option<ImageSnapshot> {
        self.images.get(id.position()).cloned()
    }
}

/// Two modules: a dependency, and an entry that imports it.
fn closure() -> Store {
    let dependency = module("set.dependency", "sha256:dep", &[]);
    let entry = module(
        "set.entry",
        "sha256:entry",
        &[("set.dependency", "sha256:dep")],
    );
    let modules = [dependency, entry];
    let images: Vec<ImageSnapshot> = modules
        .iter()
        .map(|module| {
            let (bytes, _) = tos_image::encode(module);
            ImageSnapshot::from(bytes.into_boxed_slice())
        })
        .collect();
    let mut resolutions = Vec::new();
    for module in &modules {
        let mut declared = tos_verifier::DeclaredResolution::new();
        for other in &modules {
            let wanted = other.header.module_name == module.header.module_name
                || module
                    .imports
                    .iter()
                    .any(|import| import.module_name == other.header.module_name);
            if !wanted {
                continue;
            }
            declared
                .module(&other.header.module_name, &other.header.content_id)
                .exports_declared();
            for function in &other.functions {
                declared.export(&function.signature.name);
            }
        }
        resolutions.push(declared.build());
    }
    Store {
        images,
        resolutions,
    }
}

fn launched(store: &Store) -> Launched {
    launch(
        store,
        &|position| store.resolutions[position].clone(),
        &Limits::default(),
        1,
        "answer",
    )
    .expect("the closure verifies")
}

#[test]
fn a_launch_verifies_the_closure_and_keeps_only_records_and_membership() {
    let store = closure();
    let result = launched(&store);
    assert_eq!(result.records.len(), 2);
    assert_eq!(result.manifest.modules(), 2);
    let (entry, function) = result.manifest.entry();
    assert_eq!(entry.position(), 1);
    assert_eq!(function, 0);

    // The record is fixed size, so releasing a module retains a constant.
    assert_eq!(
        core::mem::size_of::<VerifiedModuleRecord>(),
        464,
        "the record is fixed size and holds no text"
    );
    assert_eq!(core::mem::size_of::<Member>(), 36);
    assert_eq!(
        result.records[0].artifact_digest,
        tos_hash::sha256(&store.images[0]),
        "the record commits to the exact bytes that were verified"
    );
}

/// The identity is the pair, and the manifest answers with a member or with
/// nothing.
#[test]
fn membership_resolves_on_the_exact_pair() {
    let store = closure();
    let result = launched(&store);
    let manifest = &result.manifest;

    assert_eq!(
        manifest
            .resolve("set.dependency", "sha256:dep")
            .map(|id| id.position()),
        Some(0)
    );
    assert!(
        manifest.resolve("set.dependency", "sha256:other").is_none(),
        "a right name with a wrong identity does not resolve"
    );
    assert!(
        manifest.resolve("set.absent", "sha256:dep").is_none(),
        "a name outside the closure does not resolve"
    );
    assert!(
        manifest.module(manifest.modules()).is_none(),
        "the manifest does not mint an identity past the closure"
    );
}

/// A reload is byte identity, and everything else about it is a refusal.
#[test]
fn a_reload_checks_the_artifact_digest_before_parsing() {
    let store = closure();
    let result = launched(&store);
    let mut residency = Residency::new(
        ResidencyLimits {
            modules: 2,
            bytes: 64 * 1024 * 1024,
        },
        parse_limits(),
    )
    .expect("admissible bounds");
    let entry = result.manifest.module(1).expect("in the closure");
    residency
        .ensure(entry, &store, &result.records, &result.manifest)
        .expect("the entry loads");
    assert!(residency.module_of(entry).is_some());

    // A provider that hands over a different, well-formed image.
    struct Swapped<'a> {
        inner: &'a Store,
        at: usize,
        instead: ImageSnapshot,
    }
    impl ModuleProvider for Swapped<'_> {
        fn image(&self, id: ClosureModuleId) -> Option<ImageSnapshot> {
            if id.position() == self.at {
                return Some(self.instead.clone());
            }
            ModuleProvider::image(self.inner, id)
        }
    }
    let mut residency = Residency::new(
        ResidencyLimits {
            modules: 1,
            bytes: 64 * 1024 * 1024,
        },
        parse_limits(),
    )
    .expect("admissible bounds");
    let swapped = Swapped {
        inner: &store,
        at: 1,
        instead: store.images[0].clone(),
    };
    assert_eq!(
        residency.ensure(entry, &swapped, &result.records, &result.manifest),
        Err(Failure::ArtifactDigest { module: 1 })
    );

    // And one that has nothing.
    struct Absent;
    impl ModuleProvider for Absent {
        fn image(&self, _: ClosureModuleId) -> Option<ImageSnapshot> {
            None
        }
    }
    assert_eq!(
        residency.ensure(entry, &Absent, &result.records, &result.manifest),
        Err(Failure::Missing(1))
    );
}

/// Import slots and export indexes are resident derived state: built inside the
/// module the manifest fixed, complete before it is admitted, and gone when it
/// is evicted.
#[test]
fn derived_indexes_are_resident_and_die_with_the_module() {
    let store = closure();
    let result = launched(&store);
    let mut residency = Residency::new(
        ResidencyLimits {
            modules: 1,
            bytes: 64 * 1024 * 1024,
        },
        parse_limits(),
    )
    .expect("admissible bounds");
    let entry = result.manifest.module(1).expect("in the closure");
    residency
        .ensure(entry, &store, &result.records, &result.manifest)
        .expect("loads");
    assert!(
        residency.ledger().index_bytes > 0,
        "the derived indexes are in the ledger the moment the module is admitted"
    );

    let settled = residency.ledger();
    let callee = residency
        .import_of(entry, 0)
        .expect("the import slot resolves");
    assert_eq!(callee.position(), 0);
    assert_eq!(
        residency.ledger(),
        settled,
        "reading derived state allocates nothing and moves no bound"
    );

    // At a bound of one, reaching the callee evicts the caller and its indexes.
    residency
        .ensure(callee, &store, &result.records, &result.manifest)
        .expect("loads");
    assert_eq!(residency.resident(), 1);
    assert!(
        residency.module_of(entry).is_none(),
        "the caller was evicted"
    );
    assert_eq!(
        residency.export_of(callee, "answer"),
        Some(0),
        "the callee's export index is built inside the callee"
    );
    assert_eq!(residency.export_of(callee, "absent"), None);
    assert!(residency.traffic().evictions >= 1);
}

/// The derived index is inside the bound at admission, not after it.
///
/// A byte bound that a module fits only while its indexes are still unbuilt is
/// not a bound: the load would be admitted and the ledger would grow past the
/// limit on first use, with no admission decision left to make. So the same
/// module is offered two bounds — one that covers everything except the index,
/// and one that covers everything — and only the second admits it.
#[test]
fn the_derived_index_counts_before_admission() {
    let store = closure();
    let result = launched(&store);
    let entry = result.manifest.module(1).expect("in the closure");

    let mut generous = Residency::new(
        ResidencyLimits {
            modules: 2,
            bytes: 64 * 1024 * 1024,
        },
        parse_limits(),
    )
    .expect("admissible bounds");
    generous
        .ensure(entry, &store, &result.records, &result.manifest)
        .expect("loads");
    let complete = generous.ledger();
    assert!(
        complete.index_bytes > 0,
        "the entry derives an import map and an export index"
    );

    let mut tight = Residency::new(
        ResidencyLimits {
            modules: 2,
            bytes: complete.total() - complete.index_bytes,
        },
        parse_limits(),
    )
    .expect("admissible bounds");
    match tight.ensure(entry, &store, &result.records, &result.manifest) {
        Err(Failure::OverResidencyBound { module: 1, .. }) => {}
        other => panic!("the index was admitted outside the byte bound: {other:?}"),
    }
    assert_eq!(tight.resident(), 0, "a refused load leaves nothing behind");

    let mut exact = Residency::new(
        ResidencyLimits {
            modules: 2,
            bytes: complete.total(),
        },
        parse_limits(),
    )
    .expect("admissible bounds");
    exact
        .ensure(entry, &store, &result.records, &result.manifest)
        .expect("the complete cost fits a bound set to the complete cost");
}

/// A private function is not in the export index at all.
///
/// Not hidden by a visibility check at the lookup — absent. This is the rule the
/// engine's imported call already enforced, moved into the shape of the index so
/// there is no second place it could be forgotten.
#[test]
fn the_export_index_holds_public_functions_only() {
    let mut mixed = module("set.mixed", "sha256:mixed", &[]);
    let mut hidden = mixed.functions[0].clone();
    hidden.signature.name = String::from("hidden");
    hidden.signature.visibility = Visibility::Private;
    // docs/43 §2 orders functions by name, and "answer" precedes "hidden".
    mixed.functions.push(hidden);

    let (bytes, _) = tos_image::encode(&mixed);
    let store = Store {
        images: vec![ImageSnapshot::from(bytes.into_boxed_slice())],
        resolutions: vec![ResolutionSnapshot::default()],
    };
    let result = launch(
        &store,
        &|position| store.resolutions[position].clone(),
        &Limits::default(),
        0,
        "answer",
    )
    .expect("the module verifies");

    let mut residency = Residency::new(
        ResidencyLimits {
            modules: 1,
            bytes: 64 * 1024 * 1024,
        },
        parse_limits(),
    )
    .expect("admissible bounds");
    let id = result.manifest.module(0).expect("in the closure");
    residency
        .ensure(id, &store, &result.records, &result.manifest)
        .expect("loads");

    assert_eq!(residency.export_of(id, "answer"), Some(0));
    assert_eq!(
        residency.export_of(id, "hidden"),
        None,
        "a private function has no entry to find"
    );
    assert_eq!(
        residency.public_exports_of(id),
        Some(1),
        "the index holds one entry, not two"
    );
}

/// A configuration that describes no possible execution is refused, not fixed.
#[test]
fn a_residency_of_no_modules_is_refused() {
    assert_eq!(
        Residency::new(
            ResidencyLimits {
                modules: 0,
                bytes: 64 * 1024 * 1024,
            },
            parse_limits(),
        )
        .err(),
        Some(ConfigurationError::NoResidentModules),
        "zero resident modules is inadmissible, and not silently one"
    );
    assert!(Residency::new(
        ResidencyLimits {
            modules: 1,
            bytes: 64 * 1024 * 1024,
        },
        parse_limits(),
    )
    .is_ok());
}

/// Eviction is deterministic and driven by the declared bounds alone.
#[test]
fn eviction_is_least_recently_used_and_bounded_by_count() {
    let store = closure();
    let result = launched(&store);
    let mut residency = Residency::new(
        ResidencyLimits {
            modules: 1,
            bytes: 64 * 1024 * 1024,
        },
        parse_limits(),
    )
    .expect("admissible bounds");
    let first = result.manifest.module(0).expect("in the closure");
    let second = result.manifest.module(1).expect("in the closure");
    for _ in 0..4 {
        residency
            .ensure(first, &store, &result.records, &result.manifest)
            .expect("loads");
        assert_eq!(residency.resident(), 1);
        residency
            .ensure(second, &store, &result.records, &result.manifest)
            .expect("loads");
        assert_eq!(residency.resident(), 1);
    }
    assert_eq!(residency.traffic().loads, 8, "every crossing reloads");
    // Seven, not eight: the first load found an empty set and evicted nothing.
    assert_eq!(residency.traffic().evictions, 7);
}

/// A byte bound too small for one module fails the run rather than thrashing.
#[test]
fn a_bound_below_one_module_refuses() {
    let store = closure();
    let result = launched(&store);
    let mut residency = Residency::new(
        ResidencyLimits {
            modules: 4,
            bytes: 16,
        },
        parse_limits(),
    )
    .expect("admissible bounds");
    let entry = result.manifest.module(1).expect("in the closure");
    match residency.ensure(entry, &store, &result.records, &result.manifest) {
        Err(Failure::OverResidencyBound { module: 1, .. }) => {}
        other => panic!("a bound below one module did not refuse: {other:?}"),
    }
}

/// A well-formed image of a module that does not hold fails at launch, not at
/// first use.
#[test]
fn a_semantically_invalid_module_fails_the_launch() {
    let mut broken = module("set.broken", "sha256:broken", &[]);
    broken.functions[0].blocks[0].instructions[0].ty = 99;
    let (bytes, _) = tos_image::encode(&broken);
    let store = Store {
        images: vec![ImageSnapshot::from(bytes.into_boxed_slice())],
        resolutions: vec![ResolutionSnapshot::default()],
    };
    match launch(
        &store,
        &|position| store.resolutions[position].clone(),
        &Limits::default(),
        0,
        "answer",
    ) {
        Err(Failure::Verifier { module: 0, .. }) => {}
        other => panic!("an invalid module did not fail the launch: {other:?}"),
    }
}

/// A module name has no 128-byte ceiling, and a record must not invent one.
///
/// docs/44 §2 bounds an **identifier** at 128 bytes. A module name is
/// `identifier ("." identifier)*`, so a conforming name is bounded by the
/// source unit and by nothing smaller. The record commits to it rather than
/// storing it, so none of these is refused for its length.
#[test]
fn long_conforming_identities_are_representable() {
    let identifier = "i".repeat(128);
    let dotted = alloc::vec![identifier.as_str(); 12].join(".");
    assert!(dotted.len() > 1500, "well past any identifier ceiling");

    let cases = [
        ("a", "sha256:short"),
        (identifier.as_str(), "sha256:exactly128"),
        (dotted.as_str(), "sha256:dotted"),
    ];
    let mut seen = alloc::collections::BTreeSet::new();
    for (name, content) in cases {
        let identity = resolved_module_identity(name, content);
        assert!(seen.insert(identity), "{name} is its own identity");
    }

    // And a detached source-set identity of any length commits without a cap.
    let long_source_set = "detached-source-set/".repeat(64);
    assert!(long_source_set.len() > 1200);
    assert_ne!(
        source_set_identity(&long_source_set),
        source_set_identity("tos-residency-tests")
    );
}

/// The commitment separates the pair, and the length prefixes are why.
///
/// Without them `("ab", "c")` and `("a", "bc")` would hash the same bytes, and
/// two different resolutions would share one identity.
#[test]
fn different_pairs_have_different_identities() {
    let corpus = [
        ("set.a", "sha256:one"),
        ("set.a", "sha256:two"),
        ("set.b", "sha256:one"),
        ("ab", "c"),
        ("a", "bc"),
        ("", "sha256:one"),
        ("set.a", ""),
    ];
    let mut seen = alloc::collections::BTreeSet::new();
    for (name, content) in corpus {
        assert!(
            seen.insert(resolved_module_identity(name, content)),
            "({name:?}, {content:?}) collides with another pair"
        );
    }
    assert_eq!(seen.len(), corpus.len());
}

/// A launch of a module whose name is far past any identifier ceiling still
/// verifies, records and resolves.
#[test]
fn a_module_with_a_very_long_name_launches_and_resolves() {
    let segment = "s".repeat(120);
    let name = alloc::format!("{segment}.{segment}.{segment}");
    assert!(name.len() > 128);
    let long = module(&name, "sha256:long", &[]);
    let (bytes, _) = tos_image::encode(&long);
    let store = Store {
        images: alloc::vec![ImageSnapshot::from(bytes.into_boxed_slice())],
        resolutions: alloc::vec![ResolutionSnapshot::default()],
    };
    let result = launch(
        &store,
        &|position| store.resolutions[position].clone(),
        &Limits::default(),
        0,
        "answer",
    )
    .expect("a long module name is not a reason to refuse a conforming module");
    assert_eq!(
        result
            .manifest
            .resolve(&name, "sha256:long")
            .map(|id| id.position()),
        Some(0)
    );
    assert_eq!(
        result.records[0].resolved_identity,
        resolved_module_identity(&name, "sha256:long")
    );
}
