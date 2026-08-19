// SPDX-License-Identifier: GPL-3.0-or-later
//! A module reaches an accepted interface, and the artifact says so.
//!
//! ADR-0060 and `SYSTEM_INTERFACE_V1`. What is checked here is the seam the
//! decision turns on: a module declares an operation of an interface it
//! requested, calls it, and the lowered IR carries **the interface path** —
//! both on the instruction that leaves and on the effects of the function that
//! made the call. A verifier reading that artifact learns which interfaces the
//! module reaches without executing any of it.
//!
//! The refusals matter as much as the acceptance, and they are proved by
//! damaging the artifact rather than by damaging the source: a verifier that
//! only ever sees what this frontend emits proves nothing about a frontend
//! somebody else wrote.
//!
//! **The module is `profile full`, and it has to be.** docs/42 §3 forbids
//! `extern` in Bootstrap, so a module that reaches the system is a Full module
//! by that document's decision and not by this test's convenience — which also
//! means the canonical boot text, which is Bootstrap, can never call an
//! operation. That is a consequence worth seeing early rather than discovering
//! when a supervisor is written.

use tos_ir::Op;
use tos_verifier::{verify, Limits, ResolutionSnapshot};

const MODULE: &str = "\
module system.test.reach version 1.0 profile full;
import capability system.ipc.Endpoint as endpoint;

resource [
    fuel: 1024,
    stack: 4KiB,
    allocation: 1KiB,
    tasks: 1,
    workers: 1,
    sync: 0,
    shared: 0B,
    cleanup: 0,
    recursion: 4,
    imports: 1
]

extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [endpoint];

pub fn main(cap: system.ipc.Endpoint) -> i64 uses [endpoint] {
    return endpoint_send(cap, 8u64);
}
";

fn lower(text: &str) -> tos_ir::Module {
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    let diagnostics = tos_core::Checker::check(&source, &schema);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.severity() == tos_core::Severity::Error),
        "a module reaching a declared operation checks clean: {diagnostics:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("interface-schema-test"),
            path: String::from("system/test/reach.tos"),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

#[test]
fn the_artifact_names_the_interface_it_reaches() {
    let module = lower(MODULE);

    // The effect is the **interface path**, not the name the source bound it
    // to. A binding name means something only inside the module that wrote it,
    // and a reader of the artifact has no way to learn what one referred to.
    let entry = module
        .functions
        .iter()
        .find(|function| function.signature.name == "main")
        .expect("the entry is in the artifact");
    assert_eq!(entry.signature.effects, vec!["system.ipc.Endpoint"]);

    // And the instruction that leaves says which interface it leaves through,
    // which is docs/43 section 3's "accepted interface ID".
    let reaching: Vec<&str> = entry
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| instruction.unsafe_interface.as_deref())
        .collect();
    assert_eq!(reaching, vec!["system.ipc.Endpoint"]);

    // It is a call, and it is not a call to anything of this module: an
    // operation of an interface is reached, not defined here.
    assert!(entry
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| instruction.unsafe_interface.is_some()
            && matches!(&instruction.op, Op::Call { .. })));

    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("a module reaching an interface it imported and declared verifies");
}

#[test]
fn reaching_an_interface_the_module_never_imported_is_refused() {
    let mut module = lower(MODULE);
    // The artifact now claims to reach something nobody asked for. A frontend
    // that emitted this would be granting authority by writing a string.
    module.capability_imports.clear();
    let finding = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("an interface reached but never imported is refused");
    assert_eq!(finding.code, "V2033_UNSAFE");
    assert!(finding.detail.contains("never imported"), "{finding:?}");
}

#[test]
fn reaching_an_interface_the_function_never_declared_is_refused() {
    let mut module = lower(MODULE);
    // The call stays; the declaration it was permitted by is removed. docs/42
    // section 2 requires the enclosing `uses` effect to match, and this is that
    // requirement proved against the artifact rather than against the source.
    for function in &mut module.functions {
        function.signature.effects.clear();
    }
    let finding = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("an interface reached without being declared is refused");
    assert_eq!(finding.code, "V2033_UNSAFE");
    assert!(
        finding.detail.contains("without being declared"),
        "{finding:?}"
    );
}
