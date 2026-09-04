// SPDX-License-Identifier: GPL-3.0-or-later
//! Static checks over a parsed TOS Core module.
//!
//! This is the first slice of step 3 of the docs/44 section 6 order. It owns
//! the checks that need only the module's own declarations: the resource
//! envelope required by docs/41 section 6, and named-field uniqueness from
//! docs/39 section 5. Name resolution, types, effects and ownership are later
//! slices and are not performed here.
//!
//! Every diagnostic carries a code registered in docs/44 section 7. A check
//! that cannot yet be performed reports nothing rather than guessing.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use crate::parser::{
    Block, EnumVariantForm, Expression, ExpressionForm, Pattern, PatternForm, RecordField, Schema,
    Statement, StatementForm,
};
use crate::{Diagnostic, Severity, SourceUnit, Span, Stage};

/// Resource keys every module must declare, with the literal class each one
/// takes (docs/41 section 6).
const REQUIRED_LIMITS: [(&str, LimitKind); 10] = [
    ("fuel", LimitKind::Integer),
    ("stack", LimitKind::Size),
    ("allocation", LimitKind::Size),
    ("tasks", LimitKind::Integer),
    ("workers", LimitKind::Integer),
    ("sync", LimitKind::Integer),
    ("shared", LimitKind::Size),
    ("cleanup", LimitKind::Integer),
    ("recursion", LimitKind::Integer),
    ("imports", LimitKind::Integer),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LimitKind {
    Integer,
    Size,
}

impl LimitKind {
    /// Whether a literal's source text belongs to this class.
    ///
    /// The lexer already separated integer from size literals, so this only
    /// has to tell the two apart: a size literal ends in its unit suffix.
    fn accepts(self, text: &str) -> bool {
        let is_size = text.ends_with('B')
            || text.ends_with("KiB")
            || text.ends_with("MiB")
            || text.ends_with("GiB");
        match self {
            LimitKind::Integer => {
                !is_size && text.chars().next().is_some_and(|c| c.is_ascii_digit())
            }
            LimitKind::Size => is_size,
        }
    }
}

/// Value names the language supplies without declaration (docs/39 section 2).
const PREDECLARED_VALUES: [&str; 6] = ["Some", "None", "Ok", "Err", "Completed", "Cancelled"];

const PREDECLARED_FUNCTIONS: [&str; 20] = [
    // ADR-0081 §7: device access is width- and byte-order-explicit, because a
    // register's width belongs to the transaction rather than to a type.
    "mmio_read_u8",
    "mmio_read_le_u16",
    "mmio_read_le_u32",
    "mmio_read_le_u64",
    "mmio_write_u8",
    "mmio_write_le_u16",
    "mmio_write_le_u32",
    "mmio_write_le_u64",
    // ADR-0037: sharing is an explicit typed operation, never an implicit copy.
    "share",
    "to_i8",
    "to_i16",
    "to_i32",
    "to_i64",
    "to_u8",
    "to_u16",
    "to_u32",
    "to_u64",
    "wrapping_add",
    "wrapping_sub",
    "wrapping_mul",
];

const ATOMIC_ORDERS: [&str; 5] = ["Relaxed", "Acquire", "Release", "AcqRel", "SeqCst"];

pub struct Checker;

impl Checker {
    /// Runs every implemented static check over one parsed module.
    ///
    /// The schema must have parsed without error diagnostics; checking a
    /// partial tree would report consequences of a syntax error as semantic
    /// findings.
    pub fn check(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        check_language_version(source, schema, &mut diagnostics);
        check_resource_envelope(source, schema, &mut diagnostics);
        check_record_fields(source, schema, &mut diagnostics);
        diagnostics.extend(resolve_value_names(source, schema));
        diagnostics.extend(crate::types::check_types(source, schema));
        diagnostics.extend(crate::types::check_public_signatures(source, schema));
        diagnostics.extend(crate::exhaustiveness::check_exhaustiveness(source, schema));
        diagnostics.extend(crate::returns::check_returns(source, schema));
        // Typing is derived once and handed to the slices that need it. Three
        // slices used to derive it separately, which was three full passes for
        // one answer; none of them concludes anything different for being given
        // it, and each still derives its own when run alone.
        diagnostics.extend(crate::typing::check_typing(source, schema));
        let bindings = crate::typing::binding_types(source, schema);
        diagnostics.extend(crate::ownership::check_ownership_with(
            source, schema, &bindings,
        ));
        diagnostics.extend(crate::mutability::check_mutability(source, schema));
        diagnostics.extend(crate::concurrency::check_concurrency(source, schema));
        diagnostics.extend(crate::guards::check_guards_with(source, schema, &bindings));
        diagnostics.extend(crate::capability::check_capabilities(source, schema));
        diagnostics.extend(crate::boundary::check_boundary(source, schema));
        diagnostics.extend(crate::defer::check_defer_bodies(source, schema));
        diagnostics.extend(crate::metering::check_metering(source, schema));
        diagnostics.extend(crate::constants::check_constants(source, schema));
        diagnostics.extend(crate::profile::check_profile(source, schema));
        diagnostics
    }
}

/// The checker's slices, in the order `Checker::check` runs them.
///
/// The checker is a sequence of independent slices, each reporting only what it
/// can establish on its own. Which of them costs what is invisible from the
/// total, and merging slices for speed without knowing that would trade a
/// correctness property for a guess — so the slices are nameable and runnable
/// one at a time.
pub const CHECK_SLICES: [&str; 11] = [
    "names",
    "constants",
    "types",
    "visibility",
    "exhaustiveness",
    "returns",
    "typing",
    "ownership",
    "mutability",
    "concurrency",
    "guards",
];

/// Runs one named slice of the checker, for profiling and for a test that needs
/// one slice's verdict rather than the whole checker's.
///
/// No clock lives here: `tos-core` is `no_std` and has no business knowing what
/// time it is. A caller that wants a timing brings its own.
pub fn check_slice(name: &str, source: &SourceUnit, schema: &Schema) -> Option<Vec<Diagnostic>> {
    Some(match name {
        "names" => resolve_value_names(source, schema),
        "constants" => crate::constants::check_constants(source, schema),
        "types" => crate::types::check_types(source, schema),
        "visibility" => crate::types::check_public_signatures(source, schema),
        "exhaustiveness" => crate::exhaustiveness::check_exhaustiveness(source, schema),
        "returns" => crate::returns::check_returns(source, schema),
        "typing" => crate::typing::check_typing(source, schema),
        "ownership" => crate::ownership::check_ownership(source, schema),
        "mutability" => crate::mutability::check_mutability(source, schema),
        "concurrency" => crate::concurrency::check_concurrency(source, schema),
        "guards" => crate::guards::check_guards(source, schema),
        _ => return None,
    })
}

/// Resolves every value name against the scope it appears in.
///
/// Only value positions are resolved. A field name after `.` and a named
/// argument label are field names, not values, and are checked against their
/// type by a later slice.
fn resolve_value_names(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut local_enums = BTreeMap::new();
    for declaration in schema.enums() {
        let variants: BTreeSet<&str> = declaration
            .variants()
            .iter()
            .map(|variant| variant.name().text(source))
            .collect();
        local_enums.insert(declaration.name().text(source), variants);
    }
    let mut resolver = Resolver {
        source,
        scopes: Vec::new(),
        local_enums,
        diagnostics: Vec::new(),
    };
    resolver.push_scope();
    for name in PREDECLARED_VALUES
        .iter()
        .chain(PREDECLARED_FUNCTIONS.iter())
        .chain(ATOMIC_ORDERS.iter())
    {
        resolver.declare(name);
    }

    // Module scope is collected before any body is visited, so declaration
    // order does not affect resolution and recursion resolves.
    resolver.push_scope();
    for import in schema.outline().prefix().imports() {
        let name = import.binding().text(source);
        resolver.declare(name);
    }
    for record in schema.records() {
        resolver.declare(record.name().text(source));
    }
    for declaration in schema.enums() {
        // A variant name is an unqualified module-scope constructor in V1.
        for variant in declaration.variants() {
            resolver.declare(variant.name().text(source));
        }
    }
    for declaration in schema.consts() {
        resolver.declare(declaration.name().text(source));
    }
    for signature in schema.extern_functions() {
        resolver.declare(signature.name().text(source));
    }
    for function in schema.functions() {
        resolver.declare(function.signature().name().text(source));
    }

    for declaration in schema.consts() {
        resolver.visit_expression(declaration.value());
    }
    for function in schema.functions() {
        resolver.push_scope();
        for parameter in function.signature().parameters() {
            let name = parameter.name().text(source);
            resolver.declare(name);
        }
        resolver.visit_block(function.body());
        resolver.pop_scope();
    }
    resolver.diagnostics
}

/// Where a name is written, because what an unresolved name *means* depends on
/// it (ADR-0064).
///
/// `docs/39` §5 gives calls and constructions one form, so the callee position
/// is exactly the position in which a type name is being applied to arguments.
/// A name anywhere else is being read as a value. Carrying the position rather
/// than inspecting the name is the whole point: a rule keyed on the spelling
/// would make every mention of a predeclared type a construction, which is the
/// drift this replaced.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Position {
    /// The callee of a call or construction: `Event()`.
    Callee,
    /// Any position where a value is expected: `Event`, `f(Event)`, `x + Event`.
    Value,
}

struct Resolver<'source> {
    source: &'source SourceUnit,
    scopes: Vec<BTreeSet<&'source str>>,
    /// Variant names of every enum declared in this module, for resolving a
    /// qualified constructor pattern without types.
    local_enums: BTreeMap<&'source str, BTreeSet<&'source str>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> Resolver<'source> {
    fn push_scope(&mut self) {
        self.scopes.push(BTreeSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &'source str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name);
        }
    }

    fn resolve(&mut self, span: Span, position: Position) {
        let name = span.text(self.source);
        if self.scopes.iter().any(|scope| scope.contains(name)) {
            return;
        }
        // ADR-0039 revision 4, the boundary ADR-0064 fixed: **applying** a
        // nonconstructible type to arguments is an attempt to make one out of
        // data, and that is what this code is for. The same name written alone
        // is not an attempt at anything — it is a name that does not resolve to
        // a value, which is what `E1202` says. The difference is the form of the
        // expression, never the spelling, so the caller supplies it and this
        // function does not guess from the name.
        if position == Position::Callee && crate::typing::is_nonconstructible_name(name) {
            self.diagnostics.push(
                diagnostic(
                    "E1213_NONCONSTRUCTIBLE_TYPE",
                    Stage::Type,
                    span,
                    self.source,
                )
                .with_field("type", name)
                .with_field("operation", "construct"),
            );
            return;
        }
        self.diagnostics.push(
            diagnostic("E1202_UNKNOWN_VALUE_NAME", Stage::Type, span, self.source)
                .with_field("name", name),
        );
    }

    /// Brings a pattern's bindings into the current scope.
    ///
    /// ADR-0033 resolves a bare pattern name against the pattern's expected
    /// type: it is a constructor when it names a variant of that type and a
    /// binding otherwise. This slice has no types yet, so it approximates by
    /// treating an already-resolving name as a constructor. The approximation
    /// is diagnosis-neutral — both readings admit the same set of resolvable
    /// names — and the real rule lands with the type slice.
    ///
    /// A qualified path is always a constructor and never binds, so it
    /// contributes no name here.
    fn bind_pattern(&mut self, pattern: &'source Pattern) {
        match pattern.form() {
            PatternForm::Wildcard => {}
            PatternForm::Name | PatternForm::Destructure if pattern.is_qualified() => {
                self.check_qualified_pattern(pattern);
                for element in pattern.elements() {
                    self.bind_pattern(element);
                }
            }
            PatternForm::Name => {
                let span = pattern.name().expect("a name pattern carries its name");
                let name = span.text(self.source);
                if !self.scopes.iter().any(|scope| scope.contains(name)) {
                    self.declare(name);
                }
            }
            PatternForm::Destructure | PatternForm::Tuple => {
                for element in pattern.elements() {
                    self.bind_pattern(element);
                }
            }
        }
    }

    /// Checks a qualified constructor pattern whose enum is declared locally.
    ///
    /// ADR-0033 section 5 makes a qualified path always a constructor, so a
    /// path naming no reachable variant is an error rather than a binding.
    /// Only a locally declared enum can be resolved without types: a path
    /// through an import binding needs the module closure and is left to the
    /// slice that owns it.
    fn check_qualified_pattern(&mut self, pattern: &'source Pattern) {
        let path = pattern.path();
        let [enum_name, variant_name] = path else {
            return;
        };
        let Some(variants) = self.local_enums.get(enum_name.text(self.source)) else {
            return;
        };
        let variant = variant_name.text(self.source);
        if variants.contains(variant) {
            return;
        }
        self.diagnostics.push(
            diagnostic(
                "E1202_UNKNOWN_VALUE_NAME",
                Stage::Type,
                pattern.span(),
                self.source,
            )
            .with_field("name", variant)
            .with_field("enum", enum_name.text(self.source)),
        );
    }

    fn visit_block(&mut self, block: &'source Block) {
        self.push_scope();
        for statement in block.statements() {
            self.visit_statement(statement);
        }
        self.pop_scope();
    }

    fn visit_statement(&mut self, statement: &'source Statement) {
        match statement.form() {
            StatementForm::Let => {
                // The initializer cannot see the binding it produces.
                if let Some(expression) = statement.expression() {
                    self.visit_expression(expression);
                }
                if let Some(pattern) = statement.pattern() {
                    self.bind_pattern(pattern);
                }
            }
            StatementForm::For => {
                if let Some(expression) = statement.expression() {
                    self.visit_expression(expression);
                }
                self.push_scope();
                if let Some(pattern) = statement.pattern() {
                    self.bind_pattern(pattern);
                }
                if let Some(body) = statement.body() {
                    self.visit_block(body);
                }
                self.pop_scope();
            }
            StatementForm::Match => {
                if let Some(expression) = statement.expression() {
                    self.visit_expression(expression);
                }
                for branch in statement.branches() {
                    self.push_scope();
                    self.bind_pattern(branch.pattern());
                    self.visit_block(branch.body());
                    self.pop_scope();
                }
            }
            _ => {
                if let Some(target) = statement.target() {
                    self.visit_expression(target);
                }
                if let Some(expression) = statement.expression() {
                    self.visit_expression(expression);
                }
                if let Some(body) = statement.body() {
                    self.visit_block(body);
                }
                if let Some(body) = statement.else_body() {
                    self.visit_block(body);
                }
                if let Some(nested) = statement.else_if() {
                    self.visit_statement(nested);
                }
            }
        }
    }

    fn visit_expression(&mut self, expression: &'source Expression) {
        // A left-associative operator chain is as deep as it is long, so its
        // spine is walked with a list rather than by recursion. The order is the
        // recursion's: the innermost operand, then each right operand outwards.
        if expression.form() == ExpressionForm::Binary {
            let (chain, innermost) =
                crate::walk::binary_chain(expression, |node| node.form() == ExpressionForm::Binary);
            if let Some(innermost) = innermost {
                self.visit_expression(innermost);
            }
            for node in chain.iter().rev() {
                if let Some(right) = node.right() {
                    self.visit_expression(right);
                }
            }
            return;
        }
        // And a run of prefix operators, which nests nothing either.
        if expression.form() == ExpressionForm::Unary {
            let (_, innermost) =
                crate::walk::prefix_chain(expression, |node| node.form() == ExpressionForm::Unary);
            if let Some(innermost) = innermost {
                self.visit_expression(innermost);
            }
            return;
        }
        if expression.form() == ExpressionForm::Name {
            self.resolve(expression.span(), Position::Value);
            return;
        }
        if expression.form() == ExpressionForm::Closure {
            self.push_scope();
            for parameter in expression.parameters() {
                let name = parameter.name().text(self.source);
                self.declare(name);
            }
            if let Some(body) = expression.body() {
                self.visit_block(body);
            }
            self.pop_scope();
            return;
        }
        for child in [expression.left(), expression.right(), expression.inner()]
            .into_iter()
            .flatten()
        {
            self.visit_expression(child);
        }
        // The callee of a call is the one position that is not a value read, so
        // it is resolved here — where the enclosing form is known — instead of
        // by the `Name` arm above, which cannot see what encloses it. Every
        // other callee shape (a path, a field, a parenthesised expression) is an
        // ordinary child: a construction names its type directly or not at all.
        if let Some(callee) = expression.callee() {
            if expression.form() == ExpressionForm::Call && callee.form() == ExpressionForm::Name {
                self.resolve(callee.span(), Position::Callee);
            } else {
                self.visit_expression(callee);
            }
        }
        for argument in expression.arguments() {
            self.visit_expression(argument.value());
        }
        for element in expression.elements() {
            self.visit_expression(element);
        }
        if let Some(body) = expression.body() {
            self.visit_block(body);
        }
    }
}

fn diagnostic(code: &'static str, stage: Stage, span: Span, source: &SourceUnit) -> Diagnostic {
    Diagnostic::new(code, Severity::Error, stage, span, source)
}

/// The source-language version this frontend implements (docs/42 section 1).
///
/// **1.2 since ADR-0081**, which adds device memory. 1.1 added the
/// direct-interface effect form (ADR-0080). Every earlier minor remains
/// supported and unchanged: a module declaring one keeps its meaning, its
/// diagnostics and its digest, and is refused only if it uses a form its own
/// header did not claim (`E1608`).
const LANGUAGE_VERSION: (u32, u32) = (1, 2);

/// The minor in which a direct interface effect became legal (ADR-0080 §5).
const DIRECT_INTERFACE_EFFECT_MINOR: u32 = 1;

/// The minor in which device memory became part of the language (ADR-0081 §6).
///
/// Indexed region access needed no version: it implements semantics the
/// accepted corpus already described. MMIO is not in the accepted language at
/// all, so it is additive and takes one.
const DEVICE_MEMORY_MINOR: u32 = 2;

/// Checks the declared source-language version.
///
/// docs/42 section 1 requires exactly `1.0` for V1: another major is
/// `E1601_UNSUPPORTED_LANGUAGE_VERSION` and an unknown minor is
/// `E1602_UNSUPPORTED_LANGUAGE_MINOR`. The version is the language version, not
/// a module release number, so a module cannot opt into a dialect.
fn check_language_version(source: &SourceUnit, schema: &Schema, out: &mut Vec<Diagnostic>) {
    let header = schema.outline().prefix().header();
    let (major, minor) = header.version();
    let (expected_major, expected_minor) = LANGUAGE_VERSION;
    if major != expected_major {
        out.push(
            diagnostic(
                "E1601_UNSUPPORTED_LANGUAGE_VERSION",
                Stage::Type,
                header.span(),
                source,
            )
            .with_field("declared", major)
            .with_field("supported", expected_major),
        );
        return;
    }
    if minor > expected_minor {
        out.push(
            diagnostic(
                "E1602_UNSUPPORTED_LANGUAGE_MINOR",
                Stage::Type,
                header.span(),
                source,
            )
            .with_field("declared", minor)
            .with_field("supported", expected_minor),
        );
        return;
    }
    check_features_against_minor(source, schema, minor, out);
}

/// Refuses a source form the module's **own header** did not claim (ADR-0080 §5).
///
/// A module whose header says `1.0` and whose body uses a 1.1 form would work
/// here and be refused by a 1.0 frontend, with nothing in the source saying
/// which was right. The header is a fact about the source, so a module gets
/// exactly the language it declared — and the diagnostic names the feature and
/// the minor it needs rather than the form it happened to see.
fn check_features_against_minor(
    source: &SourceUnit,
    schema: &Schema,
    minor: u32,
    out: &mut Vec<Diagnostic>,
) {
    let signatures: Vec<_> = schema
        .functions()
        .iter()
        .map(|function| function.signature())
        .chain(schema.extern_functions().iter())
        .collect();
    if minor < DIRECT_INTERFACE_EFFECT_MINOR {
        for signature in &signatures {
            for effect in signature.effects() {
                if effect.is_binding() {
                    continue;
                }
                out.push(
                    diagnostic(
                        "E1608_FEATURE_REQUIRES_LANGUAGE_MINOR",
                        Stage::Type,
                        effect.span(),
                        source,
                    )
                    .with_field("feature", "direct interface effect")
                    .with_field("declared", minor)
                    .with_field("requires", DIRECT_INTERFACE_EFFECT_MINOR),
                );
            }
        }
    }
    if minor < DEVICE_MEMORY_MINOR {
        refuse_device_memory(source, schema, &signatures, minor, out);
    }
}

/// Refuses the device-memory feature in a module that did not claim it.
///
/// Both halves of the feature are caught: naming one of the types, and calling
/// one of the accesses. A module that could name the type but not the operation
/// would still be a 1.0 module holding a 1.2 value.
fn refuse_device_memory(
    source: &SourceUnit,
    schema: &Schema,
    signatures: &[&crate::parser::FunctionSignature],
    minor: u32,
    out: &mut Vec<Diagnostic>,
) {
    let mut report = |span| {
        out.push(
            diagnostic(
                "E1608_FEATURE_REQUIRES_LANGUAGE_MINOR",
                Stage::Type,
                span,
                source,
            )
            .with_field("feature", "device memory")
            .with_field("declared", minor)
            .with_field("requires", DEVICE_MEMORY_MINOR),
        );
    };
    for signature in signatures {
        for parameter in signature.parameters() {
            if names_device_memory(source, parameter.ty()) {
                report(parameter.span());
            }
        }
        if names_device_memory(source, signature.result()) {
            report(signature.span());
        }
    }
    for function in schema.functions() {
        let mut found = Vec::new();
        device_accesses_in(source, function.body(), &mut found);
        for span in found {
            report(span);
        }
    }
}

/// Every device-memory access written inside a block, however deeply nested.
fn device_accesses_in(source: &SourceUnit, block: &crate::parser::Block, out: &mut Vec<Span>) {
    for statement in block.statements() {
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            crate::walk::walk_tree(expression, false, |node| {
                if let crate::walk::Node::Expression(inner) = node {
                    if inner.form() == crate::parser::ExpressionForm::Call {
                        if let Some(callee) = inner.inner() {
                            if callee.form() == crate::parser::ExpressionForm::Name
                                && crate::typing::mmio_access(callee.span().text(source)).is_some()
                            {
                                out.push(inner.span());
                            }
                        }
                    }
                }
                crate::walk::Descend::Children
            });
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            device_accesses_in(source, nested, out);
        }
        if let Some(chained) = statement.else_if() {
            for nested in [chained.body(), chained.else_body()].into_iter().flatten() {
                device_accesses_in(source, nested, out);
            }
        }
    }
}

/// Whether a written type names device memory.
fn names_device_memory(source: &SourceUnit, ty: &crate::parser::TypeSyntax) -> bool {
    match ty {
        crate::parser::TypeSyntax::Name { path, .. } => path
            .last()
            .is_some_and(|segment| matches!(segment.text(source), "MmioRegion" | "MmioRegionMut")),
        crate::parser::TypeSyntax::Constructed {
            name, arguments, ..
        } => {
            matches!(name.text(source), "MmioRegion" | "MmioRegionMut")
                || arguments
                    .iter()
                    .any(|inner| names_device_memory(source, inner))
        }
        _ => false,
    }
}

/// Checks the module resource declaration against docs/41 section 6.
fn check_resource_envelope(source: &SourceUnit, schema: &Schema, out: &mut Vec<Diagnostic>) {
    let resource = schema.outline().resource();
    let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
    for limit in resource.limits() {
        let name = limit.name().text(source);
        if let Some(first) = seen.get(name) {
            out.push(
                diagnostic(
                    "E1703_DUPLICATE_RESOURCE_DECLARATION",
                    Stage::Resource,
                    limit.name(),
                    source,
                )
                .with_field("key", name)
                .with_field("first_declared_at", first.start()),
            );
            continue;
        }
        seen.insert(name, limit.name());

        let Some((_, kind)) = REQUIRED_LIMITS.iter().find(|(key, _)| *key == name) else {
            // A module may declare stricter named limits, but an unrecognized
            // key is not one of them: docs/41 names no extension mechanism.
            out.push(
                diagnostic(
                    "E1704_UNKNOWN_RESOURCE_LIMIT",
                    Stage::Resource,
                    limit.name(),
                    source,
                )
                .with_field("key", name),
            );
            continue;
        };
        if !kind.accepts(limit.value().text(source)) {
            out.push(
                diagnostic(
                    "E1704_UNKNOWN_RESOURCE_LIMIT",
                    Stage::Resource,
                    limit.value(),
                    source,
                )
                .with_field("key", name)
                .with_field(
                    "expected",
                    if *kind == LimitKind::Size {
                        "size"
                    } else {
                        "integer"
                    },
                ),
            );
        }
    }

    for (key, _) in REQUIRED_LIMITS {
        if seen.contains_key(key) {
            continue;
        }
        out.push(
            diagnostic(
                "E1700_RESOURCE_DECLARATION_REQUIRED",
                Stage::Resource,
                resource.span(),
                source,
            )
            .with_field("key", key),
        );
    }
}

/// Checks that named field lists declare each name once (docs/39 section 5).
fn check_record_fields(source: &SourceUnit, schema: &Schema, out: &mut Vec<Diagnostic>) {
    for record in schema.records() {
        check_field_list(source, record.fields(), out);
    }
    for declaration in schema.enums() {
        for variant in declaration.variants() {
            check_field_list(source, variant.fields(), out);
        }
    }
    let constructors = named_field_constructors(source, schema);
    let mut findings = Vec::new();
    walk_expressions(schema, &mut |expression| {
        check_named_arguments(source, &constructors, expression, &mut findings);
    });
    out.extend(findings);
}

fn check_field_list(source: &SourceUnit, fields: &[RecordField], out: &mut Vec<Diagnostic>) {
    let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
    for field in fields {
        let name = field.name().text(source);
        match seen.get(name) {
            Some(first) => out.push(duplicate_field(source, field.name(), name, *first)),
            None => {
                seen.insert(name, field.name());
            }
        }
    }
}

fn duplicate_field(source: &SourceUnit, span: Span, name: &str, first: Span) -> Diagnostic {
    diagnostic("E1205_DUPLICATE_RECORD_FIELD", Stage::Type, span, source)
        .with_field("field", name)
        .with_field("first_declared_at", first.start())
}

/// The declared field names of every local named-field constructor.
///
/// docs/39 section 5 gives records and named-field enum variants the same
/// construction form, so both are collected here. A name declared by more than
/// one constructor is dropped: choosing between them needs types.
fn named_field_constructors<'source>(
    source: &'source SourceUnit,
    schema: &Schema,
) -> BTreeMap<&'source str, Vec<&'source str>> {
    let mut constructors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut ambiguous: BTreeSet<&str> = BTreeSet::new();
    let mut record = |name: &'source str, fields: Vec<&'source str>| {
        if constructors.insert(name, fields).is_some() {
            ambiguous.insert(name);
        }
    };
    for declaration in schema.records() {
        let fields = declaration
            .fields()
            .iter()
            .map(|field| field.name().text(source))
            .collect();
        record(declaration.name().text(source), fields);
    }
    for declaration in schema.enums() {
        for variant in declaration.variants() {
            if variant.form() != EnumVariantForm::NamedFields {
                continue;
            }
            let fields = variant
                .fields()
                .iter()
                .map(|field| field.name().text(source))
                .collect();
            record(variant.name().text(source), fields);
        }
    }
    for name in ambiguous {
        constructors.remove(name);
    }
    constructors
}

/// Checks a named argument list against the constructor it names.
///
/// docs/39 section 5 makes named construction exact-once over the declared
/// fields: an unknown name is `E1207_UNKNOWN_RECORD_FIELD`, a duplicate is
/// `E1205_DUPLICATE_RECORD_FIELD` and an omitted field is
/// `E1206_MISSING_RECORD_FIELD`.
///
/// The duplicate check runs on every named argument list, because it needs no
/// declaration. The other two run only when the callee names one local
/// constructor, which is nominal resolution rather than typing.
fn check_named_arguments(
    source: &SourceUnit,
    constructors: &BTreeMap<&str, Vec<&str>>,
    expression: &Expression,
    out: &mut Vec<Diagnostic>,
) {
    let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
    for argument in expression.arguments() {
        let Some(span) = argument.name() else {
            continue;
        };
        let name = span.text(source);
        match seen.get(name) {
            Some(first) => out.push(duplicate_field(source, span, name, *first)),
            None => {
                seen.insert(name, span);
            }
        }
    }
    if seen.is_empty() {
        return;
    }
    let Some(callee) = expression.callee() else {
        return;
    };
    if callee.form() != ExpressionForm::Name {
        return;
    }
    let Some(declared) = constructors.get(callee.span().text(source)) else {
        return;
    };
    for (name, span) in &seen {
        if declared.contains(name) {
            continue;
        }
        out.push(
            diagnostic("E1207_UNKNOWN_RECORD_FIELD", Stage::Type, *span, source)
                .with_field("field", *name)
                .with_field("constructor", callee.span().text(source)),
        );
    }
    for field in declared {
        if seen.contains_key(field) {
            continue;
        }
        out.push(
            diagnostic(
                "E1206_MISSING_RECORD_FIELD",
                Stage::Type,
                expression.span(),
                source,
            )
            .with_field("field", *field)
            .with_field("constructor", callee.span().text(source)),
        );
    }
}

/// Visits every expression reachable from a module's items.
fn walk_expressions(schema: &Schema, visit: &mut impl FnMut(&Expression)) {
    for declaration in schema.consts() {
        walk_expression(declaration.value(), visit);
    }
    for function in schema.functions() {
        walk_block(function.body(), visit);
    }
}

fn walk_block(block: &Block, visit: &mut impl FnMut(&Expression)) {
    for statement in block.statements() {
        walk_statement(statement, visit);
    }
}

fn walk_statement(statement: &Statement, visit: &mut impl FnMut(&Expression)) {
    if let Some(target) = statement.target() {
        walk_expression(target, visit);
    }
    if let Some(expression) = statement.expression() {
        walk_expression(expression, visit);
    }
    if let Some(body) = statement.body() {
        walk_block(body, visit);
    }
    if let Some(body) = statement.else_body() {
        walk_block(body, visit);
    }
    if let Some(nested) = statement.else_if() {
        walk_statement(nested, visit);
    }
    for branch in statement.branches() {
        walk_block(branch.body(), visit);
    }
}

fn walk_expression(expression: &Expression, visit: &mut impl FnMut(&Expression)) {
    // A left-associative operator chain is as deep as it is long. Its nodes are
    // visited in the same pre-order the recursion produced — outermost first,
    // down the spine — and then the right operands from the innermost outwards.
    if expression.form() == ExpressionForm::Binary {
        let (chain, innermost) =
            crate::walk::binary_chain(expression, |node| node.form() == ExpressionForm::Binary);
        for node in &chain {
            visit(node);
        }
        if let Some(innermost) = innermost {
            walk_expression(innermost, visit);
        }
        for node in chain.iter().rev() {
            if let Some(right) = node.right() {
                walk_expression(right, visit);
            }
        }
        return;
    }
    if expression.form() == ExpressionForm::Unary {
        let (chain, innermost) =
            crate::walk::prefix_chain(expression, |node| node.form() == ExpressionForm::Unary);
        for node in &chain {
            visit(node);
        }
        if let Some(innermost) = innermost {
            walk_expression(innermost, visit);
        }
        return;
    }
    visit(expression);
    for child in [
        expression.left(),
        expression.right(),
        expression.inner(),
        expression.callee(),
    ]
    .into_iter()
    .flatten()
    {
        walk_expression(child, visit);
    }
    for argument in expression.arguments() {
        walk_expression(argument.value(), visit);
    }
    for element in expression.elements() {
        walk_expression(element, visit);
    }
    if let Some(body) = expression.body() {
        walk_block(body, visit);
    }
}
