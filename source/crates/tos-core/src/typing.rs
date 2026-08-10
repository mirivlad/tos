// SPDX-License-Identifier: GPL-3.0-or-later
//! Expression typing and return-type agreement (docs/40 sections 1–3, 5).
//!
//! This slice gives expressions a type and checks one rule with it: every
//! `return` in a function must carry the function's declared result type, and
//! `return;` in a non-`unit` function is the same error
//! (`E1222_RETURN_TYPE_MISMATCH`).
//!
//! Typing is deliberately partial. Any expression whose type the declarations
//! do not determine is [`Type::Unknown`], and an `Unknown` on either side of a
//! comparison agrees with everything, so an undetermined type never produces a
//! diagnostic. That keeps the slice from inventing findings while inference is
//! incomplete.
//!
//! An unsuffixed integer literal is contextually typed: docs/40 section 3 lets
//! it take the surrounding exact integer type, so it agrees with any of them.
//! Its range check is not performed here.
//!
//! The same section restricts `as`: it converts only between integers, widening
//! while preserving signedness. Anything else is `E1212_INVALID_AS_CONVERSION`,
//! except a cast of an opaque handle, which docs/40 says is deliberately not a
//! conversion error. `E1502_FORGED_CAPABILITY` covers a capability; the other
//! opaque types are described as taking "the corresponding nonconstructible-type
//! error", which no document names, so this slice reports nothing for them
//! rather than borrowing a code that means something else.
//!
//! Section 3 also makes assigning or passing values of *different integer
//! types* `E1210_INTEGER_TYPE_MISMATCH`. It names no code for a disagreement
//! between other kinds — a `bool` assigned to a `string`, say — so this slice
//! reports only the integer case the contract states.

use std::boxed::Box;
use std::collections::BTreeMap;
use std::string::{String, ToString};
use std::vec::Vec;

use crate::parser::{
    Block, CallArgument, EnumVariantForm, Expression, ExpressionForm, Pattern, PatternForm, Schema,
    Statement, StatementForm, TypeSyntax,
};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

/// The exact fixed-width integer type names (docs/40 section 1).
const INTEGER_TYPES: [&str; 8] = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"];

/// Opaque handle types, whose cast is not a conversion error (docs/40 §3).
const OPAQUE_TYPES: [&str; 14] = [
    "Task",
    "TaskResult",
    "Shared",
    "Region",
    "DmaRegion",
    "Mutex",
    "RwLock",
    "Channel",
    "Event",
    "Semaphore",
    "Barrier",
    "Latch",
    "AtomicBool",
    "AtomicU32",
];

/// The bit width of an exact integer type, and whether it is signed.
fn integer_shape(name: &str) -> Option<(u32, bool)> {
    let signed = name.starts_with('i');
    let width = name.get(1..)?.parse().ok()?;
    Some((width, signed))
}

/// A resolved TOS Core type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Type {
    /// Not determined by the declarations available to this slice.
    Unknown,
    Unit,
    Bool,
    /// A fixed-width integer, named exactly as written.
    Integer(String),
    /// An integer literal without a suffix, which takes its surrounding type.
    UnsuffixedInteger,
    Size,
    Duration,
    Text,
    Bytes,
    /// A record or enum declared in this module.
    Nominal(String),
    Constructed(String, Vec<Type>),
    Array(Box<Type>),
    Tuple(Vec<Type>),
    Function(Vec<Type>, Box<Type>),
}

impl Type {
    /// Whether an actual type may be used where an expected one is required.
    ///
    /// `Unknown` agrees with everything: it means "not determined", and
    /// reporting against it would be a guess. An unsuffixed integer literal
    /// agrees with any exact integer type or `size`.
    fn agrees_with(&self, expected: &Type) -> bool {
        match (self, expected) {
            (Type::Unknown, _) | (_, Type::Unknown) => true,
            (Type::UnsuffixedInteger, Type::Integer(_) | Type::Size)
            | (Type::Integer(_) | Type::Size, Type::UnsuffixedInteger) => true,
            (Type::Array(actual), Type::Array(wanted)) => actual.agrees_with(wanted),
            (Type::Tuple(actual), Type::Tuple(wanted)) => {
                actual.len() == wanted.len()
                    && actual
                        .iter()
                        .zip(wanted)
                        .all(|(one, other)| one.agrees_with(other))
            }
            (Type::Constructed(actual, left), Type::Constructed(wanted, right)) => {
                actual == wanted
                    && left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(one, other)| one.agrees_with(other))
            }
            (Type::Function(left, one), Type::Function(right, other)) => {
                left.len() == right.len()
                    && left.iter().zip(right).all(|(a, b)| a.agrees_with(b))
                    && one.agrees_with(other)
            }
            _ => self == expected,
        }
    }

    /// How the type is written in a diagnostic field.
    fn spell(&self) -> String {
        match self {
            Type::Unknown => String::from("<undetermined>"),
            Type::Unit => String::from("unit"),
            Type::Bool => String::from("bool"),
            Type::Integer(name) => name.clone(),
            Type::UnsuffixedInteger => String::from("<integer literal>"),
            Type::Size => String::from("size"),
            Type::Duration => String::from("duration"),
            Type::Text => String::from("string"),
            Type::Bytes => String::from("bytes"),
            Type::Nominal(name) => name.clone(),
            Type::Constructed(name, arguments) => {
                let inner: Vec<String> = arguments.iter().map(Type::spell).collect();
                std::format!("{name}<{}>", inner.join(", "))
            }
            Type::Array(element) => std::format!("array<{}, N>", element.spell()),
            Type::Tuple(elements) => {
                let inner: Vec<String> = elements.iter().map(Type::spell).collect();
                std::format!("({})", inner.join(", "))
            }
            Type::Function(parameters, result) => {
                let inner: Vec<String> = parameters.iter().map(Type::spell).collect();
                std::format!("fn ({}) -> {}", inner.join(", "), result.spell())
            }
        }
    }
}

/// What a module declares, for typing the expressions inside it.
struct Declarations<'source> {
    records: BTreeMap<&'source str, Vec<(&'source str, Type)>>,
    /// Variant name to the enum it belongs to, with its payload types.
    variants: BTreeMap<&'source str, (String, Vec<Type>)>,
    functions: BTreeMap<&'source str, (Vec<Type>, Type)>,
    consts: BTreeMap<&'source str, Type>,
}

pub(crate) fn check_typing(source: &SourceUnit, schema: &Schema) -> Vec<Diagnostic> {
    let declarations = collect(source, schema);
    let mut checker = TypeChecker {
        source,
        declarations,
        scopes: Vec::new(),
        diagnostics: Vec::new(),
    };
    for function in schema.functions() {
        let signature = function.signature();
        let result = resolve(source, signature.result());
        checker.push_scope();
        for parameter in signature.parameters() {
            let name = parameter.name().text(source);
            let ty = resolve(source, parameter.ty());
            checker.declare(name, ty);
        }
        checker.check_block(function.body(), &result);
        checker.pop_scope();
    }
    checker.diagnostics
}

fn collect<'source>(source: &'source SourceUnit, schema: &'source Schema) -> Declarations<'source> {
    let mut records = BTreeMap::new();
    for declaration in schema.records() {
        let fields = declaration
            .fields()
            .iter()
            .map(|field| (field.name().text(source), resolve(source, field.ty())))
            .collect();
        records.insert(declaration.name().text(source), fields);
    }
    let mut variants = BTreeMap::new();
    for declaration in schema.enums() {
        let owner = declaration.name().text(source).to_string();
        for variant in declaration.variants() {
            let payload = match variant.form() {
                EnumVariantForm::Tuple => variant
                    .tuple_types()
                    .iter()
                    .map(|ty| resolve(source, ty))
                    .collect(),
                EnumVariantForm::NamedFields => variant
                    .fields()
                    .iter()
                    .map(|field| resolve(source, field.ty()))
                    .collect(),
                EnumVariantForm::Unit => Vec::new(),
            };
            variants.insert(variant.name().text(source), (owner.clone(), payload));
        }
    }
    let mut functions = BTreeMap::new();
    for signature in schema.extern_functions() {
        functions.insert(
            signature.name().text(source),
            signature_types(source, signature),
        );
    }
    for function in schema.functions() {
        functions.insert(
            function.signature().name().text(source),
            signature_types(source, function.signature()),
        );
    }
    let consts = schema
        .consts()
        .iter()
        .map(|declaration| {
            (
                declaration.name().text(source),
                resolve(source, declaration.ty()),
            )
        })
        .collect();
    Declarations {
        records,
        variants,
        functions,
        consts,
    }
}

fn signature_types(
    source: &SourceUnit,
    signature: &crate::parser::FunctionSignature,
) -> (Vec<Type>, Type) {
    let parameters = signature
        .parameters()
        .iter()
        .map(|parameter| resolve(source, parameter.ty()))
        .collect();
    (parameters, resolve(source, signature.result()))
}

/// Turns written type syntax into a resolved type.
///
/// A name this slice does not recognize becomes `Unknown` rather than an error:
/// unresolvable type names are `E1203` from the type-resolution slice, and
/// reporting them again here would double one mistake.
fn resolve(source: &SourceUnit, ty: &TypeSyntax) -> Type {
    match ty {
        TypeSyntax::Name { path, .. } => {
            let name = match path.last() {
                Some(segment) => segment.text(source),
                None => return Type::Unknown,
            };
            if path.len() > 1 {
                // A type from another module: its identity is known, its shape
                // is not available to a single-module check.
                return Type::Unknown;
            }
            match name {
                "unit" => Type::Unit,
                "bool" => Type::Bool,
                "size" => Type::Size,
                "duration" => Type::Duration,
                "string" => Type::Text,
                "bytes" => Type::Bytes,
                _ if INTEGER_TYPES.contains(&name) => Type::Integer(name.to_string()),
                _ => Type::Nominal(name.to_string()),
            }
        }
        TypeSyntax::Constructed {
            name, arguments, ..
        } => Type::Constructed(
            name.text(source).to_string(),
            arguments.iter().map(|ty| resolve(source, ty)).collect(),
        ),
        TypeSyntax::Array { element, .. } => Type::Array(Box::new(resolve(source, element))),
        TypeSyntax::Tuple { elements, .. } => {
            Type::Tuple(elements.iter().map(|ty| resolve(source, ty)).collect())
        }
        TypeSyntax::Function {
            parameters, result, ..
        } => Type::Function(
            parameters.iter().map(|ty| resolve(source, ty)).collect(),
            Box::new(resolve(source, result)),
        ),
    }
}

/// Whether a type belongs to the integer family docs/40 section 3 governs.
fn is_integer_family(ty: &Type) -> bool {
    matches!(ty, Type::Integer(_) | Type::Size)
}

/// Whether a type is an opaque handle whose cast docs/40 routes elsewhere.
fn is_opaque(ty: &Type) -> bool {
    match ty {
        Type::Constructed(name, _) => OPAQUE_TYPES.contains(&name.as_str()),
        Type::Nominal(name) => OPAQUE_TYPES.contains(&name.as_str()) || name == "AtomicU64",
        Type::Function(_, _) => true,
        _ => false,
    }
}

struct TypeChecker<'source> {
    source: &'source SourceUnit,
    declarations: Declarations<'source>,
    scopes: Vec<BTreeMap<&'source str, Type>>,
    diagnostics: Vec<Diagnostic>,
}

impl<'source> TypeChecker<'source> {
    fn push_scope(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &'source str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    fn lookup(&self, name: &str) -> Option<Type> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    fn check_block(&mut self, block: &'source Block, result: &Type) {
        self.push_scope();
        for statement in block.statements() {
            self.check_statement(statement, result);
        }
        self.pop_scope();
    }

    fn check_statement(&mut self, statement: &'source Statement, result: &Type) {
        match statement.form() {
            StatementForm::Return => self.check_return(statement, result),
            StatementForm::Assignment => {
                let target = statement
                    .target()
                    .map(|place| self.type_of(place))
                    .unwrap_or(Type::Unknown);
                if let Some(expression) = statement.expression() {
                    let actual = self.type_of(expression);
                    self.check_integer_agreement(expression.span(), &target, &actual, "assignment");
                }
            }
            StatementForm::Let => {
                let declared = statement.declared_type().map(|ty| resolve(self.source, ty));
                let inferred = statement
                    .expression()
                    .map(|expression| self.type_of(expression))
                    .unwrap_or(Type::Unknown);
                let bound = declared.unwrap_or(inferred);
                if let Some(pattern) = statement.pattern() {
                    self.bind_pattern(pattern, &bound);
                }
            }
            _ => {}
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            self.check_block(nested, result);
        }
        if let Some(nested) = statement.else_if() {
            self.check_statement(nested, result);
        }
        for branch in statement.branches() {
            self.push_scope();
            self.check_block(branch.body(), result);
            self.pop_scope();
        }
    }

    /// Binds a pattern's names, giving a simple name the whole bound type.
    ///
    /// Destructured positions need the ADR-0033 expected-type resolution to
    /// know which variant a pattern names, so they bind as `Unknown` here.
    fn bind_pattern(&mut self, pattern: &'source Pattern, bound: &Type) {
        match pattern.form() {
            PatternForm::Name if !pattern.is_qualified() => {
                if let Some(name) = pattern.name() {
                    self.declare(name.text(self.source), bound.clone());
                }
            }
            PatternForm::Tuple => {
                if let Type::Tuple(elements) = bound {
                    for (element, ty) in pattern.elements().iter().zip(elements) {
                        self.bind_pattern(element, ty);
                    }
                    return;
                }
                for element in pattern.elements() {
                    self.bind_pattern(element, &Type::Unknown);
                }
            }
            PatternForm::Destructure => {
                for element in pattern.elements() {
                    self.bind_pattern(element, &Type::Unknown);
                }
            }
            _ => {}
        }
    }

    fn check_return(&mut self, statement: &'source Statement, result: &Type) {
        if matches!(result, Type::Unknown) {
            return;
        }
        let Some(expression) = statement.expression() else {
            // docs/40 section 5: `return;` in a non-unit function is the same
            // mismatch as returning the wrong type.
            if !matches!(result, Type::Unit) {
                self.report_return(statement.span(), result, &Type::Unit);
            }
            return;
        };
        let actual = self.type_of(expression);
        if actual.agrees_with(result) {
            return;
        }
        self.report_return(expression.span(), result, &actual);
    }

    /// Reports `E1210_INTEGER_TYPE_MISMATCH` when two integer types disagree.
    ///
    /// docs/40 section 3 names this code for assigning or passing values of
    /// different integer types. A disagreement between other kinds has no
    /// allocated code, so nothing is reported for it here.
    fn check_integer_agreement(
        &mut self,
        span: crate::parser::Span,
        expected: &Type,
        actual: &Type,
        position: &'static str,
    ) {
        if !is_integer_family(expected) || !is_integer_family(actual) {
            return;
        }
        if actual.agrees_with(expected) {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                "E1210_INTEGER_TYPE_MISMATCH",
                Severity::Error,
                Stage::Type,
                span,
                self.source,
            )
            .with_field("expected", expected.spell())
            .with_field("actual", actual.spell())
            .with_field("position", position),
        );
    }

    fn report_return(&mut self, span: crate::parser::Span, expected: &Type, actual: &Type) {
        self.diagnostics.push(
            Diagnostic::new(
                "E1222_RETURN_TYPE_MISMATCH",
                Severity::Error,
                Stage::Type,
                span,
                self.source,
            )
            .with_field("expected", expected.spell())
            .with_field("actual", actual.spell()),
        );
    }

    /// The type of an expression, or `Unknown` when the declarations do not
    /// determine it.
    fn type_of(&mut self, expression: &'source Expression) -> Type {
        match expression.form() {
            ExpressionForm::Literal => self.literal_type(expression),
            ExpressionForm::Name => self.name_type(expression.span().text(self.source)),
            ExpressionForm::Group => expression
                .inner()
                .map(|inner| self.type_of(inner))
                .unwrap_or(Type::Unknown),
            ExpressionForm::Cast => self.cast_type(expression),
            ExpressionForm::Tuple => {
                let elements = expression
                    .elements()
                    .iter()
                    .map(|element| self.type_of(element))
                    .collect();
                Type::Tuple(elements)
            }
            ExpressionForm::Array => {
                let element = expression
                    .elements()
                    .first()
                    .map(|first| self.type_of(first))
                    .unwrap_or(Type::Unknown);
                Type::Array(Box::new(element))
            }
            ExpressionForm::Index => match expression.inner().map(|inner| self.type_of(inner)) {
                Some(Type::Array(element)) => *element,
                _ => Type::Unknown,
            },
            ExpressionForm::Field => self.field_type(expression),
            ExpressionForm::Call => self.call_type(expression),
            ExpressionForm::Binary => self.binary_type(expression),
            ExpressionForm::Unary => self.unary_type(expression),
            ExpressionForm::Question => match expression.inner().map(|inner| self.type_of(inner)) {
                // `expr?` propagates the Err and yields the Ok payload.
                Some(Type::Constructed(name, arguments)) if name == "Result" => {
                    arguments.first().cloned().unwrap_or(Type::Unknown)
                }
                _ => Type::Unknown,
            },
            _ => Type::Unknown,
        }
    }

    /// Types an `as` conversion and checks it against docs/40 section 3.
    ///
    /// Only an integer widening that preserves signedness is permitted. A cast
    /// whose operand type is undetermined, or is an opaque handle, reports
    /// nothing: the first would be a guess and the second is explicitly not a
    /// conversion error.
    fn cast_type(&mut self, expression: &'source Expression) -> Type {
        let Some(target_syntax) = expression.cast_type() else {
            return Type::Unknown;
        };
        let target = resolve(self.source, target_syntax);
        let source_type = expression
            .inner()
            .map(|inner| self.type_of(inner))
            .unwrap_or(Type::Unknown);
        if self.cast_is_permitted(&source_type, &target) {
            return target;
        }
        self.diagnostics.push(
            Diagnostic::new(
                "E1212_INVALID_AS_CONVERSION",
                Severity::Error,
                Stage::Type,
                expression.span(),
                self.source,
            )
            .with_field("from", source_type.spell())
            .with_field("to", target.spell()),
        );
        target
    }

    fn cast_is_permitted(&self, from: &Type, to: &Type) -> bool {
        if matches!(from, Type::Unknown) || matches!(to, Type::Unknown) {
            return true;
        }
        if is_opaque(from) || is_opaque(to) {
            // docs/40 section 3 routes these elsewhere; no code names the
            // condition for the non-capability handles, so nothing is reported.
            return true;
        }
        // An unsuffixed literal takes the target type directly.
        if matches!(from, Type::UnsuffixedInteger) && matches!(to, Type::Integer(_)) {
            return true;
        }
        let (Type::Integer(source), Type::Integer(target)) = (from, to) else {
            return false;
        };
        let (Some((source_width, source_signed)), Some((target_width, target_signed))) =
            (integer_shape(source), integer_shape(target))
        else {
            return false;
        };
        source_signed == target_signed && target_width > source_width
    }

    fn literal_type(&self, expression: &'source Expression) -> Type {
        let text = expression.span().text(self.source);
        if text == "true" || text == "false" {
            return Type::Bool;
        }
        if text.starts_with('"') {
            return Type::Text;
        }
        if text.starts_with("b\"") {
            return Type::Bytes;
        }
        for suffix in ["KiB", "MiB", "GiB", "B"] {
            if text.ends_with(suffix) {
                return Type::Size;
            }
        }
        for suffix in ["ns", "us", "ms", "min", "h", "s"] {
            if text.ends_with(suffix) {
                return Type::Duration;
            }
        }
        for name in INTEGER_TYPES {
            if text.ends_with(name) {
                return Type::Integer(name.to_string());
            }
        }
        Type::UnsuffixedInteger
    }

    fn name_type(&self, name: &str) -> Type {
        if let Some(ty) = self.lookup(name) {
            return ty;
        }
        if let Some(ty) = self.declarations.consts.get(name) {
            return ty.clone();
        }
        if let Some((_, payload)) = self.declarations.variants.get(name) {
            // A bare unit variant is a value of its enum; one with a payload is
            // a constructor, which only a call applies.
            if payload.is_empty() {
                if let Some((owner, _)) = self.declarations.variants.get(name) {
                    return Type::Nominal(owner.clone());
                }
            }
        }
        Type::Unknown
    }

    fn field_type(&mut self, expression: &'source Expression) -> Type {
        let Some(name) = expression.name() else {
            return Type::Unknown;
        };
        let Some(receiver) = expression.inner() else {
            return Type::Unknown;
        };
        let Type::Nominal(record) = self.type_of(receiver) else {
            return Type::Unknown;
        };
        let field = name.text(self.source);
        self.declarations
            .records
            .get(record.as_str())
            .and_then(|fields| {
                fields
                    .iter()
                    .find(|(declared, _)| *declared == field)
                    .map(|(_, ty)| ty.clone())
            })
            .unwrap_or(Type::Unknown)
    }

    fn call_type(&mut self, expression: &'source Expression) -> Type {
        let actual = self.type_arguments(expression.arguments());
        let Some(callee) = expression.callee() else {
            return Type::Unknown;
        };
        if callee.form() != ExpressionForm::Name {
            return Type::Unknown;
        }
        let name = callee.span().text(self.source);
        if let Some((parameters, result)) = self.declarations.functions.get(name) {
            let result = result.clone();
            let parameters = parameters.clone();
            // Only a positional list lines up with the parameter order; a named
            // list belongs to a constructor and is checked by field name.
            if expression
                .arguments()
                .iter()
                .all(|argument| argument.name().is_none())
                && parameters.len() == actual.len()
            {
                for ((wanted, given), argument) in
                    parameters.iter().zip(&actual).zip(expression.arguments())
                {
                    self.check_integer_agreement(argument.span(), wanted, given, "argument");
                }
            }
            return result;
        }
        if self.declarations.records.contains_key(name) {
            return Type::Nominal(name.to_string());
        }
        if let Some((owner, _)) = self.declarations.variants.get(name) {
            return Type::Nominal(owner.clone());
        }
        // The fixed checked conversions return `Result<D, ConversionError>`.
        if let Some(destination) = name.strip_prefix("to_") {
            if INTEGER_TYPES.contains(&destination) {
                return Type::Constructed(
                    String::from("Result"),
                    std::vec![
                        Type::Integer(destination.to_string()),
                        Type::Nominal(String::from("ConversionError")),
                    ],
                );
            }
        }
        Type::Unknown
    }

    fn type_arguments(&mut self, arguments: &'source [CallArgument]) -> Vec<Type> {
        arguments
            .iter()
            .map(|argument| self.type_of(argument.value()))
            .collect()
    }

    fn binary_type(&mut self, expression: &'source Expression) -> Type {
        let operator = expression.operator_text(self.source).unwrap_or_default();
        let left = expression
            .left()
            .map(|operand| self.type_of(operand))
            .unwrap_or(Type::Unknown);
        let right = expression
            .right()
            .map(|operand| self.type_of(operand))
            .unwrap_or(Type::Unknown);
        match operator {
            "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => Type::Bool,
            // A shift takes its width from the shifted value.
            "<<" | ">>" => left,
            _ => {
                if matches!(left, Type::UnsuffixedInteger) {
                    right
                } else {
                    left
                }
            }
        }
    }

    fn unary_type(&mut self, expression: &'source Expression) -> Type {
        let operator = expression.operator_text(self.source).unwrap_or_default();
        let inner = expression
            .inner()
            .map(|operand| self.type_of(operand))
            .unwrap_or(Type::Unknown);
        match operator {
            "!" => Type::Bool,
            // Consuming a task handle yields its outcome.
            "await" | "join" => match inner {
                Type::Constructed(name, arguments) if name == "Task" => Type::Constructed(
                    String::from("TaskResult"),
                    std::vec![arguments.first().cloned().unwrap_or(Type::Unknown)],
                ),
                _ => Type::Unknown,
            },
            _ => inner,
        }
    }
}
