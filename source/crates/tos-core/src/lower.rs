// SPDX-License-Identifier: GPL-3.0-or-later
//! Deterministic lowering of checked TOS Core source to `tos-ir/v1`.
//!
//! This is step 5 of the docs/44 section 6 order. docs/43 section 4 puts the
//! proof obligations before this point: the frontend has already established
//! syntactic well-formedness, name resolution, types, effects, ownership and
//! profile eligibility, and lowering turns that checked tree into typed IR
//! without re-deciding any of it.
//!
//! Lowering is deterministic. Identical declared inputs yield identical ordered
//! tables: types are interned in first-use order over a fixed declaration
//! walk, constants likewise, functions follow source order with each lowered
//! body appended where it was encountered, and every instruction carries a
//! source-map index. Nothing here consults a clock, a hash-map iteration order,
//! an ambient path or the host environment.
//!
//! **Coverage.** The implemented subset is the one the Bootstrap conformance
//! corpus exercises: declarations and their types, constants, `let`,
//! assignment, `return`, `if`, `while`, `loop`, `match`, expression statements,
//! literals, names, field and index paths, checked binary and unary operations,
//! widening conversions, calls, record/tuple/array construction, enum and
//! `Option`/`Result` variants, and `?` propagation. A construct outside it
//! produces a named [`Gap`] rather than a module: emitting an approximate
//! lowering would produce IR whose semantics the source does not have, and a
//! verifier cannot detect that because the IR would be internally consistent.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tos_ir::{
    BinaryOp, Block, CallTarget, CleanupCall, Constant, Function, FunctionOrigin, Header,
    Instruction, IntKind, LockMode, Module, NominalKind, Op, Operand, Parameter, PassMode, Place,
    PlaceStep, Profile, ResourceEnvelope, Signature, SourceMapEntry, Terminator, TypeDef, TypeId,
    UnaryOp, ValueId, Variant, Visibility,
};

use crate::parser::{
    Expression, ExpressionForm, Pattern, PatternForm, Schema, Span, Statement, StatementForm,
    TypeSyntax,
};
use crate::SourceUnit;

/// Which frontend produced a module, recorded in the header and source maps.
pub const FRONTEND_IDENTITY: &str = "tos-core-reference/0.1.0";

/// A source construct the implemented lowering subset does not cover.
///
/// A gap is not a diagnostic: the source is valid and checked. It says this
/// lowerer cannot yet produce faithful IR for it, and names the exact
/// construct and span so the boundary is verifiable rather than implied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gap {
    pub construct: &'static str,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// What a module needs to know about itself that its own text does not say.
#[derive(Clone, Debug)]
pub struct ModuleContext {
    pub source_set: String,
    pub path: String,
    pub content_id: String,
    pub dependency_digest: String,
    pub capability_interface_digest: String,
}

/// Lowers one checked module.
///
/// The schema must already have passed `Checker::check` with no error: docs/43
/// section 4 makes those proofs a precondition of emitting IR.
pub fn lower_module(
    source: &SourceUnit,
    schema: &Schema,
    context: &ModuleContext,
) -> Result<Module, Gap> {
    let profile = match schema.outline().prefix().header().profile() {
        crate::parser::Profile::Full => Profile::Full,
        crate::parser::Profile::Bootstrap => Profile::Bootstrap,
    };
    let module_name = schema
        .outline()
        .prefix()
        .header()
        .name()
        .iter()
        .map(|segment| segment.text(source))
        .collect::<Vec<_>>()
        .join(".");

    let mut lowerer = Lowerer {
        source,
        schema,
        profile,
        context,
        types: Vec::new(),
        type_index: BTreeMap::new(),
        constants: Vec::new(),
        constant_index: BTreeMap::new(),
        source_map: Vec::new(),
        source_index: BTreeMap::new(),
        nominals: BTreeMap::new(),
        variant_owner: BTreeMap::new(),
        functions_by_name: BTreeMap::new(),
        functions: Vec::new(),
    };

    lowerer.intern_declared_types()?;

    let mut imports = Vec::new();
    let mut capability_imports = Vec::new();
    for import in schema.outline().prefix().imports() {
        let path = import
            .path()
            .iter()
            .map(|segment| segment.text(source))
            .collect::<Vec<_>>()
            .join(".");
        let binding = import.binding().text(source).to_string();
        match import.kind() {
            crate::parser::ImportKind::Capability => {
                let ty = lowerer.intern(TypeDef::Capability(path.clone()));
                capability_imports.push(tos_ir::CapabilityImport {
                    interface: path,
                    binding,
                    ty,
                });
            }
            crate::parser::ImportKind::Module => imports.push(tos_ir::Import {
                module_name: path,
                // A single-module lowering knows the name it imported, not the
                // content of the module behind it; the source-set step binds
                // that identity into the dependency digest.
                module_content_id: String::new(),
                binding,
            }),
        }
    }
    for (index, function) in schema.functions().iter().enumerate() {
        lowerer
            .functions_by_name
            .insert(function.signature().name().text(source).to_string(), index);
    }

    // A declaration reserves its slot before any body is lowered, so a local
    // call resolves whatever order the source declared functions in and a
    // nested body appended later never shifts a declaration's index.
    for _ in schema.functions() {
        lowerer.functions.push(placeholder());
    }
    for (index, function) in schema.functions().iter().enumerate() {
        let lowered = lowerer.lower_function(function.signature(), function.body())?;
        lowerer.functions[index] = lowered;
    }
    let functions = core::mem::take(&mut lowerer.functions);

    let exports = functions
        .iter()
        .filter(|function| function.signature.visibility == Visibility::Public)
        .map(|function| function.signature.clone())
        .collect();

    let header = Header {
        schema_id: tos_ir::SCHEMA_ID.to_string(),
        language_version: tos_ir::LANGUAGE_VERSION.to_string(),
        unicode_normalization_baseline: tos_ir::UNICODE_BASELINE.to_string(),
        profile,
        module_name,
        source_set: context.source_set.clone(),
        path: context.path.clone(),
        content_id: context.content_id.clone(),
        dependency_digest: context.dependency_digest.clone(),
        frontend_identity: FRONTEND_IDENTITY.to_string(),
        source_map_revision: tos_ir::SOURCE_MAP_REVISION.to_string(),
        resource_envelope: lowerer.resource_envelope(),
        capability_interface_digest: context.capability_interface_digest.clone(),
    };

    let mut module = Module {
        header,
        types: lowerer.types,
        imports,
        capability_imports,
        exports,
        constants: lowerer.constants,
        functions,
        source_map: lowerer.source_map,
    };
    canonicalize_functions(&mut module);
    canonicalize_source_map(&mut module);
    Ok(module)
}

/// Puts the function table and the export list in canonical order.
///
/// docs/43 section 2 orders functions by fully qualified source name and the
/// exported signatures with them. Lowering walks source order, so the tables
/// are permuted here and every index that names a function — a local call, a
/// spawned body, a registered cleanup — is remapped through the same
/// permutation.
fn canonicalize_functions(module: &mut Module) {
    let mut order: Vec<usize> = (0..module.functions.len()).collect();
    order.sort_by(|left, right| {
        module.functions[*left]
            .signature
            .name
            .cmp(&module.functions[*right].signature.name)
    });
    let mut moved = alloc::vec![0usize; order.len()];
    for (position, old) in order.iter().enumerate() {
        moved[*old] = position;
    }
    let sorted: Vec<Function> = order
        .iter()
        .map(|old| module.functions[*old].clone())
        .collect();
    module.functions = sorted;
    for function in &mut module.functions {
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                match &mut instruction.op {
                    Op::Call {
                        target: CallTarget::Local(index),
                        ..
                    } => *index = moved.get(*index).copied().unwrap_or(*index),
                    Op::Spawn { body, .. }
                    | Op::Closure { body, .. }
                    | Op::RegisterCleanup { body } => {
                        *body = moved.get(*body).copied().unwrap_or(*body)
                    }
                    Op::RunCleanups { calls } => {
                        for call in calls {
                            call.body = moved.get(call.body).copied().unwrap_or(call.body);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    module.exports = module
        .functions
        .iter()
        .filter(|function| function.signature.visibility == Visibility::Public)
        .map(|function| function.signature.clone())
        .collect();
}

/// Puts the source map in the canonical order docs/43 section 2 fixes.
///
/// Entries are interned in first-use order while lowering, which is the order
/// the walk happens to reach spans in, not the order the schema requires. They
/// are sorted by source unit then byte start and end, and every reference is
/// remapped through the same permutation, so the module means exactly what it
/// meant before.
fn canonicalize_source_map(module: &mut Module) {
    let mut order: Vec<usize> = (0..module.source_map.len()).collect();
    order.sort_by(|left, right| {
        let one = &module.source_map[*left];
        let other = &module.source_map[*right];
        (&one.path, one.byte_start, one.byte_end).cmp(&(
            &other.path,
            other.byte_start,
            other.byte_end,
        ))
    });
    let mut moved = alloc::vec![0usize; order.len()];
    for (position, old) in order.iter().enumerate() {
        moved[*old] = position;
    }
    let sorted: Vec<SourceMapEntry> = order
        .iter()
        .map(|old| module.source_map[*old].clone())
        .collect();
    module.source_map = sorted;
    for entry in &mut module.source_map {
        if let Some(parent) = entry.derived_from {
            entry.derived_from = moved.get(parent).copied();
        }
    }
    for function in &mut module.functions {
        function.source = moved.get(function.source).copied().unwrap_or(0);
        for block in &mut function.blocks {
            block.source = moved.get(block.source).copied().unwrap_or(0);
            for instruction in &mut block.instructions {
                instruction.source = moved.get(instruction.source).copied().unwrap_or(0);
            }
        }
    }
}

/// A reserved slot, replaced before the module is returned.
fn placeholder() -> Function {
    Function {
        signature: Signature {
            name: String::new(),
            visibility: Visibility::Private,
            is_async: false,
            parameters: Vec::new(),
            result: 0,
            effects: Vec::new(),
        },
        origin: FunctionOrigin::Declared,
        source: 0,
        stack_contribution: 0,
        fuel_contribution: 0,
        cleanup_contribution: 0,
        values: Vec::new(),
        blocks: Vec::new(),
    }
}

struct Lowerer<'source> {
    source: &'source SourceUnit,
    schema: &'source Schema,
    profile: Profile,
    context: &'source ModuleContext,
    types: Vec<TypeDef>,
    type_index: BTreeMap<TypeDef, TypeId>,
    constants: Vec<Constant>,
    constant_index: BTreeMap<Constant, usize>,
    source_map: Vec<SourceMapEntry>,
    source_index: BTreeMap<(usize, usize), usize>,
    /// Local nominal types by export name, with their ordered field names.
    nominals: BTreeMap<String, (TypeId, Vec<String>)>,
    /// Enum variant name to its owning type and index.
    variant_owner: BTreeMap<String, (TypeId, usize)>,
    functions_by_name: BTreeMap<String, usize>,
    /// Functions in the order they were lowered: declarations first, with each
    /// nested body appended where the walk reached it.
    functions: Vec<Function>,
}

impl<'source> Lowerer<'source> {
    // ------------------------------------------------------------- interning

    /// Interns a type, returning the id of an identical one when it exists.
    ///
    /// The index is keyed on the definition itself. It used to be keyed on
    /// `format!("{definition:?}")`, which allocated a string, ran the whole
    /// `core::fmt` machinery and then compared strings — on **every** type
    /// reference in the module. Lowering a module at the published 256 KiB
    /// ceiling interns tens of thousands of times, so that was tens of
    /// thousands of allocations and formatted strings to answer a question a
    /// structural comparison answers by looking at a discriminant.
    ///
    /// Structural equality is also the *correct* key: two definitions are the
    /// same type exactly when they are equal, whereas a debug rendering is a
    /// presentation that merely happened to be injective.
    fn intern(&mut self, definition: TypeDef) -> TypeId {
        if let Some(&existing) = self.type_index.get(&definition) {
            return existing;
        }
        let id = self.types.len();
        self.type_index.insert(definition.clone(), id);
        self.types.push(definition);
        id
    }

    /// Interns a constant, on the same principle as [`Self::intern`].
    fn intern_constant(&mut self, constant: Constant) -> usize {
        if let Some(&existing) = self.constant_index.get(&constant) {
            return existing;
        }
        let id = self.constants.len();
        self.constant_index.insert(constant.clone(), id);
        self.constants.push(constant);
        id
    }

    /// Interns a source-map entry for a span, reusing the entry for a span the
    /// module already mapped.
    fn map(&mut self, span: Span) -> usize {
        let key = (span.start(), span.end());
        if let Some(&existing) = self.source_index.get(&key) {
            return existing;
        }
        let id = self.source_map.len();
        self.source_map.push(SourceMapEntry {
            source_set: self.context.source_set.clone(),
            path: self.context.path.clone(),
            content_id: self.context.content_id.clone(),
            frontend_identity: FRONTEND_IDENTITY.to_string(),
            language_version: tos_ir::LANGUAGE_VERSION.to_string(),
            profile: self.profile,
            unicode_normalization_baseline: tos_ir::UNICODE_BASELINE.to_string(),
            byte_start: span.start(),
            byte_end: span.end(),
            derived_from: None,
        });
        self.source_index.insert(key, id);
        id
    }

    /// Interns every type a declaration names, in declaration order.
    ///
    /// Nominal types are created before any body is lowered so a record that
    /// mentions another resolves whatever order the source declared them in.
    fn intern_declared_types(&mut self) -> Result<(), Gap> {
        let content_id = self.context.content_id.clone();
        // Two passes: shells first, so mutually referring declarations resolve.
        for declaration in self.schema.records() {
            let name = declaration.name().text(self.source).to_string();
            let id = self.intern(TypeDef::Nominal {
                module_content_id: content_id.clone(),
                export_name: name.clone(),
                kind: NominalKind::Record,
                fields: Vec::new(),
                variants: Vec::new(),
            });
            self.nominals.insert(name, (id, Vec::new()));
        }
        for declaration in self.schema.enums() {
            let name = declaration.name().text(self.source).to_string();
            let id = self.intern(TypeDef::Nominal {
                module_content_id: content_id.clone(),
                export_name: name.clone(),
                kind: NominalKind::Enum,
                fields: Vec::new(),
                variants: Vec::new(),
            });
            self.nominals.insert(name, (id, Vec::new()));
        }
        for declaration in self.schema.records() {
            let name = declaration.name().text(self.source).to_string();
            let mut fields = Vec::new();
            let mut names = Vec::new();
            for field in declaration.fields() {
                fields.push(self.resolve_type(field.ty())?);
                names.push(field.name().text(self.source).to_string());
            }
            let (id, _) = self.nominals[&name];
            self.types[id] = TypeDef::Nominal {
                module_content_id: content_id.clone(),
                export_name: name.clone(),
                kind: NominalKind::Record,
                fields,
                variants: Vec::new(),
            };
            self.nominals.insert(name, (id, names));
        }
        for declaration in self.schema.enums() {
            let name = declaration.name().text(self.source).to_string();
            let mut variants = Vec::new();
            for (index, variant) in declaration.variants().iter().enumerate() {
                let mut payload = Vec::new();
                for ty in variant.tuple_types() {
                    payload.push(self.resolve_type(ty)?);
                }
                for field in variant.fields() {
                    payload.push(self.resolve_type(field.ty())?);
                }
                let variant_name = variant.name().text(self.source).to_string();
                let (id, _) = self.nominals[&name];
                self.variant_owner.insert(variant_name.clone(), (id, index));
                variants.push(Variant {
                    name: variant_name,
                    payload,
                });
            }
            let (id, _) = self.nominals[&name];
            self.types[id] = TypeDef::Nominal {
                module_content_id: content_id.clone(),
                export_name: name,
                kind: NominalKind::Enum,
                fields: Vec::new(),
                variants,
            };
        }
        Ok(())
    }

    /// Resolves a written type to a table entry.
    fn resolve_type(&mut self, ty: &TypeSyntax) -> Result<TypeId, Gap> {
        match ty {
            TypeSyntax::Name { path, span } => {
                let spelled = path
                    .iter()
                    .map(|segment| segment.text(self.source))
                    .collect::<Vec<_>>()
                    .join(".");
                if let Some(kind) = IntKind::parse(&spelled) {
                    return Ok(self.intern(TypeDef::Int(kind)));
                }
                let definition = match spelled.as_str() {
                    "unit" => TypeDef::Unit,
                    "bool" => TypeDef::Bool,
                    "size" => TypeDef::Size,
                    "duration" => TypeDef::Duration,
                    "string" => TypeDef::Text,
                    "bytes" => TypeDef::Bytes,
                    "ConversionError" => TypeDef::ConversionError,
                    "Event" => TypeDef::Event,
                    "Semaphore" => TypeDef::Semaphore,
                    "Barrier" => TypeDef::Barrier,
                    "Latch" => TypeDef::Latch,
                    "AtomicBool" => TypeDef::AtomicBool,
                    "AtomicU32" => TypeDef::AtomicU32,
                    "AtomicU64" => TypeDef::AtomicU64,
                    _ => {
                        if let Some(&(id, _)) = self.nominals.get(&spelled) {
                            return Ok(id);
                        }
                        // A type from another module, or a capability
                        // interface: its identity is the path, and its shape
                        // belongs to the module that declares it.
                        return Ok(self.intern(TypeDef::Nominal {
                            module_content_id: String::new(),
                            export_name: spelled,
                            kind: NominalKind::Record,
                            fields: Vec::new(),
                            variants: Vec::new(),
                        }));
                    }
                };
                let _ = span;
                Ok(self.intern(definition))
            }
            TypeSyntax::Constructed {
                name,
                arguments,
                mutable,
                span,
            } => {
                let spelled = name.text(self.source);
                let mut lowered = Vec::new();
                for argument in arguments {
                    lowered.push(self.resolve_type(argument)?);
                }
                let first = lowered.first().copied().unwrap_or(0);
                let definition = match spelled {
                    "Option" => TypeDef::Option(first),
                    "Task" => TypeDef::Task(first),
                    "TaskResult" => TypeDef::TaskResult(first),
                    "Shared" => TypeDef::Shared(first),
                    // ADR-0037: the granted mode is part of the type, so it
                    // reaches the IR as a distinct constructor. A verifier that
                    // had to infer the mode could not recheck it.
                    "Region" if *mutable => TypeDef::RegionMut(first),
                    "DmaRegion" if *mutable => TypeDef::DmaRegionMut(first),
                    "Region" => TypeDef::Region(first),
                    "DmaRegion" => TypeDef::DmaRegion(first),
                    "MutexGuard" => TypeDef::MutexGuard(first),
                    "ReadGuard" => TypeDef::ReadGuard(first),
                    "WriteGuard" => TypeDef::WriteGuard(first),
                    "Mutex" => TypeDef::Mutex(first),
                    "RwLock" => TypeDef::RwLock(first),
                    "Channel" => TypeDef::Channel(first),
                    "slice" => TypeDef::Slice(first),
                    "Result" => TypeDef::Result(first, lowered.get(1).copied().unwrap_or(0)),
                    _ => {
                        return Err(self.gap("constructed type", *span));
                    }
                };
                Ok(self.intern(definition))
            }
            TypeSyntax::Array {
                element,
                length,
                span,
            } => {
                let element = self.resolve_type(element)?;
                let text = length.text(self.source);
                let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
                let Ok(count) = digits.parse::<u64>() else {
                    return Err(self.gap("array length that is not a literal", *span));
                };
                Ok(self.intern(TypeDef::Array(element, count)))
            }
            TypeSyntax::Tuple { elements, .. } => {
                let mut lowered = Vec::new();
                for element in elements {
                    lowered.push(self.resolve_type(element)?);
                }
                Ok(self.intern(TypeDef::Tuple(lowered)))
            }
            TypeSyntax::Function {
                parameters, result, ..
            } => {
                let mut lowered = Vec::new();
                for parameter in parameters {
                    lowered.push(self.resolve_type(parameter)?);
                }
                let result = self.resolve_type(result)?;
                Ok(self.intern(TypeDef::Function(lowered, result)))
            }
        }
    }

    fn gap(&self, construct: &'static str, span: Span) -> Gap {
        Gap {
            construct,
            byte_start: span.start(),
            byte_end: span.end(),
        }
    }

    fn resource_envelope(&self) -> ResourceEnvelope {
        let mut envelope = ResourceEnvelope::default();
        for limit in self.schema.outline().resource().limits() {
            let text = limit.value().text(self.source);
            let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
            let scale: u128 = if text.ends_with("KiB") {
                1024
            } else if text.ends_with("MiB") {
                1024 * 1024
            } else if text.ends_with("GiB") {
                1024 * 1024 * 1024
            } else {
                1
            };
            let value = digits.parse::<u128>().unwrap_or(0) * scale;
            match limit.name().text(self.source) {
                "fuel" => envelope.fuel = value,
                "stack" => envelope.stack = value,
                "allocation" => envelope.allocation = value,
                "tasks" => envelope.tasks = value,
                "workers" => envelope.workers = value,
                "sync" => envelope.sync = value,
                "shared" => envelope.shared = value,
                "cleanup" => envelope.cleanup = value,
                "recursion" => envelope.recursion = value,
                "imports" => envelope.imports = value,
                _ => {}
            }
        }
        envelope
    }

    // ------------------------------------------------------------- functions

    fn lower_function(
        &mut self,
        signature: &'source crate::parser::FunctionSignature,
        body: &'source crate::parser::Block,
    ) -> Result<Function, Gap> {
        let mut parameters = Vec::new();
        let mut values: Vec<TypeId> = Vec::new();
        let mut scope: Vec<(String, ValueId)> = Vec::new();
        for parameter in signature.parameters() {
            let ty = self.resolve_type(parameter.ty())?;
            let mode = match parameter.borrow_mode() {
                crate::parser::BorrowMode::Owned => PassMode::Owned,
                crate::parser::BorrowMode::Shared => PassMode::SharedBorrow,
                crate::parser::BorrowMode::Mutable => PassMode::MutableBorrow,
            };
            let name = parameter.name().text(self.source).to_string();
            let slot = values.len();
            values.push(ty);
            scope.push((name.clone(), slot));
            parameters.push(Parameter { name, ty, mode });
        }
        let declared = self.resolve_type(signature.result())?;
        // docs/40 section 4: an `async fn` declared `-> T` produces `Task<T>`,
        // so the declaration's own result is the task and its body is the child
        // the declaration spawns.
        let result = if signature.is_async() {
            self.intern(TypeDef::Task(declared))
        } else {
            declared
        };
        let effects = signature
            .effects()
            .iter()
            .map(|effect| effect.text(self.source).to_string())
            .collect();
        let lowered_signature = Signature {
            name: signature.name().text(self.source).to_string(),
            visibility: match signature.visibility() {
                crate::parser::Visibility::Public => Visibility::Public,
                crate::parser::Visibility::Private => Visibility::Private,
            },
            is_async: signature.is_async(),
            parameters,
            result,
            effects,
        };

        let entry_source = self.map(signature.span());
        let mut builder = BodyBuilder {
            values,
            in_unsafe: false,
            blocks: alloc::vec![Block {
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Trap(String::from("RUNTIME_UNREACHABLE")),
                source: entry_source,
            }],
            current: 0,
            scope,
            loops: Vec::new(),
            result,
            cleanups: Vec::new(),
        };
        if signature.is_async() {
            // The declaration's own body is one instruction: spawn the child
            // that carries the work, and return its handle.
            let name = self.body_name("async", signature.span());
            let (id, captures) =
                self.lower_captured_body(&name, Vec::new(), body, declared, &mut builder)?;
            let task = builder.define(result);
            builder.push(Instruction {
                result: Some(task),
                ty: result,
                op: Op::Spawn { body: id, captures },
                source: entry_source,
                unsafe_block: false,
                unsafe_interface: None,
                runtime_contract: Some(String::from("tos-runtime/task/v1")),
            });
            builder.set_terminator(Terminator::Return(Some(Operand::Value(task))));
        } else {
            self.lower_block(body, &mut builder)?;
        }
        // A body that falls off its end returns unit; a non-unit function
        // reaching here was already `E1221_MISSING_RETURN`.
        if !builder.is_terminated() {
            builder.set_terminator(Terminator::Return(None));
        }

        let envelope = self.resource_envelope();
        Ok(Function {
            signature: lowered_signature,
            origin: FunctionOrigin::Declared,
            source: entry_source,
            stack_contribution: envelope.stack,
            fuel_contribution: envelope.fuel,
            cleanup_contribution: envelope.cleanup,
            values: builder.values,
            blocks: builder.blocks,
        })
    }

    /// The type a nested body returns, read from its own `return` statements.
    ///
    /// docs/39 gives a closure or spawned body no result annotation, and V1 has
    /// no inference; the body says what it produces with an explicit `return`,
    /// which is exactly what this reads.
    fn body_result(&mut self, body: &'source crate::parser::Block) -> Option<TypeId> {
        let expression = first_returned_expression(body)?;
        self.static_expression_type(expression)
    }

    /// The type of an expression that a declaration already fixes.
    ///
    /// Only forms whose type is stated by a declaration are answered: a
    /// literal, a call to a declared function, a constructor. Anything else
    /// returns nothing, and the caller falls back rather than guessing.
    fn static_expression_type(&mut self, expression: &'source Expression) -> Option<TypeId> {
        match expression.form() {
            ExpressionForm::Literal => {
                let constant = self.literal_constant(expression).ok()?;
                Some(self.constant_type(constant))
            }
            ExpressionForm::Group => self.static_expression_type(expression.inner()?),
            ExpressionForm::Binary => {
                let operator = expression.operator_text(self.source)?;
                let op = binary_op(operator)?;
                if op.is_comparison() {
                    return Some(self.intern(TypeDef::Bool));
                }
                self.static_expression_type(expression.left()?)
            }
            ExpressionForm::Call => {
                let callee = expression.callee()?;
                if callee.form() != ExpressionForm::Name {
                    return None;
                }
                let name = callee.span().text(self.source).to_string();
                if let Some(&(ty, _)) = self.nominals.get(&name) {
                    return Some(ty);
                }
                let index = *self.functions_by_name.get(&name)?;
                let result = self.schema.functions()[index].signature().result();
                self.resolve_type(result).ok()
            }
            ExpressionForm::Name => {
                let name = expression.span().text(self.source);
                let &(ty, _) = self.variant_owner.get(name)?;
                Some(ty)
            }
            _ => None,
        }
    }

    fn constant_type(&mut self, constant: usize) -> TypeId {
        let definition = match self.constants.get(constant) {
            Some(Constant::Bool(_)) => TypeDef::Bool,
            Some(Constant::Int(kind, _)) => TypeDef::Int(*kind),
            Some(Constant::Size(_)) => TypeDef::Size,
            Some(Constant::Duration(_)) => TypeDef::Duration,
            Some(Constant::Text(_)) => TypeDef::Text,
            Some(Constant::Bytes(_)) => TypeDef::Bytes,
            _ => TypeDef::Unit,
        };
        self.intern(definition)
    }

    // ---------------------------------------------------------- nested bodies

    /// A deterministic name for a body lowered out of a source construct.
    ///
    /// The byte offset makes it unique and stable, and `#` keeps it out of the
    /// identifier space, so a synthetic name can never collide with a declared
    /// function or be called by source.
    fn body_name(&self, kind: &str, span: Span) -> String {
        alloc::format!("#{kind}@{}", span.start())
    }

    fn cleanup_name(&self, span: Span) -> String {
        self.body_name("defer", span)
    }

    /// Lowers a nested block into its own function and returns it with the
    /// operands the enclosing scope must pass.
    ///
    /// A nested body is its own return scope (docs/43 section 3), so it becomes
    /// a real function rather than inlined blocks. What it uses from the
    /// enclosing scope becomes explicit ordered captures naming the enclosing
    /// slots: nothing reaches a nested body by ambient scope.
    fn lower_captured_body(
        &mut self,
        name: &str,
        declared: Vec<(String, TypeId)>,
        body: &'source crate::parser::Block,
        result: TypeId,
        outer: &mut BodyBuilder,
    ) -> Result<(usize, Vec<Operand>), Gap> {
        self.lower_captured_body_with(name, declared, body, result, PassMode::Owned, outer)
    }

    /// As [`Self::lower_captured_body`], with the mode captures are passed in.
    ///
    /// A closure or a spawned child takes what it captures, so its captures are
    /// owned. A deferred cleanup acts on the bindings of the scope it runs in,
    /// so ADR-0035 makes its captures mutable borrows of those bindings: what
    /// one cleanup leaves is what the next one and the scope observe.
    fn lower_captured_body_with(
        &mut self,
        name: &str,
        declared: Vec<(String, TypeId)>,
        body: &'source crate::parser::Block,
        result: TypeId,
        capture_mode: PassMode,
        outer: &mut BodyBuilder,
    ) -> Result<(usize, Vec<Operand>), Gap> {
        let mut bound: BTreeSet<String> = declared.iter().map(|(name, _)| name.clone()).collect();
        let mut free: Vec<String> = Vec::new();
        collect_free_names(self.source, body, &mut bound, &mut free);

        let mut parameters = Vec::new();
        let mut values: Vec<TypeId> = Vec::new();
        let mut scope: Vec<(String, ValueId)> = Vec::new();
        for (name, ty) in &declared {
            let slot = values.len();
            values.push(*ty);
            scope.push((name.clone(), slot));
            parameters.push(Parameter {
                name: name.clone(),
                ty: *ty,
                mode: PassMode::Owned,
            });
        }
        let mut captures: Vec<Operand> = Vec::new();
        for name in &free {
            let Some(slot) = outer.lookup(name) else {
                // Not a binding of the enclosing scope: a module item, a
                // constructor or a predeclared name, which the body resolves
                // for itself.
                continue;
            };
            let ty = outer.values.get(slot).copied().unwrap_or(result);
            let captured = values.len();
            values.push(ty);
            scope.push((name.clone(), captured));
            parameters.push(Parameter {
                name: name.clone(),
                ty,
                mode: capture_mode,
            });
            captures.push(Operand::Value(slot));
        }

        let source = self.map(body.span());
        let mut nested = BodyBuilder {
            values,
            in_unsafe: outer.in_unsafe,
            blocks: alloc::vec![Block {
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Trap(String::from("RUNTIME_UNREACHABLE")),
                source,
            }],
            current: 0,
            scope,
            loops: Vec::new(),
            result,
            cleanups: Vec::new(),
        };
        self.lower_block(body, &mut nested)?;
        if !nested.is_terminated() {
            nested.set_terminator(Terminator::Return(None));
        }

        let envelope = self.resource_envelope();
        let id = self.functions.len();
        self.functions.push(Function {
            signature: Signature {
                name: name.to_string(),
                visibility: Visibility::Private,
                is_async: false,
                parameters,
                result,
                effects: Vec::new(),
            },
            origin: FunctionOrigin::LoweredBody,
            source,
            stack_contribution: envelope.stack,
            fuel_contribution: envelope.fuel,
            cleanup_contribution: envelope.cleanup,
            values: nested.values,
            blocks: nested.blocks,
        });
        Ok((id, captures))
    }

    /// Lowers `for pattern in (sequence) { ... }` to an explicit counted loop.
    ///
    /// docs/39 gives `for` a sequence to walk, so the loop is over its indices:
    /// a counter, a bound read from the sequence, a bounds comparison and an
    /// indexed read that binds the pattern. Nothing is implicit, which is what
    /// makes the back edge and the iteration count verifier-visible.
    fn lower_for(
        &mut self,
        statement: &'source Statement,
        builder: &mut BodyBuilder,
        at: usize,
    ) -> Result<(), Gap> {
        let Some(sequence) = statement.expression() else {
            return Err(self.gap("for without a sequence", statement.span()));
        };
        let sequence = self.lower_expression(sequence, builder)?;
        let sequence_type = builder.type_of(&sequence);
        let (element, length) = match self.types.get(sequence_type) {
            Some(TypeDef::Array(element, length)) => (*element, *length),
            // A slice has a runtime length, which V1 gives no source form to
            // read, so it is not lowered rather than guessed at.
            _ => {
                return Err(self.gap(
                    "for over a sequence without a static length",
                    statement.span(),
                ))
            }
        };

        let size = self.intern(TypeDef::Size);
        let bool_type = self.intern(TypeDef::Bool);
        let zero = self.intern_constant(Constant::Size(0));
        let one = self.intern_constant(Constant::Size(1));
        let bound = self.intern_constant(Constant::Size(length as u128));

        let counter = builder.define(size);
        builder.push(Instruction {
            result: Some(counter),
            ty: size,
            op: Op::Const(zero),
            source: at,
            unsafe_block: builder.in_unsafe,
            unsafe_interface: None,
            runtime_contract: None,
        });

        let head_block = builder.new_block(at);
        let body_block = builder.new_block(at);
        let exit_block = builder.new_block(at);
        builder.set_terminator(Terminator::Branch {
            target: head_block,
            arguments: Vec::new(),
        });

        builder.current = head_block;
        let more = builder.define(bool_type);
        builder.push(Instruction {
            result: Some(more),
            ty: bool_type,
            op: Op::Binary {
                op: BinaryOp::Less,
                left: Operand::Value(counter),
                right: Operand::Constant(bound),
            },
            source: at,
            unsafe_block: builder.in_unsafe,
            unsafe_interface: None,
            runtime_contract: None,
        });
        builder.set_terminator(Terminator::BranchIf {
            condition: Operand::Value(more),
            true_target: body_block,
            true_arguments: Vec::new(),
            false_target: exit_block,
            false_arguments: Vec::new(),
        });

        builder.current = body_block;
        let depth = builder.scope.len();
        let Operand::Value(root) = sequence else {
            return Err(self.gap("for over a constant sequence", statement.span()));
        };
        let item = builder.define(element);
        builder.push(Instruction {
            result: Some(item),
            ty: element,
            op: Op::Read {
                place: Place {
                    root,
                    path: alloc::vec![PlaceStep::DynamicIndex(counter)],
                },
            },
            source: at,
            unsafe_block: builder.in_unsafe,
            unsafe_interface: None,
            runtime_contract: None,
        });
        if let Some(pattern) = statement.pattern() {
            if let Some(name) = pattern.name() {
                builder
                    .scope
                    .push((name.text(self.source).to_string(), item));
            }
        }
        builder.loops.push(LoopFrame {
            head: head_block,
            exit: exit_block,
            cleanup_depth: builder.cleanups.len(),
        });
        if let Some(body) = statement.body() {
            self.lower_block(body, builder)?;
        }
        builder.loops.pop();
        builder.scope.truncate(depth);
        if !builder.is_terminated() {
            let next = builder.define(size);
            builder.push(Instruction {
                result: Some(next),
                ty: size,
                op: Op::Binary {
                    op: BinaryOp::Add,
                    left: Operand::Value(counter),
                    right: Operand::Constant(one),
                },
                source: at,
                unsafe_block: builder.in_unsafe,
                unsafe_interface: None,
                runtime_contract: None,
            });
            builder.push(Instruction {
                result: None,
                ty: size,
                op: Op::Write {
                    place: Place {
                        root: counter,
                        path: Vec::new(),
                    },
                    value: Operand::Value(next),
                },
                source: at,
                unsafe_block: builder.in_unsafe,
                unsafe_interface: None,
                runtime_contract: None,
            });
            builder.set_terminator(Terminator::Branch {
                target: head_block,
                arguments: Vec::new(),
            });
        }

        builder.current = exit_block;
        Ok(())
    }

    // ------------------------------------------------------------ statements

    fn lower_block(
        &mut self,
        block: &'source crate::parser::Block,
        builder: &mut BodyBuilder,
    ) -> Result<(), Gap> {
        let depth = builder.scope.len();
        builder.cleanups.push(Vec::new());
        for statement in block.statements() {
            if builder.is_terminated() {
                break;
            }
            self.lower_statement(statement, builder)?;
        }
        // Normal completion leaves this block, so its own cleanups run here.
        if !builder.is_terminated() {
            self.emit_cleanups(builder.cleanups.len() - 1, builder);
        }
        builder.cleanups.pop();
        builder.scope.truncate(depth);
        Ok(())
    }

    /// Emits the cleanups of every lexical block from `from` inward.
    ///
    /// ADR-0035 runs them in reverse registration order, and the innermost
    /// block's cleanups run before an enclosing block's, so the whole suffix is
    /// walked from the inside out.
    fn emit_cleanups(&mut self, from: usize, builder: &mut BodyBuilder) {
        let mut calls: Vec<CleanupCall> = Vec::new();
        for scope in builder.cleanups[from..].iter().rev() {
            calls.extend(scope.iter().rev().cloned());
        }
        if calls.is_empty() {
            return;
        }
        let unit = self.unit_type();
        let source = builder.blocks[builder.current].source;
        builder.push(Instruction {
            result: None,
            ty: unit,
            op: Op::RunCleanups { calls },
            source,
            unsafe_block: builder.in_unsafe,
            unsafe_interface: None,
            runtime_contract: None,
        });
    }

    fn lower_statement(
        &mut self,
        statement: &'source Statement,
        builder: &mut BodyBuilder,
    ) -> Result<(), Gap> {
        let at = self.map(statement.span());
        match statement.form() {
            StatementForm::Let => {
                let Some(initializer) = statement.expression() else {
                    return Err(self.gap("let without an initializer", statement.span()));
                };
                let value = self.lower_expression(initializer, builder)?;
                let ty = match statement.declared_type() {
                    Some(declared) => self.resolve_type(declared)?,
                    None => builder.type_of(&value),
                };
                let Some(pattern) = statement.pattern() else {
                    return Err(self.gap("let without a pattern", statement.span()));
                };
                self.bind_pattern(pattern, ty, value, builder, at)?;
                Ok(())
            }
            StatementForm::Assignment => {
                let (Some(target), Some(expression)) = (statement.target(), statement.expression())
                else {
                    return Err(self.gap("assignment without both sides", statement.span()));
                };
                let place = self.lower_place(target, builder)?;
                let value = self.lower_expression(expression, builder)?;
                builder.push(Instruction {
                    result: None,
                    ty: self.unit_type(),
                    op: Op::Write { place, value },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                Ok(())
            }
            StatementForm::Return => {
                // ADR-0035: the action that caused the exit is evaluated first,
                // then the cleanups of every block this return leaves.
                let value = match statement.expression() {
                    Some(expression) => Some(self.lower_expression(expression, builder)?),
                    None => None,
                };
                self.emit_cleanups(0, builder);
                builder.set_terminator(Terminator::Return(value));
                Ok(())
            }
            StatementForm::Expression => {
                let Some(expression) = statement.expression() else {
                    return Ok(());
                };
                self.lower_expression(expression, builder)?;
                Ok(())
            }
            StatementForm::If => self.lower_if(statement, builder, at),
            StatementForm::While => self.lower_while(statement, builder, at),
            StatementForm::Loop => self.lower_loop(statement, builder, at),
            StatementForm::Break => {
                let Some(loop_frame) = builder.loops.last().copied() else {
                    return Err(self.gap("break outside a loop", statement.span()));
                };
                self.emit_cleanups(loop_frame.cleanup_depth, builder);
                builder.set_terminator(Terminator::Branch {
                    target: loop_frame.exit,
                    arguments: Vec::new(),
                });
                Ok(())
            }
            StatementForm::Continue => {
                let Some(loop_frame) = builder.loops.last().copied() else {
                    return Err(self.gap("continue outside a loop", statement.span()));
                };
                self.emit_cleanups(loop_frame.cleanup_depth, builder);
                builder.set_terminator(Terminator::Branch {
                    target: loop_frame.head,
                    arguments: Vec::new(),
                });
                Ok(())
            }
            StatementForm::Match => self.lower_match(statement, builder, at),
            StatementForm::Parallel => {
                // docs/41 makes `parallel` a lexical task scope. Bootstrap
                // serializes it, and the scope's obligations were already
                // proved by the checker, so its body lowers in place.
                match statement.body() {
                    Some(body) => self.lower_block(body, builder),
                    None => Ok(()),
                }
            }
            StatementForm::For => self.lower_for(statement, builder, at),
            StatementForm::Defer => {
                let Some(body) = statement.body() else {
                    return Err(self.gap("defer without a body", statement.span()));
                };
                let unit = self.unit_type();
                let (cleanup, captures) = self.lower_captured_body_with(
                    &self.cleanup_name(statement.span()),
                    Vec::new(),
                    body,
                    unit,
                    PassMode::MutableBorrow,
                    builder,
                )?;
                builder.push(Instruction {
                    result: None,
                    ty: unit,
                    op: Op::RegisterCleanup { body: cleanup },
                    source: at,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                    runtime_contract: None,
                });
                if let Some(scope) = builder.cleanups.last_mut() {
                    // The operands name slots, so the cleanup reads their state
                    // where it runs rather than where it registered.
                    scope.push(CleanupCall {
                        body: cleanup,
                        captures,
                    });
                }
                Ok(())
            }
            StatementForm::Cancel => {
                let Some(expression) = statement.expression() else {
                    return Err(self.gap("cancel without a task", statement.span()));
                };
                let task = self.lower_expression(expression, builder)?;
                let unit = self.unit_type();
                builder.push(Instruction {
                    result: None,
                    ty: unit,
                    op: Op::Cancel { task },
                    source: at,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                    runtime_contract: Some(String::from("tos-runtime/task/v1")),
                });
                Ok(())
            }
            StatementForm::Unsafe => {
                // docs/43 section 3 separates the marker from an interface ID:
                // an `unsafe` block marks ordinary operations, and V1 accepts
                // no external interface for any of them to name.
                let Some(body) = statement.body() else {
                    return Ok(());
                };
                let outer = builder.in_unsafe;
                builder.in_unsafe = true;
                let outcome = self.lower_block(body, builder);
                builder.in_unsafe = outer;
                outcome
            }
        }
    }

    fn lower_if(
        &mut self,
        statement: &'source Statement,
        builder: &mut BodyBuilder,
        at: usize,
    ) -> Result<(), Gap> {
        let Some(head) = statement.expression() else {
            return Err(self.gap("if without a condition", statement.span()));
        };
        let condition = self.lower_expression(head, builder)?;
        let then_block = builder.new_block(at);
        let else_block = builder.new_block(at);
        let join_block = builder.new_block(at);
        builder.set_terminator(Terminator::BranchIf {
            condition,
            true_target: then_block,
            true_arguments: Vec::new(),
            false_target: else_block,
            false_arguments: Vec::new(),
        });

        builder.current = then_block;
        if let Some(body) = statement.body() {
            self.lower_block(body, builder)?;
        }
        if !builder.is_terminated() {
            builder.set_terminator(Terminator::Branch {
                target: join_block,
                arguments: Vec::new(),
            });
        }

        builder.current = else_block;
        if let Some(body) = statement.else_body() {
            self.lower_block(body, builder)?;
        } else if let Some(chained) = statement.else_if() {
            self.lower_statement(chained, builder)?;
        }
        if !builder.is_terminated() {
            builder.set_terminator(Terminator::Branch {
                target: join_block,
                arguments: Vec::new(),
            });
        }

        builder.current = join_block;
        Ok(())
    }

    fn lower_while(
        &mut self,
        statement: &'source Statement,
        builder: &mut BodyBuilder,
        at: usize,
    ) -> Result<(), Gap> {
        let head_block = builder.new_block(at);
        let body_block = builder.new_block(at);
        let exit_block = builder.new_block(at);
        builder.set_terminator(Terminator::Branch {
            target: head_block,
            arguments: Vec::new(),
        });

        builder.current = head_block;
        let Some(head) = statement.expression() else {
            return Err(self.gap("while without a condition", statement.span()));
        };
        let condition = self.lower_expression(head, builder)?;
        builder.set_terminator(Terminator::BranchIf {
            condition,
            true_target: body_block,
            true_arguments: Vec::new(),
            false_target: exit_block,
            false_arguments: Vec::new(),
        });

        builder.current = body_block;
        builder.loops.push(LoopFrame {
            head: head_block,
            exit: exit_block,
            cleanup_depth: builder.cleanups.len(),
        });
        if let Some(body) = statement.body() {
            self.lower_block(body, builder)?;
        }
        builder.loops.pop();
        if !builder.is_terminated() {
            builder.set_terminator(Terminator::Branch {
                target: head_block,
                arguments: Vec::new(),
            });
        }

        builder.current = exit_block;
        Ok(())
    }

    fn lower_loop(
        &mut self,
        statement: &'source Statement,
        builder: &mut BodyBuilder,
        at: usize,
    ) -> Result<(), Gap> {
        let body_block = builder.new_block(at);
        let exit_block = builder.new_block(at);
        builder.set_terminator(Terminator::Branch {
            target: body_block,
            arguments: Vec::new(),
        });

        builder.current = body_block;
        builder.loops.push(LoopFrame {
            head: body_block,
            exit: exit_block,
            cleanup_depth: builder.cleanups.len(),
        });
        if let Some(body) = statement.body() {
            self.lower_block(body, builder)?;
        }
        builder.loops.pop();
        if !builder.is_terminated() {
            builder.set_terminator(Terminator::Branch {
                target: body_block,
                arguments: Vec::new(),
            });
        }

        builder.current = exit_block;
        Ok(())
    }

    fn lower_match(
        &mut self,
        statement: &'source Statement,
        builder: &mut BodyBuilder,
        at: usize,
    ) -> Result<(), Gap> {
        let Some(head) = statement.expression() else {
            return Err(self.gap("match without a subject", statement.span()));
        };
        let subject = self.lower_expression(head, builder)?;
        let join_block = builder.new_block(at);

        // Arms are taken in **source order**, and the first irrefutable one ends
        // the match: nothing written after it can be reached. An irrefutable arm
        // is a wildcard, a bare binding, or a tuple pattern (ADR-0046).
        //
        // This shape matters for more than tidiness. Lowering a wildcard as a
        // *default* while still building a variant map from every later arm let
        // a variant arm written after a catch-all run instead of it, which is
        // the opposite of what source order says. And a subject that is not a
        // sum type has no variants to map at all: emitting `MatchEnum` for one
        // produced IR the verifier accepted and the engine trapped on, which is
        // worse than refusing to lower it.
        let branches = statement.branches();
        let irrefutable = |pattern: &crate::parser::Pattern| match pattern.form() {
            PatternForm::Wildcard | PatternForm::Tuple => true,
            PatternForm::Name if !pattern.is_qualified() => pattern
                .name()
                .map(|name| self.variant_index(name.text(self.source)).is_none())
                .unwrap_or(false),
            _ => false,
        };
        let first_irrefutable = branches
            .iter()
            .position(|branch| irrefutable(branch.pattern()));
        // Arms after the first irrefutable one are unreachable and contribute
        // nothing; the checker owns whether writing one is an error.
        let reachable = match first_irrefutable {
            Some(index) => &branches[..=index],
            None => branches,
        };

        // No variant arm at all: the match discriminates on nothing, so it is an
        // unconditional bind-and-run of its first arm.
        if reachable.iter().all(|branch| irrefutable(branch.pattern())) {
            let Some(branch) = reachable.first() else {
                builder.set_terminator(Terminator::Branch {
                    target: join_block,
                    arguments: Vec::new(),
                });
                builder.current = join_block;
                return Ok(());
            };
            let arm_block = builder.new_block(at);
            builder.set_terminator(Terminator::Branch {
                target: arm_block,
                arguments: Vec::new(),
            });
            builder.current = arm_block;
            let depth = builder.scope.len();
            match branch.pattern().form() {
                PatternForm::Tuple => {
                    let subject_ty = builder.type_of(&subject);
                    self.bind_pattern(branch.pattern(), subject_ty, subject.clone(), builder, at)?;
                }
                _ => self.bind_match_pattern(branch.pattern(), &subject, builder, at)?,
            }
            self.lower_block(branch.body(), builder)?;
            builder.scope.truncate(depth);
            if !builder.is_terminated() {
                builder.set_terminator(Terminator::Branch {
                    target: join_block,
                    arguments: Vec::new(),
                });
            }
            builder.current = join_block;
            return Ok(());
        }

        let mut arms: Vec<(usize, usize)> = Vec::new();
        let mut wildcard: Option<usize> = None;
        let mut bodies: Vec<(usize, &'source crate::parser::MatchBranch)> = Vec::new();
        for branch in reachable {
            let arm_block = builder.new_block(at);
            bodies.push((arm_block, branch));
            match branch.pattern().form() {
                PatternForm::Wildcard => wildcard = Some(arm_block),
                PatternForm::Name | PatternForm::Destructure => {
                    let Some(name) = branch.pattern().name() else {
                        return Err(self.gap("match pattern without a name", branch.span()));
                    };
                    let spelled = name.text(self.source);
                    match self.variant_index(spelled) {
                        Some(index) => arms.push((index, arm_block)),
                        // A bare binding catches everything the arms before it
                        // did not (ADR-0033).
                        None => wildcard = Some(arm_block),
                    }
                }
                PatternForm::Tuple => {
                    // A tuple pattern is irrefutable, so it is the last
                    // reachable arm and catches whatever preceded it did not.
                    wildcard = Some(arm_block);
                }
            }
        }
        let default = wildcard.unwrap_or(join_block);
        // `match_enum` carries a complete variant-to-target map; a wildcard or
        // binding arm fills every variant the source did not name. The checker
        // already proved completeness, so the missing entries are exactly the
        // default's.
        let covered: Vec<usize> = arms.iter().map(|(variant, _)| *variant).collect();
        let total = self.variant_count_of(&subject, builder);
        let mut complete = arms.clone();
        for index in 0..total {
            if !covered.contains(&index) {
                complete.push((index, default));
            }
        }
        complete.sort_by_key(|(variant, _)| *variant);
        builder.set_terminator(Terminator::MatchEnum {
            subject: subject.clone(),
            arms: complete,
        });

        for (arm_block, branch) in bodies {
            builder.current = arm_block;
            let depth = builder.scope.len();
            self.bind_match_pattern(branch.pattern(), &subject, builder, at)?;
            self.lower_block(branch.body(), builder)?;
            builder.scope.truncate(depth);
            if !builder.is_terminated() {
                builder.set_terminator(Terminator::Branch {
                    target: join_block,
                    arguments: Vec::new(),
                });
            }
        }
        builder.current = join_block;
        Ok(())
    }

    /// Binds the names a match arm destructures out of the subject.
    ///
    /// A payload name reads the subject's place at the position the variant
    /// declares, so the binding is the value that lives there rather than a
    /// fresh one. A bare binding arm names the whole subject.
    fn bind_match_pattern(
        &mut self,
        pattern: &'source Pattern,
        subject: &Operand,
        builder: &mut BodyBuilder,
        at: usize,
    ) -> Result<(), Gap> {
        let Operand::Value(root) = subject else {
            // A constant subject has no place to destructure; the checker
            // already rejected a pattern that would need one.
            return Ok(());
        };
        if pattern.form() == PatternForm::Name
            && !pattern.is_qualified()
            && pattern.elements().is_empty()
        {
            if let Some(name) = pattern.name() {
                let spelled = name.text(self.source);
                // A name that is a variant is a constructor, not a binding.
                if self.variant_index(spelled).is_none() {
                    builder.scope.push((spelled.to_string(), *root));
                }
            }
            return Ok(());
        }
        if pattern.form() != PatternForm::Destructure {
            return Ok(());
        }
        let variant = pattern
            .name()
            .and_then(|name| self.variant_index(name.text(self.source)));
        for (index, element) in pattern.elements().iter().enumerate() {
            let Some(name) = element.name() else {
                continue;
            };
            if element.form() != PatternForm::Name || element.is_qualified() {
                continue;
            }
            let place = Place {
                root: *root,
                path: alloc::vec![PlaceStep::Field(index)],
            };
            let ty = self.payload_type(*root, variant, index, builder);
            let slot = builder.define(ty);
            builder.push(Instruction {
                result: Some(slot),
                ty,
                op: Op::Read { place },
                source: at,
                runtime_contract: None,
                unsafe_block: builder.in_unsafe,
                unsafe_interface: None,
            });
            builder
                .scope
                .push((name.text(self.source).to_string(), slot));
        }
        Ok(())
    }

    /// The declared type of one payload position of a variant.
    fn payload_type(
        &self,
        root: ValueId,
        variant: Option<usize>,
        position: usize,
        builder: &BodyBuilder,
    ) -> TypeId {
        let subject = builder.values.get(root).copied().unwrap_or(0);
        match (self.types.get(subject), variant) {
            (Some(TypeDef::Nominal { variants, .. }), Some(index)) => variants
                .get(index)
                .and_then(|variant| variant.payload.get(position))
                .copied()
                .unwrap_or(subject),
            (Some(TypeDef::Option(inner)), _) => *inner,
            (Some(TypeDef::TaskResult(inner)), _) => *inner,
            (Some(TypeDef::Result(ok, error)), Some(index)) => {
                if index == 0 {
                    *ok
                } else {
                    *error
                }
            }
            _ => subject,
        }
    }

    fn variant_index(&self, name: &str) -> Option<usize> {
        match name {
            "None" => Some(0),
            "Some" => Some(1),
            "Ok" => Some(0),
            "Err" => Some(1),
            "Completed" => Some(0),
            "Cancelled" => Some(1),
            _ => self.variant_owner.get(name).map(|(_, index)| *index),
        }
    }

    fn variant_count_of(&self, subject: &Operand, builder: &BodyBuilder) -> usize {
        let Operand::Value(value) = subject else {
            return 0;
        };
        let Some(&ty) = builder.values.get(*value) else {
            return 0;
        };
        match self.types.get(ty) {
            Some(TypeDef::Nominal { variants, .. }) => variants.len(),
            Some(TypeDef::Option(_))
            | Some(TypeDef::Result(_, _))
            | Some(TypeDef::TaskResult(_)) => 2,
            _ => 0,
        }
    }

    fn bind_pattern(
        &mut self,
        pattern: &'source Pattern,
        ty: TypeId,
        value: Operand,
        builder: &mut BodyBuilder,
        at: usize,
    ) -> Result<(), Gap> {
        match pattern.form() {
            PatternForm::Name if !pattern.is_qualified() => {
                let Some(name) = pattern.name() else {
                    return Ok(());
                };
                let slot = builder.define(ty);
                builder.push(Instruction {
                    result: Some(slot),
                    ty,
                    op: match value {
                        Operand::Constant(constant) => Op::Const(constant),
                        Operand::Value(source_value) => Op::Move {
                            place: Place {
                                root: source_value,
                                path: Vec::new(),
                            },
                        },
                    },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                builder
                    .scope
                    .push((name.text(self.source).to_string(), slot));
                Ok(())
            }
            PatternForm::Wildcard => Ok(()),
            // A tuple pattern destructures by *place*, recursively.
            //
            // `tos-ir/v1` already expresses this: a tuple element is a
            // `PlaceStep::Field`, and taking one is the same `Move` the named
            // case emits. So destructuring needs no new IR operation and no new
            // schema — which also means the verifier's existing ownership and
            // type rules see it exactly as they see any other move out of an
            // aggregate, with no way to tell it came from a pattern.
            //
            // Nothing here decides ownership. The checker has already proved
            // what may be moved and what may not; this expresses that proof.
            PatternForm::Tuple => {
                let Operand::Value(root) = value else {
                    return Err(self.gap("destructuring a constant tuple", pattern.span()));
                };
                let Some(TypeDef::Tuple(elements)) = self.types.get(ty).cloned() else {
                    return Err(self.gap("destructuring a non-tuple", pattern.span()));
                };
                if elements.len() != pattern.elements().len() {
                    return Err(self.gap("tuple pattern arity", pattern.span()));
                }
                for (position, (element, element_ty)) in
                    pattern.elements().iter().zip(elements).enumerate()
                {
                    // A wildcard binds nothing, so it takes nothing: emitting a
                    // move for `_` would consume a component the program never
                    // named.
                    if element.form() == PatternForm::Wildcard {
                        continue;
                    }
                    let taken = builder.define(element_ty);
                    builder.push(Instruction {
                        result: Some(taken),
                        ty: element_ty,
                        op: Op::Move {
                            place: Place {
                                root,
                                path: alloc::vec![PlaceStep::Field(position)],
                            },
                        },
                        source: at,
                        runtime_contract: None,
                        unsafe_block: builder.in_unsafe,
                        unsafe_interface: None,
                    });
                    self.bind_pattern(element, element_ty, Operand::Value(taken), builder, at)?;
                }
                Ok(())
            }
            _ => Err(self.gap("destructuring let pattern", pattern.span())),
        }
    }

    // ----------------------------------------------------------- expressions

    fn unit_type(&mut self) -> TypeId {
        self.intern(TypeDef::Unit)
    }

    fn lower_place(
        &mut self,
        expression: &'source Expression,
        builder: &mut BodyBuilder,
    ) -> Result<Place, Gap> {
        match expression.form() {
            ExpressionForm::Name => {
                let name = expression.span().text(self.source);
                let Some(slot) = builder.lookup(name) else {
                    return Err(self.gap("unbound place", expression.span()));
                };
                Ok(Place {
                    root: slot,
                    path: Vec::new(),
                })
            }
            ExpressionForm::Group => {
                let Some(inner) = expression.inner() else {
                    return Err(self.gap("empty group place", expression.span()));
                };
                self.lower_place(inner, builder)
            }
            ExpressionForm::Field => {
                let Some(inner) = expression.inner() else {
                    return Err(self.gap("field without a base", expression.span()));
                };
                let mut place = self.lower_place(inner, builder)?;
                let Some(name) = expression.name() else {
                    return Err(self.gap("field without a name", expression.span()));
                };
                let base = builder.values.get(place.root).copied().unwrap_or(0);
                let index = self.field_index(base, name.text(self.source));
                place.path.push(PlaceStep::Field(index));
                Ok(place)
            }
            ExpressionForm::Index => {
                let Some(inner) = expression.inner() else {
                    return Err(self.gap("index without a base", expression.span()));
                };
                let mut place = self.lower_place(inner, builder)?;
                let constant = expression
                    .right()
                    .and_then(|index| constant_index(index, self.source));
                place.path.push(PlaceStep::Index(constant));
                Ok(place)
            }
            _ => Err(self.gap("expression is not a place", expression.span())),
        }
    }

    /// The declared position of a field in its record, by type.
    fn field_index(&self, ty: TypeId, field: &str) -> usize {
        let Some(TypeDef::Nominal { export_name, .. }) = self.types.get(ty) else {
            return 0;
        };
        let Some((_, names)) = self.nominals.get(export_name) else {
            return 0;
        };
        names
            .iter()
            .position(|declared| declared == field)
            .unwrap_or(0)
    }

    fn lower_expression(
        &mut self,
        expression: &'source Expression,
        builder: &mut BodyBuilder,
    ) -> Result<Operand, Gap> {
        let at = self.map(expression.span());
        match expression.form() {
            ExpressionForm::Literal => {
                let constant = self.literal_constant(expression)?;
                Ok(Operand::Constant(constant))
            }
            ExpressionForm::Group => {
                let Some(inner) = expression.inner() else {
                    return Err(self.gap("empty group", expression.span()));
                };
                self.lower_expression(inner, builder)
            }
            ExpressionForm::Name => {
                let name = expression.span().text(self.source);
                if let Some(slot) = builder.lookup(name) {
                    return Ok(Operand::Value(slot));
                }
                // A nullary variant constructor, such as a bare enum variant.
                if let Some(index) = self.variant_index(name) {
                    let ty = self.nullary_variant_type(name);
                    let value = builder.define(ty);
                    builder.push(Instruction {
                        result: Some(value),
                        ty,
                        op: Op::Variant {
                            ty,
                            index,
                            operands: Vec::new(),
                        },
                        source: at,
                        runtime_contract: None,
                        unsafe_block: builder.in_unsafe,
                        unsafe_interface: None,
                    });
                    return Ok(Operand::Value(value));
                }
                Err(self.gap("unresolved value name", expression.span()))
            }
            ExpressionForm::Field | ExpressionForm::Index => {
                let place = self.lower_place(expression, builder)?;
                let ty = self.place_type(&place, builder);
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Read { place },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                Ok(Operand::Value(value))
            }
            ExpressionForm::Binary => {
                let Some(operator) = expression.operator_text(self.source) else {
                    return Err(self.gap("binary without an operator", expression.span()));
                };
                let Some(op) = binary_op(operator) else {
                    return Err(self.gap("binary operator", expression.span()));
                };
                let (Some(left), Some(right)) = (expression.left(), expression.right()) else {
                    return Err(self.gap("binary without both sides", expression.span()));
                };
                let left = self.lower_expression(left, builder)?;
                let right = self.lower_expression(right, builder)?;
                let ty = if op.is_comparison() {
                    self.intern(TypeDef::Bool)
                } else {
                    builder.type_of(&left)
                };
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Binary { op, left, right },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                Ok(Operand::Value(value))
            }
            ExpressionForm::Unary => {
                let Some(operator) = expression.operator_text(self.source) else {
                    return Err(self.gap("unary without an operator", expression.span()));
                };
                if let Some(kind) = borrow_kind(operator) {
                    let Some(operand) = expression.inner() else {
                        return Err(self.gap("borrow without an operand", expression.span()));
                    };
                    let place = self.lower_place(operand, builder)?;
                    let ty = self.place_type(&place, builder);
                    let value = builder.define(ty);
                    builder.push(Instruction {
                        result: Some(value),
                        ty,
                        op: Op::Borrow { place, kind },
                        source: at,
                        runtime_contract: None,
                        unsafe_block: builder.in_unsafe,
                        unsafe_interface: None,
                    });
                    return Ok(Operand::Value(value));
                }
                if operator == "join" || operator == "await" {
                    let Some(operand) = expression.inner() else {
                        return Err(self.gap("join without an operand", expression.span()));
                    };
                    let task = self.lower_expression(operand, builder)?;
                    let payload = match self.types.get(builder.type_of(&task)) {
                        Some(TypeDef::Task(inner)) => *inner,
                        _ => self.unit_type(),
                    };
                    let ty = self.intern(TypeDef::TaskResult(payload));
                    let value = builder.define(ty);
                    builder.push(Instruction {
                        result: Some(value),
                        ty,
                        op: if operator == "join" {
                            Op::Join { task }
                        } else {
                            Op::Await { task }
                        },
                        source: at,
                        runtime_contract: Some(String::from("tos-runtime/task/v1")),
                        unsafe_block: builder.in_unsafe,
                        unsafe_interface: None,
                    });
                    return Ok(Operand::Value(value));
                }
                let op = match operator {
                    "-" => UnaryOp::Negate,
                    "!" => UnaryOp::Not,
                    _ => return Err(self.gap("unary operator", expression.span())),
                };
                let Some(operand) = expression.inner() else {
                    return Err(self.gap("unary without an operand", expression.span()));
                };
                let operand = self.lower_expression(operand, builder)?;
                let ty = builder.type_of(&operand);
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Unary { op, operand },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                Ok(Operand::Value(value))
            }
            ExpressionForm::Cast => {
                let Some(target) = expression.cast_type() else {
                    return Err(self.gap("cast without a type", expression.span()));
                };
                let ty = self.resolve_type(target)?;
                let Some(TypeDef::Int(kind)) = self.types.get(ty).cloned() else {
                    return Err(self.gap("cast target is not an integer", expression.span()));
                };
                let Some(operand) = expression.inner() else {
                    return Err(self.gap("cast without an operand", expression.span()));
                };
                let operand = self.lower_expression(operand, builder)?;
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Widen { operand, to: kind },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                Ok(Operand::Value(value))
            }
            ExpressionForm::Call => self.lower_call(expression, builder, at),
            ExpressionForm::Tuple => {
                let mut operands = Vec::new();
                let mut element_types = Vec::new();
                for element in expression.elements() {
                    let lowered = self.lower_expression(element, builder)?;
                    element_types.push(builder.type_of(&lowered));
                    operands.push(lowered);
                }
                let ty = self.intern(TypeDef::Tuple(element_types));
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Aggregate { ty, operands },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                Ok(Operand::Value(value))
            }
            ExpressionForm::Array => {
                let mut operands = Vec::new();
                let mut element_type = self.unit_type();
                for element in expression.elements() {
                    let lowered = self.lower_expression(element, builder)?;
                    element_type = builder.type_of(&lowered);
                    operands.push(lowered);
                }
                let ty = self.intern(TypeDef::Array(element_type, operands.len() as u64));
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Aggregate { ty, operands },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                Ok(Operand::Value(value))
            }
            ExpressionForm::Question => {
                let Some(operand) = expression.inner() else {
                    return Err(self.gap("propagation without an operand", expression.span()));
                };
                let result = self.lower_expression(operand, builder)?;
                let ok_block = builder.new_block(at);
                builder.set_terminator(Terminator::PropagateError {
                    result: result.clone(),
                    ok_target: ok_block,
                });
                builder.current = ok_block;
                // The Ok payload arrives as the block's value; its type is the
                // Ok arm of the propagated Result.
                let ty = match self.types.get(builder.type_of(&result)) {
                    Some(TypeDef::Result(ok, _)) => *ok,
                    _ => self.unit_type(),
                };
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Read {
                        place: Place {
                            root: match &result {
                                Operand::Value(id) => *id,
                                Operand::Constant(_) => 0,
                            },
                            path: alloc::vec![PlaceStep::Field(0)],
                        },
                    },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                Ok(Operand::Value(value))
            }
            ExpressionForm::Closure => {
                let Some(body) = expression.body() else {
                    return Err(self.gap("closure without a body", expression.span()));
                };
                let mut declared = Vec::new();
                let mut parameter_types = Vec::new();
                for parameter in expression.parameters() {
                    let ty = self.resolve_type(parameter.ty())?;
                    parameter_types.push(ty);
                    declared.push((parameter.name().text(self.source).to_string(), ty));
                }
                // A closure body is its own return scope; its result is the
                // type its declared function type gives it, and `unit` when the
                // body produces nothing.
                let result = self.body_result(body).unwrap_or_else(|| self.unit_type());
                let name = self.body_name("closure", expression.span());
                let (id, captures) =
                    self.lower_captured_body(&name, declared, body, result, builder)?;
                let ty = self.intern(TypeDef::Function(parameter_types, result));
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Closure { body: id, captures },
                    source: at,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                    runtime_contract: None,
                });
                Ok(Operand::Value(value))
            }
            ExpressionForm::Spawn => {
                let Some(body) = expression.body() else {
                    return Err(self.gap("spawn without a body", expression.span()));
                };
                let payload = self.body_result(body).unwrap_or_else(|| self.unit_type());
                let name = self.body_name("spawn", expression.span());
                let (id, captures) =
                    self.lower_captured_body(&name, Vec::new(), body, payload, builder)?;
                let ty = self.intern(TypeDef::Task(payload));
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Spawn { body: id, captures },
                    source: at,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                    runtime_contract: Some(String::from("tos-runtime/task/v1")),
                });
                Ok(Operand::Value(value))
            }
        }
    }

    fn place_type(&self, place: &Place, builder: &BodyBuilder) -> TypeId {
        let mut current = builder.values.get(place.root).copied().unwrap_or(0);
        for step in &place.path {
            current = match (self.types.get(current), step) {
                (Some(TypeDef::Nominal { fields, .. }), PlaceStep::Field(index)) => {
                    fields.get(*index).copied().unwrap_or(current)
                }
                (Some(TypeDef::Tuple(elements)), PlaceStep::Field(index)) => {
                    elements.get(*index).copied().unwrap_or(current)
                }
                (Some(TypeDef::Array(element, _)), PlaceStep::Index(_)) => *element,
                (Some(TypeDef::Slice(element)), PlaceStep::Index(_)) => *element,
                _ => current,
            };
        }
        current
    }

    fn nullary_variant_type(&mut self, name: &str) -> TypeId {
        if let Some(&(ty, _)) = self.variant_owner.get(name) {
            return ty;
        }
        let unit = self.intern(TypeDef::Unit);
        match name {
            "None" | "Some" => self.intern(TypeDef::Option(unit)),
            "Ok" | "Err" => self.intern(TypeDef::Result(unit, unit)),
            _ => self.intern(TypeDef::TaskResult(unit)),
        }
    }

    /// Lowers a call written as `receiver.operation(...)`.
    ///
    /// docs/43 section 3 requires the family to be visible: an atomic, a
    /// capability operation and an imported function are three different
    /// verifier-visible things, and none of them may hide behind an opaque
    /// helper. The receiver decides which one this is.
    fn lower_qualified_call(
        &mut self,
        expression: &'source Expression,
        callee: &'source Expression,
        builder: &mut BodyBuilder,
        at: usize,
    ) -> Result<Operand, Gap> {
        let Some(operation) = callee.name() else {
            return Err(self.gap("qualified call without an operation", expression.span()));
        };
        let operation = operation.text(self.source).to_string();
        let Some(receiver) = callee.inner() else {
            return Err(self.gap("qualified call without a receiver", expression.span()));
        };
        if receiver.form() != ExpressionForm::Name {
            return Err(self.gap("call through a computed callee", expression.span()));
        }
        let base = receiver.span().text(self.source).to_string();

        // A capability operation names its declared import and right.
        if let Some(import) = self.capability_binding(&base) {
            let mut operands = Vec::new();
            for argument in expression.arguments() {
                operands.push(self.lower_expression(argument.value(), builder)?);
            }
            let ty = self.unit_type();
            let value = builder.define(ty);
            builder.push(Instruction {
                result: Some(value),
                ty,
                op: Op::Capability {
                    import,
                    right: operation,
                    operands,
                },
                source: at,
                runtime_contract: Some(String::from("tos-runtime/capability/v1")),
                unsafe_block: builder.in_unsafe,
                unsafe_interface: None,
            });
            return Ok(Operand::Value(value));
        }

        // An atomic operation on a value of one of the three V1 atomic types.
        if let Some(slot) = builder.lookup(&base) {
            let receiver_type = builder.values.get(slot).copied().unwrap_or(0);
            let is_atomic = matches!(
                self.types.get(receiver_type),
                Some(TypeDef::AtomicBool) | Some(TypeDef::AtomicU32) | Some(TypeDef::AtomicU64)
            );
            if is_atomic {
                return self.lower_atomic(expression, slot, &operation, builder, at);
            }
            // A lock operation, decided from the receiver's *type* and never
            // from the operation's name (ADR-0035): `.lock()` written on
            // anything that is not a `Mutex<T>` is not an acquisition.
            let acquired = match (self.types.get(receiver_type), operation.as_str()) {
                (Some(TypeDef::Mutex(inner)), "lock") => {
                    Some((LockMode::Mutex, TypeDef::MutexGuard(*inner)))
                }
                (Some(TypeDef::RwLock(inner)), "read") => {
                    Some((LockMode::Read, TypeDef::ReadGuard(*inner)))
                }
                (Some(TypeDef::RwLock(inner)), "write") => {
                    Some((LockMode::Write, TypeDef::WriteGuard(*inner)))
                }
                _ => None,
            };
            if let Some((mode, guard)) = acquired {
                if !expression.arguments().is_empty() {
                    return Err(self.gap("lock operation arity", expression.span()));
                }
                let object = self.lower_expression(receiver, builder)?;
                let ty = self.intern(guard);
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Lock { object, mode },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                return Ok(Operand::Value(value));
            }
        }

        // A function of an imported module.
        if let Some(import) = self.module_binding(&base) {
            let mut operands = Vec::new();
            for argument in expression.arguments() {
                operands.push(self.lower_expression(argument.value(), builder)?);
            }
            // A single-module lowering knows the callee's name, not its
            // signature; the source-set step binds the imported type.
            let ty = self.unit_type();
            let value = builder.define(ty);
            builder.push(Instruction {
                result: Some(value),
                ty,
                op: Op::Call {
                    target: CallTarget::Imported {
                        import,
                        name: operation,
                    },
                    operands,
                },
                source: at,
                runtime_contract: None,
                unsafe_block: builder.in_unsafe,
                unsafe_interface: None,
            });
            return Ok(Operand::Value(value));
        }

        Err(self.gap("call through a computed callee", expression.span()))
    }

    fn lower_atomic(
        &mut self,
        expression: &'source Expression,
        receiver: ValueId,
        operation: &str,
        builder: &mut BodyBuilder,
        at: usize,
    ) -> Result<Operand, Gap> {
        let atomic = match operation {
            "load" => tos_ir::AtomicOp::Load,
            "store" => tos_ir::AtomicOp::Store,
            "swap" => tos_ir::AtomicOp::Swap,
            "fetch_add" => tos_ir::AtomicOp::FetchAdd,
            "fetch_sub" => tos_ir::AtomicOp::FetchSub,
            "fetch_and" => tos_ir::AtomicOp::FetchAnd,
            "fetch_or" => tos_ir::AtomicOp::FetchOr,
            "fetch_xor" => tos_ir::AtomicOp::FetchXor,
            "compare_exchange" => tos_ir::AtomicOp::CompareExchange,
            _ => return Err(self.gap("atomic operation", expression.span())),
        };
        let arguments = expression.arguments();
        let mut orders = Vec::new();
        let mut operands = Vec::new();
        for argument in arguments {
            let value = argument.value();
            if value.form() == ExpressionForm::Name {
                if let Some(order) = memory_order(value.span().text(self.source)) {
                    orders.push(order);
                    continue;
                }
            }
            operands.push(self.lower_expression(value, builder)?);
        }
        let Some(&order) = orders.first() else {
            return Err(self.gap("atomic call without an order", expression.span()));
        };
        let failure_order = orders.get(1).copied();
        let ty = builder.values.get(receiver).copied().unwrap_or(0);
        let result_type = match self.types.get(ty) {
            Some(TypeDef::AtomicBool) => self.intern(TypeDef::Bool),
            Some(TypeDef::AtomicU32) => self.intern(TypeDef::Int(IntKind::U32)),
            Some(TypeDef::AtomicU64) => self.intern(TypeDef::Int(IntKind::U64)),
            _ => self.unit_type(),
        };
        let value = builder.define(result_type);
        builder.push(Instruction {
            result: Some(value),
            ty: result_type,
            op: Op::Atomic {
                operation: atomic,
                target: Operand::Value(receiver),
                operands,
                order,
                failure_order,
            },
            source: at,
            runtime_contract: Some(String::from("tos-runtime/atomic/v1")),
            unsafe_block: builder.in_unsafe,
            unsafe_interface: None,
        });
        Ok(Operand::Value(value))
    }

    fn capability_binding(&self, name: &str) -> Option<usize> {
        self.schema
            .outline()
            .prefix()
            .imports()
            .iter()
            .filter(|import| import.kind() == crate::parser::ImportKind::Capability)
            .position(|import| import.binding().text(self.source) == name)
    }

    fn module_binding(&self, name: &str) -> Option<usize> {
        self.schema
            .outline()
            .prefix()
            .imports()
            .iter()
            .filter(|import| import.kind() == crate::parser::ImportKind::Module)
            .position(|import| import.binding().text(self.source) == name)
    }

    fn lower_call(
        &mut self,
        expression: &'source Expression,
        builder: &mut BodyBuilder,
        at: usize,
    ) -> Result<Operand, Gap> {
        let Some(callee) = expression.callee() else {
            return Err(self.gap("call without a callee", expression.span()));
        };
        if callee.form() == ExpressionForm::Field {
            return self.lower_qualified_call(expression, callee, builder, at);
        }
        if callee.form() != ExpressionForm::Name {
            return Err(self.gap("call through a computed callee", expression.span()));
        }
        let name = callee.span().text(self.source).to_string();

        // A name bound to a closure value calls that value, not a declared
        // function: the callee is an operand, so no name is resolved at run
        // time.
        if let Some(slot) = builder.lookup(&name) {
            let mut operands = Vec::new();
            for argument in expression.arguments() {
                operands.push(self.lower_expression(argument.value(), builder)?);
            }
            let ty = match self
                .types
                .get(builder.values.get(slot).copied().unwrap_or(0))
            {
                Some(TypeDef::Function(_, result)) => *result,
                _ => self.unit_type(),
            };
            let value = builder.define(ty);
            builder.push(Instruction {
                result: Some(value),
                ty,
                op: Op::CallValue {
                    callee: Operand::Value(slot),
                    operands,
                },
                source: at,
                unsafe_block: builder.in_unsafe,
                unsafe_interface: None,
                runtime_contract: None,
            });
            return Ok(Operand::Value(value));
        }

        // A record constructor supplies named arguments in declared order.
        if let Some(&(ty, ref field_names)) = self.nominals.get(&name) {
            let field_names = field_names.clone();
            let mut ordered: Vec<Option<Operand>> = alloc::vec![None; field_names.len()];
            for (position, argument) in expression.arguments().iter().enumerate() {
                let lowered = self.lower_expression(argument.value(), builder)?;
                let index = match argument.name() {
                    Some(label) => field_names
                        .iter()
                        .position(|declared| declared == label.text(self.source))
                        .unwrap_or(position),
                    None => position,
                };
                if index < ordered.len() {
                    ordered[index] = Some(lowered);
                }
            }
            let unit = self.unit_type();
            let operands = ordered
                .into_iter()
                .map(|operand| {
                    operand.unwrap_or(Operand::Constant(
                        self.constant_index
                            .get(&Constant::Unit)
                            .copied()
                            .unwrap_or(0),
                    ))
                })
                .collect::<Vec<_>>();
            let _ = unit;
            let value = builder.define(ty);
            builder.push(Instruction {
                result: Some(value),
                ty,
                op: Op::Aggregate { ty, operands },
                source: at,
                runtime_contract: None,
                unsafe_block: builder.in_unsafe,
                unsafe_interface: None,
            });
            return Ok(Operand::Value(value));
        }

        // An enum, Option or Result variant with a payload.
        if let Some(index) = self.variant_index(&name) {
            if self.variant_owner.contains_key(&name) || is_predeclared_variant(&name) {
                let mut operands = Vec::new();
                for argument in expression.arguments() {
                    operands.push(self.lower_expression(argument.value(), builder)?);
                }
                let ty = match self.variant_owner.get(&name) {
                    Some(&(ty, _)) => ty,
                    None => {
                        let payload = operands
                            .first()
                            .map(|operand| builder.type_of(operand))
                            .unwrap_or_else(|| self.intern(TypeDef::Unit));
                        match name.as_str() {
                            "Some" | "None" => self.intern(TypeDef::Option(payload)),
                            "Ok" => {
                                let unit = self.intern(TypeDef::Unit);
                                self.intern(TypeDef::Result(payload, unit))
                            }
                            "Err" => {
                                let unit = self.intern(TypeDef::Unit);
                                self.intern(TypeDef::Result(unit, payload))
                            }
                            _ => self.intern(TypeDef::TaskResult(payload)),
                        }
                    }
                };
                let value = builder.define(ty);
                builder.push(Instruction {
                    result: Some(value),
                    ty,
                    op: Op::Variant {
                        ty,
                        index,
                        operands,
                    },
                    source: at,
                    runtime_contract: None,
                    unsafe_block: builder.in_unsafe,
                    unsafe_interface: None,
                });
                return Ok(Operand::Value(value));
            }
        }

        let mut operands = Vec::new();
        for argument in expression.arguments() {
            operands.push(self.lower_expression(argument.value(), builder)?);
        }
        // `share` lowers to its own operation, never to an opaque helper call:
        // docs/43 section 3 forbids hiding shared-memory access behind one, and
        // the verifier rechecks the shareability requirement on this operation.
        if name == "share" && !self.functions_by_name.contains_key(&name) {
            let [operand] = operands.as_slice() else {
                return Err(self.gap("share arity", expression.span()));
            };
            let operand = operand.clone();
            let inner = builder.type_of(&operand);
            let ty = self.intern(TypeDef::Shared(inner));
            let value = builder.define(ty);
            builder.push(Instruction {
                result: Some(value),
                ty,
                op: Op::Share { operand },
                source: at,
                runtime_contract: None,
                unsafe_block: builder.in_unsafe,
                unsafe_interface: None,
            });
            return Ok(Operand::Value(value));
        }
        let (target, ty) = match self.functions_by_name.get(&name) {
            Some(&index) => {
                let result = self.schema.functions()[index].signature().result();
                let ty = self.resolve_type(result)?;
                (CallTarget::Local(index), ty)
            }
            None => {
                let ty = self.unit_type();
                (CallTarget::Predeclared(name), ty)
            }
        };
        let value = builder.define(ty);
        builder.push(Instruction {
            result: Some(value),
            ty,
            op: Op::Call { target, operands },
            source: at,
            runtime_contract: None,
            unsafe_block: builder.in_unsafe,
            unsafe_interface: None,
        });
        Ok(Operand::Value(value))
    }

    fn literal_constant(&mut self, expression: &'source Expression) -> Result<usize, Gap> {
        let text = expression.span().text(self.source);
        if text == "true" {
            return Ok(self.intern_constant(Constant::Bool(true)));
        }
        if text == "false" {
            return Ok(self.intern_constant(Constant::Bool(false)));
        }
        if let Some(rest) = text.strip_prefix("b\"") {
            let body = rest.strip_suffix('"').unwrap_or(rest);
            return Ok(self.intern_constant(Constant::Bytes(body.as_bytes().to_vec())));
        }
        if let Some(rest) = text.strip_prefix('"') {
            let body = rest.strip_suffix('"').unwrap_or(rest);
            return Ok(self.intern_constant(Constant::Text(body.to_string())));
        }
        let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return Err(self.gap("literal form", expression.span()));
        }
        let suffix = &text[digits.len()..];
        let Ok(magnitude) = digits.parse::<i128>() else {
            return Err(self.gap("integer literal magnitude", expression.span()));
        };
        if let Some(kind) = IntKind::parse(suffix) {
            return Ok(self.intern_constant(Constant::Int(kind, magnitude)));
        }
        let constant = match suffix {
            "" => Constant::Int(IntKind::I32, magnitude),
            "B" => Constant::Size(magnitude as u128),
            "KiB" => Constant::Size(magnitude as u128 * 1024),
            "MiB" => Constant::Size(magnitude as u128 * 1024 * 1024),
            "GiB" => Constant::Size(magnitude as u128 * 1024 * 1024 * 1024),
            "ns" => Constant::Duration(magnitude as u128),
            "us" => Constant::Duration(magnitude as u128 * 1_000),
            "ms" => Constant::Duration(magnitude as u128 * 1_000_000),
            "s" => Constant::Duration(magnitude as u128 * 1_000_000_000),
            _ => return Err(self.gap("literal suffix", expression.span())),
        };
        Ok(self.intern_constant(constant))
    }
}

fn is_predeclared_variant(name: &str) -> bool {
    matches!(
        name,
        "Some" | "None" | "Ok" | "Err" | "Completed" | "Cancelled"
    )
}

#[derive(Clone, Copy, Debug)]
struct LoopFrame {
    head: usize,
    exit: usize,
    /// How many lexical cleanup scopes were open when the loop began, so a
    /// `break` or `continue` runs the cleanups of exactly the blocks it leaves.
    cleanup_depth: usize,
}

/// The blocks and values of one function under construction.
struct BodyBuilder {
    values: Vec<TypeId>,
    /// Whether the walk is inside an `unsafe` block, which marks every
    /// operation it emits (docs/43 section 3).
    in_unsafe: bool,
    blocks: Vec<Block>,
    current: usize,
    scope: Vec<(String, ValueId)>,
    loops: Vec<LoopFrame>,
    result: TypeId,
    /// Cleanup bodies registered by `defer`, innermost lexical block last.
    ///
    /// ADR-0035 makes cleanup lexical, so what runs at an exit is the suffix
    /// registered by the blocks that exit leaves, in reverse order.
    cleanups: Vec<Vec<CleanupCall>>,
}

impl BodyBuilder {
    fn define(&mut self, ty: TypeId) -> ValueId {
        let id = self.values.len();
        self.values.push(ty);
        id
    }

    fn type_of(&self, operand: &Operand) -> TypeId {
        match operand {
            Operand::Value(value) => self.values.get(*value).copied().unwrap_or(self.result),
            // A constant's type is fixed by the constant table; the operand
            // alone does not carry it, and the verifier reads it from there.
            Operand::Constant(_) => self.result,
        }
    }

    fn new_block(&mut self, source: usize) -> usize {
        let id = self.blocks.len();
        self.blocks.push(Block {
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Trap(String::from("RUNTIME_UNREACHABLE")),
            source,
        });
        id
    }

    fn push(&mut self, instruction: Instruction) {
        self.blocks[self.current].instructions.push(instruction);
    }

    fn set_terminator(&mut self, terminator: Terminator) {
        self.blocks[self.current].terminator = terminator;
    }

    /// Whether the current block already ends.
    fn is_terminated(&self) -> bool {
        !matches!(
            &self.blocks[self.current].terminator,
            Terminator::Trap(code) if code == "RUNTIME_UNREACHABLE"
        )
    }

    fn lookup(&self, name: &str) -> Option<ValueId> {
        self.scope
            .iter()
            .rev()
            .find(|(declared, _)| declared == name)
            .map(|(_, slot)| *slot)
    }
}

fn binary_op(operator: &str) -> Option<BinaryOp> {
    Some(match operator {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Subtract,
        "*" => BinaryOp::Multiply,
        "/" => BinaryOp::Divide,
        "%" => BinaryOp::Remainder,
        "<<" => BinaryOp::ShiftLeft,
        ">>" => BinaryOp::ShiftRight,
        "&" => BinaryOp::BitAnd,
        "|" => BinaryOp::BitOr,
        "^" => BinaryOp::BitXor,
        "==" => BinaryOp::Equal,
        "!=" => BinaryOp::NotEqual,
        "<" => BinaryOp::Less,
        "<=" => BinaryOp::LessOrEqual,
        ">" => BinaryOp::Greater,
        ">=" => BinaryOp::GreaterOrEqual,
        "&&" => BinaryOp::LogicalAnd,
        "||" => BinaryOp::LogicalOr,
        _ => return None,
    })
}

/// Collects, in first-use order, the names a block uses but does not declare.
///
/// Declarations are lexical: a `let` covers the statements after it in its own
/// block, and a nested block, branch, arm or loop pattern gets a child
/// environment, so a name declared in one place never hides a use in a sibling.
fn collect_free_names(
    source: &SourceUnit,
    block: &crate::parser::Block,
    bound: &mut BTreeSet<String>,
    free: &mut Vec<String>,
) {
    let mut scope = bound.clone();
    for statement in block.statements() {
        for expression in [statement.target(), statement.expression()]
            .into_iter()
            .flatten()
        {
            free_names_in_expression(source, expression, &scope, free);
        }
        if statement.form() == StatementForm::Let {
            if let Some(pattern) = statement.pattern() {
                declare_pattern_names(source, pattern, &mut scope);
            }
        }
        let mut nested_scope = scope.clone();
        if statement.form() == StatementForm::For {
            if let Some(pattern) = statement.pattern() {
                declare_pattern_names(source, pattern, &mut nested_scope);
            }
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            let mut child = nested_scope.clone();
            collect_free_names(source, nested, &mut child, free);
        }
        if let Some(chained) = statement.else_if() {
            let mut child = scope.clone();
            free_names_in_statement(source, chained, &mut child, free);
        }
        for branch in statement.branches() {
            let mut arm = scope.clone();
            declare_pattern_names(source, branch.pattern(), &mut arm);
            collect_free_names(source, branch.body(), &mut arm, free);
        }
    }
}

fn free_names_in_statement(
    source: &SourceUnit,
    statement: &Statement,
    bound: &mut BTreeSet<String>,
    free: &mut Vec<String>,
) {
    for expression in [statement.target(), statement.expression()]
        .into_iter()
        .flatten()
    {
        free_names_in_expression(source, expression, bound, free);
    }
    for nested in [statement.body(), statement.else_body()]
        .into_iter()
        .flatten()
    {
        let mut child = bound.clone();
        collect_free_names(source, nested, &mut child, free);
    }
    if let Some(chained) = statement.else_if() {
        let mut child = bound.clone();
        free_names_in_statement(source, chained, &mut child, free);
    }
    for branch in statement.branches() {
        let mut arm = bound.clone();
        declare_pattern_names(source, branch.pattern(), &mut arm);
        collect_free_names(source, branch.body(), &mut arm, free);
    }
}

fn declare_pattern_names(source: &SourceUnit, pattern: &Pattern, bound: &mut BTreeSet<String>) {
    match pattern.form() {
        PatternForm::Name if !pattern.is_qualified() => {
            if let Some(name) = pattern.name() {
                bound.insert(name.text(source).to_string());
            }
        }
        PatternForm::Destructure | PatternForm::Tuple => {
            for element in pattern.elements() {
                declare_pattern_names(source, element, bound);
            }
        }
        _ => {}
    }
}

fn free_names_in_expression(
    source: &SourceUnit,
    expression: &Expression,
    bound: &BTreeSet<String>,
    free: &mut Vec<String>,
) {
    if expression.form() == ExpressionForm::Name {
        let name = expression.span().text(source).to_string();
        if !bound.contains(&name) && !free.contains(&name) {
            free.push(name);
        }
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
        free_names_in_expression(source, child, bound, free);
    }
    for argument in expression.arguments() {
        free_names_in_expression(source, argument.value(), bound, free);
    }
    for element in expression.elements() {
        free_names_in_expression(source, element, bound, free);
    }
    if let Some(body) = expression.body() {
        // A nested closure sees this environment plus its own parameters;
        // whatever it uses freely is also free here.
        let mut inner = bound.clone();
        for parameter in expression.parameters() {
            inner.insert(parameter.name().text(source).to_string());
        }
        collect_free_names(source, body, &mut inner, free);
    }
}

/// The expression of the first `return` a block performs, if it has one.
fn first_returned_expression(block: &crate::parser::Block) -> Option<&Expression> {
    for statement in block.statements() {
        if statement.form() == StatementForm::Return {
            return statement.expression();
        }
        for nested in [statement.body(), statement.else_body()]
            .into_iter()
            .flatten()
        {
            if let Some(found) = first_returned_expression(nested) {
                return Some(found);
            }
        }
        for branch in statement.branches() {
            if let Some(found) = first_returned_expression(branch.body()) {
                return Some(found);
            }
        }
    }
    None
}

fn borrow_kind(operator: &str) -> Option<tos_ir::BorrowKind> {
    match operator {
        "borrow" => Some(tos_ir::BorrowKind::Shared),
        "borrow mut" => Some(tos_ir::BorrowKind::Mutable),
        _ => None,
    }
}

fn memory_order(name: &str) -> Option<tos_ir::MemoryOrder> {
    Some(match name {
        "Relaxed" => tos_ir::MemoryOrder::Relaxed,
        "Acquire" => tos_ir::MemoryOrder::Acquire,
        "Release" => tos_ir::MemoryOrder::Release,
        "AcqRel" => tos_ir::MemoryOrder::AcqRel,
        "SeqCst" => tos_ir::MemoryOrder::SeqCst,
        _ => return None,
    })
}

fn constant_index(expression: &Expression, source: &SourceUnit) -> Option<u64> {
    if expression.form() != ExpressionForm::Literal {
        return None;
    }
    let text = expression.span().text(source);
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}
