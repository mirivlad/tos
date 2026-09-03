// SPDX-License-Identifier: GPL-3.0-or-later
//! The unsafe and FFI boundary (docs/40 section 7, docs/42 section 5).
//!
//! V1 reserved `extern` syntax so the boundary would be visible from the first
//! implementation, and rejected every `extern` item because there was no
//! accepted interface schema to name. ADR-0060 supplied the first one
//! (`SYSTEM_INTERFACE_V1`), so the rejection stopped being unconditional and
//! became what docs/44 always said it was: **an `extern` item names no accepted
//! interface schema**. Everything the schema does not declare is rejected
//! exactly as before, and docs/42 §5's prohibitions are untouched — no build
//! flag, host library or `unsafe` block enables anything here.
//!
//! An accepted item is one where all four of these hold, and the diagnostic
//! names which one did not:
//!
//! - its `uses` list names capability imports of this module, so an operation
//!   cannot be reached without having requested the authority it belongs to;
//! - the interface of that import is one an accepted schema declares;
//! - that interface declares an operation of this name;
//! - and the item's parameters and result are the operation's — the first
//!   parameter being the capability itself, of the interface's own type.
//!
//! An `unsafe { ... }` block must open with a line comment beginning `SAFETY:`
//! that names its local preconditions; a missing rationale is
//! `E1802_UNSAFE_RATIONALE_REQUIRED`. The lexer discards comments, so the
//! rationale is read from the source text of the block itself.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::interfaces;
use crate::parser::{
    Block, Expression, ExpressionForm, FunctionSignature, ImportKind, Schema, Statement,
    StatementForm, TypeSyntax,
};
use crate::{Diagnostic, Severity, SourceUnit, Span, Stage};

pub(crate) fn check_boundary(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // What this module requested, by the name it bound: an `extern` item's
    // `uses` list names one of these, and the interface is that request's type.
    let mut requested: BTreeMap<String, String> = BTreeMap::new();
    for import in schema.outline().prefix().imports() {
        if import.kind() != ImportKind::Capability {
            continue;
        }
        let path: Vec<&str> = import
            .path()
            .iter()
            .map(|segment| segment.text(source))
            .collect();
        requested.insert(import.binding().text(source).to_string(), path.join("."));
    }
    for signature in schema.extern_functions() {
        if let Some(reason) = unavailable(source, signature, &requested) {
            diagnostics.push(
                Diagnostic::new(
                    "E1801_FFI_NOT_AVAILABLE",
                    Severity::Error,
                    Stage::Effect,
                    signature.span(),
                    source,
                )
                .with_field("item", signature.name().text(source))
                .with_field("reason", reason),
            );
        }
    }
    // Which `extern` items are operations of an accepted schema, so a call site
    // can be checked against the operation it reaches.
    //
    // **Keyed by name *and* interface**, because one operation may be declared
    // by several interfaces. `endow_for_launch` is (ADR-0077 §3): it is reached
    // through the capability being delegated, so a module that endows an
    // endpoint and a memory authority declares two `extern` items of that name,
    // differing in the interface of their first parameter. Keying by name alone
    // would make the second declaration silently take the first's schema entry,
    // and a call would be checked against the wrong interface's requirements.
    let mut operations: BTreeMap<(String, String), &'static interfaces::Operation> =
        BTreeMap::new();
    let mut declared: BTreeMap<String, &'static interfaces::Operation> = BTreeMap::new();
    for signature in schema.extern_functions() {
        let name = signature.name().text(source);
        let Some(first) = signature.effects().first() else {
            continue;
        };
        let Some(path) = crate::effects::resolve(source, &requested, first)
            .interface()
            .map(alloc::string::ToString::to_string)
        else {
            continue;
        };
        let Some(operation) =
            interfaces::interface(&path).and_then(|interface| interface.operation(name))
        else {
            continue;
        };
        operations.insert((name.to_string(), path.clone()), operation);
        // And by name alone, for a call whose first argument is not of any
        // interface declaring this operation. That call is wrong, and the
        // reason it is wrong is the check below — so it has to find *a*
        // declaration to be checked against. Which one does not matter: every
        // declaration of one name agrees about the first parameter's position,
        // and the diagnostic reports that position.
        declared.entry(name.to_string()).or_insert(operation);
    }
    let sites = Sites {
        requested: &requested,
        operations: &operations,
        declared: &declared,
    };
    for function in schema.functions() {
        check_block(source, &sites, function.body(), &mut diagnostics);
    }
    diagnostics
}

/// Why this `extern` item names no accepted operation, or nothing when it does.
///
/// The reasons are ordered so that the first thing wrong is the thing reported:
/// a module that did not request the authority is told that, rather than being
/// told about a signature mismatch on an operation it could not have reached.
fn unavailable(
    source: &SourceUnit,
    signature: &FunctionSignature,
    requested: &BTreeMap<String, String>,
) -> Option<&'static str> {
    let effects = signature.effects();
    // The **first** effect names the interface the operation belongs to — the
    // one the instruction records and `Signature.effects` states. An item naming
    // none has no interface at all.
    let Some(first) = effects.first() else {
        return Some("expected a capability effect");
    };
    let Some(path) = crate::effects::resolve(source, requested, first)
        .interface()
        .map(alloc::string::ToString::to_string)
    else {
        return Some("uses names no capability import and no accepted interface");
    };
    let Some(interface) = interfaces::interface(&path) else {
        return Some("no accepted interface schema declares this interface");
    };
    let Some(operation) = interface.operation(signature.name().text(source)) else {
        return Some("the interface declares no operation of this name");
    };
    // And there is one effect per capability the operation requires, in the
    // order the schema declares them (ADR-0063). Fewer would leave a required
    // authority unrequested; more would name a binding the operation does not
    // take, which is authority declared and never used.
    if effects.len() != operation.capabilities.len() {
        return Some("the operation requires a different number of capability effects");
    }
    for (required, effect) in operation.capabilities.iter().zip(effects) {
        // **The interface, whichever way it was written** (ADR-0080). A binding
        // resolves to what it imported; a dotted item is the interface itself.
        // What an operation requires is an interface, so that is what is
        // compared — and a runtime-obtained capability has no import to name.
        match crate::effects::resolve(source, requested, effect).interface() {
            None => return Some("uses names no capability import and no accepted interface"),
            Some(path) if path != required.interface => {
                return Some("a capability effect is not of the interface the operation requires")
            }
            Some(_) => {}
        }
    }
    let parameters = signature.parameters();
    // The capabilities come first, in the order §4 lists them: an operation is
    // reached *through* a capability, so a declaration that did not take one
    // would be an operation reachable without authority — and one that took the
    // wrong one, or took them in another order, would be reachable through
    // authority the schema did not ask for.
    if parameters.len() < operation.capabilities.len() {
        return Some("the operation requires more capabilities than are declared");
    }
    let (capabilities, values) = parameters.split_at(operation.capabilities.len());
    for (position, (required, written)) in
        operation.capabilities.iter().zip(capabilities).enumerate()
    {
        if type_text(source, written.ty()).as_deref() != Some(required.interface) {
            return Some(match position {
                0 => "the first parameter is not a capability of this interface",
                _ => "a capability parameter is not of the interface the operation requires",
            });
        }
    }
    // The first capability is the operation's own interface, which is what the
    // instruction records and what `uses` names. A schema whose first
    // requirement named some other interface would make an operation reachable
    // from an interface that does not declare it.
    if operation
        .capabilities
        .first()
        .is_some_and(|first| first.interface != interface.path)
    {
        return Some("the operation's first capability is not this interface");
    }
    if values.len() != operation.parameters.len() {
        return Some("the operation takes a different number of values");
    }
    for (declared, written) in operation.parameters.iter().zip(values) {
        if type_text(source, written.ty()).as_deref() != Some(declared.ty) {
            return Some("a value parameter is not the type the operation takes");
        }
    }
    if type_text(source, signature.result()).as_deref() != Some(operation.result) {
        return Some("the result is not the type the operation returns");
    }
    None
}

/// A written type as its canonical text, for the forms an interface may name.
///
/// `Name` and `Constructed`, and no others. A name is a capability interface or
/// a primitive; a construction is how a *semantic result* is written, which
/// `SYSTEM_INTERFACE_V1` §5 now requires — an operation returns the value it
/// produced and `Result<T, i64>` is the refusal model, so a schema that admitted
/// only names could declare no operation that can fail and return something.
///
/// Array, tuple and function types stay out. Nothing declares one, and a schema
/// that admitted them would be admitting a shape no operation has.
///
/// The text is canonical rather than the source's own: one space after each
/// comma and none inside the angle brackets, so that a declaration written
/// across three lines and one written on one compare equal to the same schema
/// entry. What is compared is a type, and whitespace is not part of one.
fn type_text(source: &SourceUnit, ty: &TypeSyntax) -> Option<String> {
    match ty {
        TypeSyntax::Name { path, .. } => {
            let segments: Vec<&str> = path.iter().map(|segment| segment.text(source)).collect();
            Some(segments.join("."))
        }
        TypeSyntax::Constructed {
            name,
            arguments,
            mutable,
            ..
        } => {
            let mut written = String::new();
            for (position, argument) in arguments.iter().enumerate() {
                if position > 0 {
                    written.push_str(", ");
                }
                // ADR-0037: the granted mode is part of the type, so it is part
                // of the text. A schema entry that could not tell `Region<u8>`
                // from `Region<mut u8>` would be one that could not say which
                // of the two an operation returns. The grammar admits `mut` for
                // the one-argument region constructors only, so it belongs to
                // the element and is written there.
                if *mutable && position == 0 {
                    written.push_str("mut ");
                }
                written.push_str(&type_text(source, argument)?);
            }
            Some(alloc::format!("{}<{written}>", name.text(source)))
        }
        _ => None,
    }
}

/// What a call site is checked against: what this module requested, and which
/// accepted operation each `extern` name reaches.
struct Sites<'a> {
    requested: &'a BTreeMap<String, String>,
    operations: &'a BTreeMap<(String, String), &'static interfaces::Operation>,
    /// The same, by name alone: the entry a call reaches when its first
    /// argument is of no interface that declares the operation.
    declared: &'a BTreeMap<String, &'static interfaces::Operation>,
}

fn check_block(source: &SourceUnit, sites: &Sites<'_>, block: &Block, out: &mut Vec<Diagnostic>) {
    for statement in block.statements() {
        check_statement(source, sites, statement, out);
    }
}

fn check_statement(
    source: &SourceUnit,
    sites: &Sites<'_>,
    statement: &Statement,
    out: &mut Vec<Diagnostic>,
) {
    if statement.form() == StatementForm::Unsafe && !has_safety_rationale(source, statement.span())
    {
        out.push(
            Diagnostic::new(
                "E1802_UNSAFE_RATIONALE_REQUIRED",
                Severity::Error,
                Stage::Effect,
                statement.span(),
                source,
            )
            .with_field("expected", "leading SAFETY: line comment"),
        );
    }
    for expression in [statement.target(), statement.expression()]
        .into_iter()
        .flatten()
    {
        check_expression(source, sites, expression, out);
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        check_block(source, sites, nested, out);
    }
    if let Some(nested) = statement.else_if() {
        check_statement(source, sites, nested, out);
    }
    for branch in statement.branches() {
        check_block(source, sites, branch.body(), out);
    }
}

fn check_expression(
    source: &SourceUnit,
    sites: &Sites<'_>,
    expression: &Expression,
    out: &mut Vec<Diagnostic>,
) {
    // Iteratively: a flat operator chain is as deep as it is long.
    crate::walk::walk_tree(expression, false, |node| {
        match node {
            crate::walk::Node::Block(block) => check_block(source, sites, block, out),
            crate::walk::Node::Expression(expression) => inspect(source, sites, expression, out),
        }
        crate::walk::Descend::Children
    });
}

fn inspect(
    source: &SourceUnit,
    sites: &Sites<'_>,
    expression: &Expression,
    out: &mut Vec<Diagnostic>,
) {
    // A call to an operation of an accepted schema: each capability argument
    // must name an import **of the interface that position requires**
    // (ADR-0063). Without this, an operation taking two capabilities accepts
    // them in either order — and "reply here and wait there" becomes "wait here
    // and reply there" by writing the arguments the other way round, with
    // nothing in the artifact saying so. The types alone do not catch it: a
    // capability binding used as a value has no type the checker infers.
    if let Some(callee) = expression.callee() {
        if callee.form() == ExpressionForm::Name {
            // Which declaration this call reaches is decided by the interface of
            // its **first** argument, which §4.1 makes the operation's own. That
            // is the same rule the lowerer applies, so a call site and the
            // instruction it becomes agree about which interface was reached.
            let through = expression
                .arguments()
                .first()
                .map(|argument| argument.value())
                .filter(|written| written.form() == ExpressionForm::Name)
                .and_then(|written| sites.requested.get(written.span().text(source)));
            let name = callee.span().text(source);
            if let Some(operation) = through
                .and_then(|path| sites.operations.get(&(name.to_string(), path.clone())))
                .or_else(|| sites.declared.get(name))
            {
                for (required, argument) in
                    operation.capabilities.iter().zip(expression.arguments())
                {
                    let written = argument.value();
                    let held = (written.form() == ExpressionForm::Name)
                        .then(|| sites.requested.get(written.span().text(source)))
                        .flatten();
                    // **Only an import supplied at the wrong position is
                    // reported here.** Since ADR-0078 a capability position may
                    // also be filled by a capability *value* — one an operation
                    // produced — and this pass has no types with which to tell
                    // one of those from an ordinary local. What it can still
                    // tell, and what it exists for, is an import of the wrong
                    // interface: that is how "reply here and wait there" became
                    // "wait here and reply there" by writing the arguments the
                    // other way round, and it is caught unchanged.
                    //
                    // A value's exact interface is checked where the types are:
                    // by the lowerer, which refuses to build an instruction from
                    // a value that is not a capability of the required
                    // interface, and by the verifier, which checks the artifact
                    // rather than trusting either.
                    if held.is_none() {
                        continue;
                    }
                    if held.map(String::as_str) != Some(required.interface) {
                        out.push(
                            Diagnostic::new(
                                "E1215_ARGUMENT_TYPE_MISMATCH",
                                Severity::Error,
                                Stage::Effect,
                                written.span(),
                                source,
                            )
                            .with_field("callee", callee.span().text(source))
                            .with_field("expected", required.interface)
                            .with_field(
                                "actual",
                                held.map(String::as_str)
                                    .unwrap_or("not a capability import"),
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// Whether an `unsafe` block opens with a `SAFETY:` line comment.
///
/// The rationale must lead the block, so only the text between the opening
/// brace and the first other content is considered.
fn has_safety_rationale(source: &SourceUnit, span: Span) -> bool {
    let text = span.text(source);
    let Some(body) = text.find('{').map(|index| &text[index + 1..]) else {
        return false;
    };
    let leading = body.trim_start_matches([' ', '\n']);
    let Some(comment) = leading.strip_prefix("//") else {
        return false;
    };
    comment.trim_start().starts_with("SAFETY:")
}
