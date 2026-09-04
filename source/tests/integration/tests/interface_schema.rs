// SPDX-License-Identifier: GPL-3.0-or-later
//! A module reaches an accepted interface, and the artifact says so.
//!
//! ADR-0060 and `SYSTEM_INTERFACE_V1`. What is checked here is the seam the
//! decision turns on: a module declares an operation of an interface it
//! requested, calls it **on the name it bound that request to** (ADR-0061), and
//! the lowered IR carries **the interface path** —
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

pub fn main() -> i64 uses [endpoint] {
    return endpoint_send(endpoint, 8u64);
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

/// Whether a written type is one of the **memory** region kinds.
///
/// `Region` and `DmaRegion` are what `docs/42` §2 is about: a grant whose
/// interface must declare element type, alignment, access, size, DMA
/// domain, lifetime and transfer rules, none of which any schema here
/// declares.
///
/// **`MmioRegion` is deliberately not one of them** (ADR-0081 §5). It is a
/// separate sealed kind: it has no element type, nothing funds it, nothing
/// reclaims it to the pool, and the facts a device window needs — the two
/// access forms, the page-granular extent, and a lifetime tied to the PCI
/// assignment it descends from — *are* declared, by `PLATFORM_INTERFACE_V1`.
/// Matching on the shared word would make this assertion about spelling
/// rather than about the rule.
fn names_a_memory_region(ty: &str) -> bool {
    // **Keyed on the type constructor, not on a substring.** The written type
    // is split into whole names — `<`, `>`, `,` and spaces separate them, and a
    // `.` does not — and each is compared entire. So:
    //
    //   `MmioRegion`          a different constructor, not a prefixed `Region`
    //   `platform.mmio.Region` an interface path, whose last segment is not a
    //                          type constructor at all
    //
    // Both are differences a `contains` would erase, and both are the rule's
    // actual subject: an interface must not acquire *ordinary memory* region
    // semantics without declaring the contract `docs/42` §2 requires.
    ty.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.')
        .any(|name| matches!(name, "Region" | "DmaRegion" | "region"))
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

    // And it is an operation on a declared capability import, not a call to
    // anything of this module: `tos-ir/v1` reserved `Op::Capability` for exactly
    // "an operation on a declared imported capability", and ADR-0061 makes the
    // import the thing the operation is performed under.
    assert!(entry
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|instruction| instruction.unsafe_interface.is_some()
            && matches!(&instruction.op, Op::Capability { .. })));

    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("a module reaching an interface it imported and declared verifies");
}

#[test]
fn an_operation_performed_under_a_request_nobody_made_is_refused() {
    let mut module = lower(MODULE);
    // The artifact now performs an operation under an import that is not there.
    // A frontend that emitted this would be granting authority by writing an
    // index, which is why the index is bounds-checked against the module's own
    // table rather than trusted.
    module.capability_imports.clear();
    let finding = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("an operation under an import nobody declared is refused");
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(finding.detail.contains("outside the table"), "{finding:?}");
}

#[test]
fn an_operation_that_names_one_interface_and_acts_under_another_is_refused() {
    let mut module = lower(MODULE);
    // The instruction says two things about which interface it reaches: the
    // import it acts under, and the accepted interface ID docs/43 section 3 asks
    // it to carry. This makes them disagree, which every other check would let
    // through — the import is in range, the interface was imported, the function
    // declared it — while the operation is performed on authority of the wrong
    // type.
    for function in &mut module.functions {
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                if instruction.unsafe_interface.is_some() {
                    instruction.unsafe_interface = Some(String::from("system.ipc.Reply"));
                }
            }
        }
    }
    let finding = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("an operation naming one interface and acting under another is refused");
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(
        finding.detail.contains("is performed through"),
        "{finding:?}"
    );
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
    // Reported per capability position since ADR-0078: the rule is about every
    // authority an operation acts through, not only the first, so the finding
    // says which position failed it.
    assert!(finding.detail.contains("does not declare"), "{finding:?}");
}

#[test]
fn no_accepted_interface_admits_a_region() {
    // `IPC_V1` §9.6 asks for evidence that a region transferred linearly is
    // unmapped from the sender. This is the evidence that the question does not
    // arise in this contract version, which the accepted documents decide
    // between them rather than leave to an implementation:
    //
    //   - `IPC_V1` §5 makes the mode a property of a declaration: a region
    //     "leaves the sender's address space at transfer, **if the interface
    //     declares the transfer linear**";
    //   - `SYSTEM_INTERFACE_V1` §8 declares no region operation at all, and says
    //     why: `docs/42` §2 requires a region grant to originate through an
    //     operation whose interface declares element type, alignment, access,
    //     size, DMA domain, lifetime and transfer rules.
    //
    // So no interface declares a region operation, so no region originates, so
    // nothing travels linearly or otherwise. That is stricter than refusing a
    // message with too many regions, and it is checked here rather than asserted
    // in prose — a schema that quietly grew a region parameter would make §9.6
    // reachable and unevidenced in the same commit.
    for interface in tos_core::interfaces::ACCEPTED {
        assert_ne!(
            interface.object,
            tos_core::interfaces::ObjectKind::Region,
            "{} names a region, so a capability of it would be a region grant",
            interface.path
        );
        for operation in interface.operations {
            for parameter in operation.parameters {
                assert!(
                    !names_a_memory_region(parameter.ty),
                    "{}::{} takes {}, so a region crosses an interface that declares no rules for one",
                    interface.path,
                    operation.name,
                    parameter.ty
                );
            }
            // §5 no longer fixes every result at `i64` — an operation returns
            // the semantic value it produced, and `Result<T, i64>` is the
            // refusal model. What is still fixed is that a region is not one of
            // those values, for the same reason it is not a parameter: the
            // seven facts `docs/42` §2 requires a region grant's interface to
            // declare are declared nowhere in this schema.
            assert!(
                !names_a_memory_region(operation.result),
                "{}::{} returns {}, so a region originates through an interface that \
                 declares no rules for one",
                interface.path,
                operation.name,
                operation.result
            );
        }
    }
}

/// A module reaching the two-capability operation, with a body the caller picks.
///
/// Two imports, two bindings, two rights — and the call names each by its own
/// name. That is ADR-0063's whole shape: nothing here derives one capability
/// from the other, and nothing merges them.
fn two_capability_module(call: &str) -> String {
    format!(
        "module system.test.reach version 1.0 profile full;
import capability system.ipc.Reply as answer;
import capability system.ipc.Endpoint as inbox;

resource [
    fuel: 1024, stack: 4KiB, allocation: 1KiB, tasks: 1, workers: 1,
    sync: 0, shared: 0B, cleanup: 0, recursion: 4, imports: 2
]

extern fn endpoint_reply_receive(
    reply: system.ipc.Reply,
    on: system.ipc.Endpoint,
    length: u64
) -> i64 uses [answer, inbox];

pub fn main() -> i64 uses [answer, inbox] {{
    return {call};
}}
"
    )
}

#[test]
fn an_operation_may_require_two_capabilities_named_separately() {
    let module = lower(&two_capability_module(
        "endpoint_reply_receive(answer, inbox, 8u64)",
    ));
    let entry = module
        .functions
        .iter()
        .find(|function| function.signature.name == "main")
        .expect("the entry is in the artifact");
    let reaching: Vec<_> = entry
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match &instruction.op {
            Op::Capability {
                capabilities,
                right,
                ..
            } => Some((capabilities.clone(), right.clone())),
            _ => None,
        })
        .collect();

    // The first capability is the operation's own interface — the one the
    // instruction records — and the second is a separate import index. Neither
    // is an operand: there is no capability anywhere in the artifact.
    assert_eq!(
        reaching,
        vec![(
            vec![
                tos_ir::CapabilitySource::Import(0),
                tos_ir::CapabilitySource::Import(1)
            ],
            String::from("endpoint_reply_receive")
        )]
    );
    assert_eq!(module.capability_imports[0].binding, "answer");
    assert_eq!(module.capability_imports[0].interface, "system.ipc.Reply");
    assert_eq!(module.capability_imports[1].binding, "inbox");
    assert_eq!(
        module.capability_imports[1].interface,
        "system.ipc.Endpoint"
    );
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("an operation naming both its capabilities verifies");
}

#[test]
fn substituting_one_capability_for_the_other_is_refused() {
    // The same two imports, passed the other way round. The checker refuses it
    // before anything is lowered, because a capability parameter's interface is
    // declared and `system.ipc.Endpoint` is not `system.ipc.Reply` — which is
    // what keeps "reply here and wait there" from becoming "wait here and reply
    // there" by writing the arguments in a different order.
    let text = two_capability_module("endpoint_reply_receive(inbox, answer, 8u64)");
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    let diagnostics = tos_core::Checker::check(&source, &schema);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.severity() == tos_core::Severity::Error),
        "passing the endpoint where the reply belongs was accepted"
    );
}

#[test]
fn one_capability_cannot_stand_in_for_two() {
    // An artifact naming the same import twice would be one grant doing the work
    // of two authorities, which is exactly what ADR-0063 forbids: the two are
    // separate, separately granted and separately attenuable. Damaged here
    // rather than written, because a frontend that emitted it is the thing the
    // verifier exists to catch.
    let mut module = lower(&two_capability_module(
        "endpoint_reply_receive(answer, inbox, 8u64)",
    ));
    for function in &mut module.functions {
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                if let Op::Capability { capabilities, .. } = &mut instruction.op {
                    let first = capabilities[0].clone();
                    capabilities[1] = first;
                }
            }
        }
    }
    let finding = verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("one import standing in for two is refused");
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(finding.detail.contains("more than once"), "{finding:?}");
}

/// The rule §8's assertion rests on, exercised directly (Stage 4B closure).
///
/// The invariant is **not** "no accepted type may contain the word Region". It
/// is that no accepted interface silently acquires ordinary `Region`/
/// `DmaRegion` semantics without declaring the complete memory-region contract
/// `docs/42` §2 requires — element type, alignment, access, size, DMA domain,
/// lifetime and transfer rules.
///
/// The spellings below are the canonical ones `boundary::type_text` produces,
/// so this is a test of the classification the schema check actually performs
/// rather than of a paraphrase of it.
#[test]
fn memory_regions_are_classified_by_constructor_not_by_spelling() {
    // Ordinary and DMA memory, in every granted mode: these must be rejected by
    // the interface rule, because no schema declares what a grant of one means.
    for ty in [
        "Region<u8>",
        "Region<mut u8>",
        "DmaRegion<u8>",
        "DmaRegion<mut u8>",
        // And nested, because a result type is where one would actually appear.
        "Result<Region<u8>, i64>",
        "Result<DmaRegion<mut u8>, i64>",
    ] {
        assert!(
            names_a_memory_region(ty),
            "{ty} is ordinary or DMA memory and must be caught by the rule"
        );
    }

    // Device memory is a separate sealed kind (ADR-0081 §5): no element type,
    // nothing funds it, nothing reclaims it to the pool, and the facts a window
    // needs *are* declared by `PLATFORM_INTERFACE_V1`. It shares six letters
    // with `Region` and nothing else.
    for ty in [
        "MmioRegion",
        "MmioRegionMut",
        "Result<MmioRegion, i64>",
        "Result<MmioRegionMut, i64>",
        // A future platform interface path must not be caught either, merely
        // for having the word in its last segment.
        "platform.mmio.Region",
        "platform.pci.FunctionConfig",
    ] {
        assert!(
            !names_a_memory_region(ty),
            "{ty} is not ordinary or DMA memory and must not be caught by the rule"
        );
    }

    // And the rule still catches the thing it was written for, however it is
    // reached: a bare constructor name, and the lower-case spelling `docs/44`
    // uses for an element.
    assert!(names_a_memory_region("Region"));
    assert!(names_a_memory_region("region"));
}

/// The device kinds really are distinct object kinds, not a spelling of one.
///
/// Structured rather than textual: whatever the types are called, the frontend
/// must not classify a device window as an ordinary memory region.
#[test]
fn device_memory_is_a_distinct_ir_kind() {
    use tos_ir::TypeDef;
    assert_ne!(TypeDef::MmioRegion, TypeDef::MmioRegionMut);
    for device in [TypeDef::MmioRegion, TypeDef::MmioRegionMut] {
        assert!(
            !matches!(
                device,
                TypeDef::Region(_)
                    | TypeDef::RegionMut(_)
                    | TypeDef::DmaRegion(_)
                    | TypeDef::DmaRegionMut(_)
            ),
            "a device window is being carried as an ordinary memory region"
        );
    }
}
