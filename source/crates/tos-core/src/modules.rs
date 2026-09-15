// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic module resolution over a source set (docs/42 section 1).
//!
//! A module name `a.b.c` maps to the canonical repository path `a/b/c.tos`
//! relative to a declared module root, and a source whose path disagrees with
//! its header is `E1603_MODULE_PATH_MISMATCH`. An import that names no module
//! in the source set is `E1604_IMPORT_NOT_FOUND`, and an import graph cycle is
//! `E1606_IMPORT_CYCLE` carrying the ordered cycle path.
//!
//! Resolution reads only the source set it is given. It never consults an
//! ambient directory, the host filesystem, the network, the clock or the
//! environment, and an import never triggers a fetch.
//!
//! ADR-0038 settles how a name with several candidates resolves. The declared
//! module roots are searched in order and the candidate in the earliest root
//! wins, which is what layering a private root over a shared one means. That
//! order settles roots and only roots: `E1605_AMBIGUOUS_IMPORT` covers the two
//! cases it says nothing about — the same name declared more than once inside
//! one root, and a name offered by more than one reachable declared dependency
//! source set, which nothing orders against each other.
//!
//! A capability import names an interface contract, not a module of this source
//! set (docs/42 section 4), so it is not resolved against the set.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::parser::Schema;
use crate::summary::ModuleSummary;
use crate::{Checker, Diagnostic, ModuleIdentity, Severity, SourceUnit, Stage};

/// One module of a source set: its canonical repository path and parsed tree.
///
/// A module also records where it came from, because ADR-0038 resolves a name
/// with several candidates from exactly that: which declared root holds it, and
/// which declared dependency source set provides it.
pub struct ModuleEntry<'source> {
    path: String,
    /// Position in the declared ordered root list; earlier roots win.
    root: usize,
    /// The declared dependency source set this module came from, or `None` for
    /// the source set being compiled.
    dependency_set: Option<String>,
    source: &'source SourceUnit,
    schema: &'source Schema,
}

impl<'source> ModuleEntry<'source> {
    /// Registers a module at its canonical repository path, relative to the
    /// declared module root and using `/` separators.
    pub fn new(path: &str, source: &'source SourceUnit, schema: &'source Schema) -> Self {
        ModuleEntry {
            path: path.to_string(),
            root: 0,
            dependency_set: None,
            source,
            schema,
        }
    }

    /// Registers a module in a declared root of the ordered root list.
    ///
    /// The index is the root's position: a lower index shadows a higher one.
    pub fn in_root(
        root: usize,
        path: &str,
        source: &'source SourceUnit,
        schema: &'source Schema,
    ) -> Self {
        ModuleEntry {
            path: path.to_string(),
            root,
            dependency_set: None,
            source,
            schema,
        }
    }

    /// Registers a module provided by a declared dependency source set.
    pub fn from_dependency(
        dependency_set: &str,
        root: usize,
        path: &str,
        source: &'source SourceUnit,
        schema: &'source Schema,
    ) -> Self {
        ModuleEntry {
            path: path.to_string(),
            root,
            dependency_set: Some(dependency_set.to_string()),
            source,
            schema,
        }
    }

    /// Which declared root holds this module.
    pub fn root(&self) -> usize {
        self.root
    }

    /// Which declared dependency source set provides it, if not the local one.
    pub fn dependency_set(&self) -> Option<&str> {
        self.dependency_set.as_deref()
    }

    /// The canonical repository path this module was registered at.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The identity every diagnostic from this module carries.
    ///
    /// The content ID is the SHA-256 of the normalized source bytes, so it
    /// names exactly the text the frontend accepted rather than the transport
    /// form it arrived in.
    pub fn identity(&self) -> ModuleIdentity {
        let digest = tos_hash::sha256(self.source.bytes());
        let mut hex = [0u8; 64];
        tos_hash::hex(&digest, &mut hex);
        let content_id = alloc::format!(
            "sha256:{}",
            core::str::from_utf8(&hex).expect("hex output is ASCII")
        );
        ModuleIdentity::new(self.declared_name(), self.path.clone(), content_id)
    }

    /// The compact derived view set-wide resolution reads.
    ///
    /// Nothing the summary holds borrows the tree, so a caller may drop the
    /// parse tree as soon as this returns — which is the point.
    pub fn summarize(&self) -> ModuleSummary {
        ModuleSummary::derive(
            &self.path,
            self.root,
            self.dependency_set.as_deref(),
            self.source,
            self.schema,
        )
    }

    /// The same, without this module's qualified uses (see
    /// [`ModuleSummary::derive_membership`]).
    pub fn summarize_membership(&self) -> ModuleSummary {
        ModuleSummary::derive_membership(
            &self.path,
            self.root,
            self.dependency_set.as_deref(),
            self.source,
            self.schema,
        )
    }

    /// The qualified type names this module writes, without the rest of a
    /// summary.
    ///
    /// For a second pass: a caller that already has the set's type surfaces and
    /// wants only this module's uses, to resolve them and drop them, rather than
    /// holding every module's uses until a set-wide check runs.
    pub fn qualified_uses(&self) -> Vec<crate::QualifiedUse> {
        crate::types::qualified_type_uses(self.source, self.schema)
            .into_iter()
            .map(|(binding, name, span)| crate::QualifiedUse {
                binding: binding.to_string(),
                name: name.to_string(),
                spelled: span.text(self.source).to_string(),
                at: crate::Located::of(self.source, span),
            })
            .collect()
    }

    /// Runs the per-module checks with this module's identity attached.
    pub fn check(&self) -> Vec<Diagnostic> {
        let identity = self.identity();
        Checker::check(self.source, self.schema)
            .into_iter()
            .map(|diagnostic| diagnostic.with_module(identity.clone()))
            .collect()
    }

    /// The declared module name, dot-separated.
    fn declared_name(&self) -> String {
        self.schema
            .outline()
            .prefix()
            .header()
            .name()
            .iter()
            .map(|segment| segment.text(self.source))
            .collect::<Vec<_>>()
            .join(".")
    }
}

/// Why a name has candidates nothing orders (ADR-0038).
struct Collision {
    /// `root` when one root declares the name twice, `dependency` when several
    /// declared dependency source sets provide it.
    kind: &'static str,
    candidates: usize,
    /// The identities that collided, so the configuration mistake is nameable.
    identities: Vec<String>,
}

/// What every declared module name resolves to, and what does not resolve.
pub struct Resolution {
    resolved: BTreeMap<String, usize>,
    ambiguous: BTreeMap<String, Collision>,
}

/// Resolves every declared name under the ADR-0038 rule.
///
/// The declared roots are searched in order, so the candidate in the earliest
/// root wins and layering a private root over a shared one works. That order
/// settles roots and only roots: a name declared twice inside one root has
/// nothing ordering it, and several declared dependency source sets offering
/// one name have nothing ordering them either.
/// Resolves a set's declared names once, for a caller running the set-wide
/// phases itself.
///
/// The phases each need it, and each computing its own would make a two-pass
/// checker quadratic in the set for no answer it did not already have.
pub fn resolve_set(modules: &[ModuleSummary]) -> Resolution {
    resolve_names(modules)
}

fn resolve_names(modules: &[ModuleSummary]) -> Resolution {
    let mut candidates: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, module) in modules.iter().enumerate() {
        candidates
            .entry(module.name.clone())
            .or_default()
            .push(index);
    }

    let mut resolved = BTreeMap::new();
    let mut ambiguous = BTreeMap::new();
    for (name, found) in candidates {
        // Case 2: more than one reachable declared dependency source set
        // provides the name. Nothing orders dependencies against each other.
        let mut sets: BTreeSet<&str> = BTreeSet::new();
        for index in &found {
            sets.insert(
                modules[*index]
                    .dependency_set
                    .as_deref()
                    .unwrap_or("<local>"),
            );
        }
        if sets.len() > 1 {
            ambiguous.insert(
                name,
                Collision {
                    kind: "dependency",
                    candidates: found.len(),
                    identities: sets.iter().map(|set| set.to_string()).collect(),
                },
            );
            continue;
        }
        // Case 1: one root declares the name more than once, so the declared
        // order says nothing about which of them is meant.
        let mut by_root: BTreeMap<usize, usize> = BTreeMap::new();
        let mut repeated: Option<usize> = None;
        for index in &found {
            let root = modules[*index].root;
            let count = by_root.entry(root).or_insert(0);
            *count += 1;
            if *count > 1 {
                repeated = Some(root);
            }
        }
        if let Some(root) = repeated {
            let count = by_root[&root];
            ambiguous.insert(
                name,
                Collision {
                    kind: "root",
                    candidates: count,
                    identities: alloc::vec![alloc::format!("root {root}")],
                },
            );
            continue;
        }
        // Otherwise the earliest declared root resolves it.
        let winner = found
            .iter()
            .min_by_key(|index| modules[**index].root)
            .copied()
            .expect("a name with candidates has at least one");
        resolved.insert(name, winner);
    }
    Resolution {
        resolved,
        ambiguous,
    }
}

/// Checks module identity and the import graph of a source set.
///
/// Per-module checks stay with `Checker::check`; this adds only what needs more
/// than one module to see.
pub fn check_module_set(modules: &[ModuleEntry]) -> Vec<Diagnostic> {
    let summaries: Vec<ModuleSummary> = modules.iter().map(ModuleEntry::summarize).collect();
    check_module_summaries(&summaries)
}

/// Checks module identity and the import graph of a source set, over summaries.
///
/// This is the whole of set-wide resolution, and it reads a compact derived
/// view of each module rather than its parse tree. A loader can therefore parse
/// one module, summarize it, drop the tree, and hold only summaries while it
/// resolves — which is what keeps a dependency closure's memory bounded by the
/// closure's *interfaces* instead of by its bodies.
///
/// Per-module checks stay with `Checker::check`; this adds only what needs more
/// than one module to see.
pub fn check_module_summaries(modules: &[ModuleSummary]) -> Vec<Diagnostic> {
    let resolution = resolve_names(modules);
    let mut diagnostics = check_module_membership(modules, &resolution);
    let by_name = &resolution.resolved;
    check_qualified_types(modules, by_name, &mut diagnostics);
    diagnostics.extend(find_cycles(modules, by_name));
    // **Last, and only over a graph that resolved.** `resource imports` counts
    // the modules reachable from one module, and a graph with an unresolvable
    // import, an ambiguous one, a cycle, or more edges than the closure ceiling
    // admits has no exact reachable set to count. Those four own the earlier
    // refusal; this one waits for a graph worth measuring (ADR-0091 §3).
    if graph_resolved(&diagnostics) {
        diagnostics.extend(check_import_envelopes(modules, &resolution));
    }
    diagnostics
}

/// Whether the import graph resolved into something with an exact shape.
///
/// **Only the four graph findings gate it.** A path that disagrees with a
/// header, or a qualified name a module does not declare, leaves the graph
/// exactly as resolvable as it was: the modules are the same, the edges are the
/// same, and the reachable set is the same. Suppressing a resource finding for
/// an unrelated one would make a module's declared envelope go unchecked
/// because of something else the same set happened to get wrong.
pub fn graph_resolved(diagnostics: &[Diagnostic]) -> bool {
    !diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code(),
            "E1604_IMPORT_NOT_FOUND"
                | "E1605_AMBIGUOUS_IMPORT"
                | "E1606_IMPORT_CYCLE"
                | "E1609_IMPORT_EDGE_LIMIT"
        )
    })
}

/// Everything set-wide resolution can decide **without a module's type
/// surface**: that a path matches the name it declares, and that every import
/// resolves to exactly one module of the set.
///
/// Split out because it is what a first pass can finish. The qualified-type
/// check needs every module's declared names at once and the cycle search needs
/// the resolved graph, so both come after; what this answers needs only a name,
/// a path and a list of imports, which is what a caller can afford to keep for
/// every module of a closure.
///
/// **The order is the contract.** `check_module_summaries` emits path
/// mismatches, then import failures, then qualified-type failures, then cycles,
/// and a caller assembling the phases itself has to emit them in that order or
/// it is not reporting the same thing.
pub fn check_module_membership(
    modules: &[ModuleSummary],
    resolution: &Resolution,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let by_name = &resolution.resolved;

    for module in modules {
        let expected = alloc::format!("{}.tos", module.name.replace('.', "/"));
        if module.path != expected {
            diagnostics.push(
                located("E1603_MODULE_PATH_MISMATCH", Stage::Type, module.header)
                    .with_module(module.identity())
                    .with_field("declared", module.name.clone())
                    .with_field("path", module.path.clone())
                    .with_field("expected", expected),
            );
        }
    }

    for module in modules {
        for import in module.module_imports() {
            if let Some(collision) = resolution.ambiguous.get(&import.target) {
                diagnostics.push(
                    located("E1605_AMBIGUOUS_IMPORT", Stage::Type, import.at)
                        .with_module(module.identity())
                        .with_field("import", import.target.clone())
                        .with_field("importer", module.name.clone())
                        .with_field("candidates", collision.candidates)
                        .with_field("collision", collision.kind)
                        .with_field("collided", collision.identities.join(", ")),
                );
                continue;
            }
            if by_name.contains_key(&import.target) {
                continue;
            }
            diagnostics.push(
                located("E1604_IMPORT_NOT_FOUND", Stage::Type, import.at)
                    .with_module(module.identity())
                    .with_field("import", import.target.clone())
                    .with_field("importer", module.name.clone()),
            );
        }
    }

    diagnostics.extend(check_import_edges(modules, resolution));
    diagnostics
}

/// The accepted ceiling on direct dependency edges in one resolved closure
/// (`docs/44` §2, ADR-0090).
pub const MAX_IMPORT_EDGES: usize = 1024;

/// Refuses a resolved closure with more direct dependency edges than the
/// accepted ceiling (ADR-0090).
///
/// **An edge is one unique `(caller, resolved dependency)` pair**, not one
/// import declaration. Two bindings of the same module from one caller are one
/// relationship and cost one edge, because they are one relationship to every
/// consumer of the graph: a launch authenticates that dependency once and checks
/// every call site in that caller against the one reopened surface.
///
/// **It is a topology quota, not a resource envelope.** `docs/41` §6's
/// `resource imports` is "maximum transitive module dependencies" — a count of
/// modules reachable from one module — and this is a count of edges in the whole
/// resolved closure. Using one for the other would bound neither.
///
/// The refusal lands on the import declaration whose new unique edge first
/// crosses the ceiling, walking modules in the order the resolver produced them
/// and each module's imports in source order, so the same set always names the
/// same declaration.
///
/// An unresolvable or ambiguous import contributes no edge: `E1604` and `E1605`
/// already own those, and counting an edge that does not exist would refuse a
/// closure for a relationship the source set does not contain.
fn check_import_edges(modules: &[ModuleSummary], resolution: &Resolution) -> Vec<Diagnostic> {
    let mut edges = 0usize;
    for module in modules {
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        for import in module.module_imports() {
            let Some(&dependency) = resolution.resolved.get(&import.target) else {
                continue;
            };
            if !seen.insert(dependency) {
                continue;
            }
            edges += 1;
            if edges > MAX_IMPORT_EDGES {
                return alloc::vec![located("E1609_IMPORT_EDGE_LIMIT", Stage::Type, import.at)
                    .with_module(module.identity())
                    .with_field("limit", MAX_IMPORT_EDGES)
                    .with_field("actual", edges)];
            }
        }
    }
    Vec::new()
}

/// The qualified-type check for **one** module, against the set it belongs to.
///
/// The same rule `check_qualified_types` applies, for a caller that has one
/// module's uses in hand and does not intend to keep them: a second pass over
/// the source can enumerate a module's qualified names, resolve them here, and
/// drop them, instead of every module's uses being held until the set-wide
/// check runs.
///
/// `modules` supplies the type surfaces the names resolve against, and `uses`
/// is this module's own — which is why they are separate arguments: after a
/// two-pass split the second is not in the first.
pub fn check_qualified_types_of(
    module: &ModuleSummary,
    uses: &[crate::QualifiedUse],
    modules: &[ModuleSummary],
    resolution: &Resolution,
    out: &mut Vec<Diagnostic>,
) {
    qualified_types_of(module, uses, modules, &resolution.resolved, out);
}

/// The cycle search, for a caller that runs the phases itself.
pub fn check_module_cycles(modules: &[ModuleSummary], resolution: &Resolution) -> Vec<Diagnostic> {
    find_cycles(modules, &resolution.resolved)
}

/// A diagnostic at a span whose positions were derived when the source was in
/// hand.
fn located(code: &'static str, stage: Stage, at: crate::summary::Located) -> Diagnostic {
    Diagnostic::at(code, Severity::Error, stage, at.span, at.start, at.end)
}

/// Refuses a module whose exact resolved transitive dependency set is larger
/// than its declared `resource imports` (`docs/41` §6, ADR-0091).
///
/// **The count is of modules, not of declarations.** Unique resolved module
/// identities reachable from the module, direct and indirect together, the
/// module itself excluded, and two bindings of one dependency counted once —
/// which is the same definition of a dependency ADR-0090 §2a settled for edges,
/// because the word cannot mean two things one paragraph apart. A
/// capability-interface import names a contract rather than a module of this
/// source set (`docs/42` §4), so it contributes nothing.
///
/// Computed from the **resolved** graph rather than from source spelling: two
/// names that resolve to one module are one dependency however they are
/// written, and a name that resolves to nothing is not a dependency at all.
///
/// The walk is over indices of the resolved set, which resolution has already
/// proved acyclic and bounded at `MAX_IMPORT_EDGES` edges, so it terminates and
/// its cost is bounded by the closure the caller was handed.
pub fn check_import_envelopes(
    modules: &[ModuleSummary],
    resolution: &Resolution,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (index, module) in modules.iter().enumerate() {
        // A module declaring no `imports` key has nothing to breach here;
        // `E1700_RESOURCE_DECLARATION_REQUIRED` already refuses the omission.
        let Some(declared) = module.declared_imports.as_ref() else {
            continue;
        };
        let reached = reachable_from(index, modules, resolution);
        let actual = reached.len() as u128;
        if actual <= declared.value {
            continue;
        }
        diagnostics.push(
            located(
                "E1705_IMPORT_ENVELOPE_EXCEEDED",
                Stage::Resource,
                declared.at,
            )
            .with_module(module.identity())
            .with_field("declared", declared.value)
            .with_field("actual", actual),
        );
    }
    diagnostics
}

/// Every module reachable from `index`, by resolved-set position, excluding
/// `index` itself.
fn reachable_from(
    index: usize,
    modules: &[ModuleSummary],
    resolution: &Resolution,
) -> BTreeSet<usize> {
    let mut reached: BTreeSet<usize> = BTreeSet::new();
    let mut pending: Vec<usize> = alloc::vec![index];
    while let Some(at) = pending.pop() {
        for import in modules[at].module_imports() {
            let Some(&dependency) = resolution.resolved.get(&import.target) else {
                continue;
            };
            // The module is not its own dependency. A cycle would be the only
            // way back to it and `E1606` has already refused one, but the guard
            // is written rather than argued: a set is exact or it is not.
            if dependency == index {
                continue;
            }
            if reached.insert(dependency) {
                pending.push(dependency);
            }
        }
    }
    reached
}

/// Resolves every qualified type name against the module its binding names.
///
/// A single module cannot see another module's type table, so the per-module
/// slice accepts any qualified name whose binding is an import. Here the target
/// module is known: a name it does not declare is `E1203_UNKNOWN_TYPE_NAME`
/// (ADR-0034). A binding whose import itself does not resolve is already
/// `E1604_IMPORT_NOT_FOUND` and is not reported twice.
fn check_qualified_types(
    modules: &[ModuleSummary],
    by_name: &BTreeMap<String, usize>,
    out: &mut Vec<Diagnostic>,
) {
    for module in modules {
        qualified_types_of(module, &module.qualified_uses, modules, by_name, out);
    }
}

/// One module's qualified names, resolved against the set.
fn qualified_types_of(
    module: &ModuleSummary,
    uses: &[crate::QualifiedUse],
    modules: &[ModuleSummary],
    by_name: &BTreeMap<String, usize>,
    out: &mut Vec<Diagnostic>,
) {
    {
        let mut targets: BTreeMap<&str, usize> = BTreeMap::new();
        for import in module.module_imports() {
            if let Some(&index) = by_name.get(&import.target) {
                targets.insert(import.binding.as_str(), index);
            }
        }
        for used in uses {
            let Some(&index) = targets.get(used.binding.as_str()) else {
                continue;
            };
            let target = &modules[index];
            if target.declared_types.contains(&used.name) {
                continue;
            }
            out.push(
                located("E1203_UNKNOWN_TYPE_NAME", Stage::Type, used.at)
                    .with_field("type", used.spelled.clone())
                    .with_module(module.identity())
                    .with_field("module", target.name.clone()),
            );
        }
    }
}

/// Checks every module of a source set, per module and across the set.
///
/// Each diagnostic carries the identity of the module it belongs to, which
/// docs/41 section 7 requires and a single source unit cannot supply.
pub fn check_source_set(modules: &[ModuleEntry]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for module in modules {
        diagnostics.extend(module.check());
    }
    diagnostics.extend(check_module_set(modules));
    diagnostics
}

/// Reports one diagnostic per import cycle, with the cycle path in order.
///
/// The search starts from modules in declared-name order and follows imports in
/// source order, so the reported path is the same on every run.
fn find_cycles(modules: &[ModuleSummary], by_name: &BTreeMap<String, usize>) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut settled: BTreeSet<usize> = BTreeSet::new();
    let mut reported: BTreeSet<Vec<usize>> = BTreeSet::new();

    for &start in by_name.values() {
        let mut path: Vec<usize> = Vec::new();
        visit(
            start,
            modules,
            by_name,
            &mut path,
            &mut settled,
            &mut reported,
            &mut diagnostics,
        );
    }
    diagnostics
}

fn visit(
    index: usize,
    modules: &[ModuleSummary],
    by_name: &BTreeMap<String, usize>,
    path: &mut Vec<usize>,
    settled: &mut BTreeSet<usize>,
    reported: &mut BTreeSet<Vec<usize>>,
    out: &mut Vec<Diagnostic>,
) {
    if let Some(position) = path.iter().position(|&entry| entry == index) {
        let cycle: Vec<usize> = path[position..].to_vec();
        // One cycle is one finding, however many members it is entered from.
        let mut identity = cycle.clone();
        identity.sort_unstable();
        if !reported.insert(identity) {
            return;
        }
        let module = &modules[cycle[0]];
        let names: Vec<String> = cycle
            .iter()
            .map(|&member| modules[member].name.clone())
            .collect();
        let closed = alloc::format!("{} -> {}", names.join(" -> "), names[0]);
        out.push(
            located("E1606_IMPORT_CYCLE", Stage::Type, module.header)
                .with_module(module.identity())
                .with_field("cycle", closed)
                .with_field("members", cycle.len()),
        );
        return;
    }
    if settled.contains(&index) {
        return;
    }
    path.push(index);
    let module = &modules[index];
    for import in module.module_imports() {
        // A missing import is E1604; it contributes no edge.
        if let Some(&next) = by_name.get(&import.target) {
            visit(next, modules, by_name, path, settled, reported, out);
        }
    }
    path.pop();
    settled.insert(index);
}
