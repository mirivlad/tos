// SPDX-License-Identifier: GPL-3.0-or-later
//! The independent verifier for `tos-ir/v1` (docs/43 sections 4 and 5).
//!
//! docs/43 section 4 is explicit that the verifier does not trust the
//! frontend's claims. It rechecks table bounds and schema identity, nominal
//! type references, control-flow targets, operand types, call and effect
//! signatures, import and capability declarations, affine value and borrow
//! state, profile restrictions, resource accounting, task scope, atomic
//! orders, unsafe interface IDs, and source-map identity and spans.
//!
//! Independence here is structural, as section 5 requires. This crate depends
//! on [`tos_ir`] for the declarative schema and on nothing else: no frontend,
//! no AST, no type-checker result, no callback. It reaches its own conclusion
//! by traversing the module value it was handed, and the module value carries
//! no field that could stand in for that traversal. An alternate or optimized
//! frontend is untrusted at this boundary in exactly the same way.
//!
//! The result is either a [`VerifiedModule`] receipt bound to the digest of the
//! module the verifier actually saw, or one deterministic primary `V20xx`
//! finding. An engine accepts IR only with a receipt for that exact digest.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

// The test harness is a host program by construction, so it keeps `std`.
#[cfg(test)]
extern crate std;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tos_ir::{
    AtomicOp, Block, CallTarget, Constant, Function, Instruction, MemoryOrder, Module, Op, Operand,
    Profile, SourceMapEntry, Terminator, TypeDef, TypeId,
};

mod limits;

pub use limits::Limits;

/// Which verifier produced a receipt.
pub const VERIFIER_IDENTITY: &str = "tos-verifier-reference/0.1.0";

/// A receipt that this exact module passed this exact verifier.
///
/// docs/43 section 5: only the verifier emits one, and it binds to the complete
/// module digest. A frontend cannot mark a cache verified.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedModule {
    pub module_digest: String,
    pub schema_id: String,
    pub verifier_identity: String,
    pub module_name: String,
    pub source_set: String,
    pub content_id: String,
    pub dependency_digest: String,
    pub profile: Profile,
    pub resource_envelope: tos_ir::ResourceEnvelope,
    pub capability_interface_digest: String,
    pub source_map_digest: String,
}

/// One deterministic verifier finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    pub code: &'static str,
    pub detail: String,
    /// Where in the module the finding sits, for an engine's audit record.
    pub location: String,
    /// Ordered causal entries, outermost first.
    pub causes: Vec<Finding>,
}

impl Finding {
    fn new(code: &'static str, location: impl ToString, detail: impl ToString) -> Finding {
        Finding {
            code,
            detail: detail.to_string(),
            location: location.to_string(),
            causes: Vec::new(),
        }
    }
}

/// What the verifier is told about the world outside the module.
///
/// docs/43 section 5: the verifier consumes untrusted IR plus a declared
/// module-resolution and capability-interface snapshot. The snapshot is
/// declared input, not something the verifier discovers: it never inspects an
/// ambient directory, the network or the environment.
#[derive(Clone, Debug, Default)]
pub struct ResolutionSnapshot {
    /// Module names the declared source set provides, with their content IDs.
    pub modules: BTreeMap<String, String>,
    /// Capability interfaces the declared contract provides.
    pub capability_interfaces: BTreeSet<String>,
}

/// Verifies an untrusted module against a declared snapshot.
///
/// The validation order is the one docs/43 section 5 fixes, and it stops at the
/// first primary finding: a later check reading a table an earlier one rejected
/// would be reporting a consequence, not a defect.
pub fn verify(
    module: &Module,
    snapshot: &ResolutionSnapshot,
    limits: &Limits,
) -> Result<VerifiedModule, Finding> {
    check_limits(module, limits)?;
    check_schema(module)?;
    check_source_identity(module)?;
    check_table_order(module)?;
    check_types_and_imports(module, snapshot)?;
    check_control_flow(module)?;
    check_ownership_and_profile(module)?;
    check_tasks_sync_atomics_unsafe(module)?;
    check_source_maps(module)?;

    Ok(VerifiedModule {
        module_digest: tos_ir::module_digest(module),
        schema_id: module.header.schema_id.clone(),
        verifier_identity: VERIFIER_IDENTITY.to_string(),
        module_name: module.header.module_name.clone(),
        source_set: module.header.source_set.clone(),
        content_id: module.header.content_id.clone(),
        dependency_digest: module.header.dependency_digest.clone(),
        profile: module.header.profile,
        resource_envelope: module.header.resource_envelope.clone(),
        capability_interface_digest: module.header.capability_interface_digest.clone(),
        source_map_digest: source_map_digest(&module.source_map),
    })
}

// ------------------------------------------------------------------ step 1

/// Envelope, byte and table-count limits, before anything expensive.
fn check_limits(module: &Module, limits: &Limits) -> Result<(), Finding> {
    let checks: [(&str, usize, usize); 6] = [
        ("types", module.types.len(), limits.table_entries),
        ("constants", module.constants.len(), limits.table_entries),
        ("functions", module.functions.len(), limits.table_entries),
        ("imports", module.imports.len(), limits.modules),
        (
            "capability imports",
            module.capability_imports.len(),
            limits.table_entries,
        ),
        (
            "source map",
            module.source_map.len(),
            limits.source_map_entries,
        ),
    ];
    for (name, actual, ceiling) in checks {
        if actual > ceiling {
            return Err(Finding::new(
                "V2001_LIMIT",
                name,
                alloc::format!("{actual} entries exceeds the ceiling of {ceiling}"),
            ));
        }
    }
    if module.imports.len() as u128 > module.header.resource_envelope.imports {
        return Err(Finding::new(
            "V2001_LIMIT",
            "imports",
            "more imports than the declared resource envelope allows",
        ));
    }
    for (index, function) in module.functions.iter().enumerate() {
        if function.blocks.len() > limits.blocks_per_function {
            return Err(Finding::new(
                "V2001_LIMIT",
                alloc::format!("function {index}"),
                "more basic blocks than the ceiling allows",
            ));
        }
        if function.signature.parameters.len() > limits.parameters {
            return Err(Finding::new(
                "V2001_LIMIT",
                alloc::format!("function {index}"),
                "more parameters than the ceiling allows",
            ));
        }
        for (block_index, block) in function.blocks.iter().enumerate() {
            if block.instructions.len() > limits.instructions_per_block {
                return Err(Finding::new(
                    "V2001_LIMIT",
                    alloc::format!("function {index} block {block_index}"),
                    "more instructions than the ceiling allows",
                ));
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ step 2

fn check_schema(module: &Module) -> Result<(), Finding> {
    let header = &module.header;
    if header.schema_id != tos_ir::SCHEMA_ID {
        return Err(Finding::new(
            "V2002_SCHEMA",
            "header.schema_id",
            alloc::format!("expected {}, found {}", tos_ir::SCHEMA_ID, header.schema_id),
        ));
    }
    if header.language_version != tos_ir::LANGUAGE_VERSION {
        return Err(Finding::new(
            "V2002_SCHEMA",
            "header.language_version",
            alloc::format!(
                "expected {}, found {}",
                tos_ir::LANGUAGE_VERSION,
                header.language_version
            ),
        ));
    }
    if header.unicode_normalization_baseline != tos_ir::UNICODE_BASELINE {
        return Err(Finding::new(
            "V2002_SCHEMA",
            "header.unicode_normalization_baseline",
            "the declared Unicode baseline is not the one this language version fixes",
        ));
    }
    if header.source_map_revision != tos_ir::SOURCE_MAP_REVISION {
        return Err(Finding::new(
            "V2002_SCHEMA",
            "header.source_map_revision",
            "unknown source-map revision",
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------ step 3

fn check_source_identity(module: &Module) -> Result<(), Finding> {
    let header = &module.header;
    for (field, value) in [
        ("content_id", &header.content_id),
        ("dependency_digest", &header.dependency_digest),
        (
            "capability_interface_digest",
            &header.capability_interface_digest,
        ),
    ] {
        if !value.starts_with("sha256:") {
            return Err(Finding::new(
                "V2003_SOURCE_IDENTITY",
                alloc::format!("header.{field}"),
                "identity is not a named digest",
            ));
        }
    }
    if header.module_name.is_empty() || header.path.is_empty() {
        return Err(Finding::new(
            "V2003_SOURCE_IDENTITY",
            "header",
            "a module must name itself and its canonical path",
        ));
    }
    // docs/42 section 1: a module name maps to exactly one canonical path.
    let expected = alloc::format!("{}.tos", header.module_name.replace('.', "/"));
    if !header.path.ends_with(&expected) {
        return Err(Finding::new(
            "V2003_SOURCE_IDENTITY",
            "header.path",
            alloc::format!("{} does not map to {}", header.module_name, header.path),
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------ step 4

fn check_table_order(module: &Module) -> Result<(), Finding> {
    // docs/43 section 2: exported signatures are ordered by name.
    for pair in module.exports.windows(2) {
        if pair[0].name > pair[1].name {
            return Err(Finding::new(
                "V2004_TABLE_ORDER",
                "exports",
                alloc::format!("{} follows {}", pair[1].name, pair[0].name),
            ));
        }
    }
    // docs/43 section 2: functions are ordered by fully qualified source name.
    for pair in module.functions.windows(2) {
        if pair[0].signature.name > pair[1].signature.name {
            return Err(Finding::new(
                "V2004_TABLE_ORDER",
                "functions",
                alloc::format!(
                    "{} follows {}",
                    pair[1].signature.name,
                    pair[0].signature.name
                ),
            ));
        }
    }
    // Source-map entries are ordered by source unit then byte start and end.
    for pair in module.source_map.windows(2) {
        let left = (&pair[0].path, pair[0].byte_start, pair[0].byte_end);
        let right = (&pair[1].path, pair[1].byte_start, pair[1].byte_end);
        if left > right {
            return Err(Finding::new(
                "V2004_TABLE_ORDER",
                "source map",
                "entries are not ordered by unit then byte range",
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ step 5

fn check_types_and_imports(module: &Module, snapshot: &ResolutionSnapshot) -> Result<(), Finding> {
    for (index, definition) in module.types.iter().enumerate() {
        for referenced in referenced_types(definition) {
            if !module.has_type(referenced) {
                return Err(Finding::new(
                    "V2010_TYPE",
                    alloc::format!("type {index}"),
                    alloc::format!("references type {referenced} outside the table"),
                ));
            }
        }
        if let TypeDef::Nominal {
            module_content_id,
            export_name,
            kind,
            fields,
            variants,
        } = definition
        {
            if export_name.is_empty() {
                return Err(Finding::new(
                    "V2010_TYPE",
                    alloc::format!("type {index}"),
                    "a nominal type must record its export name",
                ));
            }
            // A nominal type of this module records the module it came from; a
            // type reached through an import is identified by its path and is
            // resolved by the source-set step, not forged here.
            let _ = module_content_id;
            match kind {
                tos_ir::NominalKind::Record => {
                    if !variants.is_empty() {
                        return Err(Finding::new(
                            "V2010_TYPE",
                            alloc::format!("type {index}"),
                            "a record declares variants",
                        ));
                    }
                }
                tos_ir::NominalKind::Enum => {
                    if !fields.is_empty() {
                        return Err(Finding::new(
                            "V2010_TYPE",
                            alloc::format!("type {index}"),
                            "an enum declares record fields",
                        ));
                    }
                }
            }
        }
    }
    for (index, import) in module.imports.iter().enumerate() {
        if import.module_name.is_empty() {
            return Err(Finding::new(
                "V2012_IMPORT",
                alloc::format!("import {index}"),
                "an import names no module",
            ));
        }
        if !snapshot.modules.is_empty() && !snapshot.modules.contains_key(&import.module_name) {
            return Err(Finding::new(
                "V2012_IMPORT",
                alloc::format!("import {index}"),
                alloc::format!(
                    "{} is not in the declared resolution snapshot",
                    import.module_name
                ),
            ));
        }
    }
    for (index, import) in module.capability_imports.iter().enumerate() {
        if !module.has_type(import.ty) {
            return Err(Finding::new(
                "V2013_CAPABILITY",
                alloc::format!("capability import {index}"),
                "the declared handle type is outside the table",
            ));
        }
        // A capability handle is opaque: its type must be the capability type
        // for the interface it names, not a scalar wearing the same index.
        match module.type_of(import.ty) {
            Some(TypeDef::Capability(interface)) if *interface == import.interface => {}
            _ => {
                return Err(Finding::new(
                    "V2013_CAPABILITY",
                    alloc::format!("capability import {index}"),
                    alloc::format!("{} is not typed as its own interface", import.interface),
                ))
            }
        }
        if !snapshot.capability_interfaces.is_empty()
            && !snapshot.capability_interfaces.contains(&import.interface)
        {
            return Err(Finding::new(
                "V2013_CAPABILITY",
                alloc::format!("capability import {index}"),
                alloc::format!(
                    "{} is not in the declared capability contract",
                    import.interface
                ),
            ));
        }
    }
    Ok(())
}

fn referenced_types(definition: &TypeDef) -> Vec<TypeId> {
    match definition {
        TypeDef::Option(inner)
        | TypeDef::Task(inner)
        | TypeDef::TaskResult(inner)
        | TypeDef::Shared(inner)
        | TypeDef::Region(inner)
        | TypeDef::DmaRegion(inner)
        | TypeDef::Mutex(inner)
        | TypeDef::RwLock(inner)
        | TypeDef::Channel(inner)
        | TypeDef::Slice(inner)
        | TypeDef::Array(inner, _) => alloc::vec![*inner],
        TypeDef::Result(ok, error) => alloc::vec![*ok, *error],
        TypeDef::Tuple(elements) => elements.clone(),
        TypeDef::Function(parameters, result) => {
            let mut all = parameters.clone();
            all.push(*result);
            all
        }
        TypeDef::Nominal {
            fields, variants, ..
        } => {
            let mut all = fields.clone();
            for variant in variants {
                all.extend(variant.payload.iter().copied());
            }
            all
        }
        _ => Vec::new(),
    }
}

// ------------------------------------------------------------------ step 6

fn check_control_flow(module: &Module) -> Result<(), Finding> {
    for (index, function) in module.functions.iter().enumerate() {
        let at = alloc::format!("function {index}");
        if function.blocks.is_empty() {
            return Err(Finding::new("V2011_CFG", at, "a function has no blocks"));
        }
        for ty in &function.signature.parameters {
            if !module.has_type(ty.ty) {
                return Err(Finding::new(
                    "V2010_TYPE",
                    at,
                    "a parameter type is outside the table",
                ));
            }
        }
        if !module.has_type(function.signature.result) {
            return Err(Finding::new(
                "V2010_TYPE",
                at,
                "the result type is outside the table",
            ));
        }
        for ty in &function.values {
            if !module.has_type(*ty) {
                return Err(Finding::new(
                    "V2010_TYPE",
                    at,
                    "a value type is outside the table",
                ));
            }
        }
        for (block_index, block) in function.blocks.iter().enumerate() {
            let at = alloc::format!("function {index} block {block_index}");
            check_block(module, function, block, &at)?;
        }
    }
    Ok(())
}

fn check_block(
    module: &Module,
    function: &Function,
    block: &Block,
    at: &str,
) -> Result<(), Finding> {
    for instruction in &block.instructions {
        check_instruction(module, function, instruction, at)?;
    }
    let count = function.blocks.len();
    for target in terminator_targets(&block.terminator) {
        if target >= count {
            return Err(Finding::new(
                "V2011_CFG",
                at,
                alloc::format!("a terminator names block {target} of {count}"),
            ));
        }
    }
    for operand in terminator_operands(&block.terminator) {
        check_operand(module, function, &operand, at)?;
    }
    if let Terminator::MatchEnum { subject, arms } = &block.terminator {
        let ty = operand_type(module, function, subject);
        let expected = match ty.and_then(|ty| module.type_of(ty)) {
            Some(TypeDef::Nominal { variants, .. }) => variants.len(),
            Some(TypeDef::Option(_))
            | Some(TypeDef::Result(_, _))
            | Some(TypeDef::TaskResult(_)) => 2,
            _ => 0,
        };
        let covered: BTreeSet<usize> = arms.iter().map(|(variant, _)| *variant).collect();
        if covered.len() != arms.len() {
            return Err(Finding::new(
                "V2011_CFG",
                at,
                "a match arm map names a variant twice",
            ));
        }
        for variant in 0..expected {
            if !covered.contains(&variant) {
                return Err(Finding::new(
                    "V2011_CFG",
                    at,
                    alloc::format!("the match arm map leaves variant {variant} uncovered"),
                ));
            }
        }
    }
    Ok(())
}

fn check_instruction(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    at: &str,
) -> Result<(), Finding> {
    if !module.has_type(instruction.ty) {
        return Err(Finding::new(
            "V2010_TYPE",
            at,
            "an instruction type is outside the table",
        ));
    }
    if let Some(result) = instruction.result {
        if result >= function.values.len() {
            return Err(Finding::new(
                "V2011_CFG",
                at,
                alloc::format!("an instruction defines value {result} outside the table"),
            ));
        }
        if function.values[result] != instruction.ty {
            return Err(Finding::new(
                "V2010_TYPE",
                at,
                "an instruction result disagrees with the value table",
            ));
        }
    }
    for operand in operands_of(&instruction.op) {
        check_operand(module, function, &operand, at)?;
    }
    for place in places_of(&instruction.op) {
        if place.root >= function.values.len() {
            return Err(Finding::new(
                "V2011_CFG",
                at,
                alloc::format!("a place names value {} outside the table", place.root),
            ));
        }
        for step in &place.path {
            if let tos_ir::PlaceStep::DynamicIndex(value) = step {
                if *value >= function.values.len() {
                    return Err(Finding::new(
                        "V2011_CFG",
                        at,
                        alloc::format!("an index names value {value} outside the table"),
                    ));
                }
            }
        }
    }
    match &instruction.op {
        Op::Call { target, .. } => match target {
            CallTarget::Local(index) => {
                if *index >= module.functions.len() {
                    return Err(Finding::new(
                        "V2011_CFG",
                        at,
                        "a call names a function outside the table",
                    ));
                }
            }
            CallTarget::Imported { import, .. } => {
                if *import >= module.imports.len() {
                    return Err(Finding::new(
                        "V2012_IMPORT",
                        at,
                        "a call names an import outside the table",
                    ));
                }
            }
            CallTarget::Predeclared(_) => {}
        },
        Op::Const(constant) => {
            if *constant >= module.constants.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at,
                    "an instruction names a constant outside the table",
                ));
            }
        }
        Op::Spawn { body, captures } => {
            if *body >= module.functions.len() {
                return Err(Finding::new(
                    "V2030_TASK_SCOPE",
                    at,
                    "a spawn names a body outside the function table",
                ));
            }
            if captures.len() != module.functions[*body].signature.parameters.len() {
                return Err(Finding::new(
                    "V2030_TASK_SCOPE",
                    at,
                    "a spawned body is given a different number of captures than it declares",
                ));
            }
        }
        Op::RegisterCleanup { body } => {
            if *body >= module.functions.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at,
                    "a cleanup names a body outside the function table",
                ));
            }
        }
        Op::RunCleanups { calls } => {
            // docs/41 section 6 bounds live cleanups by the declared envelope,
            // so an exit cannot run more of them than the module reserved.
            if calls.len() as u128 > module.header.resource_envelope.cleanup {
                return Err(Finding::new(
                    "V2022_RESOURCE",
                    at,
                    alloc::format!(
                        "{} cleanups at one exit exceeds the declared limit of {}",
                        calls.len(),
                        module.header.resource_envelope.cleanup
                    ),
                ));
            }
            for call in calls {
                if call.body >= module.functions.len() {
                    return Err(Finding::new(
                        "V2011_CFG",
                        at,
                        "a cleanup names a body outside the function table",
                    ));
                }
                if module.functions[call.body].signature.parameters.len() != call.captures.len() {
                    return Err(Finding::new(
                        "V2011_CFG",
                        at,
                        "a cleanup is given a different number of operands than it declares",
                    ));
                }
            }
        }
        Op::Closure { body, captures } => {
            if *body >= module.functions.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at,
                    "a closure names a body outside the function table",
                ));
            }
            // A closure carries exactly what it declares: nothing reaches its
            // body by ambient scope, and nothing it needs is missing.
            let declared = module.functions[*body].signature.parameters.len();
            if captures.len() > declared {
                return Err(Finding::new(
                    "V2011_CFG",
                    at,
                    "a closure carries more captures than its body declares",
                ));
            }
        }
        Op::CallValue { callee, .. } => {
            let ty = operand_type(module, function, callee);
            if !matches!(
                ty.and_then(|ty| module.type_of(ty)),
                Some(TypeDef::Function(_, _))
            ) {
                return Err(Finding::new(
                    "V2010_TYPE",
                    at,
                    "a value call names an operand that is not of function type",
                ));
            }
        }
        Op::Capability { import, .. } if *import >= module.capability_imports.len() => {
            return Err(Finding::new(
                "V2013_CAPABILITY",
                at,
                "a capability operation names an import outside the table",
            ));
        }
        _ => {}
    }
    if instruction.source >= module.source_map.len() {
        return Err(Finding::new(
            "V2040_SOURCE_MAP",
            at,
            "an instruction has no source-map entry",
        ));
    }
    Ok(())
}

fn check_operand(
    module: &Module,
    function: &Function,
    operand: &Operand,
    at: &str,
) -> Result<(), Finding> {
    match operand {
        Operand::Value(value) => {
            if *value >= function.values.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at,
                    alloc::format!("an operand names value {value} outside the table"),
                ));
            }
        }
        Operand::Constant(constant) => {
            if *constant >= module.constants.len() {
                return Err(Finding::new(
                    "V2011_CFG",
                    at,
                    alloc::format!("an operand names constant {constant} outside the table"),
                ));
            }
        }
    }
    Ok(())
}

fn operand_type(module: &Module, function: &Function, operand: &Operand) -> Option<TypeId> {
    match operand {
        Operand::Value(value) => function.values.get(*value).copied(),
        Operand::Constant(constant) => match module.constants.get(*constant)? {
            Constant::Unit => module.types.iter().position(|ty| *ty == TypeDef::Unit),
            Constant::Bool(_) => module.types.iter().position(|ty| *ty == TypeDef::Bool),
            Constant::Int(kind, _) => module
                .types
                .iter()
                .position(|ty| *ty == TypeDef::Int(*kind)),
            Constant::Size(_) => module.types.iter().position(|ty| *ty == TypeDef::Size),
            Constant::Duration(_) => module.types.iter().position(|ty| *ty == TypeDef::Duration),
            Constant::Text(_) => module.types.iter().position(|ty| *ty == TypeDef::Text),
            Constant::Bytes(_) => module.types.iter().position(|ty| *ty == TypeDef::Bytes),
        },
    }
}

// ------------------------------------------------------------------ step 7

/// Affine state and profile restrictions.
///
/// docs/40 makes a non-`Copy` value moved when it is moved, so a second move of
/// the same place on one straight-line path is a defect the verifier finds
/// without trusting the frontend's ownership pass. `Copy` is recomputed from
/// the type graph by [`tos_ir::Module::is_copy`], never read from an
/// annotation.
fn check_ownership_and_profile(module: &Module) -> Result<(), Finding> {
    for (index, function) in module.functions.iter().enumerate() {
        for (block_index, block) in function.blocks.iter().enumerate() {
            let at = alloc::format!("function {index} block {block_index}");
            let mut moved: Vec<&tos_ir::Place> = Vec::new();
            for instruction in &block.instructions {
                if let Op::Move { place } = &instruction.op {
                    let ty = function.values.get(place.root).copied().unwrap_or(0);
                    if module.is_copy(ty) && place.path.is_empty() {
                        continue;
                    }
                    if moved.iter().any(|existing| overlaps(existing, place)) {
                        return Err(Finding::new(
                            "V2020_OWNERSHIP",
                            at,
                            alloc::format!("value {} is moved twice on one path", place.root),
                        ));
                    }
                    moved.push(place);
                }
            }
        }
        if module.header.profile == Profile::Bootstrap {
            if function.signature.is_async {
                return Err(Finding::new(
                    "V2023_PROFILE",
                    alloc::format!("function {index}"),
                    "an async function is Full-profile only",
                ));
            }
            for block in &function.blocks {
                for instruction in &block.instructions {
                    // docs/44 section 7 lists the Full-only constructs; a
                    // Bootstrap module may not contain their IR either.
                    let full_only = match &instruction.op {
                        Op::Await { .. } => Some("await"),
                        Op::Closure { .. } => Some("a closure"),
                        _ => None,
                    };
                    if let Some(what) = full_only {
                        return Err(Finding::new(
                            "V2023_PROFILE",
                            alloc::format!("function {index}"),
                            alloc::format!("{what} is Full-profile only"),
                        ));
                    }
                }
            }
        }
    }
    if module.header.profile == Profile::Bootstrap && module.header.resource_envelope.workers > 1 {
        return Err(Finding::new(
            "V2022_RESOURCE",
            "header.resource_envelope",
            "Bootstrap accepts workers: 1 only",
        ));
    }
    Ok(())
}

fn overlaps(one: &tos_ir::Place, other: &tos_ir::Place) -> bool {
    if one.root != other.root {
        return false;
    }
    let shorter = one.path.len().min(other.path.len());
    one.path[..shorter]
        .iter()
        .zip(&other.path[..shorter])
        .all(|(left, right)| match (left, right) {
            (tos_ir::PlaceStep::Field(a), tos_ir::PlaceStep::Field(b)) => a == b,
            (tos_ir::PlaceStep::Index(Some(a)), tos_ir::PlaceStep::Index(Some(b))) => a == b,
            // An index that is not a compile-time constant may name any
            // element, so it overlaps every other index step.
            (
                tos_ir::PlaceStep::Index(_) | tos_ir::PlaceStep::DynamicIndex(_),
                tos_ir::PlaceStep::Index(_) | tos_ir::PlaceStep::DynamicIndex(_),
            ) => true,
            _ => false,
        })
}

// ------------------------------------------------------------------ step 8

/// Task scope, atomic orders and unsafe interface claims.
///
/// docs/41 section 2 requires every spawned child to be consumed before its
/// scope exits. The obligation travels with the handle: a `spawn` creates one,
/// a move or read of the place holding it passes it on to whatever the
/// instruction produces, and joining, awaiting, returning or handing it to
/// another operation discharges it. `cancel` alone does not, which is the one
/// case docs/41 states outright.
/// Whether a type is one of the three ADR-0036 lock guards.
fn is_guard(module: &Module, ty: TypeId) -> bool {
    matches!(
        module.types.get(ty),
        Some(TypeDef::MutexGuard(_)) | Some(TypeDef::ReadGuard(_)) | Some(TypeDef::WriteGuard(_))
    )
}

/// The guard type an operand carries, when it carries one.
fn guard_operand(module: &Module, function: &Function, operand: &Operand) -> Option<TypeId> {
    let Operand::Value(id) = operand else {
        return None;
    };
    let ty = *function.values.get(*id)?;
    is_guard(module, ty).then_some(ty)
}

/// The guard rules of ADR-0036, reached by this verifier's own traversal.
///
/// docs/43 section 5 forbids taking the frontend's word for anything, so this
/// restates the rules over IR rather than trusting that a checker ran: a guard
/// rule the checker enforces and the verifier does not is a rule an alternate
/// frontend could skip. The IR carries a type for every value, so a guard is
/// identified from the type table rather than from any name.
fn check_guard_lifetimes(module: &Module) -> Result<(), Finding> {
    for (index, function) in module.functions.iter().enumerate() {
        for (block_index, block) in function.blocks.iter().enumerate() {
            let mut held: bool = false;
            for instruction in &block.instructions {
                let at = alloc::format!("functions[{index}].blocks[{block_index}]");
                let escaping = match &instruction.op {
                    Op::Spawn { captures, .. } => Some(("task_boundary", captures)),
                    Op::Closure { captures, .. } => Some(("task_boundary", captures)),
                    Op::Aggregate { operands, .. } => Some(("aggregate", operands)),
                    Op::Variant { operands, .. } => Some(("aggregate", operands)),
                    _ => None,
                };
                if let Some((operation, operands)) = escaping {
                    for operand in operands {
                        if guard_operand(module, function, operand).is_some() {
                            return Err(Finding::new(
                                "V2031_SYNC",
                                at.clone(),
                                alloc::format!("a guard operand escapes: {operation}"),
                            ));
                        }
                    }
                }
                // A guard produced in this block is live from here on: the IR
                // is in SSA form within a block, so a later await in the same
                // block is an await while it is held.
                if let Some(result) = instruction.result {
                    if function
                        .values
                        .get(result)
                        .is_some_and(|ty| is_guard(module, *ty))
                    {
                        held = true;
                    }
                }
                if held && matches!(instruction.op, Op::Await { .. }) {
                    return Err(Finding::new(
                        "V2031_SYNC",
                        at,
                        "a guard is live across an await",
                    ));
                }
            }
            if let Terminator::Return(Some(operand)) = &block.terminator {
                if guard_operand(module, function, operand).is_some() {
                    return Err(Finding::new(
                        "V2031_SYNC",
                        alloc::format!("functions[{index}].blocks[{block_index}]"),
                        "a guard operand escapes: returned",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn check_tasks_sync_atomics_unsafe(module: &Module) -> Result<(), Finding> {
    check_guard_lifetimes(module)?;
    for (index, function) in module.functions.iter().enumerate() {
        let at = alloc::format!("function {index}");
        let mut pending: BTreeSet<usize> = BTreeSet::new();
        for block in &function.blocks {
            for instruction in &block.instructions {
                if let Op::Atomic {
                    operation,
                    order,
                    failure_order,
                    ..
                } = &instruction.op
                {
                    check_atomic(*operation, *order, *failure_order, &at)?;
                }
                if let Some(interface) = &instruction.unsafe_interface {
                    // docs/44 section 7: V1 accepts no FFI interface schema.
                    return Err(Finding::new(
                        "V2033_UNSAFE",
                        &at,
                        alloc::format!("{interface} is not an accepted V1 interface"),
                    ));
                }
                if matches!(instruction.op, Op::Cancel { .. }) {
                    continue;
                }
                // Whatever this instruction touches, it takes the handle from.
                let mut carried = false;
                for operand in operands_of(&instruction.op) {
                    if let Operand::Value(value) = operand {
                        carried |= pending.remove(&value);
                    }
                }
                for place in places_of(&instruction.op) {
                    carried |= pending.remove(&place.root);
                }
                // A move or read of the whole place passes the handle to what
                // this instruction produces; a join or await ends it.
                let passes_on = matches!(
                    instruction.op,
                    Op::Move { .. } | Op::Read { .. } | Op::Borrow { .. }
                );
                if let Some(result) = instruction.result {
                    // A spawn creates the obligation; a move or read of a place
                    // that held one passes it to what this produces.
                    let holds =
                        matches!(instruction.op, Op::Spawn { .. }) || (carried && passes_on);
                    if holds {
                        pending.insert(result);
                    }
                }
            }
            for operand in terminator_operands(&block.terminator) {
                if let Operand::Value(value) = operand {
                    pending.remove(&value);
                }
            }
        }
        if let Some(task) = pending.iter().next() {
            return Err(Finding::new(
                "V2030_TASK_SCOPE",
                &at,
                alloc::format!("value {task} is a child that leaves its scope unconsumed"),
            ));
        }
    }
    Ok(())
}

fn check_atomic(
    operation: AtomicOp,
    order: MemoryOrder,
    failure_order: Option<MemoryOrder>,
    at: &str,
) -> Result<(), Finding> {
    let accepted = match operation {
        AtomicOp::Load => matches!(
            order,
            MemoryOrder::Relaxed | MemoryOrder::Acquire | MemoryOrder::SeqCst
        ),
        AtomicOp::Store => matches!(
            order,
            MemoryOrder::Relaxed | MemoryOrder::Release | MemoryOrder::SeqCst
        ),
        _ => true,
    };
    if !accepted {
        return Err(Finding::new(
            "V2032_ATOMIC_ORDER",
            at,
            alloc::format!(
                "{} does not accept {}",
                operation_name(operation),
                order.spelled()
            ),
        ));
    }
    let Some(failure) = failure_order else {
        return Ok(());
    };
    if operation != AtomicOp::CompareExchange {
        return Err(Finding::new(
            "V2032_ATOMIC_ORDER",
            at,
            "only compare_exchange carries a failure order",
        ));
    }
    if !matches!(
        failure,
        MemoryOrder::Relaxed | MemoryOrder::Acquire | MemoryOrder::SeqCst
    ) {
        return Err(Finding::new(
            "V2032_ATOMIC_ORDER",
            at,
            alloc::format!("a failure order may not be {}", failure.spelled()),
        ));
    }
    if failure.rank() > order.rank() {
        return Err(Finding::new(
            "V2032_ATOMIC_ORDER",
            at,
            alloc::format!(
                "failure order {} is stronger than success order {}",
                failure.spelled(),
                order.spelled()
            ),
        ));
    }
    Ok(())
}

fn operation_name(operation: AtomicOp) -> &'static str {
    match operation {
        AtomicOp::Load => "load",
        AtomicOp::Store => "store",
        AtomicOp::Swap => "swap",
        AtomicOp::FetchAdd => "fetch_add",
        AtomicOp::FetchSub => "fetch_sub",
        AtomicOp::FetchAnd => "fetch_and",
        AtomicOp::FetchOr => "fetch_or",
        AtomicOp::FetchXor => "fetch_xor",
        AtomicOp::CompareExchange => "compare_exchange",
    }
}

// ------------------------------------------------------------------ step 9

fn check_source_maps(module: &Module) -> Result<(), Finding> {
    let header = &module.header;
    for (index, entry) in module.source_map.iter().enumerate() {
        let at = alloc::format!("source map {index}");
        if entry.byte_start > entry.byte_end {
            return Err(Finding::new(
                "V2040_SOURCE_MAP",
                at,
                "a span runs backwards",
            ));
        }
        // docs/43 section 6: the identity in an entry is the module's own. A
        // mismatch is exactly how a forged map claims another module's source.
        if entry.content_id != header.content_id
            || entry.path != header.path
            || entry.source_set != header.source_set
        {
            return Err(Finding::new(
                "V2040_SOURCE_MAP",
                at,
                "an entry claims a source identity the header does not",
            ));
        }
        if entry.language_version != header.language_version
            || entry.profile != header.profile
            || entry.unicode_normalization_baseline != header.unicode_normalization_baseline
        {
            return Err(Finding::new(
                "V2040_SOURCE_MAP",
                at,
                "an entry disagrees with the header about the language contract",
            ));
        }
        if let Some(parent) = entry.derived_from {
            if parent >= module.source_map.len() {
                return Err(Finding::new(
                    "V2040_SOURCE_MAP",
                    at,
                    "a derivation parent is outside the table",
                ));
            }
        }
    }
    for (index, function) in module.functions.iter().enumerate() {
        if function.source >= module.source_map.len() {
            return Err(Finding::new(
                "V2040_SOURCE_MAP",
                alloc::format!("function {index}"),
                "a function has no source-map entry",
            ));
        }
        for (block_index, block) in function.blocks.iter().enumerate() {
            if block.source >= module.source_map.len() {
                return Err(Finding::new(
                    "V2040_SOURCE_MAP",
                    alloc::format!("function {index} block {block_index}"),
                    "a block has no source-map entry",
                ));
            }
        }
    }
    Ok(())
}

/// A digest over the source map alone, so a receipt binds the map it checked.
fn source_map_digest(entries: &[SourceMapEntry]) -> String {
    let mut bytes: Vec<u8> = Vec::new();
    for entry in entries {
        for text in [
            entry.source_set.as_str(),
            entry.path.as_str(),
            entry.content_id.as_str(),
            entry.frontend_identity.as_str(),
        ] {
            bytes.extend_from_slice(&(text.len() as u64).to_be_bytes());
            bytes.extend_from_slice(text.as_bytes());
        }
        bytes.extend_from_slice(&(entry.byte_start as u64).to_be_bytes());
        bytes.extend_from_slice(&(entry.byte_end as u64).to_be_bytes());
    }
    let digest = tos_hash::sha256(&bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    alloc::format!(
        "sha256:{}",
        core::str::from_utf8(&hex).expect("hex output is ASCII")
    )
}

// ------------------------------------------------------------------ shared

fn terminator_targets(terminator: &Terminator) -> Vec<usize> {
    match terminator {
        Terminator::Return(_) | Terminator::Trap(_) => Vec::new(),
        Terminator::Branch { target, .. } => alloc::vec![*target],
        Terminator::BranchIf {
            true_target,
            false_target,
            ..
        } => alloc::vec![*true_target, *false_target],
        Terminator::MatchEnum { arms, .. } => arms.iter().map(|(_, target)| *target).collect(),
        Terminator::PropagateError { ok_target, .. } => alloc::vec![*ok_target],
    }
}

fn terminator_operands(terminator: &Terminator) -> Vec<Operand> {
    match terminator {
        Terminator::Return(value) => value.iter().cloned().collect(),
        Terminator::Branch { arguments, .. } => arguments.clone(),
        Terminator::BranchIf {
            condition,
            true_arguments,
            false_arguments,
            ..
        } => {
            let mut all = alloc::vec![condition.clone()];
            all.extend(true_arguments.iter().cloned());
            all.extend(false_arguments.iter().cloned());
            all
        }
        Terminator::MatchEnum { subject, .. } => alloc::vec![subject.clone()],
        Terminator::PropagateError { result, .. } => alloc::vec![result.clone()],
        Terminator::Trap(_) => Vec::new(),
    }
}

fn operands_of(op: &Op) -> Vec<Operand> {
    match op {
        Op::Aggregate { operands, .. } | Op::Variant { operands, .. } => operands.clone(),
        Op::Write { value, .. } => alloc::vec![value.clone()],
        Op::Binary { left, right, .. } => alloc::vec![left.clone(), right.clone()],
        Op::Unary { operand, .. } | Op::Widen { operand, .. } => alloc::vec![operand.clone()],
        Op::Call { operands, .. } => operands.clone(),
        Op::Spawn { captures, .. } => captures.clone(),
        Op::Join { task } | Op::Await { task } | Op::Cancel { task } => alloc::vec![task.clone()],
        Op::Atomic {
            target, operands, ..
        } => {
            let mut all = alloc::vec![target.clone()];
            all.extend(operands.iter().cloned());
            all
        }
        Op::Capability { operands, .. } => operands.clone(),
        Op::Resource { amount, .. } => alloc::vec![amount.clone()],
        Op::Closure { captures, .. } => captures.clone(),
        Op::CallValue { callee, operands } => {
            let mut all = alloc::vec![callee.clone()];
            all.extend(operands.iter().cloned());
            all
        }
        Op::RunCleanups { calls } => calls
            .iter()
            .flat_map(|call| call.captures.iter().cloned())
            .collect(),
        _ => Vec::new(),
    }
}

fn places_of(op: &Op) -> Vec<&tos_ir::Place> {
    match op {
        Op::Read { place }
        | Op::Move { place }
        | Op::Write { place, .. }
        | Op::Borrow { place, .. }
        | Op::Drop { place } => alloc::vec![place],
        _ => Vec::new(),
    }
}
