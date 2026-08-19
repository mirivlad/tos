// SPDX-License-Identifier: GPL-3.0-or-later
//! The identity plane: what a derived object is allowed to stand for.
//!
//! docs/43 section 6 makes a cache a convenience that must never become a
//! canonical representation. The properties that keep it safe are checked here
//! against real lowered and verified modules:
//!
//! - the whole chain from canonical source to a running component's identity;
//! - a change to the source, a dependency, the frontend, the verifier, the
//!   schema, the backend, the ABI, the policy or the envelope changes the key,
//!   so a stale object is missed rather than reused;
//! - a substituted object fails closed;
//! - removing every object leaves the source able to regenerate the same
//!   identity.

use tos_cache::{Cache, CacheIdentity, Rejection, RunningIdentity};
use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_engine::{run, Unreachable, Value};
use tos_ir::Module;
use tos_verifier::{verify, Limits, ResolutionSnapshot, VerifiedModule};

const ENGINE: &str = "tos-engine-reference/0.1.0";

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}

/// Source, checked, lowered, verified — the inputs the identity is read from.
fn build(body: &str) -> (Module, VerifiedModule) {
    let text = format!(
        "module app.sample version 1.0 profile bootstrap; \
         resource [fuel: 10000, stack: 64KiB, allocation: 4KiB, tasks: 1, workers: 1, \
         sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0] {body}"
    );
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("source parses");
    assert!(Checker::check(&source, &schema).is_empty());
    let context = ModuleContext {
        source_set: String::from("tos-cache-tests"),
        path: String::from("app/sample.tos"),
        content_id: content_id(text.as_bytes()),
        dependency_digest: String::from("sha256:0000"),
        capability_interface_digest: String::from("sha256:0000"),
    };
    let module = lower_module(&source, &schema, &context).expect("source lowers");
    let receipt = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("lowered IR verifies");
    (module, receipt)
}

fn identity_of(module: &Module, receipt: &VerifiedModule) -> CacheIdentity {
    CacheIdentity::of(
        module,
        receipt,
        vec![(String::from("app.helper"), String::from("sha256:aaaa"))],
        "tos-engine-reference",
        "reference-abi/64",
        "policy/safe-v1",
    )
}

const BODY: &str = "pub fn answer() -> i32 { return 42i32; }";

#[test]
fn the_identity_chain_reaches_from_canonical_source_to_a_running_component() {
    let (module, receipt) = build(BODY);
    let identity = identity_of(&module, &receipt);
    let running = RunningIdentity::of(&module, &receipt, &identity, ENGINE);

    // Every link of docs/37's Stage 2 chain is present and connected.
    assert_eq!(running.canonical_path, "app/sample.tos");
    assert_eq!(running.content_id, module.header.content_id);
    assert_eq!(running.frontend_identity, tos_core::FRONTEND_IDENTITY);
    assert_eq!(running.verifier_identity, tos_verifier::VERIFIER_IDENTITY);
    assert_eq!(running.engine_identity, ENGINE);
    assert_eq!(running.module_digest, tos_ir::module_digest(&module));
    assert_eq!(running.cache_key, identity.key());
    assert!(running.source_map_digest.starts_with("sha256:"));

    // And the thing it identifies actually runs.
    let outcome = run(&module, &receipt, "answer", vec![], &mut Unreachable)
        .expect("the entry exists")
        .expect("no trap");
    assert_eq!(outcome.value, Value::Int(tos_ir::IntKind::I32, 42));
}

#[test]
fn an_admitted_object_is_found_under_its_own_identity() {
    let (module, receipt) = build(BODY);
    let identity = identity_of(&module, &receipt);
    let mut cache = Cache::new();
    cache.admit(identity.clone(), receipt.clone());
    let entry = cache
        .lookup(&identity, &module)
        .expect("the object is found");
    assert_eq!(entry.receipt, receipt);
}

#[test]
fn changing_the_source_invalidates_the_object() {
    let (module, receipt) = build(BODY);
    let identity = identity_of(&module, &receipt);
    let mut cache = Cache::new();
    cache.admit(identity, receipt);

    let (changed, changed_receipt) = build("pub fn answer() -> i32 { return 43i32; }");
    let changed_identity = identity_of(&changed, &changed_receipt);
    assert_eq!(
        cache.lookup(&changed_identity, &changed),
        Err(Rejection::Miss),
        "a changed source must not find the old object"
    );
}

#[test]
fn changing_a_dependency_invalidates_the_object() {
    let (module, receipt) = build(BODY);
    let mut cache = Cache::new();
    cache.admit(identity_of(&module, &receipt), receipt.clone());

    let mut moved = identity_of(&module, &receipt);
    moved.dependency_closure = vec![(String::from("app.helper"), String::from("sha256:bbbb"))];
    assert_eq!(cache.lookup(&moved, &module), Err(Rejection::Miss));
}

#[test]
fn every_declared_identity_field_participates_in_the_key() {
    // docs/43 section 6 lists what a key must contain. Each field is changed
    // one at a time, and each change must move the key: a field that did not
    // would let a stale object survive a change that altered its meaning.
    let (module, receipt) = build(BODY);
    let base = identity_of(&module, &receipt);
    let key = base.key();

    type Mutation = (&'static str, Box<dyn Fn(&mut CacheIdentity)>);
    let mutations: Vec<Mutation> = vec![
        (
            "content id",
            Box::new(|id: &mut CacheIdentity| id.content_id = String::from("sha256:x")),
        ),
        (
            "dependency closure",
            Box::new(|id: &mut CacheIdentity| id.dependency_closure.clear()),
        ),
        (
            "source set",
            Box::new(|id: &mut CacheIdentity| id.source_set = String::from("other")),
        ),
        (
            "module name",
            Box::new(|id: &mut CacheIdentity| id.module_name = String::from("other")),
        ),
        (
            "canonical path",
            Box::new(|id: &mut CacheIdentity| id.canonical_path = String::from("o.tos")),
        ),
        (
            "frontend",
            Box::new(|id: &mut CacheIdentity| id.frontend_identity = String::from("other")),
        ),
        (
            "language version",
            Box::new(|id: &mut CacheIdentity| id.language_version = String::from("2.0")),
        ),
        (
            "feature revision",
            Box::new(|id: &mut CacheIdentity| id.feature_revision = String::from("r2")),
        ),
        (
            "unicode baseline",
            Box::new(|id: &mut CacheIdentity| id.unicode_baseline = String::from("other")),
        ),
        (
            "ir schema",
            Box::new(|id: &mut CacheIdentity| id.ir_schema = String::from("tos-ir/v2")),
        ),
        (
            "source map revision",
            Box::new(|id: &mut CacheIdentity| id.source_map_revision = String::from("v2")),
        ),
        (
            "verifier",
            Box::new(|id: &mut CacheIdentity| id.verifier_identity = String::from("other")),
        ),
        (
            "backend",
            Box::new(|id: &mut CacheIdentity| id.backend_identity = String::from("other")),
        ),
        (
            "target abi",
            Box::new(|id: &mut CacheIdentity| id.target_abi = String::from("other")),
        ),
        (
            "policy",
            Box::new(|id: &mut CacheIdentity| id.policy_identity = String::from("other")),
        ),
        (
            "resource envelope",
            Box::new(|id: &mut CacheIdentity| {
                id.resource_envelope_digest = String::from("sha256:x")
            }),
        ),
        (
            "capability contract",
            Box::new(|id: &mut CacheIdentity| {
                id.capability_contract_digest = String::from("sha256:x")
            }),
        ),
    ];
    for (name, mutate) in mutations {
        let mut altered = base.clone();
        mutate(&mut altered);
        assert_ne!(
            altered.key(),
            key,
            "changing the {name} must change the key"
        );
    }
}

#[test]
fn a_substituted_object_fails_closed() {
    // docs/44 section 3 requires a cache-substitution negative. The object is
    // placed under a key that is not its own, which is exactly what a
    // substitution looks like from the store's side.
    let (module, receipt) = build(BODY);
    let (other, other_receipt) = build("pub fn answer() -> i32 { return 7i32; }");
    let identity = identity_of(&module, &receipt);
    let mut cache = Cache::new();
    cache.place_under_key(
        &identity.key(),
        tos_cache::Entry {
            identity: identity_of(&other, &other_receipt),
            receipt: other_receipt,
        },
    );
    assert_eq!(
        cache.lookup(&identity, &module),
        Err(Rejection::KeyDoesNotMatchIdentity),
        "an object under a key it does not hash to must be refused"
    );
}

#[test]
fn a_receipt_for_another_module_fails_closed() {
    let (module, receipt) = build(BODY);
    let (_, other_receipt) = build("pub fn answer() -> i32 { return 7i32; }");
    let identity = identity_of(&module, &receipt);
    let mut cache = Cache::new();
    cache.admit(identity.clone(), other_receipt);
    assert_eq!(
        cache.lookup(&identity, &module),
        Err(Rejection::ReceiptDoesNotMatchModule)
    );
}

#[test]
fn removing_every_object_leaves_the_source_able_to_regenerate() {
    // docs/43 section 1: deleting the cache costs work, never functionality.
    let (module, receipt) = build(BODY);
    let identity = identity_of(&module, &receipt);
    let mut cache = Cache::new();
    cache.admit(identity.clone(), receipt.clone());
    assert!(!cache.is_empty());

    cache.clear();
    assert!(cache.is_empty());
    assert_eq!(cache.lookup(&identity, &module), Err(Rejection::Miss));

    // Regenerating from the same canonical source reproduces the same
    // identity, the same receipt and the same result.
    let (again, again_receipt) = build(BODY);
    let again_identity = identity_of(&again, &again_receipt);
    assert_eq!(again_identity.key(), identity.key());
    assert_eq!(again_receipt.module_digest, receipt.module_digest);
    let outcome = run(&again, &again_receipt, "answer", vec![], &mut Unreachable)
        .expect("the entry exists")
        .expect("no trap");
    assert_eq!(outcome.value, Value::Int(tos_ir::IntKind::I32, 42));
}
