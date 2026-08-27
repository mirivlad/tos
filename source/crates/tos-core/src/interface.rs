// SPDX-License-Identifier: GPL-3.0-or-later
//! What a lowered dependency contributes to the modules that import it.
//!
//! Lowering a set is ordered so that a module's dependencies are lowered first.
//! Handing the next module a whole `Module` for each of them means every
//! dependency's bodies, blocks, instructions and source map stay alive until the
//! last module of the closure is lowered — `N` decoded modules before the first
//! instruction, which is the retained-IR slope ADR-0071 removed from execution
//! and which has no more right to exist in lowering.
//!
//! So a dependency is reduced, the moment it is lowered, to the only three
//! things the lowerer ever reads from it:
//!
//! - its **computed** content identity, for an import's `module_content_id`;
//! - its **public exported signatures**, to find the one a call names;
//! - the **type graph reachable from those signatures**, so a type carried
//!   across the boundary is rebuilt from what the dependency actually declared.
//!
//! ## What this is not
//!
//! **Not authority.** It is a derived implementation representation, built from
//! a `Module` this process just lowered, used by the same process, and gone when
//! lowering ends. It is not a cache object, not a trust object, nothing is
//! admitted on the strength of it, and no receipt binds to it. The verifier
//! never sees one.
//!
//! **Not a summary.** It is built from lowered IR, never from the frontend's
//! view of a module. A content identity taken from a declaration rather than
//! from what was lowered would be a claim the source never made.
//!
//! **Not a narrowing.** Nominal identity is carried through exactly as the type
//! table states it — defining module content id, export name, kind, fields and
//! variants — because a nominal type is not its shape. Two records with the same
//! fields from two modules are two types, and an interface that dropped the
//! defining identity would silently make them one.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use tos_ir::{Module, Parameter, Signature, TypeDef, TypeId, Variant};

/// One lowered dependency, reduced to what its importers read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoweredInterface {
    module_name: String,
    content_id: String,
    /// The exported signatures, with their type ids rewritten into [`types`].
    ///
    /// [`types`]: LoweredInterface::types
    exports: Vec<Signature>,
    /// The type graph reachable from the exports' parameter and result types,
    /// interned, in the order it was reached.
    types: Vec<TypeDef>,
    /// The capability interfaces this module imports, for the declared
    /// resolution a verifier is handed.
    capabilities: Vec<String>,
}

impl LoweredInterface {
    /// Reduces a module that has just been lowered.
    ///
    /// Called while the module is alive and immediately before it is released.
    /// Everything below is read out of the lowered IR: nothing is taken from the
    /// source, the schema or a summary.
    pub fn of(module: &Module) -> LoweredInterface {
        let mut compact = Compaction {
            source: module,
            types: Vec::new(),
            seen: BTreeMap::new(),
        };
        // Parameters as well as results. Only a result crosses the boundary in
        // V1, but a signature is not honestly carried if the types its
        // parameters name are missing, and a reference lowerer that begins to
        // check an argument must find them here rather than discover that the
        // interface was built for exactly one use.
        let exports = module
            .exports
            .iter()
            .map(|signature| Signature {
                name: signature.name.clone(),
                visibility: signature.visibility,
                is_async: signature.is_async,
                parameters: signature
                    .parameters
                    .iter()
                    .map(|parameter| Parameter {
                        name: parameter.name.clone(),
                        ty: compact.carry(parameter.ty),
                        mode: parameter.mode,
                    })
                    .collect(),
                result: compact.carry(signature.result),
                effects: signature.effects.clone(),
            })
            .collect();
        LoweredInterface {
            module_name: module.header.module_name.clone(),
            content_id: module.header.content_id.clone(),
            exports,
            types: compact.types,
            capabilities: module
                .capability_imports
                .iter()
                .map(|import| import.interface.clone())
                .collect(),
        }
    }

    /// The dotted module name this interface was lowered under.
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// The identity computed from the module's own normalized source.
    pub fn content_id(&self) -> &str {
        &self.content_id
    }

    /// The exported signatures, ordered as the module orders them.
    pub fn exports(&self) -> &[Signature] {
        &self.exports
    }

    /// The signature an exported name reaches, if the module exports it.
    pub fn export(&self, name: &str) -> Option<&Signature> {
        self.exports.iter().find(|export| export.name == name)
    }

    /// The compact type table the exported signatures index.
    pub fn types(&self) -> &[TypeDef] {
        &self.types
    }

    /// The capability interfaces the module imports.
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    /// What the figure below is made of, so an owner can be named rather than
    /// guessed: `(exports, export names, parameters, effects, types, other)`.
    pub fn retained_breakdown(&self) -> [usize; 6] {
        let mut parts = [0usize; 6];
        parts[0] = self.exports.capacity() * core::mem::size_of::<Signature>();
        for signature in &self.exports {
            parts[1] += signature.name.capacity();
            parts[2] += signature.parameters.capacity() * core::mem::size_of::<Parameter>();
            for parameter in &signature.parameters {
                parts[2] += parameter.name.capacity();
            }
            parts[3] += signature.effects.capacity() * core::mem::size_of::<String>();
            for effect in &signature.effects {
                parts[3] += effect.capacity();
            }
        }
        parts[4] = self.types.capacity() * core::mem::size_of::<TypeDef>();
        for definition in &self.types {
            parts[4] += tos_ir::footprint::type_definition_bytes(definition);
        }
        parts[5] = core::mem::size_of::<LoweredInterface>()
            + self.module_name.capacity()
            + self.content_id.capacity()
            + self.capabilities.capacity() * core::mem::size_of::<String>()
            + self
                .capabilities
                .iter()
                .map(|capability| capability.capacity())
                .sum::<usize>();
        parts
    }

    /// What the type table is made of: `(entries, inline bytes, nominal count,
    /// nominal identity text, other heap)`.
    pub fn type_table_breakdown(&self) -> [usize; 5] {
        let mut parts = [0usize; 5];
        parts[0] = self.types.len();
        parts[1] = self.types.capacity() * core::mem::size_of::<TypeDef>();
        for definition in &self.types {
            if let TypeDef::Nominal {
                module_content_id,
                export_name,
                ..
            } = definition
            {
                parts[2] += 1;
                parts[3] += module_content_id.capacity() + export_name.capacity();
                parts[4] += tos_ir::footprint::type_definition_bytes(definition)
                    - module_content_id.capacity()
                    - export_name.capacity();
            } else {
                parts[4] += tos_ir::footprint::type_definition_bytes(definition);
            }
        }
        parts
    }

    /// How many exported signatures this interface carries.
    pub fn export_count(&self) -> usize {
        self.exports.len()
    }

    /// How many type-table entries it carries.
    pub fn type_count(&self) -> usize {
        self.types.len()
    }

    /// Every byte this interface owns, for a caller bounding what it accumulates.
    ///
    /// The same rule as `tos_ir::retained_bytes`: capacities, not lengths, and
    /// no allocator metadata.
    pub fn retained_bytes(&self) -> usize {
        let mut bytes = core::mem::size_of::<LoweredInterface>();
        bytes += self.module_name.capacity() + self.content_id.capacity();
        bytes += self.exports.capacity() * core::mem::size_of::<Signature>();
        for signature in &self.exports {
            bytes += signature.name.capacity();
            bytes += signature.parameters.capacity() * core::mem::size_of::<Parameter>();
            for parameter in &signature.parameters {
                bytes += parameter.name.capacity();
            }
            bytes += signature.effects.capacity() * core::mem::size_of::<String>();
            for effect in &signature.effects {
                bytes += effect.capacity();
            }
        }
        bytes += self.types.capacity() * core::mem::size_of::<TypeDef>();
        for definition in &self.types {
            bytes += tos_ir::footprint::type_definition_bytes(definition);
        }
        bytes += self.capabilities.capacity() * core::mem::size_of::<String>();
        for capability in &self.capabilities {
            bytes += capability.capacity();
        }
        bytes
    }
}

/// Rebuilds a reachable type graph into a table of its own.
struct Compaction<'a> {
    source: &'a Module,
    types: Vec<TypeDef>,
    /// Source type id to compact type id. Memoized so a type reached twice is
    /// carried once, which is also what keeps sharing in a DAG from becoming a
    /// re-traversal.
    seen: BTreeMap<TypeId, TypeId>,
}

impl Compaction<'_> {
    /// Carries one type across, with everything it names beneath it.
    ///
    /// The structure is preserved exactly. Only the indices change, and they
    /// change because they are per-table positions rather than identities —
    /// which is the same reason the lowerer re-interns a type it adopts.
    fn carry(&mut self, ty: TypeId) -> TypeId {
        if let Some(already) = self.seen.get(&ty) {
            return *already;
        }
        let Some(definition) = self.source.types.get(ty) else {
            // The lowerer answers an out-of-range type with `unit`, and an
            // interface that answered differently would move a decision out of
            // the lowerer and into this file.
            return self.intern(TypeDef::Unit);
        };
        let rebuilt = match definition.clone() {
            TypeDef::Option(inner) => TypeDef::Option(self.carry(inner)),
            TypeDef::Task(inner) => TypeDef::Task(self.carry(inner)),
            TypeDef::TaskResult(inner) => TypeDef::TaskResult(self.carry(inner)),
            TypeDef::Shared(inner) => TypeDef::Shared(self.carry(inner)),
            TypeDef::Region(inner) => TypeDef::Region(self.carry(inner)),
            TypeDef::RegionMut(inner) => TypeDef::RegionMut(self.carry(inner)),
            TypeDef::DmaRegion(inner) => TypeDef::DmaRegion(self.carry(inner)),
            TypeDef::DmaRegionMut(inner) => TypeDef::DmaRegionMut(self.carry(inner)),
            TypeDef::Mutex(inner) => TypeDef::Mutex(self.carry(inner)),
            TypeDef::RwLock(inner) => TypeDef::RwLock(self.carry(inner)),
            TypeDef::MutexGuard(inner) => TypeDef::MutexGuard(self.carry(inner)),
            TypeDef::ReadGuard(inner) => TypeDef::ReadGuard(self.carry(inner)),
            TypeDef::WriteGuard(inner) => TypeDef::WriteGuard(self.carry(inner)),
            TypeDef::Channel(inner) => TypeDef::Channel(self.carry(inner)),
            TypeDef::Slice(inner) => TypeDef::Slice(self.carry(inner)),
            TypeDef::Result(ok, error) => {
                let ok = self.carry(ok);
                let error = self.carry(error);
                TypeDef::Result(ok, error)
            }
            TypeDef::Array(element, length) => {
                let element = self.carry(element);
                TypeDef::Array(element, length)
            }
            TypeDef::Tuple(elements) => {
                TypeDef::Tuple(elements.into_iter().map(|at| self.carry(at)).collect())
            }
            TypeDef::Function(parameters, result) => {
                let parameters = parameters.into_iter().map(|at| self.carry(at)).collect();
                let result = self.carry(result);
                TypeDef::Function(parameters, result)
            }
            // Nominal identity is carried whole: the defining module's content
            // id, the export name, the kind, the fields and the variants.
            // Dropping any of them would make two types one.
            TypeDef::Nominal {
                module_content_id,
                export_name,
                kind,
                fields,
                variants,
            } => TypeDef::Nominal {
                module_content_id,
                export_name,
                kind,
                fields: fields.into_iter().map(|at| self.carry(at)).collect(),
                variants: variants
                    .into_iter()
                    .map(|variant| Variant {
                        name: variant.name,
                        payload: variant
                            .payload
                            .into_iter()
                            .map(|at| self.carry(at))
                            .collect(),
                    })
                    .collect(),
            },
            scalar => scalar,
        };
        let at = self.intern(rebuilt);
        self.seen.insert(ty, at);
        at
    }

    /// One entry per distinct definition, as the module's own table does.
    fn intern(&mut self, definition: TypeDef) -> TypeId {
        if let Some(at) = self.types.iter().position(|held| *held == definition) {
            return at;
        }
        self.types.push(definition);
        self.types.len() - 1
    }
}

/// A module this one imports, already lowered and already released.
///
/// What a dependency contributes is its *computed* identity and its exported
/// surface — never a declaration about itself that this module took on trust,
/// and no longer the whole of it.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedImport<'a> {
    /// The dotted module name, as the importing module writes it.
    pub name: &'a str,
    /// What survived the dependency's lowering.
    pub interface: &'a LoweredInterface,
}

impl<'a> ResolvedImport<'a> {
    /// The interface of an already-lowered module, under the name that reaches
    /// it.
    pub fn new(name: &'a str, interface: &'a LoweredInterface) -> ResolvedImport<'a> {
        ResolvedImport { name, interface }
    }
}
