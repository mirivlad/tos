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

use tos_verifier::{verify_image, VerifiedModule};

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
    let mut records: Vec<Option<VerifiedModuleRecord>> = Vec::with_capacity(count);
    records.resize(count, None);
    let mut entry_index: Option<usize> = None;
    let mut entry_receipt: Option<VerifiedModule> = None;

    for (position, slot) in records.iter_mut().enumerate() {
        let image = source.image(position).ok_or(Failure::Missing(position))?;

        // The whole trusted path in one call: the digest is taken over the
        // exact bytes that are then parsed, the parser treats them as hostile,
        // and the verifier reaches its own conclusion from what it reconstructs.
        let verified = verify_image(&image, &resolution(position), limits)
            .map_err(|refusal| Failure::from_refusal(position, refusal))?;

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
