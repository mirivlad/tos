// SPDX-License-Identifier: GPL-3.0-or-later
//! An operation acts on authority it was **given at runtime** (ADR-0078).
//!
//! The narrow proof the Project Architect required before Section H continues.
//! Before this repair, `Op::Capability` named an operation's capabilities only
//! as import indices, so an operation acting on authority an *earlier operation
//! produced* could not be represented at all — while TOS Core V1's accepted
//! semantics already admitted capability values and capability-derived
//! authority. That was the representation narrowing the accepted semantics, and
//! `CapabilitySource::Import | ::Value` is the repair.
//!
//! Eight things had to be true, and they are checked one at a time:
//!
//! 1. operation 19 returns a child `system.process.Control`;
//! 2. that runtime capability is the **own** capability of `process_terminate`;
//! 3. lower → TOSIMAGE → decode → independent verifier → engine, in that order,
//!    over the decoded artifact rather than the one handed to the writer;
//! 4. operation 16 returns a scoped `system.memory.Authority`;
//! 5. that runtime authority is the capability `endow_for_launch` acts through;
//! 6. attenuation and release of a runtime-obtained capability are
//!    representable;
//! 7. a value that is not a capability of the required interface — a scalar, or
//!    a capability of the wrong interface — is refused, and by the *verifier*,
//!    over the artifact;
//! 8. a **non-first** capability position can be runtime-sourced too, so the
//!    hole is not left one position along.
//!
//! The import-only path is unchanged and is checked here as well: an import is
//! now the explicit `Import` case and means exactly what it meant.

use tos_ir::{CapabilitySource, Op, Operand, TypeDef};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

/// A supervisor that scopes a budget, plans a child, creates it, refines its
/// authority, ends it and lets go — every step after the first acting on a
/// capability the step before it produced.
const SUPERVISOR: &str = "\
module system.test.runtime version 1.0 profile full;
import capability system.process.Control as process;
import capability system.ipc.Endpoint as inbox;
import capability system.memory.Authority as memory;

resource [
    fuel: 8192,
    stack: 8KiB,
    allocation: 4KiB,
    tasks: 1,
    workers: 1,
    sync: 0,
    shared: 0B,
    cleanup: 0,
    recursion: 8,
    imports: 3
]

extern fn capability_attenuate_scoped(
    cap: system.memory.Authority,
    bytes: u64
) -> Result<system.memory.Authority, i64> uses [memory];

extern fn launch_plan_create(
    cap: system.process.Control
) -> Result<system.process.LaunchPlanBuilder, i64> uses [process];

extern fn endow_for_launch(
    held: system.memory.Authority,
    plan: system.process.LaunchPlanBuilder,
    rights: u64,
    binding: string
) -> i64 uses [memory];

extern fn endow_for_launch(
    held: system.ipc.Endpoint,
    plan: system.process.LaunchPlanBuilder,
    rights: u64,
    binding: string
) -> i64 uses [inbox];

extern fn launch_plan_seal(
    cap: system.process.Control,
    plan: system.process.LaunchPlanBuilder
) -> Result<system.process.LaunchPlan, i64> uses [process];

extern fn process_create_funded(
    cap: system.process.Control,
    funding: system.memory.Authority,
    plan: system.process.LaunchPlan,
    entry: string,
    grant: u64,
    self_rights: u64
) -> Result<system.process.CreatedProcess, i64> uses [process, memory];

extern fn capability_attenuate(
    cap: system.process.Control,
    rights: u64
) -> Result<system.process.Control, i64> uses [process];

extern fn process_terminate(cap: system.process.Control) -> i64 uses [process];

extern fn capability_release(cap: system.process.Control) -> i64 uses [process];

pub fn main() -> i64 uses [process, inbox, memory] {
    // 4 — a scoped budget, produced by operation 16.
    match (capability_attenuate_scoped(memory, 16777216u64)) {
        Ok(scoped) => {
            match (launch_plan_create(process)) {
                Ok(builder) => {
                    // 5 — the operation acts *through* the scoped authority,
                    // which no import answers.
                    let funded: i64 = endow_for_launch(scoped, builder, 128u64, \"memory\");
                    let inboxed: i64 = endow_for_launch(inbox, builder, 2u64, \"inbox\");
                    if (funded + inboxed != 0i64) {
                        return funded + inboxed;
                    }
                    // 8 — a non-first capability position from a runtime value:
                    // the plan is the second, the process import the first.
                    match (launch_plan_seal(process, builder)) {
                        Ok(plan) => {
                            // 1 — a child capability, and 8 again: the second
                            // capability of this operation is the scoped
                            // authority, which is also a runtime value.
                            match (process_create_funded(
                                process, scoped, plan, \"system/boot/init.tos\", 56623104u64, 0u64
                            )) {
                                Ok(created) => {
                                    // 6 — refinement of a runtime capability,
                                    // taken out of the record the creation
                                    // produced: a capability field of a
                                    // schema-declared record is a capability
                                    // value like any other.
                                    let child: system.process.Control = created.control;
                                    match (capability_attenuate(child, 16u64)) {
                                        Ok(weaker) => {
                                            // 2 — a runtime capability as the
                                            // operation's **own** capability.
                                            let ended: i64 = process_terminate(weaker);
                                            // 6 again — and release of one.
                                            let let_go: i64 = capability_release(child);
                                            return ended + let_go + 1i64;
                                        }
                                        Err(status) => {
                                            return status;
                                        }
                                    }
                                }
                                Err(status) => {
                                    return status;
                                }
                            }
                        }
                        Err(status) => {
                            return status;
                        }
                    }
                }
                Err(status) => {
                    return status;
                }
            }
        }
        Err(status) => {
            return status;
        }
    }
}
";

fn lower_at(text: &str, path: &str) -> tos_ir::Module {
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    let diagnostics = tos_core::Checker::check(&source, &schema);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.severity() == tos_core::Severity::Error),
        "a module acting on runtime authority checks clean: {diagnostics:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("capability-source-test"),
            path: String::from(path),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

fn supervisor() -> tos_ir::Module {
    lower_at(SUPERVISOR, "system/test/runtime.tos")
}

fn parse_limits() -> tos_image::ParseLimits {
    let limits = Limits::default();
    tos_image::ParseLimits {
        table_entries: limits.table_entries,
        modules: limits.modules,
        fields: limits.fields,
        parameters: limits.parameters,
        blocks_per_function: limits.blocks_per_function,
        instructions_per_block: limits.instructions_per_block,
        source_map_entries: limits.source_map_entries,
    }
}

/// Every interface operation of the entry, in order, with its sources.
fn reaching(module: &tos_ir::Module) -> Vec<(String, Vec<CapabilitySource>)> {
    let entry = module
        .functions
        .iter()
        .find(|function| function.signature.name == "main")
        .expect("the entry is in the artifact");
    let mut found = Vec::new();
    for block in &entry.blocks {
        for instruction in &block.instructions {
            if let Op::Capability {
                capabilities,
                right,
                ..
            } = &instruction.op
            {
                found.push((right.clone(), capabilities.clone()));
            }
        }
    }
    found
}

#[test]
fn a_runtime_capability_is_the_operations_own_capability() {
    let module = supervisor();
    let found = reaching(&module);
    let by_name = |name: &str| {
        found
            .iter()
            .find(|(right, _)| right == name)
            .unwrap_or_else(|| panic!("{name} is in the artifact"))
            .1
            .clone()
    };

    // 2 — `process_terminate` takes one capability, and it is a **value**: the
    // refined child authority `capability_attenuate` produced. There is no
    // import of the child, because the child did not exist at startup.
    let terminate = by_name("process_terminate");
    assert_eq!(terminate.len(), 1);
    assert!(
        matches!(terminate[0], CapabilitySource::Value(_)),
        "process_terminate acts on an import: {terminate:?}"
    );

    // 6 — refinement and release of runtime-obtained authority, likewise.
    for name in ["capability_attenuate", "capability_release"] {
        let sources = by_name(name);
        assert_eq!(sources.len(), 1);
        assert!(
            matches!(sources[0], CapabilitySource::Value(_)),
            "{name} acts on an import: {sources:?}"
        );
    }

    // 5 — the scoped authority is what `endow_for_launch` acts through in one
    // of its two declarations, and an import in the other. One selector, two
    // sources, exact nominal types either way.
    let endowments: Vec<_> = found
        .iter()
        .filter(|(right, _)| right == "endow_for_launch")
        .map(|(_, sources)| sources.clone())
        .collect();
    assert_eq!(endowments.len(), 2);
    assert!(matches!(endowments[0][0], CapabilitySource::Value(_)));
    assert!(matches!(endowments[1][0], CapabilitySource::Import(_)));

    // 8 — a **non-first** capability position, runtime-sourced. Operation 19
    // requires two capabilities: authority over the process a child is created
    // under, and the `MemoryAuthority` its footprint is charged to. The first
    // is an import; the **second** is the scoped budget operation 16 produced.
    // If the repair had stopped at the operation's own capability, this would
    // be the same hole one position along.
    let created = by_name("process_create_funded");
    assert_eq!(created.len(), 2);
    assert!(matches!(created[0], CapabilitySource::Import(_)));
    assert!(
        matches!(created[1], CapabilitySource::Value(_)),
        "the second capability position is not runtime-sourced: {created:?}"
    );

    // And the import-only path is unchanged: an import is the explicit `Import`
    // case and means what it always meant.
    let planned = by_name("launch_plan_create");
    assert_eq!(planned, vec![CapabilitySource::Import(0)]);

    // 1 and 4 — the results are capabilities of the exact nominal interfaces,
    // in the type table.
    let result_of = |name: &str| {
        let entry = module
            .functions
            .iter()
            .find(|function| function.signature.name == "main")
            .expect("the entry");
        for block in &entry.blocks {
            for instruction in &block.instructions {
                if let Op::Capability { right, .. } = &instruction.op {
                    if right == name {
                        let TypeDef::Result(ok, _) = module.types[instruction.ty].clone() else {
                            panic!("{name} does not return a Result");
                        };
                        return module.types[ok].clone();
                    }
                }
            }
        }
        panic!("{name} is in the artifact")
    };
    // The creation's result is the record §4.2 declares, and its first field is
    // the child's authority — the value `process_terminate` later acts through.
    let TypeDef::Nominal {
        export_name,
        fields,
        ..
    } = result_of("process_create_funded")
    else {
        panic!("a creation does not return a record");
    };
    assert_eq!(export_name, "system.process.CreatedProcess");
    assert_eq!(
        module.types[fields[0]],
        TypeDef::Capability(String::from("system.process.Control"))
    );
    assert_eq!(
        result_of("capability_attenuate_scoped"),
        TypeDef::Capability(String::from("system.memory.Authority"))
    );
}

#[test]
fn the_image_carries_the_sources_and_an_independent_reader_verifies_them() {
    let module = supervisor();

    // 3 — the whole path, and the verifier runs over what came **back**.
    let (image, _) = tos_image::encode(&module);
    // The container version moved, because the bytes did: an older reader must
    // refuse this rather than read a source tag as an index.
    assert_eq!(
        u32::from_be_bytes([image[8], image[9], image[10], image[11]]),
        tos_image::ENCODING_VERSION
    );
    let restored = tos_image::parse(&image, &parse_limits()).expect("the image parses");
    assert_eq!(reaching(&restored), reaching(&module));
    verify(
        &restored,
        &ResolutionSnapshot::default(),
        &Limits::default(),
    )
    .expect("an artifact acting on runtime authority verifies");
}

#[test]
fn an_unknown_container_version_is_refused_and_the_old_one_is_read_as_imports() {
    // Fail closed above, and bounded and canonical below: version 3 wrote every
    // capability position as an import because that was the only source it had,
    // so reading one back as `Import` invents nothing.
    let module = supervisor();
    let (mut image, _) = tos_image::encode(&module);
    image[8..12].copy_from_slice(&99u32.to_be_bytes());
    tos_image::reseal(&mut image);
    let refused = tos_image::parse(&image, &parse_limits())
        .expect_err("a container version this reader does not implement is refused");
    assert!(
        matches!(refused, tos_image::ImageError::UnknownEncodingVersion(99)),
        "{refused:?}"
    );
    assert!(tos_image::READABLE_ENCODING_VERSIONS.contains(&3));
}

/// A host that answers each operation with what its schema says it produces.
struct Runtime {
    /// Interface, operation, and how many handles the call carried.
    reached: Vec<(String, String, usize)>,
    next: u64,
}

impl tos_engine::System for Runtime {
    fn granted(&mut self, request: tos_engine::Request<'_>) -> Option<tos_engine::Handle> {
        Some(tos_engine::Handle::new(match request.binding {
            "process" => 0x10,
            "inbox" => 0x11,
            "memory" => 0x12,
            _ => return None,
        }))
    }

    fn reach(
        &mut self,
        call: tos_engine::Reach<'_>,
    ) -> Result<tos_engine::Value, tos_engine::Trap> {
        let handles = call
            .arguments
            .iter()
            .filter(|value| matches!(value, tos_engine::Value::Capability(_)))
            .count();
        self.reached.push((
            String::from(call.interface),
            String::from(call.operation),
            handles,
        ));
        Ok(match call.operation {
            "endow_for_launch" | "process_terminate" | "capability_release" => {
                tos_engine::Value::Int(tos_ir::IntKind::I64, 0)
            }
            // A creation produces the record §4.2 declares: the child's
            // authority and its instance identity, in that order.
            "process_create_funded" => {
                self.next += 1;
                tos_engine::Value::Variant {
                    index: 0,
                    payload: vec![tos_engine::Value::Aggregate(vec![
                        tos_engine::Value::Capability(tos_engine::Handle::new(0x100 + self.next)),
                        tos_engine::Value::Int(tos_ir::IntKind::U64, i128::from(self.next)),
                    ])],
                }
            }
            _ => {
                self.next += 1;
                tos_engine::Value::Variant {
                    index: 0,
                    payload: vec![tos_engine::Value::Capability(tos_engine::Handle::new(
                        0x100 + self.next,
                    ))],
                }
            }
        })
    }
}

#[test]
fn the_engine_carries_runtime_authority_back_across_the_boundary() {
    let module = supervisor();
    let mut system = Runtime {
        reached: Vec::new(),
        next: 0,
    };
    let mut prepared = tos_pipeline::Prepared::launch(
        core::slice::from_ref(&&module),
        &ResolutionSnapshot::default(),
        "main",
        tos_pipeline::ResidencyLimits {
            modules: 1,
            bytes: 64 * 1024 * 1024,
        },
    )
    .expect("the fixture launches");
    let outcome = prepared
        .run(Vec::new(), &mut system)
        .expect("every request is granted")
        .expect("the run completes");
    assert_eq!(
        outcome.value,
        tos_engine::Value::Int(tos_ir::IntKind::I64, 1)
    );

    // Each operation reached the interface of the capability it acts through —
    // the *value's* interface for a runtime source, not the import's — and
    // carried the number of handles its schema declares.
    let reached: Vec<(&str, &str, usize)> = system
        .reached
        .iter()
        .map(|(interface, operation, handles)| (interface.as_str(), operation.as_str(), *handles))
        .collect();
    assert_eq!(
        reached,
        vec![
            ("system.memory.Authority", "capability_attenuate_scoped", 1),
            ("system.process.Control", "launch_plan_create", 1),
            // Through the scoped authority, which is a value: the interface on
            // the instruction is that value's own.
            ("system.memory.Authority", "endow_for_launch", 2),
            ("system.ipc.Endpoint", "endow_for_launch", 2),
            ("system.process.Control", "launch_plan_seal", 2),
            ("system.process.Control", "process_create_funded", 3),
            ("system.process.Control", "capability_attenuate", 1),
            ("system.process.Control", "process_terminate", 1),
            ("system.process.Control", "capability_release", 1),
        ]
    );
}

/// The artifact with one operation's capability sources rewritten, for the
/// refusals.
///
/// Damaged rather than written: a verifier that only ever sees what this
/// frontend emits proves nothing about a frontend somebody else wrote.
fn damaged(rewrite: impl Fn(&str, &mut Vec<CapabilitySource>)) -> tos_verifier::Finding {
    let mut module = supervisor();
    for function in &mut module.functions {
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                if let Op::Capability {
                    capabilities,
                    right,
                    ..
                } = &mut instruction.op
                {
                    rewrite(right, capabilities);
                }
            }
        }
    }
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("a damaged capability source is refused")
}

#[test]
fn a_forged_scalar_in_a_capability_position_is_refused() {
    // 7 — a constant where a capability belongs. This is the shape a frontend
    // that had learned to write handles would emit, and it is refused against
    // the artifact rather than trusted because the frontend produced it.
    let finding = damaged(|right, sources| {
        if right == "process_terminate" {
            sources[0] = CapabilitySource::Value(Operand::Constant(0));
        }
    });
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(
        finding.detail.contains("not of any capability type"),
        "{finding:?}"
    );
}

#[test]
fn a_capability_of_the_wrong_interface_is_refused() {
    // 7 — the exact nominal type is what is checked, not "is it a capability".
    // The scoped `system.memory.Authority` is a perfectly good capability and
    // is not a `system.process.Control`, so it cannot end a process.
    let module = supervisor();
    let scoped = module
        .functions
        .iter()
        .find(|function| function.signature.name == "main")
        .expect("the entry")
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|instruction| {
            matches!(&instruction.op, Op::Capability { right, .. }
                if right == "capability_attenuate_scoped")
        })
        .and_then(|instruction| instruction.result)
        .expect("the scoped authority is a value of the entry");

    // The `Ok` payload of that result is what a module binds, so the value the
    // instruction defines is the `Result`; using it directly is a capability of
    // neither interface and is refused for the same reason a scalar is. What
    // this checks is the interface comparison, so the substitute is taken from
    // the pattern binding instead.
    let finding = damaged(|right, sources| {
        if right == "process_terminate" {
            sources[0] = CapabilitySource::Value(Operand::Value(scoped));
        }
    });
    assert_eq!(finding.code, "V2013_CAPABILITY");
    // Either message is the right refusal: the substituted value is not a
    // capability type at all (it is a `Result`), and a `Result` is not a
    // `system.process.Control`. What must not happen is acceptance.
    assert!(
        finding.detail.contains("not of any capability type")
            || finding.detail.contains("is performed through"),
        "{finding:?}"
    );
}

#[test]
fn one_capability_still_cannot_stand_in_for_two() {
    // ADR-0063 held of imports and holds of sources: an operation requiring two
    // authorities may not be given one twice, whichever kind each is.
    // Operation 19's second capability is the scoped budget; this makes it the
    // process authority again, so one grant would stand in for two.
    let finding = damaged(|right, sources| {
        if right == "process_create_funded" {
            sources[1] = sources[0].clone();
        }
    });
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(finding.detail.contains("more than once"), "{finding:?}");
}
