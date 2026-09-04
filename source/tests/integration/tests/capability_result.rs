// SPDX-License-Identifier: GPL-3.0-or-later
//! An operation returns **authority**, and the whole pipeline carries it.
//!
//! The narrow proof the Project Architect asked for before the typed
//! `SYSTEM_INTERFACE` bridge is built out:
//!
//! ```text
//! extern operation -> Result<nominal capability, i64>
//!     -> lower -> TOSIMAGE -> independent verify -> engine execution
//! ```
//!
//! The question it answers is whether a capability-valued result needs a new
//! `tos-ir/v1` variant or a new TOS Core type constructor. It does not, and this
//! file is why: `TypeDef::Capability` and `TypeDef::Result` have both been in
//! the IR since it was written, the image format encodes and decodes both, and
//! the engine's `reach` already returns a `Value` rather than an `i64`. What was
//! missing was one line of the *frontend* — `boundary::type_text` admitted only
//! `TypeSyntax::Name`, so a schema could declare no result that was not a bare
//! name — and one line of the *lowerer*, which resolved an interface path to a
//! nominal record rather than to the capability type it is.
//!
//! Each stage is checked separately rather than end to end, because "it ran" is
//! the weakest of the five things that had to be true.

use tos_ir::{Op, TypeDef};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

/// A module that asks the system for a launch plan and returns whether it got
/// one.
///
/// It cannot do anything *with* the plan, and deliberately does not try: what is
/// under test is that authority crosses the boundary as a typed value, is
/// matched on like any other `Result`, and reaches the arm the host chose.
const MODULE: &str = "\
module system.test.plan version 1.0 profile full;
import capability system.process.Control as process;

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

extern fn launch_plan_create(
    cap: system.process.Control
) -> Result<system.process.LaunchPlanBuilder, i64> uses [process];

pub fn main() -> i64 uses [process] {
    match (launch_plan_create(process)) {
        Ok(plan) => {
            return 1i64;
        }
        Err(status) => {
            return status;
        }
    }
}
";

fn lower(text: &str) -> tos_ir::Module {
    lower_at(text, "system/test/plan.tos")
}

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
        "a module declaring a capability-valued result checks clean: {diagnostics:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("capability-result-test"),
            path: String::from(path),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

/// The lowered result type, as the type table records it.
fn result_type(module: &tos_ir::Module) -> (TypeDef, TypeDef, TypeDef) {
    let entry = module
        .functions
        .iter()
        .find(|function| function.signature.name == "main")
        .expect("the entry is in the artifact");
    let reaching = entry
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|instruction| matches!(instruction.op, Op::Capability { .. }))
        .expect("the entry reaches the operation");
    let outer = module.types[reaching.ty].clone();
    let TypeDef::Result(ok, error) = outer.clone() else {
        panic!("the operation's result is not a Result: {outer:?}");
    };
    (outer, module.types[ok].clone(), module.types[error].clone())
}

#[test]
fn the_result_is_a_capability_in_the_type_table() {
    let module = lower(MODULE);
    let (_, ok, error) = result_type(&module);

    // Not a nominal record whose name happens to look like an interface path.
    // A reader of the artifact learns from the *type* that this value is
    // authority, which is what `docs/42` §2 admits into provenance — the
    // interface, never the handle.
    assert_eq!(
        ok,
        TypeDef::Capability(String::from("system.process.LaunchPlanBuilder"))
    );
    assert_eq!(error, TypeDef::Int(tos_ir::IntKind::I64));

    // And no operand anywhere holds a capability: the input is still an import
    // index, and the output is an SSA value with a capability *type*. A type is
    // not a representation.
    let entry = module
        .functions
        .iter()
        .find(|function| function.signature.name == "main")
        .expect("the entry is in the artifact");
    for instruction in entry.blocks.iter().flat_map(|block| &block.instructions) {
        if let Op::Capability {
            capabilities,
            operands,
            ..
        } = &instruction.op
        {
            assert_eq!(capabilities, &vec![tos_ir::CapabilitySource::Import(0)]);
            assert!(operands.is_empty(), "the operation takes no values");
        }
    }
}

#[test]
fn the_image_carries_it_and_an_independent_reader_verifies_it() {
    let module = lower(MODULE);

    // TOSIMAGE, written and read back by the format's own encoder and decoder.
    // `TypeDef::Capability` and `TypeDef::Result` both already had tags; this is
    // the first artifact that puts one inside the other.
    let (image, _) = tos_image::encode(&module);
    let limits = Limits::default();
    let restored = tos_image::parse(
        &image,
        &tos_image::ParseLimits {
            table_entries: limits.table_entries,
            modules: limits.modules,
            fields: limits.fields,
            parameters: limits.parameters,
            blocks_per_function: limits.blocks_per_function,
            instructions_per_block: limits.instructions_per_block,
            source_map_entries: limits.source_map_entries,
        },
    )
    .expect("an image this crate wrote parses");
    assert_eq!(restored.types, module.types);

    // And the verifier accepts what came back rather than what was handed to
    // the writer, which is the only way round that proves the image carried it.
    verify(
        &restored,
        &ResolutionSnapshot::default(),
        &Limits::default(),
    )
    .expect("an artifact whose operation returns authority verifies");
}

/// A host that answers the one operation with a capability, or with a refusal.
struct Grantor {
    /// What `reach` answers: `Some` is authority, `None` is a status.
    answer: Option<u64>,
    reached: Vec<String>,
}

impl tos_engine::System for Grantor {
    /// No device is reachable on this run, and saying so is the only honest
    /// answer: a device access here has reached hardware that does not exist.
    fn observe(
        &mut self,
        _access: tos_engine::Observe,
    ) -> Result<tos_engine::Value, tos_engine::Trap> {
        Err(tos_engine::Trap::new(
            "RUNTIME_DEVICE_UNREACHABLE",
            String::from("a device access was made on a run with no device to reach"),
            0,
        ))
    }

    fn granted(&mut self, request: tos_engine::Request<'_>) -> Option<tos_engine::Handle> {
        (request.binding == "process").then(|| tos_engine::Handle::new(0x2_0000_0000))
    }

    fn reach(
        &mut self,
        call: tos_engine::Reach<'_>,
    ) -> Result<tos_engine::Value, tos_engine::Trap> {
        self.reached.push(String::from(call.operation));
        // `Result` is variant 0 for `Ok` and 1 for `Err`, which is the language's
        // own representation and not this host's invention.
        Ok(match self.answer {
            Some(handle) => tos_engine::Value::Variant {
                index: 0,
                payload: vec![tos_engine::Value::Capability(tos_engine::Handle::new(
                    handle,
                ))],
            },
            None => tos_engine::Value::Variant {
                index: 1,
                payload: vec![tos_engine::Value::Int(tos_ir::IntKind::I64, -6)],
            },
        })
    }
}

fn run(answer: Option<u64>) -> (i64, Vec<String>) {
    let module = lower(MODULE);
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("the artifact verifies");
    let mut system = Grantor {
        answer,
        reached: Vec::new(),
    };
    // The production path: encode, verify the image, run through a bounded
    // resident set. There is no other way to execute a module.
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
        .expect("the entry takes no arguments and its request is granted")
        .expect("the run completes");
    let tos_engine::Value::Int(_, produced) = outcome.value else {
        panic!("the entry returns an i64");
    };
    (produced as i64, system.reached)
}

#[test]
fn the_engine_carries_authority_into_the_arm_that_matched() {
    // Granted: the `Ok` arm runs, so the capability travelled through the
    // engine, was bound by a pattern, and the match selected on it.
    let (produced, reached) = run(Some(0x3_0000_0000));
    assert_eq!(produced, 1);
    assert_eq!(reached, vec![String::from("launch_plan_create")]);

    // Refused: the same operation, the same type, the other arm. A refusal is an
    // ordinary value (§5), and the module reads the status the system produced.
    let (produced, _) = run(None);
    assert_eq!(produced, -6);
}

/// The whole chain a textual supervisor performs, in one module.
///
/// Three requests, three interfaces, and one plan that travels between five
/// calls as an ordinary value: made, written twice through *different* nominal
/// capability types, sealed, and then used to create a funded process. Every
/// capability in it keeps its exact nominal type — there is no `AnyCapability`
/// anywhere, and no raw handle in the source or in the artifact.
const SUPERVISOR: &str = "\
module system.test.supervisor version 1.0 profile full;
import capability system.process.Control as process;
import capability system.ipc.Endpoint as inbox;
import capability system.memory.Authority as memory;

resource [
    fuel: 4096,
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

extern fn launch_plan_create(
    cap: system.process.Control
) -> Result<system.process.LaunchPlanBuilder, i64> uses [process];

extern fn launch_plan_seal(
    cap: system.process.Control,
    plan: system.process.LaunchPlanBuilder
) -> Result<system.process.LaunchPlan, i64> uses [process];

// The standard family: one name, one ABI selector, and the exact nominal type
// of what is delegated retained at each declaration.
extern fn endow_for_launch(
    held: system.ipc.Endpoint,
    plan: system.process.LaunchPlanBuilder,
    rights: u64,
    binding: string
) -> i64 uses [inbox];

extern fn endow_for_launch(
    held: system.memory.Authority,
    plan: system.process.LaunchPlanBuilder,
    rights: u64,
    binding: string
) -> i64 uses [memory];

extern fn process_create_funded(
    cap: system.process.Control,
    funding: system.memory.Authority,
    plan: system.process.LaunchPlan,
    entry: string,
    grant: u64,
    self_rights: u64
) -> Result<system.process.CreatedProcess, i64> uses [process, memory];

pub fn main() -> i64 uses [process, inbox, memory] {
    match (launch_plan_create(process)) {
        Ok(builder) => {
            let inboxed: i64 = endow_for_launch(inbox, builder, 2u64, \"inbox\");
            let funded: i64 = endow_for_launch(memory, builder, 128u64, \"memory\");
            if (inboxed != 0i64) {
                return inboxed;
            }
            if (funded != 0i64) {
                return funded;
            }
            match (launch_plan_seal(process, builder)) {
                Ok(plan) => {
                    match (process_create_funded(
                        process, memory, plan, \"system/boot/init.tos\", 56623104u64, 0u64
                    )) {
                        Ok(created) => {
                            return 1i64;
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

/// A host that performs the chain the way the nucleus does, and writes down
/// what crossed.
struct Launcher {
    /// Interface, operation, and the handles the call carried, in order.
    reached: Vec<(String, String, Vec<u64>)>,
    /// The text values, in the order they crossed.
    text: Vec<String>,
    /// The handles this host mints for what it makes.
    next: u64,
}

impl tos_engine::System for Launcher {
    /// No device is reachable on this run, and saying so is the only honest
    /// answer: a device access here has reached hardware that does not exist.
    fn observe(
        &mut self,
        _access: tos_engine::Observe,
    ) -> Result<tos_engine::Value, tos_engine::Trap> {
        Err(tos_engine::Trap::new(
            "RUNTIME_DEVICE_UNREACHABLE",
            String::from("a device access was made on a run with no device to reach"),
            0,
        ))
    }

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
        let mut handles = Vec::new();
        for argument in call.arguments {
            match argument {
                tos_engine::Value::Capability(held) => handles.push(held.get()),
                tos_engine::Value::Text(text) => self.text.push(text.clone()),
                _ => {}
            }
        }
        self.reached.push((
            String::from(call.interface),
            String::from(call.operation),
            handles,
        ));
        Ok(match call.operation {
            "endow_for_launch" => tos_engine::Value::Int(tos_ir::IntKind::I64, 0),
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
fn a_plan_is_written_through_three_interfaces_and_creates_a_process() {
    let module = lower_at(SUPERVISOR, "system/test/supervisor.tos");
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("the supervisor verifies");

    let mut system = Launcher {
        reached: Vec::new(),
        text: Vec::new(),
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

    let reached: Vec<(&str, &str, &[u64])> = system
        .reached
        .iter()
        .map(|(interface, operation, handles)| {
            (interface.as_str(), operation.as_str(), handles.as_slice())
        })
        .collect();
    // The plan the first call produced (0x101) is the second argument of both
    // endowments and of the seal, and the plan the seal produced (0x102) is what
    // the creation is given. Nothing else could have carried it: there is no
    // handle in the source and no capability operand the module could name.
    assert_eq!(
        reached,
        vec![
            (
                "system.process.Control",
                "launch_plan_create",
                &[0x10u64][..]
            ),
            // The endowment is reached **through the endpoint**, not through
            // the process authority: one selector, and the interface recorded
            // on the instruction is the one being delegated.
            (
                "system.ipc.Endpoint",
                "endow_for_launch",
                &[0x11, 0x101][..]
            ),
            (
                "system.memory.Authority",
                "endow_for_launch",
                &[0x12, 0x101][..]
            ),
            (
                "system.process.Control",
                "launch_plan_seal",
                &[0x10, 0x101][..]
            ),
            // Two capabilities from imports, then the sealed plan as a value.
            (
                "system.process.Control",
                "process_create_funded",
                &[0x10, 0x12, 0x102][..]
            ),
        ]
    );
    // And the names the policy chose crossed as values, in the order they were
    // written. A module named them; nothing here read an argument region.
    assert_eq!(
        system.text,
        vec![
            String::from("inbox"),
            String::from("memory"),
            String::from("system/boot/init.tos"),
        ]
    );
}
