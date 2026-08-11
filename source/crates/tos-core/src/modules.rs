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
//! `E1605_AMBIGUOUS_IMPORT` is reported when the declared source set contains
//! the same module name more than once. Resolution then has more than one
//! candidate and nothing in the set says which one wins, so an import of that
//! name is ambiguous rather than silently decided.
//!
//! **Boundary.** docs/42 section 1 also gives resolution "a declared ordered
//! list of module roots", and whether a name matching under several roots is
//! ambiguous or resolved by that order is not stated: the order makes it
//! decidable, yet the code exists for a condition the order would prevent.
//! That case is left to the compilation-driver configuration and is not
//! approximated here.
//!
//! A capability import names an interface contract, not a module of this source
//! set (docs/42 section 4), so it is not resolved against the set.

use std::collections::{BTreeMap, BTreeSet};
use std::string::{String, ToString};
use std::vec::Vec;

use crate::parser::{ImportKind, Schema};
use crate::{Checker, Diagnostic, ModuleIdentity, Severity, SourceUnit, Stage};

/// One module of a source set: its canonical repository path and parsed tree.
pub struct ModuleEntry<'source> {
    path: String,
    source: &'source SourceUnit,
    schema: &'source Schema,
}

impl<'source> ModuleEntry<'source> {
    /// Registers a module at its canonical repository path, relative to the
    /// declared module root and using `/` separators.
    pub fn new(path: &str, source: &'source SourceUnit, schema: &'source Schema) -> Self {
        ModuleEntry {
            path: path.to_string(),
            source,
            schema,
        }
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
        let content_id = std::format!(
            "sha256:{}",
            core::str::from_utf8(&hex).expect("hex output is ASCII")
        );
        ModuleIdentity::new(self.declared_name(), self.path.clone(), content_id)
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

/// Checks module identity and the import graph of a source set.
///
/// Per-module checks stay with `Checker::check`; this adds only what needs more
/// than one module to see.
pub fn check_module_set(modules: &[ModuleEntry]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut by_name: BTreeMap<String, usize> = BTreeMap::new();
    let mut duplicates: BTreeMap<String, usize> = BTreeMap::new();
    for (index, module) in modules.iter().enumerate() {
        let name = module.declared_name();
        *duplicates.entry(name.clone()).or_insert(0) += 1;
        by_name.entry(name).or_insert(index);
    }
    let ambiguous: BTreeSet<&String> = duplicates
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(name, _)| name)
        .collect();

    for module in modules {
        let name = module.declared_name();
        let expected = std::format!("{}.tos", name.replace('.', "/"));
        if module.path != expected {
            diagnostics.push(
                Diagnostic::new(
                    "E1603_MODULE_PATH_MISMATCH",
                    Severity::Error,
                    Stage::Type,
                    module.schema.outline().prefix().header().span(),
                    module.source,
                )
                .with_module(module.identity())
                .with_field("declared", name)
                .with_field("path", module.path.clone())
                .with_field("expected", expected),
            );
        }
    }

    for module in modules {
        for import in module.schema.outline().prefix().imports() {
            // A capability import names an interface contract rather than a
            // module of this source set, so it is not resolved here.
            if import.kind() == ImportKind::Capability {
                continue;
            }
            let target = import
                .path()
                .iter()
                .map(|segment| segment.text(module.source))
                .collect::<Vec<_>>()
                .join(".");
            if ambiguous.contains(&target) {
                diagnostics.push(
                    Diagnostic::new(
                        "E1605_AMBIGUOUS_IMPORT",
                        Severity::Error,
                        Stage::Type,
                        import.span(),
                        module.source,
                    )
                    .with_module(module.identity())
                    .with_field("import", target.clone())
                    .with_field("importer", module.declared_name())
                    .with_field("candidates", duplicates[&target]),
                );
                continue;
            }
            if by_name.contains_key(&target) {
                continue;
            }
            diagnostics.push(
                Diagnostic::new(
                    "E1604_IMPORT_NOT_FOUND",
                    Severity::Error,
                    Stage::Type,
                    import.span(),
                    module.source,
                )
                .with_module(module.identity())
                .with_field("import", target)
                .with_field("importer", module.declared_name()),
            );
        }
    }

    check_qualified_types(modules, &by_name, &mut diagnostics);
    diagnostics.extend(find_cycles(modules, &by_name));
    diagnostics
}

/// Resolves every qualified type name against the module its binding names.
///
/// A single module cannot see another module's type table, so the per-module
/// slice accepts any qualified name whose binding is an import. Here the target
/// module is known: a name it does not declare is `E1203_UNKNOWN_TYPE_NAME`
/// (ADR-0034). A binding whose import itself does not resolve is already
/// `E1604_IMPORT_NOT_FOUND` and is not reported twice.
fn check_qualified_types(
    modules: &[ModuleEntry],
    by_name: &BTreeMap<String, usize>,
    out: &mut Vec<Diagnostic>,
) {
    for module in modules {
        let mut targets: BTreeMap<&str, usize> = BTreeMap::new();
        for import in module.schema.outline().prefix().imports() {
            let path = import
                .path()
                .iter()
                .map(|segment| segment.text(module.source))
                .collect::<Vec<_>>()
                .join(".");
            if let Some(&index) = by_name.get(&path) {
                targets.insert(import.binding().text(module.source), index);
            }
        }
        for (binding, name, span) in crate::types::qualified_type_uses(module.source, module.schema)
        {
            let Some(&index) = targets.get(binding) else {
                continue;
            };
            let target = &modules[index];
            let declared = crate::types::declared_type_names(target.source, target.schema);
            if declared.contains(name) {
                continue;
            }
            out.push(
                crate::types::unknown_qualified_type(
                    module.source,
                    span,
                    span.text(module.source).to_string(),
                )
                .with_module(module.identity())
                .with_field("module", target.declared_name()),
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
fn find_cycles(modules: &[ModuleEntry], by_name: &BTreeMap<String, usize>) -> Vec<Diagnostic> {
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
    modules: &[ModuleEntry],
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
            .map(|&member| modules[member].declared_name())
            .collect();
        let closed = std::format!("{} -> {}", names.join(" -> "), names[0]);
        out.push(
            Diagnostic::new(
                "E1606_IMPORT_CYCLE",
                Severity::Error,
                Stage::Type,
                module.schema.outline().prefix().header().span(),
                module.source,
            )
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
    for import in module.schema.outline().prefix().imports() {
        let target = import
            .path()
            .iter()
            .map(|segment| segment.text(module.source))
            .collect::<Vec<_>>()
            .join(".");
        // A missing import is E1604; it contributes no edge.
        if let Some(&next) = by_name.get(&target) {
            visit(next, modules, by_name, path, settled, reported, out);
        }
    }
    path.pop();
    settled.insert(index);
}
