// SPDX-License-Identifier: GPL-3.0-or-later
//! What survives a dependency's lowering turn.
//!
//! Lowering a set is ordered so that a module's dependencies are lowered first.
//! Holding a whole `Module` for each of them means every dependency's bodies,
//! blocks, instructions and source map stay alive until the last module of the
//! closure is lowered. Holding a general-purpose `Vec<TypeDef>` and a full
//! `Signature` for each of them is the same mistake one size down: measured over
//! 128 ceiling-sized modules, that shape cost `117 MiB`, two thirds of it type
//! tables at `104` bytes an entry with the defining module's `sha256:` string
//! repeated once per nominal type.
//!
//! So a dependency is reduced to **exactly what is read from it**, and the two
//! readers want different things:
//!
//! - the **lowerer** resolves an import's content identity, finds the result
//!   type of an exported function a call names, and rebuilds that type in its
//!   own table. It never reads a parameter, a mode, an effect, a visibility or
//!   an async flag. [`LoweringInterface`] carries that and nothing else;
//! - the **verifier** is handed a declared resolution, and docs/43 requires the
//!   complete surface: every public export name of every resolved module, and
//!   the capability interfaces it imports. [`VerificationSurface`] carries all
//!   of it, unabridged.
//!
//! Both are **derived implementation data**. Not authority, not a cache, not a
//! receipt: they are built from IR this process just lowered, used by this
//! process, and dropped when their last consumer is done. The verifier never
//! sees one; nothing is admitted on the strength of one.
//!
//! ## Nominal identity is not narrowed
//!
//! A nominal type is not its shape. Two records with the same fields declared by
//! two modules are two types, so the defining module's content identity, the
//! export name, the kind, the fields and the variants are all carried. What
//! changes is that the identity string is **interned**: stored once per distinct
//! identity, referenced by index, at full length. A `sha256:<hex>` is never
//! truncated and never hashed again — it is the exact bytes the type table held.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tos_ir::{IntKind, Module, NominalKind, TypeDef, TypeId, Variant};

/// A position in a compact type table.
///
/// `u32` because a module's type table is bounded by the accepted table-entry
/// ceiling (docs/44 §2, `65 536`) and by the source-unit ceiling above it, both
/// far under `u32::MAX`. The one place a wider value could arrive — a table
/// longer than `u32::MAX` — cannot be built from a conforming module, and the
/// builder checks rather than assuming.
pub type CompactTypeId = u32;

/// The widest table this representation indexes.
const INDEX_CEILING: usize = u32::MAX as usize;

/// Packed text, interned.
///
/// One buffer and one offset table rather than a `String` per entry: a name is a
/// range of the buffer, so an entry costs four bytes of offset and its own
/// bytes, and a repeated identity costs four bytes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct TextStore {
    bytes: Vec<u8>,
    /// `n + 1` offsets for `n` entries: an entry runs from its own offset to the
    /// next. No length is stored, because the next offset is the length.
    offsets: Vec<u32>,
}

impl TextStore {
    fn new() -> TextStore {
        TextStore {
            bytes: Vec::new(),
            offsets: alloc::vec![0],
        }
    }

    fn push(&mut self, text: &str) -> u32 {
        let at = self.offsets.len() as u32 - 1;
        self.bytes.extend_from_slice(text.as_bytes());
        self.offsets.push(self.bytes.len() as u32);
        at
    }

    fn get(&self, at: u32) -> &str {
        let start = self.offsets[at as usize] as usize;
        let end = self.offsets[at as usize + 1] as usize;
        // The buffer is only ever written through `push`, which appends whole
        // `&str` values, so every range is a character boundary.
        core::str::from_utf8(&self.bytes[start..end]).unwrap_or("")
    }

    fn len(&self) -> usize {
        self.offsets.len() - 1
    }

    fn retained_bytes(&self) -> usize {
        self.bytes.capacity() + self.offsets.capacity() * core::mem::size_of::<u32>()
    }
}

/// Interns text while a store is being built, and is dropped with the builder.
struct Interner {
    store: TextStore,
    seen: BTreeMap<String, u32>,
}

impl Interner {
    fn new() -> Interner {
        Interner {
            store: TextStore::new(),
            seen: BTreeMap::new(),
        }
    }

    fn intern(&mut self, text: &str) -> u32 {
        if let Some(at) = self.seen.get(text) {
            return *at;
        }
        let at = self.store.push(text);
        self.seen.insert(text.to_string(), at);
        at
    }
}

/// Which constructor a compact entry is.
///
/// Internal to this representation. It is not an encoding, nothing outside this
/// module reads it, and no artifact carries it — the format the verifier reads
/// is `tos-image`, and this is a shape held in memory for the length of one
/// lowering pass.
mod tag {
    pub const UNIT: u8 = 0;
    pub const BOOL: u8 = 1;
    pub const INT: u8 = 2;
    pub const SIZE: u8 = 3;
    pub const DURATION: u8 = 4;
    pub const TEXT: u8 = 5;
    pub const BYTES: u8 = 6;
    pub const CONVERSION_ERROR: u8 = 7;
    /// Device memory, readable and read-write (ADR-0081 §5).
    pub const MMIO_REGION: u8 = 36;
    pub const MMIO_REGION_MUT: u8 = 37;
    pub const EVENT: u8 = 8;
    pub const SEMAPHORE: u8 = 9;
    pub const BARRIER: u8 = 10;
    pub const LATCH: u8 = 11;
    pub const ATOMIC_BOOL: u8 = 12;
    pub const ATOMIC_U32: u8 = 13;
    pub const ATOMIC_U64: u8 = 14;
    pub const OPTION: u8 = 15;
    pub const TASK: u8 = 16;
    pub const TASK_RESULT: u8 = 17;
    pub const SHARED: u8 = 18;
    pub const REGION: u8 = 19;
    pub const DMA_REGION: u8 = 20;
    pub const REGION_MUT: u8 = 21;
    pub const DMA_REGION_MUT: u8 = 22;
    pub const MUTEX: u8 = 23;
    pub const RW_LOCK: u8 = 24;
    pub const MUTEX_GUARD: u8 = 25;
    pub const READ_GUARD: u8 = 26;
    pub const WRITE_GUARD: u8 = 27;
    pub const CHANNEL: u8 = 28;
    pub const SLICE: u8 = 29;
    pub const RESULT: u8 = 30;
    pub const ARRAY: u8 = 31;
    pub const TUPLE: u8 = 32;
    pub const FUNCTION: u8 = 33;
    pub const CAPABILITY: u8 = 34;
    pub const NOMINAL: u8 = 35;
}

/// One type, as twenty bytes with no heap of its own.
///
/// Every field is a plain integer and the record is `Copy`. What each of `a`,
/// `b`, `c`, `d` means depends on the tag, and every use is written down in
/// [`CompactTypes::rebuild`] beside the tag that reads it. Nothing here is
/// `repr(packed)` and nothing is read unaligned.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CompactType {
    tag: u8,
    /// `IntKind` for `INT`, `NominalKind` for `NOMINAL`, otherwise unused.
    kind: u8,
    a: u32,
    b: u32,
    c: u32,
    d: u32,
}

/// One enum variant, as a name reference and a payload range.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CompactVariant {
    name: u32,
    payload_start: u32,
    payload_end: u32,
}

/// A type graph, packed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CompactTypes {
    entries: Vec<CompactType>,
    /// Child type ids: tuple elements, function parameters, nominal fields and
    /// variant payloads, each a contiguous range.
    refs: Vec<CompactTypeId>,
    variants: Vec<CompactVariant>,
    /// Array lengths, which are the only `u64` a type carries.
    lengths: Vec<u64>,
    /// Names and interned nominal identities.
    text: TextStore,
}

impl CompactTypes {
    /// Rebuilds one entry as a `TypeDef` whose child ids are **compact** ids.
    ///
    /// The lowerer's adoption walk recurses on those ids and rebuilds the type
    /// in its own table, exactly as it did when the child ids were positions in
    /// a dependency's `Vec<TypeDef>`. The structure it sees is identical, so the
    /// order in which it interns is identical, so the IR it emits is identical.
    fn rebuild(&self, id: CompactTypeId) -> Option<TypeDef> {
        let entry = *self.entries.get(id as usize)?;
        let refs = |start: u32, end: u32| -> Vec<TypeId> {
            self.refs[start as usize..end as usize]
                .iter()
                .map(|at| *at as TypeId)
                .collect()
        };
        Some(match entry.tag {
            tag::UNIT => TypeDef::Unit,
            tag::BOOL => TypeDef::Bool,
            tag::INT => TypeDef::Int(int_kind(entry.kind)),
            tag::SIZE => TypeDef::Size,
            tag::DURATION => TypeDef::Duration,
            tag::TEXT => TypeDef::Text,
            tag::BYTES => TypeDef::Bytes,
            tag::CONVERSION_ERROR => TypeDef::ConversionError,
            tag::MMIO_REGION => TypeDef::MmioRegion,
            tag::MMIO_REGION_MUT => TypeDef::MmioRegionMut,
            tag::EVENT => TypeDef::Event,
            tag::SEMAPHORE => TypeDef::Semaphore,
            tag::BARRIER => TypeDef::Barrier,
            tag::LATCH => TypeDef::Latch,
            tag::ATOMIC_BOOL => TypeDef::AtomicBool,
            tag::ATOMIC_U32 => TypeDef::AtomicU32,
            tag::ATOMIC_U64 => TypeDef::AtomicU64,
            // `a` is the wrapped type.
            tag::OPTION => TypeDef::Option(entry.a as TypeId),
            tag::TASK => TypeDef::Task(entry.a as TypeId),
            tag::TASK_RESULT => TypeDef::TaskResult(entry.a as TypeId),
            tag::SHARED => TypeDef::Shared(entry.a as TypeId),
            tag::REGION => TypeDef::Region(entry.a as TypeId),
            tag::DMA_REGION => TypeDef::DmaRegion(entry.a as TypeId),
            tag::REGION_MUT => TypeDef::RegionMut(entry.a as TypeId),
            tag::DMA_REGION_MUT => TypeDef::DmaRegionMut(entry.a as TypeId),
            tag::MUTEX => TypeDef::Mutex(entry.a as TypeId),
            tag::RW_LOCK => TypeDef::RwLock(entry.a as TypeId),
            tag::MUTEX_GUARD => TypeDef::MutexGuard(entry.a as TypeId),
            tag::READ_GUARD => TypeDef::ReadGuard(entry.a as TypeId),
            tag::WRITE_GUARD => TypeDef::WriteGuard(entry.a as TypeId),
            tag::CHANNEL => TypeDef::Channel(entry.a as TypeId),
            tag::SLICE => TypeDef::Slice(entry.a as TypeId),
            // `a` is the ok type, `b` the error type.
            tag::RESULT => TypeDef::Result(entry.a as TypeId, entry.b as TypeId),
            // `a` is the element, `c` indexes the length table.
            tag::ARRAY => TypeDef::Array(
                entry.a as TypeId,
                *self.lengths.get(entry.c as usize).unwrap_or(&0),
            ),
            // `a..b` is the element range.
            tag::TUPLE => TypeDef::Tuple(refs(entry.a, entry.b)),
            // `a..b` is the parameter range, `c` the result.
            tag::FUNCTION => TypeDef::Function(refs(entry.a, entry.b), entry.c as TypeId),
            // `a` is the interface name.
            tag::CAPABILITY => TypeDef::Capability(self.text.get(entry.a).to_string()),
            // `a` is the interned defining identity, `b` the export name,
            // `kind` the nominal kind, `c..d` the field range, and the variant
            // range is carried in the entry that follows the fields: variants
            // are stored in their own table, keyed by the same `c..d` when the
            // kind is an enum.
            tag::NOMINAL => {
                let is_enum = matches!(nominal_kind(entry.kind), NominalKind::Enum);
                let fields = if is_enum {
                    Vec::new()
                } else {
                    refs(entry.c, entry.d)
                };
                let variants = if is_enum {
                    self.variants[entry.c as usize..entry.d as usize]
                        .iter()
                        .map(|variant| Variant {
                            name: self.text.get(variant.name).to_string(),
                            payload: refs(variant.payload_start, variant.payload_end),
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                TypeDef::Nominal {
                    module_content_id: self.text.get(entry.a).to_string(),
                    export_name: self.text.get(entry.b).to_string(),
                    kind: nominal_kind(entry.kind),
                    fields,
                    variants,
                }
            }
            _ => TypeDef::Unit,
        })
    }

    fn retained_bytes(&self) -> usize {
        self.entries.capacity() * core::mem::size_of::<CompactType>()
            + self.refs.capacity() * core::mem::size_of::<CompactTypeId>()
            + self.variants.capacity() * core::mem::size_of::<CompactVariant>()
            + self.lengths.capacity() * core::mem::size_of::<u64>()
            + self.text.retained_bytes()
    }
}

fn int_kind(code: u8) -> IntKind {
    match code {
        0 => IntKind::I8,
        1 => IntKind::I16,
        2 => IntKind::I32,
        3 => IntKind::I64,
        4 => IntKind::U8,
        5 => IntKind::U16,
        6 => IntKind::U32,
        _ => IntKind::U64,
    }
}

fn int_code(kind: IntKind) -> u8 {
    match kind {
        IntKind::I8 => 0,
        IntKind::I16 => 1,
        IntKind::I32 => 2,
        IntKind::I64 => 3,
        IntKind::U8 => 4,
        IntKind::U16 => 5,
        IntKind::U32 => 6,
        IntKind::U64 => 7,
    }
}

fn nominal_kind(code: u8) -> NominalKind {
    match code {
        0 => NominalKind::Record,
        _ => NominalKind::Enum,
    }
}

fn nominal_code(kind: NominalKind) -> u8 {
    match kind {
        NominalKind::Record => 0,
        NominalKind::Enum => 1,
    }
}

/// What the lowerer reads from a dependency, and nothing else.
///
/// Audited against the production lowerer: it resolves an import's content
/// identity, looks up the result type of an exported name a call reaches, and
/// rebuilds that type. Parameters, modes, effects, visibility and the async flag
/// are not read anywhere, so they are not carried anywhere — a field kept "for
/// later" is a field that is measured now.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoweringInterface {
    content_id: String,
    /// Export names, packed, in the module's own order.
    names: TextStore,
    /// The result type of each export, by the same index.
    results: Vec<CompactTypeId>,
    types: CompactTypes,
}

impl LoweringInterface {
    /// Reduces a module that has just been lowered.
    ///
    /// Built from lowered IR while it is still here, and never from the source,
    /// the schema or a summary: an identity taken from a declaration rather than
    /// from what was lowered would be a claim the source never made.
    pub fn of(module: &Module) -> LoweringInterface {
        let mut builder = Builder::new(module);
        let mut names = TextStore::new();
        let mut results = Vec::with_capacity(module.exports.len());
        for signature in &module.exports {
            names.push(&signature.name);
            results.push(builder.carry(signature.result));
        }
        LoweringInterface {
            content_id: module.header.content_id.clone(),
            names,
            results,
            types: builder.finish(),
        }
    }

    /// The identity computed from the module's own normalized source.
    pub fn content_id(&self) -> &str {
        &self.content_id
    }

    /// The result type of an exported name, as a compact id.
    pub fn result_of(&self, name: &str) -> Option<CompactTypeId> {
        (0..self.names.len())
            .find(|at| self.names.get(*at as u32) == name)
            .map(|at| self.results[at])
    }

    /// One entry of the type graph, as a `TypeDef` over compact child ids.
    pub fn type_at(&self, id: TypeId) -> Option<TypeDef> {
        let id = u32::try_from(id).ok()?;
        self.types.rebuild(id)
    }

    /// How many exports it indexes.
    pub fn export_count(&self) -> usize {
        self.names.len()
    }

    /// How many type entries it carries.
    pub fn type_count(&self) -> usize {
        self.types.entries.len()
    }

    /// Every byte this view owns.
    pub fn retained_bytes(&self) -> usize {
        core::mem::size_of::<LoweringInterface>()
            + self.content_id.capacity()
            + self.names.retained_bytes()
            + self.results.capacity() * core::mem::size_of::<CompactTypeId>()
            + self.types.retained_bytes()
    }

    /// `(entries, entry bytes, refs bytes, variant bytes, text bytes, names)`,
    /// so an owner can be named rather than guessed.
    pub fn retained_breakdown(&self) -> [usize; 6] {
        [
            self.types.entries.len(),
            self.types.entries.capacity() * core::mem::size_of::<CompactType>(),
            self.types.refs.capacity() * core::mem::size_of::<CompactTypeId>(),
            self.types.variants.capacity() * core::mem::size_of::<CompactVariant>(),
            self.types.text.retained_bytes(),
            self.names.retained_bytes(),
        ]
    }
}

/// The complete declared surface of one module, for the verifier.
///
/// docs/43 §5 makes the declared resolution an input to verification and
/// requires the **whole** surface, not the part a caller happens to use. Nothing
/// is abridged here: every public export name and every imported capability
/// interface, exactly as the module states them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationSurface {
    module_name: String,
    content_id: String,
    exports: TextStore,
    capabilities: TextStore,
}

impl VerificationSurface {
    pub fn of(module: &Module) -> VerificationSurface {
        let mut exports = TextStore::new();
        for signature in &module.exports {
            exports.push(&signature.name);
        }
        let mut capabilities = TextStore::new();
        for import in &module.capability_imports {
            capabilities.push(&import.interface);
        }
        VerificationSurface {
            module_name: module.header.module_name.clone(),
            content_id: module.header.content_id.clone(),
            exports,
            capabilities,
        }
    }

    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    pub fn content_id(&self) -> &str {
        &self.content_id
    }

    /// Every public export name, in the module's own order.
    pub fn exports(&self) -> impl Iterator<Item = &str> {
        (0..self.exports.len()).map(|at| self.exports.get(at as u32))
    }

    /// Every capability interface the module imports, in order.
    pub fn capabilities(&self) -> impl Iterator<Item = &str> {
        (0..self.capabilities.len()).map(|at| self.capabilities.get(at as u32))
    }

    pub fn retained_bytes(&self) -> usize {
        core::mem::size_of::<VerificationSurface>()
            + self.module_name.capacity()
            + self.content_id.capacity()
            + self.exports.retained_bytes()
            + self.capabilities.retained_bytes()
    }
}

/// Packs a module's reachable type graph.
struct Builder<'a> {
    source: &'a Module,
    types: CompactTypes,
    identities: Interner,
    /// Source type id to compact id, so a type reached twice is carried once.
    seen: BTreeMap<TypeId, CompactTypeId>,
}

impl<'a> Builder<'a> {
    fn new(source: &'a Module) -> Builder<'a> {
        Builder {
            source,
            types: CompactTypes {
                entries: Vec::new(),
                refs: Vec::new(),
                variants: Vec::new(),
                lengths: Vec::new(),
                text: TextStore::new(),
            },
            identities: Interner::new(),
            seen: BTreeMap::new(),
        }
    }

    fn finish(mut self) -> CompactTypes {
        self.types.text = self.identities.store;
        self.types
    }

    fn text(&mut self, value: &str) -> u32 {
        self.identities.intern(value)
    }

    fn push(&mut self, entry: CompactType) -> CompactTypeId {
        // A table this long cannot be built from a conforming module, and a
        // silent wrap would be a correctness bug rather than a size problem.
        assert!(
            self.types.entries.len() < INDEX_CEILING,
            "a compact type table past u32::MAX"
        );
        self.types.entries.push(entry);
        self.types.entries.len() as CompactTypeId - 1
    }

    fn carry(&mut self, ty: TypeId) -> CompactTypeId {
        if let Some(already) = self.seen.get(&ty) {
            return *already;
        }
        let Some(definition) = self.source.types.get(ty) else {
            // The lowerer answers an out-of-range type with `unit`, and a view
            // that answered differently would move a decision out of it.
            return self.push(CompactType {
                tag: tag::UNIT,
                ..CompactType::default()
            });
        };
        let entry = match definition.clone() {
            TypeDef::Unit => simple(tag::UNIT),
            TypeDef::Bool => simple(tag::BOOL),
            TypeDef::Int(kind) => CompactType {
                tag: tag::INT,
                kind: int_code(kind),
                ..CompactType::default()
            },
            TypeDef::Size => simple(tag::SIZE),
            TypeDef::Duration => simple(tag::DURATION),
            TypeDef::Text => simple(tag::TEXT),
            TypeDef::Bytes => simple(tag::BYTES),
            TypeDef::ConversionError => simple(tag::CONVERSION_ERROR),
            TypeDef::MmioRegion => simple(tag::MMIO_REGION),
            TypeDef::MmioRegionMut => simple(tag::MMIO_REGION_MUT),
            TypeDef::Event => simple(tag::EVENT),
            TypeDef::Semaphore => simple(tag::SEMAPHORE),
            TypeDef::Barrier => simple(tag::BARRIER),
            TypeDef::Latch => simple(tag::LATCH),
            TypeDef::AtomicBool => simple(tag::ATOMIC_BOOL),
            TypeDef::AtomicU32 => simple(tag::ATOMIC_U32),
            TypeDef::AtomicU64 => simple(tag::ATOMIC_U64),
            TypeDef::Option(inner) => self.wrapper(tag::OPTION, inner),
            TypeDef::Task(inner) => self.wrapper(tag::TASK, inner),
            TypeDef::TaskResult(inner) => self.wrapper(tag::TASK_RESULT, inner),
            TypeDef::Shared(inner) => self.wrapper(tag::SHARED, inner),
            TypeDef::Region(inner) => self.wrapper(tag::REGION, inner),
            TypeDef::DmaRegion(inner) => self.wrapper(tag::DMA_REGION, inner),
            TypeDef::RegionMut(inner) => self.wrapper(tag::REGION_MUT, inner),
            TypeDef::DmaRegionMut(inner) => self.wrapper(tag::DMA_REGION_MUT, inner),
            TypeDef::Mutex(inner) => self.wrapper(tag::MUTEX, inner),
            TypeDef::RwLock(inner) => self.wrapper(tag::RW_LOCK, inner),
            TypeDef::MutexGuard(inner) => self.wrapper(tag::MUTEX_GUARD, inner),
            TypeDef::ReadGuard(inner) => self.wrapper(tag::READ_GUARD, inner),
            TypeDef::WriteGuard(inner) => self.wrapper(tag::WRITE_GUARD, inner),
            TypeDef::Channel(inner) => self.wrapper(tag::CHANNEL, inner),
            TypeDef::Slice(inner) => self.wrapper(tag::SLICE, inner),
            TypeDef::Result(ok, error) => {
                let ok = self.carry(ok);
                let error = self.carry(error);
                CompactType {
                    tag: tag::RESULT,
                    a: ok,
                    b: error,
                    ..CompactType::default()
                }
            }
            TypeDef::Array(element, length) => {
                let element = self.carry(element);
                let at = self.types.lengths.len() as u32;
                self.types.lengths.push(length);
                CompactType {
                    tag: tag::ARRAY,
                    a: element,
                    c: at,
                    ..CompactType::default()
                }
            }
            TypeDef::Tuple(elements) => {
                let (start, end) = self.range(&elements);
                CompactType {
                    tag: tag::TUPLE,
                    a: start,
                    b: end,
                    ..CompactType::default()
                }
            }
            TypeDef::Function(parameters, result) => {
                let (start, end) = self.range(&parameters);
                let result = self.carry(result);
                CompactType {
                    tag: tag::FUNCTION,
                    a: start,
                    b: end,
                    c: result,
                    ..CompactType::default()
                }
            }
            TypeDef::Capability(name) => {
                let at = self.text(&name);
                CompactType {
                    tag: tag::CAPABILITY,
                    a: at,
                    ..CompactType::default()
                }
            }
            TypeDef::Nominal {
                module_content_id,
                export_name,
                kind,
                fields,
                variants,
            } => {
                // The identity string is interned: one copy per distinct
                // identity, at full length, however many nominal types the
                // module defines.
                let identity = self.text(&module_content_id);
                let name = self.text(&export_name);
                let (start, end) = match kind {
                    NominalKind::Record => self.range(&fields),
                    NominalKind::Enum => {
                        let mut packed = Vec::with_capacity(variants.len());
                        for variant in &variants {
                            let payload = self.range(&variant.payload);
                            let at = self.text(&variant.name);
                            packed.push(CompactVariant {
                                name: at,
                                payload_start: payload.0,
                                payload_end: payload.1,
                            });
                        }
                        let start = self.types.variants.len() as u32;
                        self.types.variants.extend(packed);
                        (start, self.types.variants.len() as u32)
                    }
                };
                CompactType {
                    tag: tag::NOMINAL,
                    kind: nominal_code(kind),
                    a: identity,
                    b: name,
                    c: start,
                    d: end,
                }
            }
        };
        let at = self.push(entry);
        self.seen.insert(ty, at);
        at
    }

    fn wrapper(&mut self, tag: u8, inner: TypeId) -> CompactType {
        let inner = self.carry(inner);
        CompactType {
            tag,
            a: inner,
            ..CompactType::default()
        }
    }

    /// Carries a list of child types and returns the range it occupies.
    fn range(&mut self, children: &[TypeId]) -> (u32, u32) {
        let mut carried = Vec::with_capacity(children.len());
        for child in children {
            carried.push(self.carry(*child));
        }
        let start = self.types.refs.len() as u32;
        self.types.refs.extend(carried);
        (start, self.types.refs.len() as u32)
    }
}

fn simple(tag: u8) -> CompactType {
    CompactType {
        tag,
        ..CompactType::default()
    }
}

/// A module this one imports, already lowered and already released.
///
/// What a dependency contributes is its *computed* identity and the result types
/// of what it exports — never a declaration about itself that this module took
/// on trust, and no longer the whole of it.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedImport<'a> {
    /// The dotted module name, as the importing module writes it.
    pub name: &'a str,
    /// What survived the dependency's lowering.
    pub interface: &'a LoweringInterface,
}
