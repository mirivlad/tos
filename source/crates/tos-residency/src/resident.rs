// SPDX-License-Identifier: GPL-3.0-or-later
//! The bounded resident set (ADR-0071 §5–§7).
//!
//! A module is resident when its image, its decoded form and the indexes
//! derived from it are in memory. All of that is inside the byte bound, and all
//! of it goes when the module is evicted — a bound satisfied by counting image
//! bytes alone would be no bound at all, since a `0.37 MiB` image carries about
//! `20 MiB` of decoded module behind it.
//!
//! **A reload is byte identity, not re-verification.** Full semantic
//! verification happened once, at launch; the trusted record holds the exact
//! artifact digest, and a reload hashes the returned snapshot and compares. A
//! match means these are the bytes the verifier already traversed. Two
//! conditions carry that: the snapshot must be immutable, because a mutable
//! buffer would put the digest on a different object than the one that runs;
//! and the parser stays total on every path, because a reader whose safety
//! depended on the hash having matched would be unsafe exactly when it did not.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use tos_image::ParseLimits;
use tos_ir::Module;

use crate::{
    ClosureModuleId, Failure, ImageSnapshot, ModuleProvider, VerifiedClosureManifest,
    VerifiedModuleRecord,
};

/// How much a run may hold resident.
///
/// **Fixed properties of the execution.** Not a function of free memory, not a
/// share divided among what is running, not adaptive. A run either fits its
/// bounds or fails naming the bound it hit; a program whose success depended on
/// how much memory happened to be free would be reporting a fact about
/// scheduling as a fact about itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResidencyLimits {
    /// The most modules that may be resident at once. At least one: an
    /// execution must be able to make progress with a single resident module.
    pub modules: usize,
    /// The most bytes of **module-derived state** that may be resident —
    /// images, decoded modules, derived indexes and the residency table's own
    /// bookkeeping, all of it.
    pub bytes: usize,
}

/// The three components of §7, and the bookkeeping beside them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ledger {
    pub image_bytes: usize,
    pub decoded_bytes: usize,
    pub index_bytes: usize,
    pub bookkeeping_bytes: usize,
}

impl Ledger {
    pub fn total(&self) -> usize {
        self.image_bytes + self.decoded_bytes + self.index_bytes + self.bookkeeping_bytes
    }
}

/// What happened while the bounds were being kept.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Traffic {
    pub loads: usize,
    pub evictions: usize,
    pub hashes: usize,
    pub bytes_hashed: usize,
    pub peak_ledger: usize,
}

/// One resident module and everything it keeps alive.
struct Resident {
    id: ClosureModuleId,
    /// The immutable snapshot that was hashed and then parsed. Held for its
    /// lifetime rather than read and dropped, so that what is resident is the
    /// artifact and not merely something derived from it — and so that
    /// releasing a resident releases the image too.
    #[allow(dead_code)]
    snapshot: ImageSnapshot,
    module: Module,
    /// This module's `import slot -> ClosureModuleId` mapping, resolved against
    /// the trusted membership on first use. Resident derived state: the
    /// manifest does not hold it, and it dies with the module.
    imports: Vec<ClosureModuleId>,
    /// `export name -> function index`, built inside the module the manifest
    /// already fixed, on first use. Also resident derived state.
    exports: BTreeMap<String, usize>,
    image_bytes: usize,
    decoded_bytes: usize,
    index_bytes: usize,
    used_at: u64,
}

impl Resident {
    fn cost(&self) -> usize {
        self.image_bytes + self.decoded_bytes + self.index_bytes + core::mem::size_of::<Resident>()
    }
}

/// The bounded resident set, and the only way a module becomes reachable.
pub struct Residency {
    limits: ResidencyLimits,
    parse_limits: ParseLimits,
    live: Vec<Resident>,
    clock: u64,
    traffic: Traffic,
}

impl Residency {
    /// A resident set under declared bounds.
    ///
    /// `parse_limits` are the accepted ceilings the reload parser checks table
    /// counts against, handed down as data.
    pub fn new(limits: ResidencyLimits, parse_limits: ParseLimits) -> Residency {
        Residency {
            limits: ResidencyLimits {
                modules: limits.modules.max(1),
                bytes: limits.bytes,
            },
            parse_limits,
            live: Vec::new(),
            clock: 0,
            traffic: Traffic::default(),
        }
    }

    pub fn traffic(&self) -> Traffic {
        self.traffic
    }

    pub fn resident(&self) -> usize {
        self.live.len()
    }

    pub fn ledger(&self) -> Ledger {
        Ledger {
            image_bytes: self.live.iter().map(|r| r.image_bytes).sum(),
            decoded_bytes: self.live.iter().map(|r| r.decoded_bytes).sum(),
            index_bytes: self.live.iter().map(|r| r.index_bytes).sum(),
            bookkeeping_bytes: core::mem::size_of::<Residency>()
                + self.live.capacity() * core::mem::size_of::<Resident>(),
        }
    }

    fn find(&self, id: ClosureModuleId) -> Option<usize> {
        self.live.iter().position(|r| r.id == id)
    }

    /// The resident module, or `None` if it is not resident.
    ///
    /// Deliberately not a way to make one resident: a caller that wants a
    /// module asks [`Residency::ensure`] first, and the borrow it gets back
    /// ends before anything can evict.
    pub fn module_of(&self, id: ClosureModuleId) -> Option<&Module> {
        self.find(id).map(|at| &self.live[at].module)
    }

    /// Makes `id` resident, evicting whatever the bounds require.
    ///
    /// Eviction is **deterministic**: least recently used, and never a function
    /// of free memory. The module just loaded is never the victim — an
    /// execution must be able to make progress with one resident module, so a
    /// bound too small for a single module fails the run rather than evicting
    /// what it is about to use.
    pub fn ensure(
        &mut self,
        id: ClosureModuleId,
        provider: &dyn ModuleProvider,
        records: &[VerifiedModuleRecord],
    ) -> Result<(), Failure> {
        self.clock += 1;
        if let Some(at) = self.find(id) {
            self.live[at].used_at = self.clock;
            return Ok(());
        }

        // Room by count first, so the load never overshoots the count bound.
        while self.live.len() >= self.limits.modules {
            self.evict_least_recent(None);
        }

        let resident = self.load(id, provider, records)?;
        let cost = resident.cost();
        self.live.push(resident);

        // Then room by bytes.
        while self.ledger().total() > self.limits.bytes && self.live.len() > 1 {
            self.evict_least_recent(Some(id));
        }
        if self.ledger().total() > self.limits.bytes {
            self.live.pop();
            return Err(Failure::OverResidencyBound {
                module: id.position(),
                bytes: cost,
            });
        }

        self.traffic.peak_ledger = self.traffic.peak_ledger.max(self.ledger().total());
        Ok(())
    }

    fn evict_least_recent(&mut self, keep: Option<ClosureModuleId>) {
        let Some(at) = self
            .live
            .iter()
            .enumerate()
            .filter(|(_, r)| Some(r.id) != keep)
            .min_by_key(|(_, r)| r.used_at)
            .map(|(at, _)| at)
        else {
            return;
        };
        // Image, decoded module, derived indexes and the residency entry all go
        // together. Releasing the image and keeping what was decoded from it
        // would put the execution under the byte bound while holding thirty
        // times the image in derived state.
        self.live.remove(at);
        self.traffic.evictions += 1;
    }

    /// One load: obtain the immutable snapshot, hash **that exact snapshot**,
    /// compare against the trusted artifact digest, and only then parse it.
    fn load(
        &mut self,
        id: ClosureModuleId,
        provider: &dyn ModuleProvider,
        records: &[VerifiedModuleRecord],
    ) -> Result<Resident, Failure> {
        let position = id.position();
        let record = records.get(position).ok_or(Failure::Missing(position))?;
        let snapshot = provider.image(id).ok_or(Failure::Missing(position))?;

        // The snapshot is immutable, so these are the same bytes throughout:
        // hashed here, parsed below, executed after. There is no window in
        // which they could differ, because there is no way to write them.
        self.traffic.hashes += 1;
        self.traffic.bytes_hashed += snapshot.len();
        if tos_hash::sha256(&snapshot) != record.artifact_digest {
            return Err(Failure::ArtifactDigest { module: position });
        }

        let module =
            tos_image::parse(&snapshot, &self.parse_limits).map_err(|error| Failure::Parser {
                module: position,
                error,
            })?;
        let decoded_bytes = decoded_cost(&module);

        self.traffic.loads += 1;
        Ok(Resident {
            id,
            image_bytes: snapshot.len(),
            snapshot,
            module,
            imports: Vec::new(),
            exports: BTreeMap::new(),
            index_bytes: 0,
            decoded_bytes,
            used_at: self.clock,
        })
    }

    /// Which module a resident caller's import slot names.
    ///
    /// Resolved against the trusted membership, once, when the caller is
    /// resident. Not module search: the answer can only be a member of the
    /// closure the manifest already fixed, and a slot naming anything else has
    /// no answer at all.
    pub fn import_of(
        &mut self,
        id: ClosureModuleId,
        slot: usize,
        manifest: &VerifiedClosureManifest,
    ) -> Option<ClosureModuleId> {
        let at = self.find(id)?;
        if self.live[at].imports.is_empty() && !self.live[at].module.imports.is_empty() {
            let mut resolved = Vec::with_capacity(self.live[at].module.imports.len());
            for declared in &self.live[at].module.imports {
                // The exact pair the caller's verified artifact states, hashed
                // canonically and looked up in the trusted membership. No
                // ambient lookup appears: the answer can only be a member.
                resolved
                    .push(manifest.resolve(&declared.module_name, &declared.module_content_id)?);
            }
            self.live[at].index_bytes +=
                resolved.capacity() * core::mem::size_of::<ClosureModuleId>();
            self.live[at].imports = resolved;
        }
        self.live[at].imports.get(slot).copied()
    }

    /// Which function of a resident module an export name reaches.
    ///
    /// Reconstructed inside the module the manifest already fixed, and cached
    /// as resident state. This is not module search: the module is not being
    /// chosen here, it was chosen at launch and the provider cannot widen it.
    pub fn export_of(&mut self, id: ClosureModuleId, name: &str) -> Option<usize> {
        let at = self.find(id)?;
        if self.live[at].exports.is_empty() {
            let mut exports = BTreeMap::new();
            let mut bytes = 0usize;
            for (index, function) in self.live[at].module.functions.iter().enumerate() {
                bytes += function.signature.name.len()
                    + core::mem::size_of::<String>()
                    + core::mem::size_of::<usize>()
                    + 32;
                exports
                    .entry(function.signature.name.clone())
                    .or_insert(index);
            }
            self.live[at].exports = exports;
            self.live[at].index_bytes += bytes;
        }
        self.live[at].exports.get(name).copied()
    }
}

/// What a decoded module costs, near enough to bound it.
///
/// Counted from the module's own tables rather than measured through an
/// allocator, because a bound has to hold wherever the allocator is. It is a
/// lower bound on the true footprint by construction — every term below is a
/// thing that exists — so a residency limit set against it is conservative in
/// the direction that matters.
fn decoded_cost(module: &Module) -> usize {
    let mut bytes = core::mem::size_of::<Module>();
    bytes += module.types.capacity() * core::mem::size_of::<tos_ir::TypeDef>();
    bytes += module.imports.capacity() * core::mem::size_of::<tos_ir::Import>();
    bytes +=
        module.capability_imports.capacity() * core::mem::size_of::<tos_ir::CapabilityImport>();
    bytes += module.exports.capacity() * core::mem::size_of::<tos_ir::Signature>();
    bytes += module.constants.capacity() * core::mem::size_of::<tos_ir::Constant>();
    bytes += module.functions.capacity() * core::mem::size_of::<tos_ir::Function>();
    bytes += module.source_map.capacity() * core::mem::size_of::<tos_ir::SourceMapEntry>();
    for entry in &module.source_map {
        bytes += entry.source_set.len()
            + entry.path.len()
            + entry.content_id.len()
            + entry.frontend_identity.len()
            + entry.language_version.len()
            + entry.unicode_normalization_baseline.len();
    }
    for function in &module.functions {
        bytes += function.signature.name.len();
        bytes += function.values.capacity() * core::mem::size_of::<usize>();
        bytes += function.blocks.capacity() * core::mem::size_of::<tos_ir::Block>();
        for block in &function.blocks {
            bytes += block.parameters.capacity() * core::mem::size_of::<usize>();
            bytes += block.instructions.capacity() * core::mem::size_of::<tos_ir::Instruction>();
        }
    }
    bytes
}
