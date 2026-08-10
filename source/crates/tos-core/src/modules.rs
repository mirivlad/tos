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
//! `E1605_AMBIGUOUS_IMPORT` arises when one name resolves under more than one
//! declared module root. A root list is compilation-driver configuration that
//! this API does not yet take, so that condition is not reported here rather
//! than approximated from a single root.

use std::collections::{BTreeMap, BTreeSet};
use std::string::{String, ToString};
use std::vec::Vec;

use crate::parser::Schema;
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
    for (index, module) in modules.iter().enumerate() {
        by_name.insert(module.declared_name(), index);
    }

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
            let target = import
                .path()
                .iter()
                .map(|segment| segment.text(module.source))
                .collect::<Vec<_>>()
                .join(".");
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

    diagnostics.extend(find_cycles(modules, &by_name));
    diagnostics
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
