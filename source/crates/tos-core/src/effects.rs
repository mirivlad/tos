// SPDX-License-Identifier: GPL-3.0-or-later
//! What a `uses` item means, once it is resolved (ADR-0080).
//!
//! **An effect names an interface, and never a value.** `docs/42` §2 has always
//! said a declaration is not a grant; ADR-0060 fixed `Signature.effects` as
//! interface paths; and until TOS Core 1.1 the frontend nevertheless required
//! every item to be an `import capability` binding of the enclosing module.
//! That second rule was invisible below the frontend — the artifact could not
//! represent it — and it made a runtime-obtained capability unusable, because
//! its interface had no import to name.
//!
//! This module is the one place that turns a written item into the interface it
//! denotes, so that a binding and a directly-named interface become the same
//! kind of thing exactly once, and every consumer downstream sees one model.
//!
//! **What resolution does not do**: request authority, create a binding, imply
//! an instance, or license a capability the call site does not actually hold.
//! An interface effect with no capability value is no authority at all.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

use crate::interfaces;
use crate::parser::EffectRef;
use crate::SourceUnit;

/// What one `uses` item denotes.
///
/// **Two cases, not three**, and the missing one is the point. A binding and a
/// directly-named interface both resolve to `Interface`: they denote the same
/// thing, and ADR-0080 §4 requires that nothing downstream be able to tell them
/// apart. Keeping "was this written as a request?" here would be preserving in
/// the frontend exactly the distinction the artifact was never able to hold —
/// which is the rule that decision removed.
pub(crate) enum Resolved {
    /// The interface this item denotes, however it was written.
    Interface(String),
    /// Neither an import of this module nor a path any accepted schema
    /// declares.
    ///
    /// **Kept rather than dropped**, with the text as written. A module that
    /// declared an effect nothing answers still says so in its artifact, and a
    /// diagnostic can name what it wrote.
    Unresolved(String),
}

impl Resolved {
    /// The interface path this denotes, where it denotes one.
    pub(crate) fn interface(&self) -> Option<&str> {
        match self {
            Resolved::Interface(path) => Some(path),
            Resolved::Unresolved(_) => None,
        }
    }

    /// The path to record in `Signature.effects`.
    ///
    /// The resolved interface, or the written text when nothing resolved — the
    /// behaviour the lowerer has always had, now stated in one place.
    pub(crate) fn recorded(&self) -> &str {
        match self {
            Resolved::Interface(path) => path,
            Resolved::Unresolved(written) => written,
        }
    }
}

/// Resolves one written item against the module's capability imports.
///
/// **The two forms are told apart by shape, not by a keyword**: a binding name
/// is one segment and cannot contain a dot, so a dotted item is unambiguously an
/// interface path and no source valid under TOS Core 1.0 changes meaning.
pub(crate) fn resolve(
    source: &SourceUnit,
    imports: &BTreeMap<String, String>,
    effect: &EffectRef,
) -> Resolved {
    let written = effect.path(source);
    if effect.is_binding() {
        return match imports.get(&written) {
            Some(interface) => Resolved::Interface(interface.clone()),
            None => Resolved::Unresolved(written),
        };
    }
    // A dotted item is an interface, and only if an accepted schema says so.
    // An undeclared path is not "an interface this module invented" — it is a
    // name for nothing, and refusing it here is what keeps a typo from becoming
    // a silently empty effect.
    match interfaces::interface(&written) {
        Some(interface) => Resolved::Interface(interface.path.to_string()),
        None => Resolved::Unresolved(written),
    }
}

/// The resolved interface effect set of one signature.
pub(crate) fn resolved_set(
    source: &SourceUnit,
    imports: &BTreeMap<String, String>,
    effects: &[EffectRef],
) -> alloc::collections::BTreeSet<String> {
    effects
        .iter()
        .map(|effect| resolve(source, imports, effect).recorded().to_string())
        .collect()
}
