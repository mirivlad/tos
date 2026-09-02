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
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
pub mod footprint;

pub use digest::{canonical_stream, module_digest};
pub use footprint::retained_bytes;

/// The schema every V1 module declares.
pub const SCHEMA_ID: &str = "tos-ir/v1";

/// The language version this schema represents.
pub const LANGUAGE_VERSION: &str = "1.0";

/// The Unicode baseline docs/43 section 2 fixes for V1.
pub const UNICODE_BASELINE: &str = "UCD-17.0.0/UAX15-r57/NFC";

/// The source-map revision this schema emits.
pub const SOURCE_MAP_REVISION: &str = "tos-source-map/v1";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

/// Which guard a lock operation grants (ADR-0036 section 2).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LockMode {
    /// `Mutex<T>.lock()` -> `MutexGuard<T>`
    Mutex,
    /// `RwLock<T>.read()` -> `ReadGuard<T>`
    Read,
    /// `RwLock<T>.write()` -> `WriteGuard<T>`
    Write,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NominalKind {
    Record,
    Enum,
}

/// One variant of a nominal enum, with its ordered payload types.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Variant {
    pub name: String,
    pub payload: Vec<TypeId>,
}

/// One entry of the type table.
///
/// docs/43 section 2 requires a nominal type to record its defining module
/// content ID and export name: an IR type is not valid merely because its host
/// representation has the same layout.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
    /// A mutably granted region (ADR-0037): writable, not shareable, not
    /// `Transferable`. A distinct constructor rather than a flag, so a
    /// traversal cannot read the type and forget to look at the mode.
    RegionMut(TypeId),
    /// A mutably granted device-visible region (ADR-0037).
    DmaRegionMut(TypeId),
    Mutex(TypeId),
    RwLock(TypeId),
    /// The affine mutable guard a `Mutex<T>` lock grants (ADR-0036).
    ///
    /// A guard is a distinct type from the object that granted it, because the
    /// rules differ: the object may be shared and stored, the guard may not
    /// leave the scope that took it.
    MutexGuard(TypeId),
    /// An immutable read guard an `RwLock<T>` grants (ADR-0036).
    ReadGuard(TypeId),
    /// The affine write guard an `RwLock<T>` grants (ADR-0036).
    WriteGuard(TypeId),
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
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Constant {
    Unit,
    Bool(bool),
    Int(IntKind, i128),
    Size(u128),
    Duration(u128),
    Text(String),
    Bytes(Vec<u8>),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Visibility {
    Private,
    Public,
}

/// An exact type and effect signature (docs/43 section 3).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Signature {
    pub name: String,
    pub visibility: Visibility,
    pub is_async: bool,
    pub parameters: Vec<Parameter>,
    pub result: TypeId,
    /// Declared capability effects, by interface path.
    pub effects: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PassMode {
    Owned,
    SharedBorrow,
    MutableBorrow,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Parameter {
    pub name: String,
    pub ty: TypeId,
    pub mode: PassMode,
}

/// A module this one imports, with the signatures it uses from it.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Import {
    pub module_name: String,
    pub module_content_id: String,
    pub binding: String,
}

/// A capability interface this module imports (docs/42 section 4).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CapabilityImport {
    pub interface: String,
    pub binding: String,
    pub ty: TypeId,
}

/// Where an operation came from (docs/43 section 6).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UnaryOp {
    Negate,
    Not,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BorrowKind {
    Shared,
    Mutable,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CallTarget {
    /// A function of this module, by index.
    Local(FunctionId),
    /// A function of an imported module, by import index and export name.
    Imported { import: usize, name: String },
    /// A predeclared V1 operation, such as a checked conversion.
    Predeclared(String),
}

/// A path into a binding: the places docs/40 section 5 states its rules over.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Place {
    pub root: ValueId,
    pub path: Vec<PlaceStep>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Operand {
    /// A dominating SSA value or ownership slot.
    Value(ValueId),
    /// An entry of the constant table.
    Constant(ConstId),
}

/// Where one capability of an interface operation came from (ADR-0078).
///
/// **An explicit discriminator, not a sentinel.** There is no reserved index
/// meaning "not an import", because a reader that had to know one number was
/// special would be a reader that could mistake a real index for it. The two
/// cases are two cases.
///
/// The interface and the required right still come from the accepted interface
/// schema — this says only *which thing* fills a position the schema already
/// described. A value's exact nominal interface is its own type, so nothing
/// here is erased and no capability crosses as a scalar.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CapabilitySource {
    /// The capability answering one of the module's `import capability`
    /// requests, by index into `Module::capability_imports` (ADR-0061).
    Import(usize),
    /// A capability an operation produced or a message delivered, held as an
    /// ordinary value of the module's.
    ///
    /// Its type must be `TypeDef::Capability` of the exact interface the
    /// position requires, which a verifier checks against the artifact rather
    /// than taking a frontend's word for.
    Value(Operand),
}

/// One semantic operation (docs/43 section 3).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
    /// Acquires a guard from a synchronization object (ADR-0036 section 2).
    ///
    /// Its own operation rather than a call: releasing is the guard's bounded
    /// drop, so there is no `unlock` taking a guard back, and a verifier has to
    /// be able to see an acquisition without knowing what any helper does.
    Lock {
        object: Operand,
        mode: LockMode,
    },
    /// Produces a `Shared<T>` from a transitively immutable, shareable value,
    /// consuming it (ADR-0037 section 4).
    ///
    /// Its own operation rather than an opaque helper call: docs/43 section 3
    /// forbids hiding shared-memory access behind one, and the verifier has to
    /// be able to recheck the shareability requirement without knowing what any
    /// particular helper does.
    Share {
        operand: Operand,
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
    /// An operation on one or more capabilities, each with an explicit source.
    ///
    /// `capabilities` are the capabilities the operation requires, in the order
    /// its interface declares them (`SYSTEM_INTERFACE_V1` §4.1, ADR-0063). The
    /// first is the one the operation is performed *under* — the one whose
    /// interface the instruction records and whose effect the enclosing function
    /// declares.
    ///
    /// **Every position says where its capability came from** (ADR-0078). It is
    /// either a declared capability import, or a value of the exact nominal
    /// capability type that an earlier operation produced. Before ADR-0078 the
    /// field was an import index and nothing else, which made an operation
    /// acting on authority obtained *at runtime* unrepresentable — while TOS
    /// Core V1 already admitted capability values and capability-derived
    /// authority. That was a narrowing of the representation below the accepted
    /// semantics, and this is the repair.
    ///
    /// **Neither case is a handle.** An import is a *request index*, a value is
    /// an SSA value id, and `docs/42` §2's rule that a handle's representation
    /// appears nowhere in provenance holds of both: there is no number in an
    /// artifact that a nucleus would accept as authority.
    ///
    /// docs/43's operation table already required this instruction to carry
    /// "effect/right/interface match" and "all semantic operands:
    /// capability/effect" — several capabilities, from either source, is that
    /// requirement met rather than a change to what it asks for.
    Capability {
        capabilities: Vec<CapabilitySource>,
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
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CleanupCall {
    pub body: FunctionId,
    pub captures: Vec<Operand>,
}

/// One instruction: an optional typed result and the operation producing it.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Block {
    pub parameters: Vec<TypeId>,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
    pub source: SourceRef,
}

/// Why a function exists: a source declaration, or a body lowered out of one.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FunctionOrigin {
    Declared,
    /// The body of a `spawn`, a closure, or a `defer` block.
    LoweredBody,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::string::String;
    use alloc::vec::Vec;

    use crate::*;

    fn module_with(name: &str) -> Module {
        Module {
            header: Header {
                schema_id: String::from(SCHEMA_ID),
                language_version: String::from(LANGUAGE_VERSION),
                unicode_normalization_baseline: String::from(UNICODE_BASELINE),
                profile: Profile::Bootstrap,
                module_name: String::from(name),
                source_set: String::from("tos-ir-tests"),
                path: String::from("set/a.tos"),
                content_id: String::from("sha256:00"),
                dependency_digest: String::from("sha256:01"),
                frontend_identity: String::from("tos-core/0"),
                source_map_revision: String::from(SOURCE_MAP_REVISION),
                resource_envelope: ResourceEnvelope {
                    fuel: 7,
                    stack: 8,
                    ..ResourceEnvelope::default()
                },
                capability_interface_digest: String::from("sha256:02"),
            },
            types: alloc::vec![
                TypeDef::Unit,
                TypeDef::Int(IntKind::I32),
                TypeDef::Tuple(alloc::vec![0, 1]),
                TypeDef::Nominal {
                    module_content_id: String::from("sha256:00"),
                    export_name: String::from("Point"),
                    kind: NominalKind::Record,
                    fields: alloc::vec![1, 1],
                    variants: Vec::new(),
                },
            ],
            imports: alloc::vec![Import {
                module_name: String::from("set.b"),
                module_content_id: String::from("sha256:03"),
                binding: String::from("b"),
            }],
            capability_imports: Vec::new(),
            exports: Vec::new(),
            constants: alloc::vec![
                Constant::Int(IntKind::I32, -5),
                Constant::Bytes(alloc::vec![1, 2, 3]),
                Constant::Text(String::from("hello")),
            ],
            functions: alloc::vec![Function {
                signature: Signature {
                    name: String::from("main"),
                    visibility: Visibility::Public,
                    is_async: false,
                    parameters: alloc::vec![Parameter {
                        name: String::from("x"),
                        ty: 1,
                        mode: PassMode::Owned,
                    }],
                    result: 1,
                    effects: alloc::vec![String::from("system.time/v1")],
                },
                origin: FunctionOrigin::Declared,
                source: 0,
                stack_contribution: 1,
                fuel_contribution: 2,
                cleanup_contribution: 0,
                values: alloc::vec![1, 1],
                blocks: alloc::vec![Block {
                    parameters: alloc::vec![1],
                    instructions: alloc::vec![Instruction {
                        result: Some(0),
                        ty: 1,
                        op: Op::Binary {
                            op: BinaryOp::Add,
                            left: Operand::Value(0),
                            right: Operand::Constant(0),
                        },
                        source: 0,
                        runtime_contract: None,
                        unsafe_block: false,
                        unsafe_interface: None,
                    }],
                    terminator: Terminator::Return(Some(Operand::Value(0))),
                    source: 0,
                }],
            }],
            source_map: alloc::vec![SourceMapEntry {
                source_set: String::from("tos-ir-tests"),
                path: String::from("set/a.tos"),
                content_id: String::from("sha256:00"),
                frontend_identity: String::from("tos-core/0"),
                language_version: String::from(LANGUAGE_VERSION),
                profile: Profile::Bootstrap,
                unicode_normalization_baseline: String::from(UNICODE_BASELINE),
                byte_start: 0,
                byte_end: 12,
                derived_from: None,
            }],
        }
    }

    /// The two sinks must produce the same canonical form.
    ///
    /// This is the guard against a second canonical encoder appearing: hashing
    /// the diagnostic stream and streaming the digest are two paths through one
    /// traversal, and if they ever diverge every receipt in the project is bound
    /// to a digest nobody else computes.
    #[test]
    fn the_streamed_digest_equals_the_digest_of_the_stream() {
        for name in ["set.a", "set.b", "a.very.long.module.name.for.contrast"] {
            let module = module_with(name);
            let stream = canonical_stream(&module);
            let hashed = tos_hash::sha256(&stream);
            let mut hex = [0u8; 64];
            tos_hash::hex(&hashed, &mut hex);
            let expected = alloc::format!(
                "sha256:{}",
                core::str::from_utf8(&hex).expect("hex output is ASCII")
            );
            assert_eq!(module_digest(&module), expected, "module {name}");
        }
    }

    /// A change anywhere in the module changes the digest.
    #[test]
    fn the_digest_separates_modules() {
        let one = module_digest(&module_with("set.a"));
        let other = module_digest(&module_with("set.b"));
        assert_ne!(one, other);
    }
}
