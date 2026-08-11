// SPDX-License-Identifier: GPL-3.0-or-later
//! The `tos-ir/v1` semantic schema (docs/43).
//!
//! TOS IR is a versioned, typed, verifier-visible **derived** representation of
//! TOS Core source. It is never canonical installed source. This crate defines
//! the schema and nothing else: it holds no frontend, no checker, no verifier
//! and no engine, so the frontend that produces a module and the verifier that
//! validates one share a declarative table without either depending on the
//! other.
//!
//! docs/43 section 5 makes verifier independence structural. A verifier built
//! on this crate consumes an untrusted module value and revalidates it with its
//! own traversal; nothing here carries a "checked" flag, a frontend callback or
//! a success token that a verifier could accept instead of looking.
//!
//! docs/43 section 1 deliberately does not freeze an on-disk byte encoding
//! before a production cache exists, so this crate defines no serialized form.
//! It does define the module digest, because identity is part of the semantic
//! schema: a receipt binds to the digest of the module the verifier actually
//! saw.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub mod digest;

pub use digest::module_digest;

/// The schema every V1 module declares.
pub const SCHEMA_ID: &str = "tos-ir/v1";

/// The language version this schema represents.
pub const LANGUAGE_VERSION: &str = "1.0";

/// The Unicode baseline docs/43 section 2 fixes for V1.
pub const UNICODE_BASELINE: &str = "UCD-17.0.0/UAX15-r57/NFC";

/// The source-map revision this schema emits.
pub const SOURCE_MAP_REVISION: &str = "tos-source-map/v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Bootstrap,
    Full,
}

impl Profile {
    pub fn spelled(self) -> &'static str {
        match self {
            Profile::Bootstrap => "bootstrap",
            Profile::Full => "full",
        }
    }
}

/// The ten declared limits of docs/41 section 6.
///
/// Every table count, operand count and span in a module is bounded by this
/// envelope together with the docs/44 hard limits, so the verifier reads it
/// before it reads anything sized.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResourceEnvelope {
    pub fuel: u128,
    pub stack: u128,
    pub allocation: u128,
    pub tasks: u128,
    pub workers: u128,
    pub sync: u128,
    pub shared: u128,
    pub cleanup: u128,
    pub recursion: u128,
    pub imports: u128,
}

/// The header of docs/43 section 2, in its canonical order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header {
    pub schema_id: String,
    pub language_version: String,
    pub unicode_normalization_baseline: String,
    pub profile: Profile,
    pub module_name: String,
    pub source_set: String,
    pub path: String,
    /// SHA-256 of the normalized source bytes, as `sha256:<hex>`.
    pub content_id: String,
    /// Digest over the ordered dependency closure's content IDs.
    pub dependency_digest: String,
    /// Which frontend produced this module.
    pub frontend_identity: String,
    pub source_map_revision: String,
    pub resource_envelope: ResourceEnvelope,
    /// Digest over the ordered imported capability interfaces.
    pub capability_interface_digest: String,
}

pub type TypeId = usize;
pub type ConstId = usize;
pub type FunctionId = usize;
pub type BlockId = usize;
pub type ValueId = usize;
pub type SourceRef = usize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntKind {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl IntKind {
    pub fn spelled(self) -> &'static str {
        match self {
            IntKind::I8 => "i8",
            IntKind::I16 => "i16",
            IntKind::I32 => "i32",
            IntKind::I64 => "i64",
            IntKind::U8 => "u8",
            IntKind::U16 => "u16",
            IntKind::U32 => "u32",
            IntKind::U64 => "u64",
        }
    }

    pub fn parse(text: &str) -> Option<IntKind> {
        Some(match text {
            "i8" => IntKind::I8,
            "i16" => IntKind::I16,
            "i32" => IntKind::I32,
            "i64" => IntKind::I64,
            "u8" => IntKind::U8,
            "u16" => IntKind::U16,
            "u32" => IntKind::U32,
            "u64" => IntKind::U64,
            _ => return None,
        })
    }

    /// Width in bits, and whether the type is signed.
    pub fn shape(self) -> (u32, bool) {
        match self {
            IntKind::I8 => (8, true),
            IntKind::I16 => (16, true),
            IntKind::I32 => (32, true),
            IntKind::I64 => (64, true),
            IntKind::U8 => (8, false),
            IntKind::U16 => (16, false),
            IntKind::U32 => (32, false),
            IntKind::U64 => (64, false),
        }
    }
}

/// Whether a nominal type is a record or an enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NominalKind {
    Record,
    Enum,
}

/// One variant of a nominal enum, with its ordered payload types.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Variant {
    pub name: String,
    pub payload: Vec<TypeId>,
}

/// One entry of the type table.
///
/// docs/43 section 2 requires a nominal type to record its defining module
/// content ID and export name: an IR type is not valid merely because its host
/// representation has the same layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeDef {
    Unit,
    Bool,
    Int(IntKind),
    Size,
    Duration,
    Text,
    Bytes,
    ConversionError,
    Event,
    Semaphore,
    Barrier,
    Latch,
    AtomicBool,
    AtomicU32,
    AtomicU64,
    Option(TypeId),
    Task(TypeId),
    TaskResult(TypeId),
    Shared(TypeId),
    Region(TypeId),
    DmaRegion(TypeId),
    Mutex(TypeId),
    RwLock(TypeId),
    Channel(TypeId),
    Slice(TypeId),
    Result(TypeId, TypeId),
    Array(TypeId, u64),
    Tuple(Vec<TypeId>),
    Function(Vec<TypeId>, TypeId),
    /// A capability handle, named by the interface it was imported from.
    Capability(String),
    Nominal {
        module_content_id: String,
        export_name: String,
        kind: NominalKind,
        /// Ordered field types for a record.
        fields: Vec<TypeId>,
        /// Ordered variants for an enum.
        variants: Vec<Variant>,
    },
}

/// A constant of the constant table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Constant {
    Unit,
    Bool(bool),
    Int(IntKind, i128),
    Size(u128),
    Duration(u128),
    Text(String),
    Bytes(Vec<u8>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visibility {
    Private,
    Public,
}

/// An exact type and effect signature (docs/43 section 3).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature {
    pub name: String,
    pub visibility: Visibility,
    pub is_async: bool,
    pub parameters: Vec<Parameter>,
    pub result: TypeId,
    /// Declared capability effects, by interface path.
    pub effects: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PassMode {
    Owned,
    SharedBorrow,
    MutableBorrow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub ty: TypeId,
    pub mode: PassMode,
}

/// A module this one imports, with the signatures it uses from it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    pub module_name: String,
    pub module_content_id: String,
    pub binding: String,
}

/// A capability interface this module imports (docs/42 section 4).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityImport {
    pub interface: String,
    pub binding: String,
    pub ty: TypeId,
}

/// Where an operation came from (docs/43 section 6).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMapEntry {
    pub source_set: String,
    pub path: String,
    pub content_id: String,
    pub frontend_identity: String,
    pub language_version: String,
    pub profile: Profile,
    pub unicode_normalization_baseline: String,
    pub byte_start: usize,
    pub byte_end: usize,
    /// The span this one was derived from, when lowering split an operation.
    pub derived_from: Option<SourceRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    ShiftLeft,
    ShiftRight,
    BitAnd,
    BitOr,
    BitXor,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    LogicalAnd,
    LogicalOr,
}

impl BinaryOp {
    /// Whether the operation traps on overflow, division by zero, an invalid
    /// shift count or `MIN / -1` (docs/40 section 3).
    pub fn is_checked_arithmetic(self) -> bool {
        matches!(
            self,
            BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight
        )
    }

    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessOrEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterOrEqual
                | BinaryOp::LogicalAnd
                | BinaryOp::LogicalOr
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BorrowKind {
    Shared,
    Mutable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryOrder {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}

impl MemoryOrder {
    pub fn spelled(self) -> &'static str {
        match self {
            MemoryOrder::Relaxed => "Relaxed",
            MemoryOrder::Acquire => "Acquire",
            MemoryOrder::Release => "Release",
            MemoryOrder::AcqRel => "AcqRel",
            MemoryOrder::SeqCst => "SeqCst",
        }
    }

    /// Strength, for the one comparison docs/41 section 5 states.
    pub fn rank(self) -> u8 {
        match self {
            MemoryOrder::Relaxed => 0,
            MemoryOrder::Acquire | MemoryOrder::Release => 1,
            MemoryOrder::AcqRel => 2,
            MemoryOrder::SeqCst => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomicOp {
    Load,
    Store,
    Swap,
    FetchAdd,
    FetchSub,
    FetchAnd,
    FetchOr,
    FetchXor,
    CompareExchange,
}

/// Which resource an accounting operation touches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    Fuel,
    Stack,
    Allocation,
    Task,
    Worker,
    Sync,
    Shared,
    Cleanup,
    Recursion,
}

/// What a call resolves to. A call never resolves a host symbol dynamically.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallTarget {
    /// A function of this module, by index.
    Local(FunctionId),
    /// A function of an imported module, by import index and export name.
    Imported { import: usize, name: String },
    /// A predeclared V1 operation, such as a checked conversion.
    Predeclared(String),
}

/// A path into a binding: the places docs/40 section 5 states its rules over.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Place {
    pub root: ValueId,
    pub path: Vec<PlaceStep>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaceStep {
    Field(usize),
    /// A constant index, or `None` when the index is not a constant.
    Index(Option<u64>),
    /// An index computed by a value of type `size`.
    ///
    /// Aliasing analysis treats it exactly like `Index(None)` — it may name any
    /// element — while execution reads the value it names.
    DynamicIndex(ValueId),
}

/// An operand of an operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operand {
    /// A dominating SSA value or ownership slot.
    Value(ValueId),
    /// An entry of the constant table.
    Constant(ConstId),
}

/// One semantic operation (docs/43 section 3).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Op {
    /// Materializes a constant.
    Const(ConstId),
    /// Builds a record, tuple or array value from ordered operands.
    Aggregate {
        ty: TypeId,
        operands: Vec<Operand>,
    },
    /// Builds an enum variant, `Option`, `Result` or `TaskResult` value.
    Variant {
        ty: TypeId,
        index: usize,
        operands: Vec<Operand>,
    },
    /// Reads a place without taking ownership of it.
    Read {
        place: Place,
    },
    /// Takes ownership of the value at a place.
    Move {
        place: Place,
    },
    /// Writes a value into a place.
    Write {
        place: Place,
        value: Operand,
    },
    /// Takes a borrow of a place.
    Borrow {
        place: Place,
        kind: BorrowKind,
    },
    /// Runs the bounded drop contract of a place.
    Drop {
        place: Place,
    },
    Binary {
        op: BinaryOp,
        left: Operand,
        right: Operand,
    },
    Unary {
        op: UnaryOp,
        operand: Operand,
    },
    /// An integer widening that preserves signedness (docs/40 section 3).
    Widen {
        operand: Operand,
        to: IntKind,
    },
    Call {
        target: CallTarget,
        operands: Vec<Operand>,
    },
    /// Creates a scoped child task from a lowered body function.
    Spawn {
        body: FunctionId,
        captures: Vec<Operand>,
    },
    /// Builds a closure value over a lowered body and its captured operands.
    ///
    /// The captures are ordered and explicit: a closure carries exactly what
    /// the source captured, and nothing reaches it by ambient scope.
    Closure {
        body: FunctionId,
        captures: Vec<Operand>,
    },
    /// Calls a closure value. The callee is an operand of function type, not a
    /// name resolved at run time, so no host symbol lookup is possible.
    CallValue {
        callee: Operand,
        operands: Vec<Operand>,
    },
    /// Consumes a `Task<T>` and produces a `TaskResult<T>`.
    Join {
        task: Operand,
    },
    /// Consumes a `Task<T>` asynchronously; Full profile only.
    Await {
        task: Operand,
    },
    /// A cooperative cancellation request, which consumes no ownership.
    Cancel {
        task: Operand,
    },
    /// A typed atomic operation with its exact orders.
    Atomic {
        operation: AtomicOp,
        target: Operand,
        operands: Vec<Operand>,
        order: MemoryOrder,
        failure_order: Option<MemoryOrder>,
    },
    /// An operation on a declared imported capability.
    Capability {
        import: usize,
        right: String,
        operands: Vec<Operand>,
    },
    /// Reserves or releases part of the declared resource envelope.
    Resource {
        kind: ResourceKind,
        amount: Operand,
        release: bool,
    },
    /// Registers a deferred cleanup body for this scope (docs/40 section 5).
    ///
    /// Registration is what the `cleanup` resource limit counts. It takes no
    /// ownership: ADR-0035 makes the body run at the exit it belongs to.
    RegisterCleanup {
        body: FunctionId,
    },
    /// Runs cleanup bodies at one exit, in the order given.
    ///
    /// ADR-0035 makes cleanup lexical, so which bodies run at an exit is a
    /// static fact and is written here rather than reconstructed from a runtime
    /// stack. The list is already in reverse registration order. Each call
    /// carries the operands it reads, and they are read *here*, at the exit,
    /// which is what makes registration take no ownership.
    RunCleanups {
        calls: Vec<CleanupCall>,
    },
}

/// One deferred cleanup body and the operands it reads where it runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanupCall {
    pub body: FunctionId,
    pub captures: Vec<Operand>,
}

/// One instruction: an optional typed result and the operation producing it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instruction {
    pub result: Option<ValueId>,
    pub ty: TypeId,
    pub op: Op,
    pub source: SourceRef,
    /// Set on an operation that lowers to a runtime call.
    pub runtime_contract: Option<String>,
    /// Whether the operation sits inside an `unsafe` block.
    ///
    /// docs/43 section 3 separates the marker from the interface: an `unsafe`
    /// block is an explicit marker on ordinary operations, while an interface
    /// ID names an accepted FFI schema. V1 accepts no interface, so the two
    /// must not be conflated.
    pub unsafe_block: bool,
    /// Set on an operation reaching an accepted external interface.
    pub unsafe_interface: Option<String>,
}

/// How a block ends. There is exactly one terminator and no fall-through.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Terminator {
    Return(Option<Operand>),
    Branch {
        target: BlockId,
        arguments: Vec<Operand>,
    },
    BranchIf {
        condition: Operand,
        true_target: BlockId,
        true_arguments: Vec<Operand>,
        false_target: BlockId,
        false_arguments: Vec<Operand>,
    },
    /// A complete variant-to-target map over the subject's enum type.
    MatchEnum {
        subject: Operand,
        arms: Vec<(usize, BlockId)>,
    },
    /// The `?` edge: propagate `Err` from the nearest enclosing return scope.
    PropagateError {
        result: Operand,
        ok_target: BlockId,
    },
    Trap(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub parameters: Vec<TypeId>,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
    pub source: SourceRef,
}

/// Why a function exists: a source declaration, or a body lowered out of one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FunctionOrigin {
    Declared,
    /// The body of a `spawn`, a closure, or a `defer` block.
    LoweredBody,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub signature: Signature,
    pub origin: FunctionOrigin,
    pub source: SourceRef,
    /// Declared contributions to the module envelope.
    pub stack_contribution: u128,
    pub fuel_contribution: u128,
    pub cleanup_contribution: u128,
    /// SSA values this function defines, by index, with their types.
    pub values: Vec<TypeId>,
    pub blocks: Vec<Block>,
}

/// A complete `tos-ir/v1` module, in the canonical section order of docs/43
/// section 2.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    pub header: Header,
    pub types: Vec<TypeDef>,
    pub imports: Vec<Import>,
    pub capability_imports: Vec<CapabilityImport>,
    /// Signatures this module exports, ordered by name.
    pub exports: Vec<Signature>,
    pub constants: Vec<Constant>,
    /// Functions ordered by fully qualified source name, then lowered bodies.
    pub functions: Vec<Function>,
    pub source_map: Vec<SourceMapEntry>,
}

impl Module {
    /// Whether a type index is within the type table.
    pub fn has_type(&self, ty: TypeId) -> bool {
        ty < self.types.len()
    }

    pub fn type_of(&self, ty: TypeId) -> Option<&TypeDef> {
        self.types.get(ty)
    }

    /// The docs/40 `Copy` rule, recomputed from the type graph.
    ///
    /// docs/43 section 2 forbids trusting a frontend-supplied annotation, so
    /// this is a function of the table alone. A cyclic or out-of-range
    /// reference is not `Copy`: it is a defect the verifier reports, and
    /// answering `false` never admits an affine value by accident.
    pub fn is_copy(&self, ty: TypeId) -> bool {
        self.is_copy_within(ty, 0)
    }

    fn is_copy_within(&self, ty: TypeId, depth: usize) -> bool {
        if depth > MAX_TYPE_DEPTH {
            return false;
        }
        match self.types.get(ty) {
            Some(TypeDef::Unit)
            | Some(TypeDef::Bool)
            | Some(TypeDef::Int(_))
            | Some(TypeDef::Size)
            | Some(TypeDef::Duration) => true,
            Some(TypeDef::Shared(_)) => true,
            Some(TypeDef::Tuple(elements)) => elements
                .iter()
                .all(|element| self.is_copy_within(*element, depth + 1)),
            Some(TypeDef::Array(element, _)) => self.is_copy_within(*element, depth + 1),
            _ => false,
        }
    }
}

/// A depth bound for type-graph recursion, so a forged cyclic table cannot make
/// a traversal diverge.
pub const MAX_TYPE_DEPTH: usize = 64;
