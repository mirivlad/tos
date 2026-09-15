// SPDX-License-Identifier: GPL-3.0-or-later
//! Launch: sequential verification of the exact resolved closure.
//!
//! ADR-0071 §1. Every module is verified **once, before the first
//! instruction**, one at a time, and its materialized `Module` is released
//! before the next is decoded. Two things must exist before execution starts
//! and neither can be built incrementally: the closure's membership, which is
//! not membership until the last module of it has been verified, and the exact
//! executable closure and provider authority, which is fixed at that moment and
//! never widened afterwards.
//!
//! Nothing crosses a module boundary here. No export table, no pending link, no
//! name: each module is verified, reduced to its fixed-size record, and
//! released; membership is assembled from the records afterwards.

use alloc::vec::Vec;

use tos_verifier::{
    verify_image_in_closure, VerifiedArtifactEvidence, VerifiedDependencies, VerifiedModule,
};

use crate::{
    fixed_digest, resolved_module_identity, source_set_identity, ClosureModuleId, Envelope,
    Failure, ImageSnapshot, Member, Resolution, VerifiedClosureManifest, VerifiedModuleRecord,
    VerifierLimits,
};

/// The exact resolved closure a launch is handed.
///
/// An **explicit argument**, never something launch discovers: there is no
/// method here that answers "what else is there", and no path, name or pattern
/// anywhere in it. What a launch verifies is what it was given.
pub trait ClosureSource {
    /// How many modules the resolution produced.
    fn count(&self) -> usize;

    /// The image of the module at a position of the resolved closure.
    fn image(&self, position: usize) -> Option<ImageSnapshot>;
}

/// What launch produced.
#[derive(Clone, Debug)]
pub struct Launched {
    /// One fixed-size record per module, indexed by closure position.
    pub records: Vec<VerifiedModuleRecord>,
    /// The closure's membership, and the only minter of a `ClosureModuleId`.
    pub manifest: VerifiedClosureManifest,
    /// The receipt this launch's own verifier issued for the entry module.
    ///
    /// One receipt, for the module the run is reported under. It is returned
    /// rather than recomputed because the launch already held it while the entry
    /// was materialized, and a caller that had to verify the entry a second time
    /// to describe what it ran would be running the verifier twice over the same
    /// bytes. It is **not** a per-module table: the other modules leave behind
    /// their fixed-size records and nothing else.
    pub entry_receipt: VerifiedModule,
}

/// What this launch has proved so far, offered to the verifier of the next
/// module (ADR-0071 §1, §5).
///
/// **The only source of a dependency's signature in the production path.** It
/// holds one opaque, fixed-size evidence token per module already verified —
/// no export name, no signature, nothing of variable length — and reaches the
/// bytes through the same `ClosureSource` the launch was handed. The verifier
/// hashes those bytes against the token before it reads a single export.
///
/// It lives for the length of the launch and is dropped with it. Nothing here
/// becomes execution state: what survives a launch is the fixed record per
/// module and the manifest, exactly as before.
struct Verified<'a> {
    source: &'a dyn ClosureSource,
    /// `(module name, closure position, evidence)`, in verification order.
    proved: Vec<(alloc::string::String, usize, VerifiedArtifactEvidence)>,
    /// Unique `(caller, resolved dependency)` relationships seen so far
    /// (`docs/44` §2, ADR-0090).
    ///
    /// **One `usize`, and it is the whole of the enforcement.** The frontend
    /// refuses a source set above the ceiling with `E1609`, and the launch does
    /// not take its word for that: the number of authenticated dependency
    /// reopens this launch will perform is exactly this count, so the component
    /// that performs them counts them. Fixed size, alive for the launch only,
    /// and it reaches neither the module record nor the manifest.
    edges: usize,
}

impl Verified<'_> {
    fn position_of(&self, module_name: &str) -> Option<usize> {
        self.proved
            .iter()
            .find(|(name, _, _)| name == module_name)
            .map(|(_, position, _)| *position)
    }
}

impl VerifiedDependencies for Verified<'_> {
    fn states_closure(&self) -> bool {
        true
    }

    fn evidence(&self, module_name: &str) -> Option<VerifiedArtifactEvidence> {
        self.proved
            .iter()
            .find(|(name, _, _)| name == module_name)
            .map(|(_, _, evidence)| *evidence)
    }

    fn add_edges(&mut self, count: usize) -> usize {
        self.edges += count;
        self.edges
    }

    fn read_image(&self, module_name: &str, read: &mut dyn FnMut(&[u8])) {
        let Some(position) = self.position_of(module_name) else {
            return;
        };
        // The same immutable snapshot the launch verified, reached the same
        // way. Whether it is the same *bytes* is the verifier's question, and
        // it asks it with a hash rather than with this lookup.
        if let Some(image) = self.source.image(position) {
            read(&image);
        }
    }
}

/// The accepted V1 module-dependency-closure ceiling (`docs/44` §2).
///
/// Not a tunable: a profile may declare a lower bound, and a higher one is a
/// versioned contract change rather than a configuration choice.
pub const MAX_CLOSURE_MODULES: usize = 256;

/// The refusal a closure larger than the ceiling earns.
///
/// `V2001_LIMIT` because this is a published implementation ceiling and not a
/// module's own declared envelope, and the subject names the limit in the exact
/// words of the `docs/44` §2 table so an audit record can be read against it.
fn closure_too_large(count: usize, ceiling: usize) -> tos_verifier::Finding {
    tos_verifier::Finding {
        code: "V2001_LIMIT",
        location: alloc::string::String::from("module dependency closure"),
        detail: alloc::format!("{count} modules exceeds the ceiling of {ceiling}"),
        causes: Vec::new(),
    }
}

/// The set of closure positions one module transitively depends on.
///
/// **Fixed at 256 bits**, one per admissible closure position, because the
/// closure ceiling is 256 modules and a launch refuses a larger one before this
/// type is ever constructed. Thirty-two bytes whatever a module declares: a
/// module importing 256 others costs exactly what a leaf costs, so nothing here
/// is sized by an attacker-supplied count.
///
/// **It never contains the module's own position.** Dependency-first order puts
/// every dependency before its dependents, so a bit at or above the module's
/// own position cannot be set — self-exclusion is structural rather than
/// subtracted afterwards.
#[derive(Clone, Copy, Default)]
struct Reachable {
    words: [u64; 4],
}

impl Reachable {
    /// The transitive set of `module`, from the positions already proved.
    fn of(module: &tos_ir::Module, proved: &Verified<'_>, earlier: &[Reachable]) -> Reachable {
        let mut set = Reachable::default();
        for import in &module.imports {
            // A name with no position names no module this launch verified;
            // the verifier has already refused that, and counting it here would
            // be inventing a dependency rather than proving one.
            let Some(at) = proved.position_of(&import.module_name) else {
                continue;
            };
            // Two bindings of one module reach the same position, so the same
            // bit: a duplicate declaration is one dependency (ADR-0090 §2a).
            set.words[at / 64] |= 1u64 << (at % 64);
            let Some(inherited) = earlier.get(at) else {
                continue;
            };
            for (word, carried) in set.words.iter_mut().zip(inherited.words) {
                *word |= carried;
            }
        }
        set
    }

    fn count(&self) -> u32 {
        self.words.iter().map(|word| word.count_ones()).sum()
    }
}

/// The refusal a module earns for depending on more than it declared.
///
/// `V2022_RESOURCE`: the module breaches the envelope it declared for itself,
/// which is the same class as too many cleanups at one exit. It is not
/// `V2001_LIMIT`, which belongs to published implementation ceilings.
fn over_declared_imports(actual: u32, declared: u128) -> tos_verifier::Finding {
    tos_verifier::Finding {
        code: "V2022_RESOURCE",
        location: alloc::string::String::from("header.resource_envelope.imports"),
        detail: alloc::format!(
            "{actual} transitive module dependencies exceeds the declared {declared}"
        ),
        causes: Vec::new(),
    }
}

/// Verifies the exact resolved closure, sequentially, and builds the manifest.
///
/// `entry` is the position of the entry module and `entry_function` the name of
/// the function to run. The entry's function index is resolved here, while the
/// entry module is materialized, because it is the one lookup that must survive
/// the module being released.
pub fn launch(
    source: &dyn ClosureSource,
    resolution: Resolution<'_>,
    limits: &VerifierLimits,
    entry: usize,
    entry_function: &str,
) -> Result<Launched, Failure> {
    let count = source.count();
    // **The closure ceiling, before anything proportional to the closure.**
    // `docs/44` §2 publishes 256 modules, and everything below — the record
    // table, the membership vector, the reachability sets — is sized by a
    // number the provider supplies. A ceiling checked after the allocation it
    // bounds is not a ceiling, so this is the first statement of the launch.
    //
    // A profile may publish a *lower* limit and it is honoured; it may not
    // publish a higher one, because raising an accepted V1 ceiling is a
    // versioned contract change (`docs/44` §2), so the effective bound is the
    // smaller of the two.
    let effective_modules = core::cmp::min(limits.modules, MAX_CLOSURE_MODULES);
    if count > effective_modules {
        // The module named is the first position the closure cannot admit —
        // where the ceiling is crossed, exactly as ADR-0090's edge ceiling
        // reports. No image at or after it is read.
        return Err(Failure::Verifier {
            module: effective_modules,
            finding: closure_too_large(count, effective_modules),
        });
    }
    let mut records: Vec<Option<VerifiedModuleRecord>> = Vec::with_capacity(count);
    records.resize(count, None);
    // One fixed 256-bit set per closure position, and the ceiling above is what
    // makes 256 bits enough. Launch-lifetime only: it is dropped with this
    // function and reaches neither the record nor the manifest.
    let mut reachable: Vec<Reachable> = Vec::with_capacity(count);
    let mut entry_index: Option<usize> = None;
    let mut entry_receipt: Option<VerifiedModule> = None;
    let mut proved = Verified {
        source,
        proved: Vec::with_capacity(count),
        edges: 0,
    };

    for (position, slot) in records.iter_mut().enumerate() {
        let image = source.image(position).ok_or(Failure::Missing(position))?;

        // The whole trusted path in one call: the digest is taken over the
        // exact bytes that are then parsed, the parser treats them as hostile,
        // and the verifier reaches its own conclusion from what it reconstructs.
        //
        // **And the closure is dependency-first** (ADR-0071 §1), so every module
        // this one imports has already been through this loop and left evidence
        // behind. A caller that arrives before its dependency — and therefore
        // any cycle — is refused by the verifier rather than checked against a
        // declaration.
        let verified = verify_image_in_closure(&image, &resolution(position), limits, &mut proved)
            .map_err(|refusal| Failure::from_refusal(position, refusal))?;

        // **`resource imports`, exactly** (`docs/41` §6). The module's transitive
        // dependency set is the union of its unique direct dependencies and
        // theirs, and dependency-first order (ADR-0071 §1a) means every one of
        // those sets is already final. Every input is authenticated: the import
        // table comes from the artifact this launch just verified, and a name
        // resolves to a position only for a module this launch verified earlier
        // — `check_imported_calls` already refuses an import with no evidence,
        // or whose content identity disagrees with it. `ResolutionSnapshot` is
        // not consulted, so a hostile declaration cannot shrink this count.
        let reached = Reachable::of(verified.module(), &proved, &reachable);
        let declared = verified.module().header.resource_envelope.imports;
        if u128::from(reached.count()) > declared {
            return Err(Failure::Verifier {
                module: position,
                finding: over_declared_imports(reached.count(), declared),
            });
        }
        reachable.push(reached);

        let receipt = verified.receipt();
        // The control identity is a commitment to the exact pair, so a module
        // name of any conforming length — and a module name is
        // `identifier ("." identifier)*`, so it has no 128-byte ceiling —
        // becomes thirty-two bytes here rather than being refused for its size.
        *slot = Some(VerifiedModuleRecord {
            resolved_identity: resolved_module_identity(&receipt.module_name, &receipt.content_id),
            semantic_digest: fixed_digest(&receipt.module_digest),
            artifact_digest: *verified.artifact_digest(),
            verifier_identity: fixed_digest(&receipt.verifier_identity),
            content_id: fixed_digest(&receipt.content_id),
            dependency_digest: fixed_digest(&receipt.dependency_digest),
            capability_interface_digest: fixed_digest(&receipt.capability_interface_digest),
            source_map_digest: fixed_digest(&receipt.source_map_digest),
            source_set_identity: source_set_identity(&receipt.source_set),
            profile: receipt.profile,
            envelope: Envelope::of(&receipt.resource_envelope),
        });

        if position == entry {
            entry_index = verified
                .module()
                .functions
                .iter()
                .position(|function| function.signature.name == entry_function);
            entry_receipt = Some(verified.receipt().clone());
        }

        // One fixed-size token per module, and nothing else crosses to the next
        // turn: sixty-four bytes of identity, no export name and no signature.
        proved
            .proved
            .push((receipt.module_name.clone(), position, verified.evidence()));

        // The materialized module is released here, and this is the line the
        // whole flat-peak claim rests on.
        drop(verified);
    }

    let records: Vec<VerifiedModuleRecord> = records
        .into_iter()
        .map(|record| record.expect("every position was verified"))
        .collect();

    // Membership, from the identities the verifier itself produced.
    let mut members: Vec<Member> = records
        .iter()
        .enumerate()
        .map(|(position, record)| Member {
            identity: record.resolved_identity,
            position: position as u32,
        })
        .collect();
    members.sort_by_key(|member| member.identity);
    for pair in members.windows(2) {
        if pair[0].identity == pair[1].identity {
            return Err(Failure::WrongModule {
                module: pair[1].position as usize,
            });
        }
    }

    let entry_function = entry_index.ok_or(Failure::NoEntryFunction { module: entry })?;
    let entry_receipt = entry_receipt.ok_or(Failure::WrongModule { module: entry })?;
    if entry >= count {
        return Err(Failure::WrongModule { module: entry });
    }
    Ok(Launched {
        records,
        manifest: VerifiedClosureManifest {
            members,
            entry: ClosureModuleId(entry),
            entry_function,
        },
        entry_receipt,
    })
}
