// SPDX-License-Identifier: GPL-3.0-or-later
//! The verifier derives a capability position's interface and does not believe
//! the artifact (ADR-0085 §7, §17).
//!
//! `SYSTEM_INTERFACE_V1` §4.3 separates an interface's identity from the class
//! of TOS Core values that represents it, and the whole safety of that
//! separation is that the verifier works it out for itself. So every case here
//! is an artifact **no frontend produces**: the source is lowered normally and
//! then damaged, because a verifier that only ever sees what this frontend
//! emits proves nothing about a frontend somebody else wrote.
//!
//! What the verifier reads per position is the operand's type, from the
//! artifact's own type table, and the representation → interface map, from its
//! own closed table. Neither comes from the producer. `unsafe_interface` is
//! read too — as the claim being checked, never as an answer.
//!
//! **Every case here is a rejection, and that is the state this slice is in.**
//! `1.3` is not yet a language version any implementation admits, so a
//! correctly formed capability-representation artifact is still refused — by
//! its header if it declares 1.3, and by the version gate below if it declares
//! 1.2 and uses the rule anyway. The acceptances arrive with the minor.

use tos_ir::{CapabilityImport, CapabilitySource, Module, Op, Operand, TypeDef};
use tos_verifier::{verify, Limits, ResolutionSnapshot};

/// A module holding a DMA region **as a value** and reaching an ordinary
/// interface.
///
/// The region is a parameter because nothing produces one yet: `SYSTEM_ABI_V1`
/// operation 30's schema row waits on ADR-0085 §18. A parameter is enough —
/// what the tests need is an SSA value of the family, with a real type in the
/// artifact's own table, defined the way the verifier expects values to be.
///
/// It declares **1.2**, which is what the frontend implements while this slice
/// is in progress, and every case below either fails before the version is
/// consulted or fails on the version itself.
const MODULE: &str = "\
module system.test.representation version 1.2 profile full;
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

pub fn reach(area: DmaRegion<mut u64>) -> i64 uses [endpoint] {
    return endpoint_send(endpoint, 8u64);
}
";

fn lower(text: &str) -> Module {
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    let diagnostics = tos_core::Checker::check(&source, &schema);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.severity() == tos_core::Severity::Error),
        "the base module checks clean: {diagnostics:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("capability-representation-test"),
            path: String::from("system/test/representation.tos"),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

/// The index of the function that performs the operation.
fn reaching(module: &Module) -> usize {
    module
        .functions
        .iter()
        .position(|function| function.signature.name.ends_with("reach"))
        .expect("the reaching function is in the artifact")
}

/// The SSA value the region parameter is, found by its type rather than by its
/// position — the parameter's ordinal is the lowerer's business.
fn region_value(module: &Module, function: usize) -> Operand {
    let value = module.functions[function]
        .values
        .iter()
        .position(|ty| matches!(module.type_of(*ty), Some(TypeDef::DmaRegionMut(_))))
        .expect("the region parameter is an SSA value of the region family");
    Operand::Value(value)
}

/// Rewrites the capability position, the interface the instruction claims, and
/// the effects the function declares — the three things a forger controls.
fn forge(module: &mut Module, source: CapabilitySource, claims: &str, declares: &[&str]) {
    let function = reaching(module);
    module.functions[function].signature.effects =
        declares.iter().map(|e| String::from(*e)).collect();
    for block in &mut module.functions[function].blocks {
        for instruction in &mut block.instructions {
            if let Op::Capability { capabilities, .. } = &mut instruction.op {
                capabilities[0] = source.clone();
                instruction.unsafe_interface = Some(String::from(claims));
            }
        }
    }
}

fn refuse(module: &Module) -> tos_verifier::Finding {
    verify(module, &ResolutionSnapshot::default(), &Limits::default())
        .expect_err("a forged capability position is refused")
}

/// §17.2 — an import at a `DmaRegionFamily` position, refused on its own.
///
/// The import is well formed and derives its declared interface perfectly well.
/// What is wrong is the *source*: a representation no import can be admits
/// `Value` only, because there is nowhere in an `import capability` for the
/// element type or the mutability of a `DmaRegion<T>` to come from. **No
/// frontend emits this**, which is precisely why the verifier has to refuse it
/// rather than assume nobody would ask.
#[test]
fn an_import_at_a_region_position_is_refused() {
    let mut module = lower(MODULE);
    // A **well-formed** import of that interface, typed as its own interface
    // exactly as the import table requires. Everything that could be wrong with
    // the declaration is right, so what is left is the one thing this case is
    // about: an import cannot fill this position whatever it declares.
    module
        .types
        .push(TypeDef::Capability(String::from("platform.dma.Region")));
    let ty = module.types.len() - 1;
    module.capability_imports.push(CapabilityImport {
        interface: String::from("platform.dma.Region"),
        binding: String::from("forged"),
        ty,
    });
    let index = module.capability_imports.len() - 1;
    forge(
        &mut module,
        CapabilitySource::Import(index),
        "platform.dma.Region",
        &["platform.dma.Region"],
    );
    let finding = refuse(&module);
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(
        finding.detail.contains("no import can produce a value of"),
        "{finding:?}"
    );
}

/// §17.1 — the interface's own capability type at its own position.
///
/// `Capability("platform.dma.Region")` is exactly what a pre-ADR-0085 verifier
/// would have accepted there, and it is what §4.3 excludes: the representation
/// is the family, and the interface's own path is **not** a member of it. This
/// is the case that would silently pass if the amendment had been read as
/// "the family is *also* accepted".
#[test]
fn the_interfaces_own_capability_type_is_refused_at_its_position() {
    let mut module = lower(MODULE);
    let function = reaching(&module);
    let Operand::Value(value) = region_value(&module, function) else {
        unreachable!("the region parameter is an SSA value")
    };
    // Retype the region parameter as a capability of the interface it
    // represents. Nothing else about the artifact moves: the same SSA value, in
    // the same position, defined the same way.
    module
        .types
        .push(TypeDef::Capability(String::from("platform.dma.Region")));
    let capability = module.types.len() - 1;
    module.functions[function].values[value] = capability;
    module.functions[function].signature.parameters[0].ty = capability;
    forge(
        &mut module,
        CapabilitySource::Value(Operand::Value(value)),
        "platform.dma.Region",
        &["platform.dma.Region"],
    );
    let finding = refuse(&module);
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(
        finding
            .detail
            .contains("not how a capability of that interface is represented"),
        "{finding:?}"
    );
}

/// §17.4 — the region family at an `AsInterface` position.
///
/// The derivation answers `platform.dma.Region`, the instruction claims
/// `system.ipc.Endpoint`, and the comparison the verifier already made is what
/// catches it. Nothing new had to be invented for this case, which is the point
/// of extending the derivation rather than inverting the predicate.
#[test]
fn the_region_family_at_an_as_interface_position_is_refused() {
    let mut module = lower(MODULE);
    let function = reaching(&module);
    let region = region_value(&module, function);
    forge(
        &mut module,
        CapabilitySource::Value(region),
        "system.ipc.Endpoint",
        &["system.ipc.Endpoint"],
    );
    let finding = refuse(&module);
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(
        finding
            .detail
            .contains("declares system.ipc.Endpoint and is performed through platform.dma.Region"),
        "{finding:?}"
    );
}

/// §17.3 — the derived interface beats the artifact's claim, even when the
/// claim names an accepted interface the function really does declare.
///
/// Every other check passes: the interface exists, the function's effects
/// include it, the source resolves. The only thing wrong is that the operand's
/// type says something else — which is the whole reason the verifier derives
/// rather than believes.
#[test]
fn a_claim_the_function_really_declares_does_not_make_it_true() {
    let mut module = lower(MODULE);
    let function = reaching(&module);
    let region = region_value(&module, function);
    forge(
        &mut module,
        CapabilitySource::Value(region),
        "system.ipc.Reply",
        &["system.ipc.Reply"],
    );
    let finding = refuse(&module);
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(
        finding
            .detail
            .contains("declares system.ipc.Reply and is performed through platform.dma.Region"),
        "{finding:?}"
    );
}

/// §17.5 and §16.2 — a region, a device window and a scalar are not capability
/// positions at all.
///
/// Retyping the same SSA value is what isolates the rule: the artifact is
/// identical apart from one entry of the type table, so what is being refused
/// is the *type* and not the shape of the instruction.
#[test]
fn nothing_outside_a_representation_fills_a_capability_position() {
    for outside in [
        TypeDef::Region(0),
        TypeDef::RegionMut(0),
        TypeDef::MmioRegion,
        TypeDef::MmioRegionMut,
        TypeDef::Bool,
    ] {
        let mut module = lower(MODULE);
        let function = reaching(&module);
        let Operand::Value(value) = region_value(&module, function) else {
            unreachable!("the region parameter is an SSA value")
        };
        module.types.push(outside.clone());
        let retyped = module.types.len() - 1;
        module.functions[function].values[value] = retyped;
        module.functions[function].signature.parameters[0].ty = retyped;
        forge(
            &mut module,
            CapabilitySource::Value(Operand::Value(value)),
            "system.ipc.Endpoint",
            &["system.ipc.Endpoint"],
        );
        let finding = refuse(&module);
        assert_eq!(finding.code, "V2013_CAPABILITY", "{outside:?}");
        assert!(
            finding.detail.contains("not of any capability type"),
            "{outside:?}: {finding:?}"
        );
    }
}

/// §17.6 — the right representation with the effect missing.
///
/// The four dimensions refuse independently (§16.5), and this is the one that
/// says so: representation and effect are separate checks, and satisfying the
/// first buys nothing from the second.
#[test]
fn the_right_representation_without_the_effect_is_refused() {
    let mut module = lower(MODULE);
    let function = reaching(&module);
    let region = region_value(&module, function);
    forge(
        &mut module,
        CapabilitySource::Value(region),
        "platform.dma.Region",
        &[],
    );
    let finding = refuse(&module);
    assert_eq!(finding.code, "V2033_UNSAFE");
    assert!(
        finding.detail.contains("acts through platform.dma.Region"),
        "{finding:?}"
    );
}

/// §17.7 — a module whose header declares 1.2 does not get the 1.3 rule.
///
/// Everything else about this artifact is right: the representation, the
/// interface it claims, the effect the function declares. The one thing left is
/// the version, and **the version gate is a verifier obligation and not only a
/// frontend one** — a hand-written artifact never met the frontend that would
/// have refused it in source.
#[test]
fn a_one_two_artifact_does_not_receive_the_representation_rule() {
    let mut module = lower(MODULE);
    assert_eq!(module.header.language_version, "1.2");
    let function = reaching(&module);
    let region = region_value(&module, function);
    forge(
        &mut module,
        CapabilitySource::Value(region),
        "platform.dma.Region",
        &["platform.dma.Region"],
    );
    let finding = refuse(&module);
    assert_eq!(finding.code, "V2013_CAPABILITY");
    assert!(
        finding
            .detail
            .contains("the declared language version 1.2 does not have"),
        "{finding:?}"
    );
}

/// §16.11 — an implementation that does not admit a minor rejects the module
/// **whole, by its header**, rather than part-way through a function.
///
/// While this slice is in progress that is exactly the state of `1.3`: no
/// implementation admits it yet, so a correctly formed capability-representation
/// artifact is refused for its version and never for its body.
#[test]
fn an_unadmitted_minor_is_refused_by_the_header_alone() {
    let mut module = lower(MODULE);
    module.header.language_version = String::from("1.3");
    let finding = refuse(&module);
    assert_eq!(finding.code, "V2002_SCHEMA");
    assert_eq!(finding.location, "header.language_version");
}

/// Nothing the amendment adds touches an artifact that does not use it.
///
/// §14's third compatibility claim, over the artifact this whole file damages:
/// undamaged, it verifies exactly as it did before ADR-0085, through the same
/// derivation on the same inputs.
#[test]
fn an_ordinary_artifact_is_untouched_by_the_amendment() {
    let module = lower(MODULE);
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("a module that uses no representation rule verifies as it always did");
}
