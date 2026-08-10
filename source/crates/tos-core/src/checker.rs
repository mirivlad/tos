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

use std::collections::{BTreeMap, BTreeSet};
use std::vec::Vec;

use crate::parser::{
    Block, Expression, ExpressionForm, Pattern, PatternForm, RecordField, Schema, Statement,
    StatementForm,
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

const PREDECLARED_FUNCTIONS: [&str; 11] = [
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
        check_resource_envelope(source, schema, &mut diagnostics);
        check_record_fields(source, schema, &mut diagnostics);
        diagnostics.extend(resolve_value_names(source, schema));
        diagnostics.extend(crate::returns::check_returns(source, schema));
        diagnostics.extend(crate::profile::check_profile(source, schema));
        diagnostics
    }
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

    fn resolve(&mut self, span: Span) {
        let name = span.text(self.source);
        if self.scopes.iter().any(|scope| scope.contains(name)) {
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
        if expression.form() == ExpressionForm::Name {
            self.resolve(expression.span());
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
        for child in [
            expression.left(),
            expression.right(),
            expression.inner(),
            expression.callee(),
        ]
        .into_iter()
        .flatten()
        {
            self.visit_expression(child);
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
    let mut findings = Vec::new();
    walk_expressions(schema, &mut |expression| {
        check_named_arguments(source, expression, &mut findings);
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

/// Checks that a named argument list supplies each field once.
///
/// docs/39 section 5 makes named construction exact-once, so the same rule
/// covers a declared field list and a constructor argument list.
fn check_named_arguments(source: &SourceUnit, expression: &Expression, out: &mut Vec<Diagnostic>) {
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
