// SPDX-License-Identifier: GPL-3.0-or-later
//! A capability effect names an **interface**, not a startup binding (ADR-0080).
//!
//! ADR-0078 §6 left one question open on purpose: how may a module act through a
//! capability of an interface it never requested as a startup import, when that
//! capability lawfully arrives as a runtime value? Stage 4A reached it — a
//! module claims a PCI function and cannot use the capability the claim
//! produced, because the frontend required every `uses` item to be an import
//! binding and no import can answer a request for an object that does not exist
//! until the claim runs.
//!
//! TOS Core 1.1 admits `uses [interface.path]`. What is proved here:
//!
//! 1. a 1.1 module reaches an operation through a runtime capability whose
//!    interface it never imported, and the artifact says so;
//! 2. the effect recorded in the artifact is the **interface path**, and the two
//!    spellings produce the same one;
//! 3. lower → TOSIMAGE → decode → independent verifier, over the decoded
//!    artifact rather than the one handed to the writer;
//! 4. the declaration grants nothing: a scalar in that capability position is
//!    still refused, by the verifier, over the artifact;
//! 5. a capability of the wrong interface in that position is refused too;
//! 6. the artifact records the language version the module **declared**.

use tos_ir::{CapabilitySource, Op, Operand};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

/// A module that takes a PCI function out of a bus it *did* import, and then
/// reads configuration space through the capability the claim produced.
///
/// It imports `platform.pci.Bus` and **not** `platform.pci.FunctionConfig`.
/// Under TOS Core 1.0 this module could not be written at all.
const READER: &str = "\
module system.test.effects version 1.1 profile full;
import capability platform.pci.Bus as bus;

resource [fuel: 65536, stack: 16KiB, allocation: 4KiB, tasks: 1, workers: 1,
          sync: 0, shared: 0B, cleanup: 0, recursion: 8, imports: 4]

extern fn pci_function_claim(
    cap: platform.pci.Bus, bus_number: u64, device: u64, function: u64
) -> Result<platform.pci.FunctionConfig, i64> uses [bus];

extern fn pci_config_read(
    cap: platform.pci.FunctionConfig, offset: u64, width: u64
) -> Result<u64, i64> uses [platform.pci.FunctionConfig];

pub fn main() -> i64 uses [bus, platform.pci.FunctionConfig] {
    match (pci_function_claim(bus, 0u64, 4u64, 0u64)) {
        Ok(function) => {
            match (pci_config_read(function, 0u64, 4u64)) {
                Ok(value) => {
                    return 1i64;
                }
                Err(status) => {
                    return status;
                }
            }
        }
        Err(status) => {
            return status - 100i64;
        }
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
        "a module acting through a direct interface effect checks clean: {diagnostics:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("interface-effects-test"),
            path: String::from("system/test/effects.tos"),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

fn reader() -> tos_ir::Module {
    lower(READER)
}

fn entry(module: &tos_ir::Module) -> &tos_ir::Function {
    module
        .functions
        .iter()
        .find(|function| function.signature.name == "main")
        .expect("the entry is in the artifact")
}

fn sources_of(module: &tos_ir::Module, operation: &str) -> Vec<CapabilitySource> {
    entry(module)
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match &instruction.op {
            Op::Capability {
                capabilities,
                right,
                ..
            } if right == operation => Some(capabilities.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{operation} is in the artifact"))
}

#[test]
fn an_operation_is_reached_through_a_capability_of_an_uniimported_interface() {
    let module = reader();

    // The claim acts through the imported bus.
    let claim = sources_of(&module, "pci_function_claim");
    assert_eq!(claim.len(), 1);
    assert!(
        matches!(claim[0], CapabilitySource::Import(_)),
        "the claim does not act through the module's own import: {claim:?}"
    );

    // The read acts through what the claim produced — a value, of an interface
    // this module never requested and could not have been granted at startup.
    let read = sources_of(&module, "pci_config_read");
    assert_eq!(read.len(), 1);
    assert!(
        matches!(read[0], CapabilitySource::Value(_)),
        "the read does not act through a runtime capability: {read:?}"
    );

    // And there is genuinely no import of it, so nothing was smuggled in.
    assert!(
        !module
            .capability_imports
            .iter()
            .any(|import| import.interface == "platform.pci.FunctionConfig"),
        "the module imports the interface after all"
    );
}

#[test]
fn the_recorded_effect_is_the_interface_path() {
    let module = reader();
    let effects = &entry(&module).signature.effects;
    assert_eq!(
        effects,
        &vec![
            String::from("platform.pci.Bus"),
            String::from("platform.pci.FunctionConfig"),
        ],
        "effects are recorded by interface path, whichever way each was written"
    );
}

/// The two spellings are one effect (ADR-0080 §4), so an artifact cannot tell
/// them apart — which is why the frontend no longer tries to.
#[test]
fn a_binding_and_its_interface_lower_to_one_effect() {
    const BOUND: &str = "\
module system.test.spelling version 1.1 profile full;
import capability system.ipc.Endpoint as journal;

resource [fuel: 65536, stack: 16KiB, allocation: 4KiB, tasks: 1, workers: 1,
          sync: 0, shared: 0B, cleanup: 0, recursion: 8, imports: 4]

extern fn endpoint_send_text(cap: system.ipc.Endpoint, message: string) -> i64
    uses [journal];

pub fn main() -> i64 uses [SPELLING] {
    return endpoint_send_text(journal, \"info.test.one.two\");
}
";
    let by_binding = lower(&BOUND.replace("SPELLING", "journal"));
    let by_interface = lower(&BOUND.replace("SPELLING", "system.ipc.Endpoint"));
    assert_eq!(
        entry(&by_binding).signature.effects,
        entry(&by_interface).signature.effects
    );
    assert_eq!(
        entry(&by_binding).signature.effects,
        vec![String::from("system.ipc.Endpoint")]
    );
}

/// The whole path, over the **decoded** artifact rather than the one handed to
/// the writer: an encoder that dropped the source tag would otherwise be proved
/// correct by the value it still had in memory.
#[test]
fn the_decoded_artifact_verifies() {
    let module = reader();
    let limits = Limits::default();
    let (image, _) = tos_image::encode(&module);
    let decoded = tos_image::parse(&image, &parse_limits()).expect("the image parses");
    verify(&decoded, &ResolutionSnapshot::default(), &limits)
        .expect("the decoded artifact verifies");
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

/// Rewrites one operation's capability sources and verifies the result.
fn damaged(rewrite: impl Fn(&str, &mut Vec<CapabilitySource>)) -> tos_verifier::Finding {
    let mut module = reader();
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

/// **The declaration grants nothing.** Declaring the interface as an effect does
/// not make a number into authority: the position still needs a capability, and
/// the verifier proves it against the artifact.
#[test]
fn a_direct_interface_effect_does_not_admit_a_forged_scalar() {
    let finding = damaged(|right, sources| {
        if right == "pci_config_read" {
            sources[0] = CapabilitySource::Value(Operand::Constant(0));
        }
    });
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(
        finding.detail.contains("not of any capability type"),
        "{finding:?}"
    );
}

/// Nor a capability of a different interface. The effect declares which class
/// may be exercised; the position still requires the exact nominal one.
#[test]
fn a_direct_interface_effect_does_not_admit_the_wrong_interface() {
    let module = reader();
    let bus = entry(&module)
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|instruction| {
            matches!(&instruction.op, Op::Capability { right, .. }
                if right == "pci_function_claim")
        })
        .and_then(|instruction| instruction.result)
        .expect("the claim defines a value");

    let finding = damaged(|right, sources| {
        if right == "pci_config_read" {
            sources[0] = CapabilitySource::Value(Operand::Value(bus));
        }
    });
    assert_eq!(finding.code, "V2013_CAPABILITY");
}

/// The artifact records the version the module declared, not the newest the
/// frontend implements — otherwise two languages would share one identity.
#[test]
fn the_artifact_records_the_declared_language_version() {
    assert_eq!(reader().header.language_version, "1.1");

    const ONE_ZERO: &str = "\
module system.test.spelling version 1.0 profile full;
import capability system.ipc.Endpoint as journal;

resource [fuel: 65536, stack: 16KiB, allocation: 4KiB, tasks: 1, workers: 1,
          sync: 0, shared: 0B, cleanup: 0, recursion: 8, imports: 4]

extern fn endpoint_send_text(cap: system.ipc.Endpoint, message: string) -> i64
    uses [journal];

pub fn main() -> i64 uses [journal] {
    return endpoint_send_text(journal, \"info.test.one.two\");
}
";
    assert_eq!(lower(ONE_ZERO).header.language_version, "1.0");
}
