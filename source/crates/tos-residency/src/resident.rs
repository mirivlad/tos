// SPDX-License-Identifier: GPL-3.0-or-later
//! The bounded resident set (ADR-0071 sections 5 to 7).
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
//!
//! **Admission sees the whole cost.** A load builds everything the module will
//! ever derive — the import mapping and the public export index — before it is
//! admitted, and the bound is checked against that complete figure. A resident
//! module has no lazy allocation left to make, so nothing can grow the ledger
//! between one admission and the next. An index built after admission would be
//! a byte bound decided on a figure that was already out of date.

use alloc::vec::Vec;

use tos_image::ParseLimits;
use tos_ir::{Module, Visibility};

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

/// A declared configuration that does not describe a possible execution.
///
/// Refused rather than repaired. The accepted minimum of one resident module
/// means *fewer is inadmissible*, not *round it up for the caller*: a run
/// configured to hold no module at all is a mistake in what was declared, and
/// silently substituting the smallest working value would hide it and report
/// success for bounds nobody asked for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationError {
    /// `modules` was zero.
    NoResidentModules,
}

/// The three components of section 7, and the bookkeeping beside them.
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
///
/// Complete on arrival. Every field below is filled by the load that produced
/// it, so what a resident costs is known before it is admitted and does not
/// change while it is resident.
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
    /// the trusted membership at load. Resident derived state: the manifest
    /// does not hold it, and it dies with the module.
    imports: Vec<ClosureModuleId>,
    /// Indices of this module's **public** functions, ordered by name.
    ///
    /// Indices and not names: the names are already inside the resident module,
    /// and a second copy of them would be a second thing to bound. A lookup
    /// binary-searches this table and compares against the name in the module
    /// itself, so the index costs exactly `capacity * size_of::<usize>()` and
    /// the accounting for it is a multiplication rather than a survey.
    ///
    /// A private function is not in here at all. Visibility is not enforced at
    /// the lookup by a check someone could omit; what is not exported has no
    /// entry to find.
    public_exports: Vec<usize>,
    image_bytes: usize,
    decoded_bytes: usize,
    index_bytes: usize,
    used_at: u64,
}

impl Resident {
    /// Everything this resident holds that came from the module.
    fn module_derived(&self) -> usize {
        self.image_bytes + self.decoded_bytes + self.index_bytes
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
    /// A resident set under declared bounds, or a refusal of the bounds.
    ///
    /// `parse_limits` are the accepted ceilings the reload parser checks table
    /// counts against, handed down as data.
    pub fn new(
        limits: ResidencyLimits,
        parse_limits: ParseLimits,
    ) -> Result<Residency, ConfigurationError> {
        if limits.modules == 0 {
            return Err(ConfigurationError::NoResidentModules);
        }
        Ok(Residency {
            limits,
            parse_limits,
            live: Vec::new(),
            clock: 0,
            traffic: Traffic::default(),
        })
    }

    pub fn limits(&self) -> ResidencyLimits {
        self.limits
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
    ///
    /// The manifest is required because the resident state a load builds
    /// includes the import mapping, and that is resolved against trusted
    /// membership. It is the same manifest throughout an execution; passing it
    /// per call rather than storing it keeps the resident set from owning a
    /// second reference to the closure's authority.
    pub fn ensure(
        &mut self,
        id: ClosureModuleId,
        provider: &dyn ModuleProvider,
        records: &[VerifiedModuleRecord],
        manifest: &VerifiedClosureManifest,
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

        let resident = self.load(id, provider, records, manifest)?;
        let alone = resident.module_derived() + core::mem::size_of::<Resident>();
        self.live.push(resident);

        // Then room by bytes, against the complete cost of everything resident.
        while self.ledger().total() > self.limits.bytes && self.live.len() > 1 {
            self.evict_least_recent(Some(id));
        }
        if self.ledger().total() > self.limits.bytes {
            self.live.pop();
            return Err(Failure::OverResidencyBound {
                module: id.position(),
                bytes: alone,
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
    /// compare against the trusted artifact digest, parse it, and derive
    /// everything the module will ever derive.
    fn load(
        &mut self,
        id: ClosureModuleId,
        provider: &dyn ModuleProvider,
        records: &[VerifiedModuleRecord],
        manifest: &VerifiedClosureManifest,
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

        // The import mapping, against trusted membership. Not module search:
        // the answer can only be a member of the closure the manifest already
        // fixed, and a slot naming anything else has no answer at all — which
        // is a refusal here, at load, rather than a `None` discovered mid-call.
        let mut imports = Vec::with_capacity(module.imports.len());
        for declared in &module.imports {
            let resolved = manifest
                .resolve(&declared.module_name, &declared.module_content_id)
                .ok_or(Failure::WrongModule { module: position })?;
            imports.push(resolved);
        }

        let public_exports = public_export_index(&module);

        let index_bytes = imports.capacity() * core::mem::size_of::<ClosureModuleId>()
            + public_exports.capacity() * core::mem::size_of::<usize>();

        self.traffic.loads += 1;
        Ok(Resident {
            id,
            // The `Arc<[u8]>` allocation is its two-word header and the bytes.
            image_bytes: snapshot.len() + 2 * core::mem::size_of::<usize>(),
            snapshot,
            decoded_bytes: tos_ir::retained_bytes(&module),
            module,
            imports,
            public_exports,
            index_bytes,
            used_at: self.clock,
        })
    }

    /// Which module a resident caller's import slot names.
    ///
    /// A read of state the load already built. It takes `&self` on purpose:
    /// resolving an import cannot allocate, cannot evict and cannot move the
    /// ledger, so there is no admission decision hiding behind it.
    pub fn import_of(&self, id: ClosureModuleId, slot: usize) -> Option<ClosureModuleId> {
        let at = self.find(id)?;
        self.live[at].imports.get(slot).copied()
    }

    /// Which function of a resident module an export name reaches.
    ///
    /// **Public functions only.** The index holds nothing else, so a private
    /// function is not hidden by a check here; it is absent.
    ///
    /// This is not module search: the module is not being chosen, it was chosen
    /// at launch and the provider cannot widen it. The name is compared against
    /// the name inside the resident module, so no copy of the export strings
    /// exists anywhere.
    pub fn export_of(&self, id: ClosureModuleId, name: &str) -> Option<usize> {
        let at = self.find(id)?;
        let resident = &self.live[at];
        let functions = &resident.module.functions;
        let found = resident
            .public_exports
            .binary_search_by(|index| functions[*index].signature.name.as_str().cmp(name))
            .ok()?;
        Some(resident.public_exports[found])
    }

    /// How many public exports a resident module has, for evidence.
    pub fn public_exports_of(&self, id: ClosureModuleId) -> Option<usize> {
        self.find(id).map(|at| self.live[at].public_exports.len())
    }
}

/// The public functions of a module, ordered by name.
///
/// The sort is stable and duplicates collapse to the first, so a name that two
/// public functions somehow share resolves to the lower function index — the
/// same answer a linear scan in function order gives.
fn public_export_index(module: &Module) -> Vec<usize> {
    let mut public: Vec<usize> = module
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| function.signature.visibility == Visibility::Public)
        .map(|(index, _)| index)
        .collect();
    public.sort_by(|left, right| {
        module.functions[*left]
            .signature
            .name
            .cmp(&module.functions[*right].signature.name)
    });
    public.dedup_by(|left, right| {
        module.functions[*left].signature.name == module.functions[*right].signature.name
    });
    public
}
