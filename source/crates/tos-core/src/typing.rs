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
//!
//! An index has exact type `size`; an integer literal may be contextually typed
//! as one. Any other index type is `E1211_INDEX_TYPE_MISMATCH`.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::parser::{
    Block, CallArgument, EnumVariantForm, Expression, ExpressionForm, Pattern, PatternForm, Schema,
    Statement, StatementForm, TypeSyntax,
};
use crate::{Diagnostic, Severity, SourceUnit, Stage};

/// The exact fixed-width integer type names (docs/40 section 1).
const INTEGER_TYPES: [&str; 8] = ["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64"];

/// Opaque handle types, whose cast is not a conversion error (docs/40 §3).
/// Types V1 source may not bring into existence (ADR-0039).
///
/// A value of one of these is obtained the way the language provides — a task
/// from `spawn`, a `Shared<T>` from `share` — never fabricated out of data.
/// `TaskResult<T>` is deliberately absent: `Completed` and `Cancelled` are
/// predeclared constructors, so it is an ordinary affine result value source is
/// meant to build.
const NONCONSTRUCTIBLE_TYPES: [&str; 19] = [
    // Device memory is obtained from a PCI function and from nothing else
    // (ADR-0081 §13). Writing one is a forged mapping, exactly as writing a
    // region is a forged grant.
    "MmioRegion",
    "MmioRegionMut",
    "Task",
    "Shared",
    "Region",
    "DmaRegion",
    "Mutex",
    "RwLock",
    // ADR-0036: a guard exists only as the result of a lock operation. There is
    // no constructor syntax for one, so writing one is a forged guard.
    "MutexGuard",
    "ReadGuard",
    "WriteGuard",
    "Channel",
    "Event",
    "Semaphore",
    "Barrier",
    "Latch",
    "AtomicBool",
    "AtomicU32",
    "AtomicU64",
];

/// The internal constructor name for a region written with or without `mut`.
fn mutable_region_name(written: &str, mutable: bool) -> String {
    match (written, mutable) {
        ("Region", true) => String::from(REGION_MUT),
        ("DmaRegion", true) => String::from(DMA_REGION_MUT),
        _ => String::from(written),
    }
}

/// Whether a region element may be accessed directly (ADR-0081 §3).
///
/// **Only types whose in-memory representation the language contract already
/// fixes.** A nominal record's field order is the frontend's choice, and
/// admitting `region[i]` over one would silently publish that choice as a
/// binary format shared with a device or another process. If aggregate layout
/// in shared memory is ever wanted it needs its own contract; nothing in Stage 4
/// requires it.
pub(crate) fn accessible_element(element: &Type) -> bool {
    matches!(element, Type::Bool | Type::Integer(_))
}

/// The internal name of a mutably granted region. Never written in source.
pub(crate) const REGION_MUT: &str = "Region<mut>";
/// The internal name of a mutably granted device-visible region.
pub(crate) const DMA_REGION_MUT: &str = "DmaRegion<mut>";

/// The four facts of ADR-0037 section 2, as the checker needs them.
///
/// Both DMA variants are conservative in V1: making `DmaRegion<T>` shareable
/// would let it become `Shared<DmaRegion<T>>`, and a `Shared<T>` is `Copy`, so
/// the handle could be copied into several tasks — exactly the crossing the
/// rule against a DMA region crossing a task boundary exists to forbid.
pub(crate) fn region_facts(name: &str) -> Option<RegionFacts> {
    match name {
        "Region" => Some(RegionFacts {
            mutable: false,
            shareable: true,
            transferable: true,
            reason: "region",
        }),
        REGION_MUT => Some(RegionFacts {
            mutable: true,
            shareable: false,
            transferable: false,
            reason: "mutable region",
        }),
        "DmaRegion" => Some(RegionFacts {
            mutable: false,
            shareable: false,
            transferable: false,
            reason: "DMA region",
        }),
        DMA_REGION_MUT => Some(RegionFacts {
            mutable: true,
            shareable: false,
            transferable: false,
            reason: "DMA region",
        }),
        _ => None,
    }
}

/// What ADR-0037 section 2 fixes about one region type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RegionFacts {
    pub(crate) mutable: bool,
    pub(crate) shareable: bool,
    pub(crate) transferable: bool,
    /// The word a capture diagnostic uses for this kind of region.
    pub(crate) reason: &'static str,
}

/// Whether a type is transitively immutable, as `share` requires.
///
/// `T` and everything reachable from it must contain no mutable region, no
/// mutable borrow and no guard. A borrow is not representable as a `Type` here,
/// so the reachable cases are the mutable regions and the three guards.
pub(crate) fn transitively_immutable(ty: &Type, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    match ty {
        Type::Constructed(name, arguments) => {
            if name == REGION_MUT || name == DMA_REGION_MUT || GUARDS.contains(&name.as_str()) {
                return false;
            }
            arguments
                .iter()
                .all(|argument| transitively_immutable(argument, depth + 1))
        }
        Type::Array(element) => transitively_immutable(element, depth + 1),
        Type::Tuple(elements) => elements
            .iter()
            .all(|element| transitively_immutable(element, depth + 1)),
        Type::Function(parameters, result) => {
            parameters
                .iter()
                .all(|parameter| transitively_immutable(parameter, depth + 1))
                && transitively_immutable(result, depth + 1)
        }
        _ => true,
    }
}

/// The three guard constructors, which are never transitively immutable.
const GUARDS: [&str; 3] = ["MutexGuard", "ReadGuard", "WriteGuard"];

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
    /// Whether values of this type are `Copy` (docs/40 section 5).
    ///
    /// V1 has no Copy marker, trait or user override: the set is fixed. A
    /// tuple is `Copy` exactly when every element is, and an array exactly when
    /// its element type is. User records and enums are always affine, as are
    /// `Option`, `Result` and `TaskResult`. An undetermined type is treated as
    /// `Copy` so that an unknown never produces a move diagnostic.
    pub(crate) fn is_copy(&self) -> bool {
        match self {
            Type::Unknown
            | Type::Unit
            | Type::Bool
            | Type::Integer(_)
            | Type::UnsuffixedInteger
            | Type::Size
            | Type::Duration => true,
            Type::Text | Type::Bytes | Type::Nominal(_) | Type::Function(_, _) => false,
            Type::Constructed(name, _) => name == "Shared",
            Type::Tuple(elements) => elements.iter().all(Type::is_copy),
            Type::Array(element) => element.is_copy(),
        }
    }

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
                // A mutably granted region is carried under an internal name so
                // no code path can forget the mode; it is spelled back the way
                // it was written, because a diagnostic naming a type nobody
                // typed sends the reader looking for it.
                match name.as_str() {
                    REGION_MUT => alloc::format!("Region<mut {}>", inner.join(", ")),
                    DMA_REGION_MUT => alloc::format!("DmaRegion<mut {}>", inner.join(", ")),
                    _ => alloc::format!("{name}<{}>", inner.join(", ")),
                }
            }
            Type::Array(element) => alloc::format!("array<{}, N>", element.spell()),
            Type::Tuple(elements) => {
                let inner: Vec<String> = elements.iter().map(Type::spell).collect();
                alloc::format!("({})", inner.join(", "))
            }
            Type::Function(parameters, result) => {
                let inner: Vec<String> = parameters.iter().map(Type::spell).collect();
                alloc::format!("fn ({}) -> {}", inner.join(", "), result.spell())
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
    analyse(source, schema).0
}

/// The declared field types of every local record, for resolving a place path.
///
/// Ownership needs the type at `p.x`, not just at `p`, to know whether that
/// step is `Copy`. It reads the same declarations typing already collected.
pub(crate) fn record_fields(
    source: &SourceUnit,
    schema: &Schema,
) -> BTreeMap<String, Vec<(String, Type)>> {
    collect(source, schema)
        .records
        .into_iter()
        .map(|(name, fields)| {
            (
                name.to_string(),
                fields
                    .into_iter()
                    .map(|(field, ty)| (field.to_string(), ty))
                    .collect(),
            )
        })
        .collect()
}

/// The type of every parameter and `let` binding, keyed by the byte offset of
/// its name.
///
/// Ownership needs the same types typing already computes, so it reads them
/// from this one inference rather than repeating it.
pub(crate) fn binding_types(source: &SourceUnit, schema: &Schema) -> BTreeMap<usize, Type> {
    analyse(source, schema).1
}

fn analyse(source: &SourceUnit, schema: &Schema) -> (Vec<Diagnostic>, BTreeMap<usize, Type>) {
    let declarations = collect(source, schema);
    let mut checker = TypeChecker {
        source,
        declarations,
        scopes: Vec::new(),
        bindings: BTreeMap::new(),
        diagnostics: Vec::new(),
    };
    for function in schema.functions() {
        let signature = function.signature();
        let result = resolve(source, signature.result());
        checker.push_scope();
        for parameter in signature.parameters() {
            let name = parameter.name().text(source);
            let ty = resolve(source, parameter.ty());
            checker
                .bindings
                .insert(parameter.name().start(), ty.clone());
            checker.declare(name, ty);
        }
        checker.check_block(function.body(), &result);
        checker.pop_scope();
    }
    (checker.diagnostics, checker.bindings)
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
            name,
            arguments,
            mutable,
            ..
        } => Type::Constructed(
            // ADR-0037: a region's granted mode is part of its type, because
            // the four facts — Copy, mutable, Shareable, Transferable — differ
            // between the modes. It is carried as a distinct constructor rather
            // than as a flag beside one, so no code path can read the type and
            // forget to look at the mode; `spell` prints the written form back.
            mutable_region_name(name.text(source), *mutable),
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

/// The constructor or record name a type is written with, or `""` for a type
/// that has none. A nominal rule asks what a type *is*, so a scalar, a tuple
/// and an array answer nothing rather than something that resembles a region.
fn nominal_name(ty: &Type) -> &str {
    match ty {
        Type::Constructed(name, _) | Type::Nominal(name) => name.as_str(),
        _ => "",
    }
}

/// The source type a predeclared operation's result rule denotes (ADR-0088).
///
/// The same rule the lowerer reads for the same call's IR type, so a call the
/// checker typed one way cannot be lowered another.
fn predeclared_result(rule: tos_predeclared::ResultRule, actual: &[Type]) -> Type {
    use tos_predeclared::ResultRule as Rule;
    match rule {
        Rule::Unit => Type::Unit,
        Rule::Integer(kind) => Type::Integer(String::from(kind.spelled())),
        Rule::Conversion(kind) => Type::Constructed(
            String::from("Result"),
            alloc::vec![
                Type::Integer(String::from(kind.spelled())),
                Type::Nominal(String::from("ConversionError")),
            ],
        ),
        Rule::SameAsOperand(position) => actual.get(position).cloned().unwrap_or(Type::Unknown),
        Rule::SharedOfOperand(position) => Type::Constructed(
            String::from("Shared"),
            alloc::vec![actual.get(position).cloned().unwrap_or(Type::Unknown)],
        ),
    }
}

/// Whether a type is one V1 source may not fabricate a value of (ADR-0039).
/// Whether a written type name is one V1 source may not bring into existence.
///
/// ADR-0039 fixes the set; ADR-0036 adds the three guards to it. Exposed so the
/// name resolver can tell a forged handle from a name that simply does not
/// exist: `MutexGuard(0i32)` is a guard nobody may construct, not an unknown
/// value, and reporting it as unknown would send the reader looking for a
/// declaration that must never be written.
pub(crate) fn is_nonconstructible_name(name: &str) -> bool {
    NONCONSTRUCTIBLE_TYPES.contains(&name)
}

fn is_nonconstructible(ty: &Type) -> bool {
    match ty {
        Type::Constructed(name, _) => NONCONSTRUCTIBLE_TYPES.contains(&name.as_str()),
        Type::Nominal(name) => NONCONSTRUCTIBLE_TYPES.contains(&name.as_str()),
        // A function or closure value comes from a declaration or a closure
        // expression, never from a conversion.
        Type::Function(_, _) => true,
        _ => false,
    }
}

struct TypeChecker<'source> {
    source: &'source SourceUnit,
    declarations: Declarations<'source>,
    scopes: Vec<BTreeMap<&'source str, Type>>,
    /// Binding name offset to its type, for the ownership slice.
    bindings: BTreeMap<usize, Type>,
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
                if let Some(place) = statement.target() {
                    self.check_write_through_region(place);
                }
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
                // **An annotation constrains its initializer** (docs/40 §2), and
                // until ADR-0087 allocated a code for the residual case nothing
                // said so: `let flag: bool = 1i64;` was accepted, lowered, and
                // left for the engine to discover if it discovered it at all.
                //
                // The numeric case stays `E1210`'s, which is more specific and
                // already owns it; `Unknown` reports nothing, because reporting
                // against an undetermined type would be a guess; and an
                // unsuffixed literal agrees with any exact numeric type by
                // §3's contextual rule, which `agrees_with` already encodes.
                if let (Some(declared), Some(expression)) = (&declared, statement.expression()) {
                    // **An annotation that names nothing is `E1203`'s**, and a
                    // value-type finding against a type that does not resolve
                    // would be reporting against a name rather than a type.
                    // ADR-0087 §2a's precedence, applied to the one case where
                    // the annotation itself is the defect.
                    let resolvable = self.is_declared(declared);
                    if resolvable && !inferred.agrees_with(declared) {
                        // **Which code owns the mismatch** (ADR-0087 §2a). Two
                        // of the eight exact integer types disagreeing is what
                        // `E1210` is for — "a value of one integer type is
                        // assigned … where a different integer type is
                        // required" — and an initializer under an annotation is
                        // that assignment. Everything else is the residual.
                        let numeric =
                            matches!((&inferred, declared), (Type::Integer(_), Type::Integer(_)));
                        let binding = statement
                            .pattern()
                            .map(|pattern| pattern.span().text(self.source).to_string());
                        let mut diagnostic = Diagnostic::new(
                            if numeric {
                                "E1210_INTEGER_TYPE_MISMATCH"
                            } else {
                                "E1216_VALUE_TYPE_MISMATCH"
                            },
                            Severity::Error,
                            Stage::Type,
                            expression.span(),
                            self.source,
                        )
                        .with_field("context", "binding")
                        .with_field("expected", declared.spell())
                        .with_field("actual", inferred.spell());
                        if let Some(binding) = binding {
                            diagnostic = diagnostic.with_field("binding", binding);
                        }
                        self.diagnostics.push(diagnostic);
                    }
                }
                let bound = declared.unwrap_or(inferred);
                if let Some(pattern) = statement.pattern() {
                    self.require_irrefutable(pattern, &bound, "let");
                    self.bind_pattern(pattern, &bound);
                }
            }
            // An expression statement is still a typed position: its call's
            // arguments and its operands disagree or they do not, and whether
            // the value is used cannot change that.
            StatementForm::Expression | StatementForm::Cancel => {
                if let Some(expression) = statement.expression() {
                    let _ = self.type_of(expression);
                }
            }
            // A control head is an expression too, and so is a `for` sequence.
            StatementForm::If
            | StatementForm::While
            | StatementForm::Match
            | StatementForm::For => {
                if let Some(head) = statement.expression() {
                    let sequence = self.type_of(head);
                    if statement.form() == StatementForm::For {
                        // A `for` binds one element per iteration, so its
                        // pattern must match every element it will be given.
                        if let Some(pattern) = statement.pattern() {
                            let element = match &sequence {
                                Type::Array(element) => (**element).clone(),
                                Type::Constructed(name, arguments) if name == "slice" => {
                                    arguments.first().cloned().unwrap_or(Type::Unknown)
                                }
                                _ => Type::Unknown,
                            };
                            self.require_irrefutable(pattern, &element, "for");
                        }
                    }
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
        // What the arms below are matching on. A statement with no branches has
        // no subject to compute, and asking for one would type an `if` head as
        // though it were a scrutinee.
        let subject = match statement.branches().is_empty() {
            true => Type::Unknown,
            false => statement
                .expression()
                .map(|head| self.type_of(head))
                .unwrap_or(Type::Unknown),
        };
        for branch in statement.branches() {
            self.push_scope();
            // **An arm's pattern binds, and it had not.** A scope was pushed and
            // the body checked inside it, but nothing was ever declared in it —
            // so `match (made) { Ok(region) => ... }` gave `region` no type at
            // all, and every slice that asks what a binding is got `Unknown`.
            //
            // Invisible while a payload was only ever a scalar or a record: the
            // slices that care read a *type*, and nothing they decide about an
            // `i64` differs from what they decide about nothing. It stops being
            // invisible when the payload is a mutably granted region, because
            // writing through one is permitted by its type (ADR-0037 §1) and by
            // nothing else.
            self.bind_pattern(branch.pattern(), &subject);
            self.check_block(branch.body(), result);
            self.pop_scope();
        }
    }

    /// Binds a pattern's names, giving a simple name the whole bound type.
    ///
    /// Destructured positions need the ADR-0033 expected-type resolution to
    /// know which variant a pattern names, so they bind as `Unknown` here.
    /// Reports a pattern that may fail to match where one may not (ADR-0046).
    ///
    /// `let` and `for` bind unconditionally: there is no arm to fall through to
    /// and no result to signal a miss with. A refutable pattern there would
    /// need a hidden runtime trap or a hidden conditional branch, and V1 has
    /// neither — so it is refused at compile time instead.
    ///
    /// **Precedence.** Nothing is reported until the pattern has a settled
    /// meaning: an undetermined type, an unresolved constructor or a payload
    /// whose arity does not match are other slices' findings, and reporting
    /// refutability for a pattern nobody has resolved would be describing a
    /// construct that does not yet mean anything.
    fn require_irrefutable(
        &mut self,
        pattern: &'source Pattern,
        bound: &Type,
        context: &'static str,
    ) {
        if matches!(bound, Type::Unknown) {
            return;
        }
        let Some(reason) = self.refutable_because(pattern, bound) else {
            return;
        };
        self.diagnostics.push(
            Diagnostic::new(
                "E1223_REFUTABLE_PATTERN",
                Severity::Error,
                Stage::Type,
                pattern.span(),
                self.source,
            )
            .with_field("context", context)
            .with_field("reason", reason)
            .with_field("expected", bound.spell()),
        );
    }

    /// Why a pattern may fail to match a value of `bound`, when it may.
    ///
    /// Irrefutability is recursive: a tuple pattern is irrefutable exactly when
    /// every element of it is, and a constructor pattern is irrefutable only
    /// when its type has no other variant to be.
    fn refutable_because(&self, pattern: &'source Pattern, bound: &Type) -> Option<&'static str> {
        match pattern.form() {
            PatternForm::Wildcard => None,
            PatternForm::Name if !pattern.is_qualified() => {
                let spelled = pattern.name().map(|name| name.text(self.source));
                match spelled.and_then(|name| self.declarations.variants.get(name)) {
                    // A name that is a variant of the bound type is a
                    // constructor pattern, not a binding (ADR-0033).
                    Some((owner, _)) => self.variant_is_alone(owner),
                    None => None,
                }
            }
            PatternForm::Name => {
                let spelled = pattern.path().last().map(|last| last.text(self.source));
                match spelled.and_then(|name| self.declarations.variants.get(name)) {
                    Some((owner, _)) => self.variant_is_alone(owner),
                    None => None,
                }
            }
            PatternForm::Destructure => {
                let spelled = pattern.path().last().map(|last| last.text(self.source));
                let Some((owner, payload)) =
                    spelled.and_then(|name| self.declarations.variants.get(name))
                else {
                    // An unresolved constructor is a resolution finding.
                    return None;
                };
                if let Some(reason) = self.variant_is_alone(owner) {
                    return Some(reason);
                }
                // A sole variant still binds its payload, and a refutable
                // component makes the whole pattern refutable.
                if payload.len() != pattern.elements().len() {
                    return None;
                }
                pattern
                    .elements()
                    .iter()
                    .zip(payload)
                    .find_map(|(element, ty)| self.refutable_because(element, ty))
            }
            PatternForm::Tuple => {
                let Type::Tuple(elements) = bound else {
                    return None;
                };
                if elements.len() != pattern.elements().len() {
                    return None;
                }
                pattern
                    .elements()
                    .iter()
                    .zip(elements)
                    .find_map(|(element, ty)| self.refutable_because(element, ty))
            }
        }
    }

    /// Whether an enum has a variant this pattern would fail to match.
    fn variant_is_alone(&self, owner: &str) -> Option<&'static str> {
        let siblings = self
            .declarations
            .variants
            .values()
            .filter(|(enum_name, _)| enum_name == owner)
            .count();
        (siblings > 1).then_some("the type has other variants this pattern does not match")
    }

    fn bind_pattern(&mut self, pattern: &'source Pattern, bound: &Type) {
        match pattern.form() {
            PatternForm::Name if !pattern.is_qualified() => {
                if let Some(name) = pattern.name() {
                    self.bindings.insert(name.start(), bound.clone());
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
                // **A destructured payload is typed, and it had not been.**
                // Every element was bound `Unknown`, so `match (made) { Ok(r) =>
                // ... }` gave `r` no type at all — and a slice that asks what a
                // binding is got no answer. That was invisible while the only
                // way to hold a region was a parameter, whose type is written;
                // once an operation *returns* one, the region a driver actually
                // uses is a payload, and "no type" is the wrong answer to a
                // question the mutability rule has to ask.
                let payload = self.payload_types(pattern, bound);
                for (position, element) in pattern.elements().iter().enumerate() {
                    let ty = payload.get(position).cloned().unwrap_or(Type::Unknown);
                    self.bind_pattern(element, &ty);
                }
            }
            _ => {}
        }
    }

    /// The types a constructor pattern's elements bind to.
    ///
    /// Two sources, because there are two kinds of constructor. A variant this
    /// module declared carries its payload in the declaration table. A built-in
    /// one — `Ok`, `Err`, `Some`, `Completed` — carries no declaration at all:
    /// its payload is an argument of the *bound* type, so it is read from there
    /// and from nowhere else. Anything unresolved answers with nothing, and the
    /// caller binds `Unknown`, exactly as it did for everything before.
    fn payload_types(&self, pattern: &'source Pattern, bound: &Type) -> Vec<Type> {
        let Some(name) = pattern.path().last().map(|last| last.text(self.source)) else {
            return Vec::new();
        };
        if let Some((_, payload)) = self.declarations.variants.get(name) {
            return payload.clone();
        }
        let Type::Constructed(constructor, arguments) = bound else {
            return Vec::new();
        };
        match (constructor.as_str(), name) {
            ("Result", "Ok") | ("Option", "Some") | ("TaskResult", "Completed") => {
                arguments.first().cloned().into_iter().collect()
            }
            ("Result", "Err") => arguments.get(1).cloned().into_iter().collect(),
            _ => Vec::new(),
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
            ExpressionForm::Index => self.index_type(expression),
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
            // A closure literal's type is its declared parameters and the
            // result its body produces (docs/39 §"closure"). Typed here because
            // a binding may be annotated with a function type, and an
            // annotation that the closure does not satisfy is exactly the
            // residual mismatch ADR-0087 names — undetermined, it agreed with
            // every annotation and the disagreement reached the artifact.
            ExpressionForm::Closure => {
                let parameters: Vec<Type> = expression
                    .parameters()
                    .iter()
                    .map(|parameter| resolve(self.source, parameter.ty()))
                    .collect();
                let result = match expression.body().and_then(|body| self.body_result(body)) {
                    Some(result) => result,
                    None => Type::Unit,
                };
                Type::Function(parameters, Box::new(result))
            }
            _ => Type::Unknown,
        }
    }

    /// The type a body's `return` statements produce, when they agree.
    ///
    /// A closure has no written result type, so the body is what says it. Where
    /// the body returns nothing the result is `unit`; where its returns
    /// disagree the type is undetermined rather than guessed at from the first
    /// one.
    fn body_result(&mut self, body: &'source Block) -> Option<Type> {
        let mut settled: Option<Type> = None;
        for statement in body.statements() {
            if statement.form() != StatementForm::Return {
                continue;
            }
            let produced = match statement.expression() {
                Some(expression) => self.type_of(expression),
                None => Type::Unit,
            };
            match &settled {
                Some(existing) if !existing.agrees_with(&produced) => return None,
                Some(_) => {}
                None => settled = Some(produced),
            }
        }
        settled
    }

    /// Whether a type this slice resolved is one the module actually declares.
    ///
    /// `resolve` turns any name into a `Type::Nominal`, including one that
    /// names nothing — which is the names slice's finding to report, not this
    /// one's.
    fn is_declared(&self, ty: &Type) -> bool {
        match ty {
            Type::Nominal(name) => {
                self.declarations.records.contains_key(name.as_str())
                    || self
                        .declarations
                        .variants
                        .values()
                        .any(|(owner, _)| owner == name)
            }
            Type::Array(element) => self.is_declared(element),
            Type::Tuple(elements) => elements.iter().all(|element| self.is_declared(element)),
            Type::Constructed(_, arguments) => {
                arguments.iter().all(|argument| self.is_declared(argument))
            }
            Type::Function(parameters, result) => {
                parameters.iter().all(|p| self.is_declared(p)) && self.is_declared(result)
            }
            _ => true,
        }
    }

    /// Types an index expression and checks the index itself.
    ///
    /// docs/40 section 3 gives array, slice and region indexes exact type
    /// `size`, with an integer literal contextually typed as one.
    fn index_type(&mut self, expression: &'source Expression) -> Type {
        if let Some(index) = expression.right() {
            let actual = self.type_of(index);
            let acceptable = matches!(actual, Type::Size | Type::UnsuffixedInteger | Type::Unknown);
            if !acceptable {
                self.diagnostics.push(
                    Diagnostic::new(
                        "E1211_INDEX_TYPE_MISMATCH",
                        Severity::Error,
                        Stage::Type,
                        index.span(),
                        self.source,
                    )
                    .with_field("expected", "size")
                    .with_field("actual", actual.spell()),
                );
            }
        }
        match expression.inner().map(|inner| self.type_of(inner)) {
            Some(Type::Array(element)) => *element,
            Some(Type::Constructed(name, arguments)) if name == "slice" => {
                arguments.first().cloned().unwrap_or(Type::Unknown)
            }
            // A region's element, for all four granted modes (ADR-0081 §2).
            // `docs/44`'s `E1211` has covered "an array, slice or region index"
            // since V1 and ADR-0037 §7 requires a positive vector writing
            // through a `Region<mut T>`; what was missing was this line, not
            // the decision. Indexing does **not** make a region an array: its
            // grant, affinity, transfer and lifetime rules are untouched, and
            // its backing address stays unobservable.
            Some(Type::Constructed(name, arguments)) if region_facts(&name).is_some() => {
                let element = arguments.first().cloned().unwrap_or(Type::Unknown);
                if !accessible_element(&element) {
                    self.diagnostics.push(
                        Diagnostic::new(
                            "E1215_ARGUMENT_TYPE_MISMATCH",
                            Severity::Error,
                            Stage::Type,
                            expression.span(),
                            self.source,
                        )
                        .with_field("requirement", "region element access")
                        .with_field("actual", element.spell())
                        .with_field("reason", "element representation is not fixed"),
                    );
                    return Type::Unknown;
                }
                element
            }
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
        // ADR-0039 precedence: a capability forgery is `E1502` and is reported
        // by the capability slice, which sees the imported interfaces; any
        // other nonconstructible handle is `E1213`; and only an ordinary
        // conversion between value types reaches `E1212`.
        let nonconstructible = if is_nonconstructible(&target) {
            Some(target.spell())
        } else if is_nonconstructible(&source_type) {
            Some(source_type.spell())
        } else {
            None
        };
        let diagnostic = match nonconstructible {
            Some(spelled) => Diagnostic::new(
                "E1213_NONCONSTRUCTIBLE_TYPE",
                Severity::Error,
                Stage::Type,
                expression.span(),
                self.source,
            )
            .with_field("type", spelled)
            .with_field("operation", "as")
            .with_field("from", source_type.spell())
            .with_field("to", target.spell()),
            None => Diagnostic::new(
                "E1212_INVALID_AS_CONVERSION",
                Severity::Error,
                Stage::Type,
                expression.span(),
                self.source,
            )
            .with_field("from", source_type.spell())
            .with_field("to", target.spell()),
        };
        self.diagnostics.push(diagnostic);
        target
    }

    fn cast_is_permitted(&self, from: &Type, to: &Type) -> bool {
        if matches!(from, Type::Unknown) || matches!(to, Type::Unknown) {
            return true;
        }
        // A nonconstructible handle on either side is never a permitted
        // conversion; ADR-0039 decides which code names it.
        if is_nonconstructible(from) || is_nonconstructible(to) {
            return false;
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

    /// The guard a lock operation yields (ADR-0036 section 2).
    ///
    /// `Mutex<T>.lock()`, `RwLock<T>.read()` and `RwLock<T>.write()` are typed
    /// operations on the synchronization object, in the same receiver-operation
    /// form the atomics use. The guard's type is derived from the *receiver's*
    /// type, never from the name of the operation alone: a `.lock()` written on
    /// anything else is not a guard, and inferring one from the spelling would
    /// be the guess ADR-0035 forbids.
    fn lock_operation_type(&mut self, callee: &'source Expression) -> Type {
        let (Some(name), Some(receiver)) = (callee.name(), callee.inner()) else {
            return Type::Unknown;
        };
        let Type::Constructed(object, arguments) = self.type_of(receiver) else {
            return Type::Unknown;
        };
        let Some(protected) = arguments.first().cloned() else {
            return Type::Unknown;
        };
        let guard = match (object.as_str(), name.text(self.source)) {
            ("Mutex", "lock") => "MutexGuard",
            ("RwLock", "read") => "ReadGuard",
            ("RwLock", "write") => "WriteGuard",
            _ => return Type::Unknown,
        };
        Type::Constructed(String::from(guard), alloc::vec![protected])
    }

    /// Reports a write through an immutably granted region (ADR-0037 section 6).
    ///
    /// The *region's* declared mode decides, not the binding's. A `let mut`
    /// binding of a `Region<T>` may be rebound — that is a fact about the
    /// handle — but nothing may be written through it, because the grant that
    /// produced it was immutable. Deciding this from the binding form alone
    /// would let `let mut` launder an immutable grant into a writable one.
    fn check_write_through_region(&mut self, place: &'source Expression) {
        let mut root = place;
        let mut projected = false;
        while let Some(inner) = root.inner() {
            if !matches!(root.form(), ExpressionForm::Field | ExpressionForm::Index) {
                break;
            }
            projected = true;
            root = inner;
        }
        if !projected || root.form() != ExpressionForm::Name {
            return;
        }
        let Some(Type::Constructed(name, _)) = self.lookup(root.span().text(self.source)) else {
            return;
        };
        let Some(facts) = region_facts(&name) else {
            return;
        };
        if facts.mutable {
            return;
        }
        self.diagnostics.push(
            Diagnostic::new(
                "E1201_ASSIGN_TO_IMMUTABLE",
                Severity::Error,
                Stage::Type,
                place.span(),
                self.source,
            )
            .with_field("binding", root.span().text(self.source).to_string())
            .with_field("reason", "immutably granted region"),
        );
    }

    /// Reports `E1217_CALL_ARITY_MISMATCH` (ADR-0089).
    ///
    /// A call that supplies a different number of arguments than the resolved
    /// callee declares is refused **before** any argument is compared to a
    /// parameter, because with the wrong number of arguments there is no
    /// complete positional correspondence to type-check: the third argument of
    /// a two-parameter function corresponds to nothing, and a diagnostic
    /// pairing them would be describing a correspondence the source does not
    /// have. `docs/43` §4 has always required an exact ordered operand list;
    /// this is the source-level code that says when one was not supplied.
    fn report_arity(
        &mut self,
        span: crate::Span,
        callee: &str,
        expected: usize,
        actual: usize,
        context: &'static str,
    ) {
        self.diagnostics.push(
            Diagnostic::new(
                "E1217_CALL_ARITY_MISMATCH",
                Severity::Error,
                Stage::Type,
                span,
                self.source,
            )
            .with_field("callee", callee.to_string())
            .with_field("expected", expected)
            .with_field("actual", actual)
            .with_field("context", context),
        );
    }

    /// One predeclared call, checked against the shared contract (ADR-0088).
    ///
    /// **One authority for all of them.** `share`, the device accesses, the DMA
    /// ordering points and the checked conversions were four hand-written rules
    /// in this file, each knowing its own names, its own arity and its own
    /// result, and the conversions checked no argument at all — `to_u8(true)`
    /// and `to_u8(1u64, 2u64)` were both accepted here. They are now one walk
    /// over one table, which the lowerer reads for the same result type and the
    /// independent verifier reads for the same obligations.
    ///
    /// **The specialised semantics stay where they are.** The table says that
    /// `share`'s operand satisfies the Shareable rule and that a DMA ordering
    /// point names the closed `DmaRegion` family; what it does not do is decide
    /// whether a particular type is shareable, which is this checker's own
    /// traversal over its own types and the verifier's over its own.
    fn predeclared_type(
        &mut self,
        expression: &'source Expression,
        actual: &[Type],
        operation: &'static tos_predeclared::Operation,
    ) -> Type {
        if actual.len() != operation.arity() {
            self.report_arity(
                expression.span(),
                operation.name,
                operation.arity(),
                actual.len(),
                "predeclared",
            );
            return Type::Unknown;
        }
        for (position, rule) in operation.parameters.iter().enumerate() {
            let span = expression
                .arguments()
                .get(position)
                .map_or(expression.span(), |argument| argument.span());
            self.check_parameter_rule(operation.name, position, span, *rule, actual);
        }
        predeclared_result(operation.result, actual)
    }

    /// One argument position against the rule the contract states for it.
    ///
    /// An undetermined argument reports nothing: that would be a guess rather
    /// than a disagreement. Every other rule keeps the code the registry
    /// already allocates for the kind of position it is — a byte offset that is
    /// not `size` is `E1211` exactly as an index is, two exact integer types
    /// that disagree are `E1210`, and everything else is the residual
    /// `E1215` with the fields ADR-0037 gives it.
    fn check_parameter_rule(
        &mut self,
        callee: &str,
        position: usize,
        span: crate::Span,
        rule: tos_predeclared::ParameterRule,
        actual: &[Type],
    ) {
        use tos_predeclared::ParameterRule as Rule;
        let given = &actual[position];
        if matches!(given, Type::Unknown) {
            return;
        }
        match rule {
            Rule::Integer => {
                if !matches!(given, Type::Integer(_) | Type::UnsuffixedInteger) {
                    self.report_argument(span, callee, position, "an exact integer type", given);
                }
            }
            Rule::IntegerOrSize => {
                if !is_integer_family(given) && !matches!(given, Type::UnsuffixedInteger) {
                    self.report_argument(
                        span,
                        callee,
                        position,
                        "an exact integer type or size",
                        given,
                    );
                }
            }
            // A byte offset is a bounded offset into a mapping, which is the
            // position `E1211` already owns for arrays, slices and regions.
            Rule::Size => {
                if !matches!(given, Type::Size | Type::UnsuffixedInteger) {
                    self.diagnostics.push(
                        Diagnostic::new(
                            "E1211_INDEX_TYPE_MISMATCH",
                            Severity::Error,
                            Stage::Type,
                            span,
                            self.source,
                        )
                        .with_field("expected", "size")
                        .with_field("actual", given.spell()),
                    );
                }
            }
            Rule::Exactly(kind) => {
                let wanted = Type::Integer(String::from(kind.spelled()));
                self.report_disagreement(span, callee, position, &wanted, given);
            }
            // The type of an earlier operand, which is why the rules are
            // evaluated left to right and a `SameAs` never names a later one.
            Rule::SameAs(other) => {
                let wanted = actual[other].clone();
                if matches!(wanted, Type::Unknown) {
                    return;
                }
                self.report_disagreement(span, callee, position, &wanted, given);
            }
            Rule::MmioRegion | Rule::MmioRegionMut => {
                let mutable = matches!(rule, Rule::MmioRegionMut);
                let named = nominal_name(given);
                let acceptable = if mutable {
                    named == "MmioRegionMut"
                } else {
                    named == "MmioRegion" || named == "MmioRegionMut"
                };
                if !acceptable {
                    let reported = self
                        .argument_diagnostic(span)
                        .with_field("requirement", "device-memory access")
                        .with_field(
                            "expected",
                            if mutable {
                                "MmioRegionMut"
                            } else {
                                "MmioRegion"
                            },
                        )
                        .with_field("actual", given.spell());
                    self.diagnostics.push(reported);
                }
            }
            Rule::DmaRegion => {
                let named = nominal_name(given);
                if named != "DmaRegion" && named != DMA_REGION_MUT {
                    let reported = self
                        .argument_diagnostic(span)
                        .with_field("requirement", callee.to_string())
                        .with_field("expected", "DmaRegion")
                        .with_field("actual", given.spell());
                    self.diagnostics.push(reported);
                }
            }
            Rule::Shareable => {
                let shareable = match given {
                    Type::Constructed(name, _) => region_facts(name).map(|facts| facts.shareable),
                    _ => None,
                };
                if !shareable.unwrap_or(true) || !transitively_immutable(given, 0) {
                    self.report_argument(
                        span,
                        callee,
                        position,
                        "a transitively immutable, shareable type",
                        given,
                    );
                }
            }
        }
    }

    /// An exact type disagreement at an argument position, under whichever code
    /// owns it: `E1210` when both sides are numeric and the numeric rule is the
    /// one broken, and the residual `E1215` otherwise.
    fn report_disagreement(
        &mut self,
        span: crate::Span,
        callee: &str,
        position: usize,
        wanted: &Type,
        given: &Type,
    ) {
        if given.agrees_with(wanted) {
            return;
        }
        if is_integer_family(given) && is_integer_family(wanted) {
            self.check_integer_agreement(span, wanted, given, "argument");
            return;
        }
        let expected = wanted.spell();
        self.report_argument(span, callee, position, &expected, given);
    }

    fn argument_diagnostic(&self, span: crate::Span) -> Diagnostic {
        Diagnostic::new(
            "E1215_ARGUMENT_TYPE_MISMATCH",
            Severity::Error,
            Stage::Type,
            span,
            self.source,
        )
    }

    fn report_argument(
        &mut self,
        span: crate::Span,
        callee: &str,
        position: usize,
        expected: &str,
        given: &Type,
    ) {
        let reported = self
            .argument_diagnostic(span)
            .with_field("callee", callee.to_string())
            .with_field("position", position)
            .with_field("expected", expected)
            .with_field("actual", given.spell());
        self.diagnostics.push(reported);
    }

    /// Reports an argument that does not satisfy its declared parameter type.
    ///
    /// ADR-0037 allocates `E1215_ARGUMENT_TYPE_MISMATCH` as the residual code
    /// for a resolved call: the specialized codes keep their conditions, and
    /// this covers what none of them describes. It is deliberately not a
    /// catch-all — an unresolved callee or name is a resolution finding and has
    /// precedence — and it says nothing when either side is undetermined,
    /// because that would be a guess rather than a disagreement.
    fn check_argument_agreement(
        &mut self,
        callee: &str,
        position: usize,
        span: crate::Span,
        wanted: &Type,
        given: &Type,
    ) {
        if matches!(wanted, Type::Unknown) || matches!(given, Type::Unknown) {
            return;
        }
        // Integer disagreement is `E1210` and is already reported; reporting it
        // again here would give one mistake two codes.
        if is_integer_family(wanted) && is_integer_family(given) {
            return;
        }
        if matches!(given, Type::UnsuffixedInteger) && is_integer_family(wanted) {
            return;
        }
        if wanted.agrees_with(given) {
            return;
        }
        let expected = wanted.spell();
        self.report_argument(span, callee, position, &expected, given);
    }

    /// Checks a call whose callee has a declared or derived parameter list.
    ///
    /// The list is exact in both directions (`docs/43` §4), so a wrong count is
    /// `E1217` and stops here; only a call whose arguments correspond one to
    /// one with parameters has positions worth comparing.
    fn check_positional_arguments(
        &mut self,
        expression: &'source Expression,
        callee: &str,
        parameters: &[Type],
        actual: &[Type],
        context: &'static str,
    ) {
        // Only a positional list lines up with the parameter order; a named
        // list belongs to a constructor and is checked by field name.
        if expression
            .arguments()
            .iter()
            .any(|argument| argument.name().is_some())
        {
            return;
        }
        if parameters.len() != actual.len() {
            self.report_arity(
                expression.span(),
                callee,
                parameters.len(),
                actual.len(),
                context,
            );
            return;
        }
        for (position, ((wanted, given), argument)) in parameters
            .iter()
            .zip(actual)
            .zip(expression.arguments())
            .enumerate()
        {
            self.check_integer_agreement(argument.span(), wanted, given, "argument");
            self.check_argument_agreement(callee, position, argument.span(), wanted, given);
        }
    }

    fn call_type(&mut self, expression: &'source Expression) -> Type {
        let actual = self.type_arguments(expression.arguments());
        let Some(callee) = expression.callee() else {
            return Type::Unknown;
        };
        if callee.form() == ExpressionForm::Field {
            return self.lock_operation_type(callee);
        }
        if callee.form() != ExpressionForm::Name {
            return Type::Unknown;
        }
        let name = callee.span().text(self.source);
        // **Resolved in the order the lowerer resolves it**, because a call the
        // two read differently is a call whose artifact says something its
        // source did not.
        //
        // A binding first: a callable value is an in-scope name, and an inner
        // binding shadows an outer declaration as any other name does. Its
        // parameter list is exact in the same way a declared function's is, and
        // until now nothing in source checked it at all.
        if let Some(bound) = self.lookup(name) {
            let Type::Function(parameters, result) = bound else {
                // Calling a binding that is not callable. The lowerer refuses
                // it; the registry allocates no code for it, so this reports
                // nothing rather than inventing one.
                return Type::Unknown;
            };
            self.check_positional_arguments(
                expression,
                name,
                &parameters,
                &actual,
                "callable value",
            );
            return *result;
        }
        // Then a function this module declares. **It wins over a predeclared
        // name**: `docs/39` §2 makes reserved words, primitives, predeclared
        // types and predeclared *values* unshadowable and stops there, so a
        // predeclared function name is an ordinary identifier and a module that
        // declares `fn share(...)` has declared a function. The lowerer has
        // always resolved it that way; this file used to check the predeclared
        // rule first and type the call as the operation nobody called.
        if let Some((parameters, result)) = self.declarations.functions.get(name) {
            let result = result.clone();
            let parameters = parameters.clone();
            self.check_positional_arguments(expression, name, &parameters, &actual, "local named");
            return result;
        }
        if let Some(operation) = tos_predeclared::operation(name) {
            return self.predeclared_type(expression, &actual, operation);
        }
        if self.declarations.records.contains_key(name) {
            return Type::Nominal(name.to_string());
        }
        if let Some((owner, _)) = self.declarations.variants.get(name) {
            return Type::Nominal(owner.clone());
        }
        Type::Unknown
    }

    fn type_arguments(&mut self, arguments: &'source [CallArgument]) -> Vec<Type> {
        arguments
            .iter()
            .map(|argument| self.type_of(argument.value()))
            .collect()
    }

    /// Types a binary operator chain without recursing along it.
    ///
    /// `a + b + c + d` is `(((a + b) + c) + d)`, so recursing into `left` would
    /// recurse once per operand. The chain is collected instead and folded from
    /// the innermost operand outwards — which is the order the recursion
    /// evaluated in, so every `type_of` call still happens in the same sequence.
    fn binary_type(&mut self, expression: &'source Expression) -> Type {
        let (chain, innermost) =
            crate::walk::binary_chain(expression, |node| node.form() == ExpressionForm::Binary);
        let mut left = match innermost {
            Some(operand) => self.type_of(operand),
            None => Type::Unknown,
        };
        for node in chain.iter().rev() {
            let operator = node.operator_text(self.source).unwrap_or_default();
            let right = node
                .right()
                .map(|operand| self.type_of(operand))
                .unwrap_or(Type::Unknown);
            left = binary_result(operator, left, right);
        }
        left
    }

    /// Types a run of prefix operators without recursing along it.
    ///
    /// Same shape as `binary_type`: the run is collected and folded from the
    /// operand outwards, which is the order the recursion evaluated in.
    fn unary_type(&mut self, expression: &'source Expression) -> Type {
        let (chain, innermost) =
            crate::walk::prefix_chain(expression, |node| node.form() == ExpressionForm::Unary);
        let mut inner = match innermost {
            Some(operand) => self.type_of(operand),
            None => Type::Unknown,
        };
        for node in chain.iter().rev() {
            let operator = node.operator_text(self.source).unwrap_or_default();
            inner = unary_result(operator, inner);
        }
        inner
    }
}

/// One prefix operator's result type, from its operand's.
fn unary_result(operator: &str, inner: Type) -> Type {
    match operator {
        "!" => Type::Bool,
        // Consuming a task handle yields its outcome.
        "await" | "join" => match inner {
            Type::Constructed(name, arguments) if name == "Task" => Type::Constructed(
                String::from("TaskResult"),
                alloc::vec![arguments.first().cloned().unwrap_or(Type::Unknown)],
            ),
            _ => Type::Unknown,
        },
        _ => inner,
    }
}

/// One binary operator's result type, from its operands'.
///
/// A free function rather than a method because the chain fold in `binary_type`
/// applies it repeatedly to a type it is carrying, not to an expression.
fn binary_result(operator: &str, left: Type, right: Type) -> Type {
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
