// SPDX-License-Identifier: GPL-3.0-or-later
//! A schema record's fields decide the language minor a module naming it needs.
//!
//! ADR-0098 §2a. `system.ipc.ReceivedCallRegion` has an `Option<Region<u8>>`
//! field, and `Region<u8>` is the `RegionFamily` representation that ADR-0097
//! added at TOS Core 1.5. A module that names the record **holds** a region
//! without writing one down, so the minor cannot be decided by walking written
//! types alone — which is the hole this pair of tests exists to keep closed.
//!
//! **Two enforcers, and the second is the point.** The frontend refuses the
//! source, and `tos-core`'s own tests cover that. What is proved here is that the
//! **independent verifier** refuses the *artifact*: ADR-0085 §13 and §17.7 make
//! the minor a verifier obligation because a hand-written artifact never met a
//! frontend, and an artifact is exactly what this test damages.

use tos_verifier::{verify, Limits, ResolutionSnapshot};

/// A module that serves one call carrying a region.
///
/// It writes no region type anywhere: the only region it could ever hold is the
/// one the record hands it.
const SERVES_A_CALL_WITH_A_REGION: &str = "\
module system.test.recordregion version 1.5 profile full;
import capability system.ipc.Endpoint as serve;

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

extern fn endpoint_receive_call_region(
    cap: system.ipc.Endpoint
) -> Result<system.ipc.ReceivedCallRegion, i64> uses [serve];

pub fn main() -> i64 uses [serve] {
    match (endpoint_receive_call_region(serve)) {
        Ok(request) => { return 1i64; }
        Err(status) => { return status; }
    }
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
        "a 1.5 module naming the record checks clean: {diagnostics:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("record-region-minor-test"),
            path: String::from("system/test/recordregion.tos"),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

#[test]
fn the_record_puts_a_region_in_the_artifacts_type_table() {
    let module = lower(SERVES_A_CALL_WITH_A_REGION);
    // **The fact the minor rule is about.** Nothing in the source wrote
    // `Region<u8>`; the record's field did, and the table is where it landed.
    assert!(
        module
            .types
            .iter()
            .any(|ty| matches!(ty, tos_ir::TypeDef::Region(_))),
        "naming the record interns its region field: {:?}",
        module.types
    );
}

#[test]
fn a_module_naming_the_record_verifies_at_the_minor_that_added_regions() {
    let module = lower(SERVES_A_CALL_WITH_A_REGION);
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("a 1.5 artifact holding a region verifies");
}

#[test]
fn an_artifact_claiming_an_earlier_minor_is_refused_by_the_verifier() {
    let mut module = lower(SERVES_A_CALL_WITH_A_REGION);
    // **The artifact a frontend would never emit.** Its body is unchanged and
    // every other rule still holds: the import is declared, the operation is
    // real, the effect is present, the interface is named correctly. The one
    // thing wrong with it is its version — which is what a hand-written
    // artifact gets wrong, and why the verifier keeps this rule of its own.
    module.header.language_version = String::from("1.4");
    // The source map records the language contract too, and an entry that
    // disagreed with the header would be refused for *that* — an earlier and
    // different defect. A hand-written 1.4 artifact is consistent about being
    // 1.4, which is exactly what makes its one real problem the region.
    for entry in &mut module.source_map {
        entry.language_version = String::from("1.4");
    }
    let finding = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("a 1.4 artifact whose record field holds a region is refused");
    assert_eq!(finding.code, "V2010_TYPE");
    assert!(
        finding
            .detail
            .contains("a record field holds the capability representation of TOS Core 1.5"),
        "{finding:?}"
    );
    assert!(finding.detail.contains("1.4"), "{finding:?}");
}

#[test]
fn a_module_holding_no_region_is_unaffected_by_the_rule() {
    // The same shape one minor earlier, naming the record that has no region
    // field. It must still verify at 1.4: a rule that refused every artifact
    // below 1.5 would be a rule about the version rather than about what the
    // artifact holds.
    let text = SERVES_A_CALL_WITH_A_REGION
        .replace("version 1.5", "version 1.4")
        .replace("endpoint_receive_call_region", "endpoint_receive_call")
        .replace("system.ipc.ReceivedCallRegion", "system.ipc.ReceivedCall");
    let module = lower(&text);
    assert!(
        !module
            .types
            .iter()
            .any(|ty| matches!(ty, tos_ir::TypeDef::Region(_))),
        "the region-free record interns no region"
    );
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("a 1.4 artifact holding no region verifies");
}
