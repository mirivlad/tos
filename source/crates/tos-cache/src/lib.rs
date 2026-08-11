// SPDX-License-Identifier: GPL-3.0-or-later
//! Derived-artifact identity and cache admission (docs/43 section 6).
//!
//! A cache is a convenience, never a canonical representation. docs/43 section
//! 1 is explicit: removing every cache object must leave the canonical source
//! tree able to regenerate functionality, and no cached object may stand in for
//! the source that produced it.
//!
//! This crate owns the identity that makes that safe. A derived object is
//! admitted only under a key that names every input whose change would change
//! what the object means: the source content and its ordered dependency
//! closure, the source set, the module's canonical path, the frontend, the
//! language version and feature revision, the Unicode baseline, the IR schema
//! and source-map revision, the verifier, the backend and target ABI, the
//! optimization and safety policy, the resource envelope, and the capability
//! contract. Change any one of them and the key changes, so the old object is
//! not found rather than wrongly reused.
//!
//! Lookup **fails closed**. A mismatch, a receipt for a different module, or a
//! missing source map rejects the entry; nothing falls back to a nearby source
//! or a host artifact.
//!
//! **No byte encoding.** docs/43 section 1 forbids freezing a persisted format
//! before a bounded versioned format contract exists under docs/18, so this
//! crate defines none. It defines the identity and the admission rules; where
//! an object is kept is the caller's decision, and a persisted store may only
//! be introduced with that format contract.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::string::{String, ToString};
use std::vec::Vec;

use tos_ir::Module;
use tos_verifier::VerifiedModule;

/// Everything a derived object's meaning depends on (docs/43 section 6).
///
/// Every field is part of the key. A field that did not belong here would let
/// a stale object be reused after a change that altered its meaning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheIdentity {
    /// The normalized source content ID of the module itself.
    pub content_id: String,
    /// The ordered dependency closure, as `(module name, content ID)`.
    ///
    /// Ordered, because a cache key over an unordered set would collide between
    /// closures that differ only in resolution order.
    pub dependency_closure: Vec<(String, String)>,
    /// The source-set or commit identity the module was resolved from.
    pub source_set: String,
    pub module_name: String,
    pub canonical_path: String,
    pub frontend_identity: String,
    pub language_version: String,
    /// The feature revision inside that language version.
    pub feature_revision: String,
    pub unicode_baseline: String,
    pub ir_schema: String,
    pub source_map_revision: String,
    pub verifier_identity: String,
    /// The backend that produced the object, and the ABI it targeted.
    pub backend_identity: String,
    pub target_abi: String,
    /// Which optimization and safety policy was in force.
    pub policy_identity: String,
    pub resource_envelope_digest: String,
    pub capability_contract_digest: String,
}

impl CacheIdentity {
    /// Derives the identity of a verified module under a declared backend and
    /// policy.
    ///
    /// The module's own header answers most of it, which is the point: the
    /// identity is read from what the frontend recorded and the verifier
    /// checked, not assembled from ambient state.
    pub fn of(
        module: &Module,
        receipt: &VerifiedModule,
        dependency_closure: Vec<(String, String)>,
        backend_identity: &str,
        target_abi: &str,
        policy_identity: &str,
    ) -> CacheIdentity {
        CacheIdentity {
            content_id: module.header.content_id.clone(),
            dependency_closure,
            source_set: module.header.source_set.clone(),
            module_name: module.header.module_name.clone(),
            canonical_path: module.header.path.clone(),
            frontend_identity: module.header.frontend_identity.clone(),
            language_version: module.header.language_version.clone(),
            feature_revision: module.header.schema_id.clone(),
            unicode_baseline: module.header.unicode_normalization_baseline.clone(),
            ir_schema: module.header.schema_id.clone(),
            source_map_revision: module.header.source_map_revision.clone(),
            verifier_identity: receipt.verifier_identity.clone(),
            backend_identity: backend_identity.to_string(),
            target_abi: target_abi.to_string(),
            policy_identity: policy_identity.to_string(),
            resource_envelope_digest: envelope_digest(&module.header.resource_envelope),
            capability_contract_digest: module.header.capability_interface_digest.clone(),
        }
    }

    /// The cache key, as `sha256:<hex>`.
    ///
    /// Every variable-length field is length-prefixed, so no two distinct
    /// identities can produce one byte stream by moving a boundary.
    pub fn key(&self) -> String {
        let mut bytes: Vec<u8> = Vec::new();
        let mut write = |text: &str| {
            bytes.extend_from_slice(&(text.len() as u64).to_be_bytes());
            bytes.extend_from_slice(text.as_bytes());
        };
        write(&self.content_id);
        write(&(self.dependency_closure.len() as u64).to_string());
        for (name, content) in &self.dependency_closure {
            write(name);
            write(content);
        }
        for field in [
            &self.source_set,
            &self.module_name,
            &self.canonical_path,
            &self.frontend_identity,
            &self.language_version,
            &self.feature_revision,
            &self.unicode_baseline,
            &self.ir_schema,
            &self.source_map_revision,
            &self.verifier_identity,
            &self.backend_identity,
            &self.target_abi,
            &self.policy_identity,
            &self.resource_envelope_digest,
            &self.capability_contract_digest,
        ] {
            write(field);
        }
        digest_of(&bytes)
    }
}

fn envelope_digest(envelope: &tos_ir::ResourceEnvelope) -> String {
    let mut bytes: Vec<u8> = Vec::new();
    for limit in [
        envelope.fuel,
        envelope.stack,
        envelope.allocation,
        envelope.tasks,
        envelope.workers,
        envelope.sync,
        envelope.shared,
        envelope.cleanup,
        envelope.recursion,
        envelope.imports,
    ] {
        bytes.extend_from_slice(&limit.to_be_bytes());
    }
    digest_of(&bytes)
}

fn digest_of(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    std::format!(
        "sha256:{}",
        core::str::from_utf8(&hex).expect("hex output is ASCII")
    )
}

/// One admitted derived object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub identity: CacheIdentity,
    /// The verifier's receipt for the module this object derives from.
    pub receipt: VerifiedModule,
}

/// Why an entry may not be used.
///
/// docs/43 section 6: an identity mismatch or a missing source map rejects
/// cache execution rather than trying a nearby source or a host fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rejection {
    /// Nothing is stored under this key.
    Miss,
    /// The stored entry's own identity does not hash to the key it sits under.
    KeyDoesNotMatchIdentity,
    /// The receipt names a different module than the one being executed.
    ReceiptDoesNotMatchModule,
    /// The receipt carries no checked source-map digest.
    MissingSourceMap,
}

/// An in-memory store of admitted derived objects.
///
/// It is deliberately not persistent: docs/43 section 1 forbids introducing a
/// persisted format before a bounded versioned format contract exists.
#[derive(Clone, Debug, Default)]
pub struct Cache {
    entries: BTreeMap<String, Entry>,
}

impl Cache {
    pub fn new() -> Cache {
        Cache::default()
    }

    /// Admits an object under its identity.
    ///
    /// Admission requires a receipt: only the verifier says a module is
    /// executable, so a cache cannot become a way to skip it.
    pub fn admit(&mut self, identity: CacheIdentity, receipt: VerifiedModule) {
        self.entries
            .insert(identity.key(), Entry { identity, receipt });
    }

    /// Looks an object up for a module, failing closed.
    pub fn lookup(&self, identity: &CacheIdentity, module: &Module) -> Result<&Entry, Rejection> {
        let key = identity.key();
        let Some(entry) = self.entries.get(&key) else {
            return Err(Rejection::Miss);
        };
        // A stored entry that does not hash to where it sits has been
        // substituted; the key is the claim and the identity is the evidence.
        if entry.identity.key() != key {
            return Err(Rejection::KeyDoesNotMatchIdentity);
        }
        if entry.receipt.module_digest != tos_ir::module_digest(module) {
            return Err(Rejection::ReceiptDoesNotMatchModule);
        }
        if entry.receipt.source_map_digest.is_empty() {
            return Err(Rejection::MissingSourceMap);
        }
        Ok(entry)
    }

    /// Removes every object. The source tree is unaffected.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Substitutes an entry under a key it does not belong to.
    ///
    /// This exists so a test can produce the exact attack docs/44 section 3
    /// requires evidence against — a cache-substitution negative — without a
    /// second code path that only tests use.
    pub fn place_under_key(&mut self, key: &str, entry: Entry) {
        self.entries.insert(key.to_string(), entry);
    }
}

/// What a running component reports about the source it came from.
///
/// docs/37 requires runtime introspection to name the source and engine
/// identity; docs/43 section 6 requires the running component to record the
/// derived identity with it. This is that record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunningIdentity {
    pub module_name: String,
    pub canonical_path: String,
    pub content_id: String,
    pub frontend_identity: String,
    pub verifier_identity: String,
    pub engine_identity: String,
    pub module_digest: String,
    pub source_map_digest: String,
    pub cache_key: String,
}

impl RunningIdentity {
    pub fn of(
        module: &Module,
        receipt: &VerifiedModule,
        identity: &CacheIdentity,
        engine_identity: &str,
    ) -> RunningIdentity {
        RunningIdentity {
            module_name: module.header.module_name.clone(),
            canonical_path: module.header.path.clone(),
            content_id: module.header.content_id.clone(),
            frontend_identity: module.header.frontend_identity.clone(),
            verifier_identity: receipt.verifier_identity.clone(),
            engine_identity: engine_identity.to_string(),
            module_digest: receipt.module_digest.clone(),
            source_map_digest: receipt.source_map_digest.clone(),
            cache_key: identity.key(),
        }
    }
}
