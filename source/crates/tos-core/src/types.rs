// SPDX-License-Identifier: GPL-3.0-or-later
//! Type-expression resolution (docs/40 sections 1–2, ADR-0034).
//!
//! Every type written in a module is resolved against the primitives, the fixed
//! and predeclared TOS Core types, the module's own nominal declarations and
//! its imports. A name that resolves to none of them is
//! `E1203_UNKNOWN_TYPE_NAME`. A name that resolves to a known parameterized
//! constructor applied to the wrong number of type arguments is
//! `E1204_TYPE_ARGUMENT_ARITY`.
//!
//! Precedence is fixed by ADR-0034: an unresolved name is reported and its
//! arity is not, because the arity of a type that does not exist is not a fact.
//! Argument types are only examined once the arity is right, so one mistake
//! cannot cascade into findings derived from a constructed type that does not
//! exist.
//!
//! `array<T, N>` is not one of the parameterized constructors: its second
//! argument is a compile-time constant rather than a type, and the parser keeps
//! it in its own form.
//!
//! This module also enforces the visibility rule of docs/42 section 1. `pub`
//! states a public source-level interface, so an importing module must be able
//! to name and resolve every type in it. A module-private nominal type in that
//! surface is `E1607_PRIVATE_PUBLIC_TYPE`.
//!
//! The surface is transitive: an exported record contributes its field types
//! and an exported enum its variant payload types, because a consumer cannot
//! construct or match one without naming them. A type used only inside a
//! function body, or only by a module-private item, is an implementation detail
//! and is not part of it.

use std::collections::{BTreeMap, BTreeSet};
use std::string::String;
use std::vec::Vec;

use crate::parser::{ImportKind, Schema, TypeSyntax, Visibility};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

/// Primitive type names (docs/40 section 1).
const PRIMITIVE_TYPES: [&str; 14] = [
    "bool", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "size", "duration", "string",
    "bytes", "unit",
];

/// Predeclared types that take no type arguments.
const NULLARY_TYPES: [&str; 8] = [
    "Event",
    "Semaphore",
    "Barrier",
    "Latch",
    "AtomicBool",
    "AtomicU32",
    "AtomicU64",
    "ConversionError",
];

/// The fixed arity of every parameterized V1 type constructor (docs/40
/// section 2). `array` is excluded: its second argument is a constant.
const PARAMETERIZED_TYPES: [(&str, usize); 11] = [
    ("Option", 1),
    ("Task", 1),
    ("TaskResult", 1),
    ("Shared", 1),
    ("Region", 1),
    ("DmaRegion", 1),
    ("Mutex", 1),
    ("RwLock", 1),
    ("Channel", 1),
    ("slice", 1),
    ("Result", 2),
];

/// What a local nominal type contributes to a public surface.
struct Nominal<'source> {
    exported: bool,
    surface: Vec<&'source TypeSyntax>,
}

/// Checks that no `pub` signature exposes a module-private nominal type.
pub(crate) fn check_public_signatures(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut nominals: BTreeMap<&str, Nominal> = BTreeMap::new();
    for declaration in schema.records() {
        nominals.insert(
            declaration.name().text(source),
            Nominal {
                exported: declaration.visibility() == Visibility::Public,
                surface: declaration
                    .fields()
                    .iter()
                    .map(|field| field.ty())
                    .collect(),
            },
        );
    }
    for declaration in schema.enums() {
        let mut surface: Vec<&TypeSyntax> = Vec::new();
        for variant in declaration.variants() {
            surface.extend(variant.tuple_types());
            surface.extend(variant.fields().iter().map(|field| field.ty()));
        }
        nominals.insert(
            declaration.name().text(source),
            Nominal {
                exported: declaration.visibility() == Visibility::Public,
                surface,
            },
        );
    }

    let mut walker = SurfaceWalker {
        source,
        nominals,
        diagnostics: Vec::new(),
    };
    for signature in schema.extern_functions() {
        walker.check_signature(signature);
    }
    for function in schema.functions() {
        walker.check_signature(function.signature());
    }
    walker.diagnostics
}

struct SurfaceWalker<'source> {
    source: &'source SourceUnit,
    nominals: BTreeMap<&'source str, Nominal<'source>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> SurfaceWalker<'source> {
    fn check_signature(&mut self, signature: &'source crate::parser::FunctionSignature) {
        if signature.visibility() != Visibility::Public {
            return;
        }
        let exported = signature.name().text(self.source);
        let mut visited: BTreeSet<&str> = BTreeSet::new();
        for parameter in signature.parameters() {
            self.walk(parameter.ty(), exported, &mut visited);
        }
        let result = signature.result();
        self.walk(result, exported, &mut visited);
    }

    fn walk(
        &mut self,
        ty: &'source TypeSyntax,
        exported: &'source str,
        visited: &mut BTreeSet<&'source str>,
    ) {
        match ty {
            TypeSyntax::Name { path, span } => {
                // An imported type is reachable at the module that declares it.
                if path.len() > 1 {
                    return;
                }
                let Some(name) = path.last().map(|segment| segment.text(self.source)) else {
                    return;
                };
                let Some(nominal) = self.nominals.get(name) else {
                    return;
                };
                if !nominal.exported {
                    self.diagnostics.push(
                        Diagnostic::new(
                            "E1607_PRIVATE_PUBLIC_TYPE",
                            Severity::Error,
                            Stage::Type,
                            *span,
                            self.source,
                        )
                        .with_field("type", name)
                        .with_field("exported_by", exported),
                    );
                    return;
                }
                if !visited.insert(name) {
                    return;
                }
                // An exported nominal type carries its own surface with it.
                let surface = nominal.surface.clone();
                for member in surface {
                    self.walk(member, exported, visited);
                }
            }
            TypeSyntax::Constructed { arguments, .. } => {
                for argument in arguments {
                    self.walk(argument, exported, visited);
                }
            }
            TypeSyntax::Array { element, .. } => self.walk(element, exported, visited),
            TypeSyntax::Tuple { elements, .. } => {
                for element in elements {
                    self.walk(element, exported, visited);
                }
            }
            TypeSyntax::Function {
                parameters, result, ..
            } => {
                for parameter in parameters {
                    self.walk(parameter, exported, visited);
                }
                self.walk(result, exported, visited);
            }
        }
    }
}

pub(crate) fn check_types(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let mut resolver = TypeResolver {
        source,
        local: BTreeSet::new(),
        imports: BTreeSet::new(),
        capability_interfaces: BTreeSet::new(),
        diagnostics: Vec::new(),
    };
    for declaration in schema.records() {
        resolver.local.insert(declaration.name().text(source));
    }
    for declaration in schema.enums() {
        resolver.local.insert(declaration.name().text(source));
    }
    for import in schema.outline().prefix().imports() {
        resolver.imports.insert(import.binding().text(source));
        // A capability interface is named by its full path, not through the
        // binding: `import capability system.time.Clock as clock` makes
        // `system.time.Clock` a reachable imported type (docs/42 section 4).
        if import.kind() == ImportKind::Capability {
            let path: std::vec::Vec<&str> = import
                .path()
                .iter()
                .map(|segment| segment.text(source))
                .collect();
            resolver.capability_interfaces.insert(path.join("."));
        }
    }

    for declaration in schema.records() {
        for field in declaration.fields() {
            resolver.visit(field.ty());
        }
    }
    for declaration in schema.enums() {
        for variant in declaration.variants() {
            for ty in variant.tuple_types() {
                resolver.visit(ty);
            }
            for field in variant.fields() {
                resolver.visit(field.ty());
            }
        }
    }
    for declaration in schema.consts() {
        resolver.visit(declaration.ty());
    }
    for signature in schema.extern_functions() {
        resolver.visit_signature(signature.parameters(), signature.result());
    }
    for function in schema.functions() {
        let signature = function.signature();
        resolver.visit_signature(signature.parameters(), signature.result());
        resolver.visit_block_types(function.body());
    }
    resolver.diagnostics
}

struct TypeResolver<'source> {
    source: &'source SourceUnit,
    local: BTreeSet<&'source str>,
    imports: BTreeSet<&'source str>,
    /// Full paths of the capability interfaces this module imports.
    capability_interfaces: BTreeSet<String>,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> TypeResolver<'source> {
    fn visit_signature(
        &mut self,
        parameters: &'source [crate::parser::FunctionParameter],
        result: &'source TypeSyntax,
    ) {
        for parameter in parameters {
            self.visit(parameter.ty());
        }
        self.visit(result);
    }

    fn visit_block_types(&mut self, block: &'source crate::parser::Block) {
        for statement in block.statements() {
            if let Some(ty) = statement.declared_type() {
                self.visit(ty);
            }
            for nested in [statement.body(), statement.else_body()]
                .into_iter()
                .flatten()
            {
                self.visit_block_types(nested);
            }
            if let Some(nested) = statement.else_if() {
                self.visit_statement_types(nested);
            }
            for branch in statement.branches() {
                self.visit_block_types(branch.body());
            }
        }
    }

    fn visit_statement_types(&mut self, statement: &'source crate::parser::Statement) {
        if let Some(ty) = statement.declared_type() {
            self.visit(ty);
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            self.visit_block_types(nested);
        }
        if let Some(nested) = statement.else_if() {
            self.visit_statement_types(nested);
        }
    }

    fn visit(&mut self, ty: &'source TypeSyntax) {
        match ty {
            TypeSyntax::Name { path, span } => {
                let spelled = self.spell(path);
                if self.resolves(path, &spelled) {
                    return;
                }
                self.unknown_type(*span, spelled);
            }
            TypeSyntax::Constructed {
                name,
                arguments,
                span,
            } => {
                let spelled = name.text(self.source);
                let Some((_, expected)) = PARAMETERIZED_TYPES
                    .iter()
                    .find(|(constructor, _)| *constructor == spelled)
                else {
                    // Only the fixed set is parameterized; anything else with
                    // `<...>` names no V1 constructor at all.
                    self.unknown_type(*span, String::from(spelled));
                    return;
                };
                if arguments.len() != *expected {
                    self.diagnostics.push(
                        Diagnostic::new(
                            "E1204_TYPE_ARGUMENT_ARITY",
                            Severity::Error,
                            Stage::Type,
                            *span,
                            self.source,
                        )
                        .with_field("constructor", spelled)
                        .with_field("expected_arity", *expected)
                        .with_field("actual_arity", arguments.len()),
                    );
                    // Arguments of a wrongly applied constructor are not
                    // examined: ADR-0034 forbids the cascade.
                    return;
                }
                for argument in arguments {
                    self.visit(argument);
                }
            }
            TypeSyntax::Array { element, .. } => self.visit(element),
            TypeSyntax::Tuple { elements, .. } => {
                for element in elements {
                    self.visit(element);
                }
            }
            TypeSyntax::Function {
                parameters, result, ..
            } => {
                for parameter in parameters {
                    self.visit(parameter);
                }
                self.visit(result);
            }
        }
    }

    fn spell(&self, path: &'source [crate::parser::Span]) -> String {
        path.iter()
            .map(|segment| segment.text(self.source))
            .collect::<Vec<_>>()
            .join(".")
    }

    /// Whether a type-name path resolves.
    ///
    /// ADR-0034 resolves the module or import part of a qualified name first.
    /// A binding that is not an import makes the whole qualified name resolve
    /// to nothing, so it is `E1203_UNKNOWN_TYPE_NAME` here; an `import` that
    /// itself fails to resolve is reported once by the module slice as
    /// `E1604_IMPORT_NOT_FOUND` rather than twice.
    ///
    /// A binding that is an import is accepted here, because a single module
    /// cannot see another module's type table. Whether that module declares
    /// the name is decided by the source-set slice.
    fn resolves(&self, path: &'source [crate::parser::Span], spelled: &str) -> bool {
        if path.len() > 1 {
            return self.capability_interfaces.contains(spelled)
                || self.imports.contains(path[0].text(self.source));
        }
        PRIMITIVE_TYPES.contains(&spelled)
            || NULLARY_TYPES.contains(&spelled)
            || PARAMETERIZED_TYPES
                .iter()
                .any(|(constructor, _)| *constructor == spelled)
            || spelled == "array"
            || self.local.contains(spelled)
    }

    fn unknown_type(&mut self, span: crate::parser::Span, spelled: String) {
        self.diagnostics.push(
            Diagnostic::new(
                "E1203_UNKNOWN_TYPE_NAME",
                Severity::Error,
                Stage::Type,
                span,
                self.source,
            )
            .with_field("type", spelled),
        );
    }
}

/// The nominal type names a module declares.
///
/// Used by the source-set slice to resolve a qualified type name against the
/// module its import binding names.
pub(crate) fn declared_type_names<'source>(
    source: &'source SourceUnit,
    schema: &'source Schema,
) -> BTreeSet<&'source str> {
    let mut names = BTreeSet::new();
    for declaration in schema.records() {
        names.insert(declaration.name().text(source));
    }
    for declaration in schema.enums() {
        names.insert(declaration.name().text(source));
    }
    names
}

/// Every qualified type name a module writes, as (binding, type, span).
pub(crate) fn qualified_type_uses<'source>(
    source: &'source SourceUnit,
    schema: &'source Schema,
) -> Vec<(&'source str, &'source str, crate::parser::Span)> {
    let mut uses = Vec::new();
    for declaration in schema.records() {
        for field in declaration.fields() {
            collect_qualified(source, field.ty(), &mut uses);
        }
    }
    for declaration in schema.enums() {
        for variant in declaration.variants() {
            for ty in variant.tuple_types() {
                collect_qualified(source, ty, &mut uses);
            }
            for field in variant.fields() {
                collect_qualified(source, field.ty(), &mut uses);
            }
        }
    }
    for declaration in schema.consts() {
        collect_qualified(source, declaration.ty(), &mut uses);
    }
    for signature in schema.extern_functions() {
        for parameter in signature.parameters() {
            collect_qualified(source, parameter.ty(), &mut uses);
        }
        collect_qualified(source, signature.result(), &mut uses);
    }
    for function in schema.functions() {
        let signature = function.signature();
        for parameter in signature.parameters() {
            collect_qualified(source, parameter.ty(), &mut uses);
        }
        collect_qualified(source, signature.result(), &mut uses);
        collect_qualified_in_block(source, function.body(), &mut uses);
    }
    uses
}

fn collect_qualified_in_block<'source>(
    source: &'source SourceUnit,
    block: &'source crate::parser::Block,
    out: &mut Vec<(&'source str, &'source str, crate::parser::Span)>,
) {
    for statement in block.statements() {
        if let Some(ty) = statement.declared_type() {
            collect_qualified(source, ty, out);
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            collect_qualified_in_block(source, nested, out);
        }
        for branch in statement.branches() {
            collect_qualified_in_block(source, branch.body(), out);
        }
    }
}

fn collect_qualified<'source>(
    source: &'source SourceUnit,
    ty: &'source TypeSyntax,
    out: &mut Vec<(&'source str, &'source str, crate::parser::Span)>,
) {
    match ty {
        TypeSyntax::Name { path, span } => {
            if path.len() > 1 {
                let binding = path[0].text(source);
                let name = path[path.len() - 1].text(source);
                out.push((binding, name, *span));
            }
        }
        TypeSyntax::Constructed { arguments, .. } => {
            for argument in arguments {
                collect_qualified(source, argument, out);
            }
        }
        TypeSyntax::Array { element, .. } => collect_qualified(source, element, out),
        TypeSyntax::Tuple { elements, .. } => {
            for element in elements {
                collect_qualified(source, element, out);
            }
        }
        TypeSyntax::Function {
            parameters, result, ..
        } => {
            for parameter in parameters {
                collect_qualified(source, parameter, out);
            }
            collect_qualified(source, result, out);
        }
    }
}

/// Builds the `E1203_UNKNOWN_TYPE_NAME` diagnostic for a qualified name whose
/// module resolved but does not declare it.
pub(crate) fn unknown_qualified_type(
    source: &SourceUnit,
    span: crate::parser::Span,
    spelled: String,
) -> Diagnostic {
    Diagnostic::new(
        "E1203_UNKNOWN_TYPE_NAME",
        Severity::Error,
        Stage::Type,
        span,
        source,
    )
    .with_field("type", spelled)
}
