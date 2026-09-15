// SPDX-License-Identifier: GPL-3.0-or-later
//! What lowering a module of a source set adds over lowering it alone.
//!
//! Two facts a single-module lowering cannot know and must not invent: the
//! identity of the module an import resolved to, and the type of a call across
//! the boundary. Both are asserted here against the emitted `tos-ir/v1` rather
//! than through an outcome, because a wrong type that happens to run is still
//! the frontend telling the verifier something untrue.

use tos_core::{
    lower_module, lower_module_in_set, Checker, LoweringInterface, ModuleContext, Parser,
    ResolvedImport, SourceReader,
};
use tos_ir::{CallTarget, Module, Op, TypeDef};

fn module_text(name: &str, imports: &str, body: &str) -> String {
    format!(
        "module {name} version 1.0 profile bootstrap; {imports} \
         resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 4] {body}"
    )
}

fn context(path: &str, content_id: &str) -> ModuleContext {
    ModuleContext {
        source_set: String::from("tos-core-set-tests"),
        path: String::from(path),
        content_id: String::from(content_id),
        dependency_digest: String::from("sha256:test"),
        capability_interface_digest: String::from("sha256:test"),
    }
}

/// Lowers one module alone, and then a second one that imports it.
fn lower_pair(dependency: &str, entry: &str) -> (Module, Module) {
    let dependency_source = SourceReader::read(dependency.as_bytes()).expect("dependency reads");
    let dependency_schema = Parser::parse_schema(&dependency_source)
        .into_accepted()
        .expect("dependency parses");
    assert!(
        !Checker::check(&dependency_source, &dependency_schema)
            .iter()
            .any(|entry| entry.severity() == tos_core::Severity::Error),
        "dependency must check"
    );
    let dependency_module = lower_module(
        &dependency_source,
        &dependency_schema,
        &context("system/lib/math.tos", "sha256:dependency"),
    )
    .expect("dependency lowers");

    let entry_source = SourceReader::read(entry.as_bytes()).expect("entry reads");
    let entry_schema = Parser::parse_schema(&entry_source)
        .into_accepted()
        .expect("entry parses");
    assert!(
        !Checker::check(&entry_source, &entry_schema)
            .iter()
            .any(|item| item.severity() == tos_core::Severity::Error),
        "entry must check"
    );
    let entry_module = lower_module_in_set(
        &entry_source,
        &entry_schema,
        &context("system/boot/init.tos", "sha256:entry"),
        &[ResolvedImport {
            name: "system.lib.math",
            interface: &LoweringInterface::of(&dependency_module),
        }],
    )
    .expect("entry lowers");

    (dependency_module, entry_module)
}

const DEPENDENCY: &str = "pub fn double(value: i32) -> i32 { return value * 2i32; }";

#[test]
fn an_import_carries_the_resolved_module_content_id() {
    let (dependency, entry) = lower_pair(
        &module_text("system.lib.math", "", DEPENDENCY),
        &module_text(
            "system.boot.init",
            "import system.lib.math as math;",
            "pub fn main() -> i32 { return 1i32; }",
        ),
    );
    let import = entry
        .imports
        .iter()
        .find(|import| import.module_name == "system.lib.math")
        .expect("the import is in the IR");
    assert_eq!(import.module_content_id, dependency.header.content_id);
    assert!(!import.module_content_id.is_empty());
}

/// Without the dependency the same lowering must not guess. An empty identity
/// says "unresolved"; a filled-in one would say "resolved to that".
#[test]
fn an_unresolved_import_carries_no_identity_rather_than_a_plausible_one() {
    let text = module_text(
        "system.boot.init",
        "import system.lib.math as math;",
        "pub fn main() -> i32 { return 1i32; }",
    );
    let source = SourceReader::read(text.as_bytes()).expect("reads");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("parses");
    let module = lower_module(
        &source,
        &schema,
        &context("system/boot/init.tos", "sha256:e"),
    )
    .expect("lowers");
    assert_eq!(module.imports[0].module_content_id, "");
}

/// The call's type is the callee's declared result, re-interned into this
/// module's own type table.
#[test]
fn a_cross_module_call_has_the_callee_declared_result_type() {
    let (_, entry) = lower_pair(
        &module_text("system.lib.math", "", DEPENDENCY),
        &module_text(
            "system.boot.init",
            "import system.lib.math as math;",
            "pub fn main() -> i32 { return math.double(21i32); }",
        ),
    );

    let mut found = false;
    for function in &entry.functions {
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Op::Call {
                    target: CallTarget::Imported { name, .. },
                    ..
                } = &instruction.op
                {
                    assert_eq!(name, "double");
                    assert_eq!(
                        entry.types.get(instruction.ty),
                        Some(&TypeDef::Int(tos_ir::IntKind::I32)),
                        "a cross-module call must carry the callee's result type"
                    );
                    found = true;
                }
            }
        }
    }
    assert!(found, "the module must contain a cross-module call");
}

/// A type from another module keeps its nominal identity when it crosses: the
/// content id of the module that declared it travels with it, so two imports of
/// the same type are the same type here.
#[test]
fn a_nominal_result_type_keeps_the_identity_of_the_module_that_declared_it() {
    let (dependency, entry) = lower_pair(
        &module_text(
            "system.lib.math",
            "",
            "pub record Pair [pub left: i32, pub right: i32] \
             pub fn origin() -> Pair { return Pair(left: 0i32, right: 0i32); }",
        ),
        &module_text(
            "system.boot.init",
            "import system.lib.math as math;",
            "pub fn main() -> i32 { return 1i32; } \
             pub fn take() -> unit { math.origin(); }",
        ),
    );

    let declared = dependency
        .types
        .iter()
        .find_map(|definition| match definition {
            TypeDef::Nominal {
                module_content_id,
                export_name,
                ..
            } if export_name == "Pair" => Some(module_content_id.clone()),
            _ => None,
        })
        .expect("the dependency declares the record");
    let adopted = entry
        .types
        .iter()
        .find_map(|definition| match definition {
            TypeDef::Nominal {
                module_content_id,
                export_name,
                ..
            } if export_name == "Pair" => Some(module_content_id.clone()),
            _ => None,
        })
        .expect("the entry adopted the record type");
    assert_eq!(adopted, declared);
}

/// The resolved closure's direct dependency edges are bounded (ADR-0090).
///
/// **An edge is one unique `(caller, resolved dependency)` pair.** It is not an
/// import declaration and not `docs/41` §6's `resource imports`, which bounds
/// transitive module dependencies. The ceiling exists because the number of
/// authenticated dependency reopens a launch performs is exactly this number,
/// and every other accepted ceiling left a conforming closure able to demand
/// tens of thousands of them.
mod import_edges {
    use tos_core::{check_source_set, ModuleEntry, Parser, SourceReader, MAX_IMPORT_EDGES};

    const ENVELOPE: &str = "resource [fuel: 1000, stack: 64KiB, allocation: 4KiB, tasks: 1, \
         workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 255]";

    /// A set whose resolved closure has exactly `target` unique edges.
    ///
    /// Each module imports up to eight predecessors, and the last one that
    /// contributes takes only as many as the target still needs — so the number
    /// is exact rather than "the nearest a formula reaches".
    fn texts_with_edges(target: usize) -> (Vec<String>, usize) {
        let mut texts: Vec<String> = Vec::new();
        let mut edges = 0usize;
        let mut index = 0usize;
        while edges < target || texts.is_empty() {
            let want = core::cmp::min(core::cmp::min(index, 8), target - edges);
            let head: String = (0..index)
                .rev()
                .take(want)
                .map(|d| format!("import set.e{d} as d{d}; "))
                .collect();
            texts.push(format!(
                "module set.e{index} version 1.0 profile bootstrap; {head}{ENVELOPE} \
                 pub fn f() -> i32 {{ return 0i32; }}"
            ));
            edges += want;
            index += 1;
        }
        (texts, edges)
    }

    fn first_code_of(texts: &[String]) -> Option<String> {
        let read: Vec<_> = texts
            .iter()
            .map(|text| SourceReader::read(text.as_bytes()).expect("transport-valid"))
            .collect();
        let schemas: Vec<_> = read
            .iter()
            .map(|source| {
                Parser::parse_schema(source)
                    .into_accepted()
                    .expect("the fixture parses")
            })
            .collect();
        let paths: Vec<String> = (0..texts.len())
            .map(|index| format!("set/e{index}.tos"))
            .collect();
        let entries: Vec<ModuleEntry<'_>> = (0..texts.len())
            .map(|at| ModuleEntry::new(&paths[at], &read[at], &schemas[at]))
            .collect();
        check_source_set(&entries)
            .into_iter()
            .next()
            .map(|diagnostic| diagnostic.code().to_string())
    }

    #[test]
    fn a_closure_at_the_ceiling_is_accepted() {
        let (texts, edges) = texts_with_edges(MAX_IMPORT_EDGES);
        assert_eq!(edges, 1024);
        assert_eq!(first_code_of(&texts), None);
    }

    #[test]
    fn one_edge_past_the_ceiling_is_refused() {
        let (texts, edges) = texts_with_edges(MAX_IMPORT_EDGES + 1);
        assert_eq!(edges, 1025);
        assert_eq!(
            first_code_of(&texts).as_deref(),
            Some("E1609_IMPORT_EDGE_LIMIT")
        );
    }

    /// Two bindings of one module are one relationship and cost one edge, so a
    /// set that would be over the ceiling counted by declarations is inside it
    /// counted by edges.
    #[test]
    fn duplicate_bindings_of_one_module_cost_one_edge() {
        let mut texts = vec![format!(
            "module set.e0 version 1.0 profile bootstrap; {ENVELOPE} \
             pub fn f() -> i32 {{ return 0i32; }}"
        )];
        let bindings: String = (0..64)
            .map(|at| format!("import set.e0 as d{at}; "))
            .collect();
        texts.push(format!(
            "module set.e1 version 1.0 profile bootstrap; {bindings}{ENVELOPE} \
             pub fn g() -> i32 {{ return 0i32; }}"
        ));
        let read: Vec<_> = texts
            .iter()
            .map(|text| SourceReader::read(text.as_bytes()).expect("transport-valid"))
            .collect();
        let schemas: Vec<_> = read
            .iter()
            .map(|source| {
                Parser::parse_schema(source)
                    .into_accepted()
                    .expect("the fixture parses")
            })
            .collect();
        let paths = ["set/e0.tos", "set/e1.tos"];
        let entries: Vec<ModuleEntry<'_>> = (0..2)
            .map(|at| ModuleEntry::new(paths[at], &read[at], &schemas[at]))
            .collect();
        // Sixty-four declarations, one relationship: nothing about the edge
        // budget is consumed by naming the same module again.
        assert!(check_source_set(&entries).is_empty());
    }
}

/// A module declares the most **transitive** module dependencies it may have
/// (`docs/41` §6, ADR-0091).
///
/// Not its direct declarations. The count is of unique resolved module
/// identities reachable from the module, direct and indirect together, the
/// module itself excluded, and two bindings of one dependency counted once.
/// Until this slice nothing compared the declaration with that quantity at all:
/// the only check in the tree compared it with `module.imports.len()`, which
/// admitted a module sitting above any number of indirect dependencies and
/// refused one that spelled a single dependency twice.
mod import_envelopes {
    use tos_core::{check_source_set, Diagnostic, ModuleEntry, Parser, SourceReader};

    fn envelope(imports: usize) -> String {
        format!(
            "resource [fuel: 1000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
             sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: {imports}]"
        )
    }

    /// Checks a set whose modules are `set.m0 …`, in the order given.
    fn diagnostics_of(texts: &[String]) -> Vec<Diagnostic> {
        let read: Vec<_> = texts
            .iter()
            .map(|text| SourceReader::read(text.as_bytes()).expect("transport-valid"))
            .collect();
        let schemas: Vec<_> = read
            .iter()
            .map(|source| {
                Parser::parse_schema(source)
                    .into_accepted()
                    .expect("the fixture parses")
            })
            .collect();
        let paths: Vec<String> = (0..texts.len())
            .map(|index| format!("set/m{index}.tos"))
            .collect();
        let entries: Vec<ModuleEntry<'_>> = (0..texts.len())
            .map(|at| ModuleEntry::new(&paths[at], &read[at], &schemas[at]))
            .collect();
        check_source_set(&entries)
    }

    /// A chain `m0 <- m1 <- … <- m{len-1}`, every module declaring `imports`.
    fn chain(len: usize, imports: usize) -> Vec<String> {
        (0..len)
            .map(|at| {
                let head = if at == 0 {
                    String::new()
                } else {
                    format!("import set.m{} as prev; ", at - 1)
                };
                let body = if at == 0 {
                    "pub fn f() -> i32 { return 0i32; }"
                } else {
                    "pub fn f() -> i32 { return prev.f() + 1i32; }"
                };
                format!(
                    "module set.m{at} version 1.0 profile bootstrap; {head}{} {body}",
                    envelope(imports)
                )
            })
            .collect()
    }

    /// **The fail-open shape.** `m2 -> m1 -> m0`, and `m2` declares one.
    ///
    /// Two modules are reachable from `m2`, so the declaration is short by one
    /// and the module is refused. Before this slice the whole chain ran: the
    /// only check in the path saw one direct import and was satisfied.
    #[test]
    fn a_module_above_its_declared_transitive_count_is_refused() {
        let mut texts = chain(3, 8);
        // Only the top of the chain under-declares, so exactly one module is
        // named and the finding cannot be a side effect of the others.
        texts[2] = texts[2].replace("imports: 8", "imports: 1");
        let diagnostics = diagnostics_of(&texts);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let finding = &diagnostics[0];
        assert_eq!(finding.code(), "E1705_IMPORT_ENVELOPE_EXCEEDED");
        assert_eq!(finding.field("declared"), Some("1"));
        assert_eq!(finding.field("actual"), Some("2"));
    }

    /// The indirect dependency is what the declaration was short of, so a chain
    /// one longer is short by one more.
    #[test]
    fn the_count_reaches_through_the_whole_chain() {
        let mut texts = chain(4, 8);
        texts[3] = texts[3].replace("imports: 8", "imports: 1");
        let diagnostics = diagnostics_of(&texts);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].field("actual"), Some("3"));
    }

    /// **The duplicate-binding shape.** Two declarations, one dependency.
    ///
    /// `imports: 1` is the truth about this module and it is accepted. The
    /// check in the tree before this slice refused it, because it counted the
    /// declarations rather than the modules they resolve to.
    #[test]
    fn two_bindings_of_one_dependency_are_one_dependency() {
        let texts = vec![
            format!(
                "module set.m0 version 1.0 profile bootstrap; {} \
                 pub fn f() -> i32 {{ return 0i32; }}",
                envelope(0)
            ),
            format!(
                "module set.m1 version 1.0 profile bootstrap; \
                 import set.m0 as one; import set.m0 as two; {} \
                 pub fn g() -> i32 {{ return one.f() + two.f(); }}",
                envelope(1)
            ),
        ];
        assert!(
            diagnostics_of(&texts).is_empty(),
            "{:?}",
            diagnostics_of(&texts)
        );
    }

    /// **The exact boundary.** `N` transitive dependencies with `imports: N`
    /// accepts, and `imports: N - 1` does not.
    #[test]
    fn the_boundary_is_exact() {
        for length in [2usize, 5, 17] {
            let top = length - 1;
            let mut accepted = chain(length, 255);
            accepted[top] = accepted[top].replace("imports: 255", &format!("imports: {top}"));
            assert!(
                diagnostics_of(&accepted).is_empty(),
                "a chain of {length} with imports: {top} is exact"
            );

            let mut refused = chain(length, 255);
            refused[top] = refused[top].replace("imports: 255", &format!("imports: {}", top - 1));
            let diagnostics = diagnostics_of(&refused);
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert_eq!(diagnostics[0].code(), "E1705_IMPORT_ENVELOPE_EXCEEDED");
            assert_eq!(
                diagnostics[0].field("actual").map(str::to_string),
                Some(top.to_string())
            );
        }
    }

    /// A capability import is a contract, not a module of this source set
    /// (`docs/42` §4), so it is not a dependency and does not consume the
    /// envelope.
    #[test]
    fn a_capability_import_is_not_a_module_dependency() {
        let texts = vec![format!(
            "module set.m0 version 1.0 profile bootstrap; \
             import capability system.ipc.Endpoint as endpoint; {} \
             pub fn f() -> i32 {{ return 0i32; }}",
            envelope(0)
        )];
        assert!(
            diagnostics_of(&texts).is_empty(),
            "{:?}",
            diagnostics_of(&texts)
        );
    }

    /// An unresolved graph has no exact reachable set, so the graph failure owns
    /// the refusal and this check does not add a second sentence about a count
    /// it could not compute.
    #[test]
    fn an_unresolvable_import_owns_the_refusal_alone() {
        let texts = vec![format!(
            "module set.m0 version 1.0 profile bootstrap; \
             import set.absent as gone; {} \
             pub fn f() -> i32 {{ return 0i32; }}",
            envelope(0)
        )];
        let codes: Vec<&str> = diagnostics_of(&texts)
            .iter()
            .map(|diagnostic| diagnostic.code())
            .collect();
        assert_eq!(codes, vec!["E1604_IMPORT_NOT_FOUND"]);
    }
}
