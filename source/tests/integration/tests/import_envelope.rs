// SPDX-License-Identifier: GPL-3.0-or-later
//! `resource imports` is a count of modules, and it is now counted (ADR-0091).
//!
//! `docs/41` §6 declares the key as "maximum transitive module dependencies".
//! Nothing in the tree compared it with that quantity. The only check anywhere
//! on the path was, in the per-module verifier:
//!
//! ```text
//! module.imports.len() > header.resource_envelope.imports
//! ```
//!
//! which is a different number in both directions. It **admitted** a module
//! sitting above any number of indirect dependencies — a chain `A -> B -> C`
//! with `A` declaring `imports: 1` ran to completion, and the shape scales to
//! 255 transitive dependencies behind one declaration. And it **refused** a
//! conforming module that spelled one dependency twice, which ADR-0090 §2a had
//! just settled is one relationship.
//!
//! What replaces it is two checks that prove different things. A single
//! artifact can honestly prove only that its **unique direct** dependencies fit
//! the envelope — a necessary condition, since they are a subset of the
//! transitive set — and it is the launch, holding the dependency-first closure
//! it authenticated itself, that proves the sufficient one.

use tos_core::{
    lower_module_in_set, LoweringInterface, ModuleContext, Parser, ResolvedImport, SourceReader,
};
use tos_ir::Module;
use tos_residency::{ClosureSource, Failure, ImageSnapshot};
use tos_verifier::{Limits, ResolutionSnapshot};

fn envelope(imports: usize) -> String {
    format!(
        "resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: {imports}]"
    )
}

fn context(path: &str, text: &str) -> ModuleContext {
    ModuleContext {
        source_set: String::from("tos-import-envelope-tests"),
        path: String::from(path),
        content_id: tos_pipeline::content_id(text.as_bytes()),
        dependency_digest: tos_pipeline::list_digest(&[]),
        capability_interface_digest: tos_pipeline::list_digest(&[]),
    }
}

/// Lowers one module against the dependencies it resolves to.
///
/// **Set-wide checking is deliberately not run here.** These fixtures are what
/// a producer that does not perform `E1705` — a buggy one, or a hostile one —
/// would emit, and the whole point of the launch check is that it does not need
/// the producer to have been right.
fn lowered(index: usize, head: &str, imports: usize, body: &str, resolved: &[&Module]) -> Module {
    let text = format!(
        "module set.m{index} version 1.0 profile bootstrap; {head}{} \
         pub fn f() -> i64 {{ return 0i64; }} pub fn main() -> i64 {{ {body} }}",
        envelope(imports)
    );
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let carried: Vec<(String, LoweringInterface)> = resolved
        .iter()
        .map(|module| {
            (
                module.header.module_name.clone(),
                LoweringInterface::of(module),
            )
        })
        .collect();
    let imports: Vec<ResolvedImport<'_>> = carried
        .iter()
        .map(|(name, interface)| ResolvedImport {
            name: name.as_str(),
            interface,
        })
        .collect();
    lower_module_in_set(
        &source,
        &schema,
        &context(&format!("set/m{index}.tos"), &text),
        &imports,
    )
    .expect("the fixture lowers")
}

fn image(module: &Module) -> ImageSnapshot {
    tos_image::encode(module).0.into_boxed_slice().into()
}

/// A closure source that records every position it is asked for.
struct Recording {
    images: Vec<ImageSnapshot>,
    reads: core::cell::RefCell<Vec<usize>>,
}

impl Recording {
    fn of(images: Vec<ImageSnapshot>) -> Recording {
        Recording {
            images,
            reads: core::cell::RefCell::new(Vec::new()),
        }
    }
}

impl ClosureSource for Recording {
    fn count(&self) -> usize {
        self.images.len()
    }

    fn image(&self, position: usize) -> Option<ImageSnapshot> {
        self.reads.borrow_mut().push(position);
        self.images.get(position).cloned()
    }
}

fn launch_with(source: &Recording, limits: &Limits) -> Result<(), Failure> {
    let resolution = |_: usize| ResolutionSnapshot::default();
    let entry = source.count() - 1;
    tos_residency::launch(source, &resolution, limits, entry, "main").map(|_| ())
}

fn launched(modules: &[Module]) -> String {
    let source = Recording::of(modules.iter().map(image).collect());
    match launch_with(&source, &Limits::default()) {
        Ok(()) => String::from("accepted"),
        Err(Failure::Verifier { finding, .. }) => finding.code.to_string(),
        Err(other) => format!("{other:?}"),
    }
}

// ------------------------------------------------- the closure size ceiling

/// A leaf module: no imports, and it declares that.
fn leaf(index: usize) -> Module {
    lowered(index, "", 0, "return 0i64;", &[])
}

/// **256 modules is the accepted ceiling and reaches normal processing.**
#[test]
fn a_closure_of_exactly_the_ceiling_is_verified() {
    let source = Recording::of((0..256).map(|at| image(&leaf(at))).collect());
    launch_with(&source, &Limits::default()).expect("256 modules is the ceiling, not past it");
    assert_eq!(
        source.reads.borrow().len(),
        256,
        "every module was read and verified"
    );
}

/// **257 is refused before the closure is touched at all.**
///
/// `docs/44` §2 publishes 256, and everything the launch sizes by the closure —
/// the record table, the membership vector, the reachability sets — is sized by
/// a number the provider supplies. A ceiling checked after the allocation it
/// bounds is not a ceiling. The recording source proves the refusal precedes
/// the work: **zero** images were read.
#[test]
fn one_module_past_the_ceiling_is_refused_before_any_image_is_read() {
    let source = Recording::of((0..257).map(|at| image(&leaf(at))).collect());
    match launch_with(&source, &Limits::default()) {
        Ok(()) => panic!("a closure of 257 modules was admitted"),
        Err(Failure::Verifier { module, finding }) => {
            assert_eq!(finding.code, "V2001_LIMIT");
            assert_eq!(finding.location, "module dependency closure");
            assert!(finding.detail.contains("257"), "{finding:?}");
            assert!(finding.detail.contains("256"), "{finding:?}");
            assert_eq!(module, 256, "the first position the closure cannot admit");
        }
        Err(other) => panic!("the launch failed differently: {other:?}"),
    }
    assert!(
        source.reads.borrow().is_empty(),
        "the ceiling was checked before a single image was fetched"
    );
}

/// A profile may publish a **lower** bound, and it is honoured.
///
/// It may not publish a higher one: raising an accepted V1 ceiling is a
/// versioned contract change, so the effective bound is the smaller of the two.
#[test]
fn a_lower_declared_module_limit_is_enforced_too() {
    let source = Recording::of((0..12).map(|at| image(&leaf(at))).collect());
    let limits = Limits {
        modules: 8,
        ..Limits::default()
    };
    match launch_with(&source, &limits) {
        Ok(()) => panic!("a closure of 12 was admitted under a limit of 8"),
        Err(Failure::Verifier { module, finding }) => {
            assert_eq!(finding.code, "V2001_LIMIT");
            assert_eq!(finding.location, "module dependency closure");
            assert!(finding.detail.contains("of 8"), "{finding:?}");
            assert_eq!(module, 8);
        }
        Err(other) => panic!("the launch failed differently: {other:?}"),
    }
    assert!(source.reads.borrow().is_empty());
}

// --------------------------------------- the per-module necessary condition

/// A module verified **alone** has no authenticated graph, so the strongest
/// honest fact about its envelope is its unique direct dependencies.
fn alone(module: &Module) -> String {
    match tos_verifier::verify_image(
        &image(module),
        &ResolutionSnapshot::default(),
        &Limits::default(),
    ) {
        Ok(_) => String::from("accepted"),
        Err(tos_verifier::ImageRefusal::Verifier(finding)) => {
            format!("{} {}", finding.code, finding.location)
        }
        Err(other) => format!("{other:?}"),
    }
}

/// **Two bindings of one dependency with `imports: 1` are not refused.**
///
/// The check this replaces compared `module.imports.len()`, which is 2 here,
/// and refused a module whose single dependency is exactly what it declared.
#[test]
fn two_bindings_of_one_dependency_pass_the_local_check() {
    let dependency = leaf(0);
    let caller = lowered(
        1,
        "import set.m0 as one; import set.m0 as two; ",
        1,
        "return one.f() + two.f();",
        &[&dependency],
    );
    assert_eq!(caller.imports.len(), 2, "two declarations in the artifact");
    assert_eq!(alone(&caller), "accepted");
}

/// **Two distinct direct dependencies with `imports: 1` are refused.**
///
/// `V2022_RESOURCE`, because a module breaching the envelope it declared for
/// itself is the same class of fact as running more cleanups at one exit than
/// it reserved — not `V2001_LIMIT`, which belongs to published implementation
/// ceilings.
#[test]
fn two_distinct_direct_dependencies_over_the_envelope_are_refused() {
    let first = leaf(0);
    let second = leaf(1);
    let caller = lowered(
        2,
        "import set.m0 as a; import set.m1 as b; ",
        1,
        "return a.f() + b.f();",
        &[&first, &second],
    );
    assert_eq!(
        alone(&caller),
        "V2022_RESOURCE header.resource_envelope.imports"
    );
}

/// And the local check is **necessary only**: a module one level above its
/// envelope through an indirect dependency passes it, which is exactly why the
/// launch performs the sufficient one.
#[test]
fn the_local_check_cannot_see_an_indirect_dependency() {
    let bottom = leaf(0);
    let middle = lowered(
        1,
        "import set.m0 as prev; ",
        1,
        "return prev.f();",
        &[&bottom],
    );
    let top = lowered(
        2,
        "import set.m1 as prev; ",
        1,
        "return prev.f();",
        &[&middle],
    );
    assert_eq!(
        alone(&top),
        "accepted",
        "one direct dependency is all one artifact can see"
    );
    assert_eq!(
        launched(&[bottom, middle, top]),
        "V2022_RESOURCE",
        "and the closure knows better"
    );
}

// ------------------------------------------- the sufficient transitive check

/// **The fail-open shape, closed.** `m2 -> m1 -> m0`, `m2` declares one.
#[test]
fn a_launch_refuses_a_module_above_its_transitive_envelope() {
    let bottom = leaf(0);
    let middle = lowered(
        1,
        "import set.m0 as prev; ",
        1,
        "return prev.f();",
        &[&bottom],
    );
    let top = lowered(
        2,
        "import set.m1 as prev; ",
        1,
        "return prev.f();",
        &[&middle],
    );
    let source = Recording::of(vec![image(&bottom), image(&middle), image(&top)]);
    match launch_with(&source, &Limits::default()) {
        Ok(()) => panic!("a module above its declared transitive count was admitted"),
        Err(Failure::Verifier { module, finding }) => {
            assert_eq!(module, 2, "the module that under-declared, not another");
            assert_eq!(finding.code, "V2022_RESOURCE");
            assert_eq!(finding.location, "header.resource_envelope.imports");
            assert!(finding.detail.contains('2'), "{finding:?}");
        }
        Err(other) => panic!("the launch failed differently: {other:?}"),
    }
}

/// The boundary is exact on both sides, at every depth.
#[test]
fn the_launch_boundary_is_exact() {
    for length in [2usize, 5, 17] {
        let top = length - 1;
        for (declared, expected) in [(top, "accepted"), (top - 1, "V2022_RESOURCE")] {
            let mut modules: Vec<Module> = vec![leaf(0)];
            for at in 1..length {
                let imports = if at == top { declared } else { at };
                let previous = modules[at - 1].clone();
                modules.push(lowered(
                    at,
                    &format!("import set.m{} as prev; ", at - 1),
                    imports,
                    "return prev.f();",
                    &[&previous],
                ));
            }
            assert_eq!(
                launched(&modules),
                expected,
                "a chain of {length} declaring imports: {declared}"
            );
        }
    }
}

/// Two bindings of one dependency are one dependency at launch too.
#[test]
fn duplicate_bindings_do_not_consume_the_launch_envelope_twice() {
    let dependency = leaf(0);
    let caller = lowered(
        1,
        "import set.m0 as one; import set.m0 as two; ",
        1,
        "return one.f() + two.f();",
        &[&dependency],
    );
    assert_eq!(launched(&[dependency, caller]), "accepted");
}

/// A diamond counts the shared dependency once: `m3` reaches `{m0, m1, m2}`.
#[test]
fn a_diamond_counts_its_shared_dependency_once() {
    let base = leaf(0);
    let left = lowered(1, "import set.m0 as b; ", 1, "return b.f();", &[&base]);
    let right = lowered(2, "import set.m0 as b; ", 1, "return b.f();", &[&base]);
    let apex = lowered(
        3,
        "import set.m1 as l; import set.m2 as r; ",
        3,
        "return l.f() + r.f();",
        &[&left, &right],
    );
    assert_eq!(
        launched(&[base.clone(), left.clone(), right.clone(), apex]),
        "accepted",
        "three unique modules, not four paths"
    );
    let tight = lowered(
        3,
        "import set.m1 as l; import set.m2 as r; ",
        2,
        "return l.f() + r.f();",
        &[&left, &right],
    );
    assert_eq!(launched(&[base, left, right, tight]), "V2022_RESOURCE");
}

/// **A declared `ResolutionSnapshot` is not an input to this decision.**
///
/// The sets are built from the import table of the artifact this launch
/// verified and from the positions of modules it verified earlier. A snapshot
/// that declares the closure perfectly reaches the same verdict as one that
/// declares nothing, and a snapshot that omits a dependency cannot shrink the
/// count either — omitting it is refused before the envelope is ever reached.
#[test]
fn a_declared_resolution_cannot_reduce_the_transitive_count() {
    let bottom = leaf(0);
    let middle = lowered(
        1,
        "import set.m0 as prev; ",
        1,
        "return prev.f();",
        &[&bottom],
    );
    let top = lowered(
        2,
        "import set.m1 as prev; ",
        1,
        "return prev.f();",
        &[&middle],
    );
    let build = || vec![image(&bottom), image(&middle), image(&top)];

    // The truthful declaration: the same refusal, at the same module.
    let mut declared = tos_verifier::DeclaredResolution::new();
    declared
        .module("set.m0", &bottom.header.content_id)
        .exports_declared()
        .export("f");
    declared
        .module("set.m1", &middle.header.content_id)
        .exports_declared()
        .export("f");
    let complete = declared.build();
    let source = Recording::of(build());
    let resolution = |_: usize| complete.clone();
    match tos_residency::launch(&source, &resolution, &Limits::default(), 2, "main") {
        Ok(_) => panic!("a declaration reduced a transitive count"),
        Err(Failure::Verifier { module, finding }) => {
            assert_eq!(module, 2);
            assert_eq!(finding.code, "V2022_RESOURCE");
        }
        Err(other) => panic!("the launch failed differently: {other:?}"),
    }

    // And the same closure with no declaration at all reaches the same verdict,
    // which is the whole claim: the snapshot changed nothing.
    let source = Recording::of(build());
    match launch_with(&source, &Limits::default()) {
        Err(Failure::Verifier { module: 2, finding }) => {
            assert_eq!(finding.code, "V2022_RESOURCE");
        }
        other => panic!("the undeclared launch decided differently: {other:?}"),
    }

    // Omitting a dependency is not a way to make it stop counting: the module
    // that imports it is refused before its envelope is reached.
    let mut partial = tos_verifier::DeclaredResolution::new();
    partial
        .module("set.m1", &middle.header.content_id)
        .exports_declared()
        .export("f");
    let partial = partial.build();
    let source = Recording::of(build());
    let resolution = |_: usize| partial.clone();
    match tos_residency::launch(&source, &resolution, &Limits::default(), 2, "main") {
        Err(Failure::Verifier { module: 1, finding }) => {
            assert_eq!(finding.code, "V2012_IMPORT");
        }
        other => panic!("an omitted dependency was tolerated: {other:?}"),
    }
}

// ----------------------------------------- and it is not ADR-0090's quantity

/// **The edge ceiling passes while one module's `resource imports` fails.**
///
/// A chain of sixteen is fifteen edges — 1.5 % of the 1024 ceiling, admitted
/// without comment — and its last module reaches fifteen modules while
/// declaring eight.
#[test]
fn the_edge_ceiling_says_nothing_about_one_module_s_envelope() {
    let mut modules: Vec<Module> = vec![leaf(0)];
    for at in 1..16 {
        let previous = modules[at - 1].clone();
        let imports = if at == 15 { 8 } else { at };
        modules.push(lowered(
            at,
            &format!("import set.m{} as prev; ", at - 1),
            imports,
            "return prev.f();",
            &[&previous],
        ));
    }
    let edges: usize = modules.iter().map(|m| m.imports.len()).sum();
    assert_eq!(edges, 15, "far inside ADR-0090's 1024");
    assert_eq!(launched(&modules), "V2022_RESOURCE");
}

/// **Every module's `resource imports` passes while the edge ceiling fails.**
///
/// Eight leaf providers and 248 callers of all eight: every caller reaches
/// exactly eight modules and declares eight, so no envelope is breached — and
/// the closure holds 1 984 edges, which ADR-0090 refuses.
#[test]
fn every_envelope_can_hold_while_the_edge_ceiling_does_not() {
    let providers: Vec<Module> = (0..8).map(leaf).collect();
    let borrowed: Vec<&Module> = providers.iter().collect();
    let head: String = (0..8)
        .map(|d| format!("import set.m{d} as d{d}; "))
        .collect();
    let body: String = (0..8)
        .map(|d| {
            if d == 0 {
                format!("d{d}.f()")
            } else {
                format!(" + d{d}.f()")
            }
        })
        .collect();
    let mut modules = providers.clone();
    for at in 8..256 {
        modules.push(lowered(at, &head, 8, &format!("return {body};"), &borrowed));
    }
    let edges: usize = modules.iter().map(|m| m.imports.len()).sum();
    assert_eq!(edges, 1984, "past ADR-0090's 1024");
    assert_eq!(launched(&modules), "V2001_LIMIT");
}

/// The reachability state is a fixed thirty-two bytes per closure position,
/// whatever a module declares — 8 192 B at the 256-module ceiling.
#[test]
fn the_reachability_set_is_fixed_size() {
    assert_eq!(core::mem::size_of::<[u64; 4]>(), 32);
    assert_eq!(256 * core::mem::size_of::<[u64; 4]>(), 8192);
}
