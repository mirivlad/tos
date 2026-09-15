// SPDX-License-Identifier: GPL-3.0-or-later
//! An imported call is checked against the dependency this launch verified.
//!
//! Before this file, an imported call was admitted on the strength of a
//! **declaration**. `ResolutionSnapshot` states what a producer says the closure
//! resolved to, and the verifier asked it one question — does the module named
//! by this import declare this export name — and asked the dependency's own
//! artifact nothing at all. The snapshot was never compared to any image, so a
//! hostile declaration naming an export the real module does not have was
//! accepted, and a forged caller agreeing perfectly with a hostile declaration
//! about a signature the real module does not declare was accepted too. The
//! types crossed the boundary unchecked in both directions.
//!
//! What replaces it is ADR-0071 §5's reload rule, applied during launch:
//!
//! ```text
//! dependency verified earlier in this launch
//!   -> opaque evidence naming its artifact digest
//!   -> the same bytes fetched from the closure source
//!   -> hashed here and compared before a single export is read
//!   -> export prefix reconstructed by the one image decoder
//!   -> this caller's imported calls checked against it
//!   -> the decoded prefix dropped
//! ```
//!
//! **Nothing survives.** No typed export surface reaches
//! `VerifiedModuleRecord`, `VerifiedClosureManifest`, `ResolutionSnapshot`, a
//! bundle declaration or any other permanent launch state: what crosses from
//! one module's turn to the next is sixty-four bytes of identity that cannot
//! name a single export.
//!
//! **And the evidence cannot be supplied.** `VerifiedArtifactEvidence` has
//! private fields, no public constructor and exactly one source — a successful
//! verification performed by this same launch. A provider that could assert
//! "this dependency was verified, here is its signature" would be the
//! unauthenticated snapshot again under a better name.

use tos_core::{
    check_imported_calls, lower_module_in_set, Checker, LoweringInterface, ModuleContext, Parser,
    ResolvedImport, SourceReader,
};
use tos_ir::{CallTarget, Module, Op, TypeDef};
use tos_residency::ResidencyLimits;
use tos_verifier::ResolutionSnapshot;

const RESIDENCY: ResidencyLimits = ResidencyLimits {
    modules: 8,
    bytes: 64 * 1024 * 1024,
};

const ENVELOPE: &str = "resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, \
     workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 4]";

fn context(name: &str, path: &str, text: &str) -> ModuleContext {
    let _ = name;
    ModuleContext {
        source_set: String::from("tos-imported-signature-tests"),
        path: String::from(path),
        content_id: tos_pipeline::content_id(text.as_bytes()),
        dependency_digest: tos_pipeline::list_digest(&[]),
        capability_interface_digest: tos_pipeline::list_digest(&[]),
    }
}

/// Lowers one module of a set, with the dependencies it resolves against.
fn lowered(name: &str, path: &str, body: &str, imports: &[ResolvedImport<'_>]) -> Module {
    let text = format!("module {name} version 1.0 profile bootstrap; {body}");
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let diagnostics = Checker::check(&source, &schema);
    assert!(
        diagnostics.is_empty(),
        "the fixture checks: {:?}",
        diagnostics.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
    let imported = check_imported_calls(&source, &schema, imports);
    assert!(
        imported.is_empty(),
        "the fixture's imported calls check: {:?}",
        imported.iter().map(|d| d.code()).collect::<Vec<_>>()
    );
    lower_module_in_set(&source, &schema, &context(name, path, &text), imports)
        .expect("the fixture lowers")
}

/// The dependency every case below imports: one export, one `i64` parameter.
fn dependency() -> Module {
    lowered(
        "set.up",
        "set/up.tos",
        &format!("{ENVELOPE} pub fn f(value: i64) -> i64 {{ return value; }}"),
        &[],
    )
}

/// A caller of it, lowered against its real interface.
fn caller(dependency: &Module, body: &str) -> Module {
    let interface = LoweringInterface::of(dependency);
    lowered(
        "set.down",
        "set/down.tos",
        &format!("import set.up as up; {ENVELOPE} {body}"),
        &[ResolvedImport {
            name: "set.up",
            interface: &interface,
        }],
    )
}

const CALLS: &str = "pub fn main() -> i64 { return up.f(2i64); }";

fn image(module: &Module) -> tos_residency::ImageSnapshot {
    tos_image::encode(module).0.into_boxed_slice().into()
}

/// The verdict of a launch over an exact ordered closure.
fn launch(images: Vec<tos_residency::ImageSnapshot>, snapshot: &ResolutionSnapshot) -> String {
    let entry = images.len() - 1;
    match tos_pipeline::Prepared::launch_images(images, snapshot, entry, "main", RESIDENCY) {
        Ok(_) => String::from("accepted"),
        Err(tos_residency::Failure::Verifier { finding, .. }) => String::from(finding.code),
        Err(other) => format!("{other:?}"),
    }
}

/// A snapshot that says whatever it is told to say about a module.
fn claiming(name: &str, content_id: &str, exports: &[&str]) -> ResolutionSnapshot {
    let mut declared = tos_verifier::DeclaredResolution::new();
    declared.module(name, content_id).exports_declared();
    for export in exports {
        declared.export(export);
    }
    declared.build()
}

// ------------------------------------------------------- the attacks, dead

/// **Forged closure A.** The caller and the hostile declaration agree perfectly
/// with each other, and both disagree with the artifact.
///
/// The real `set.up` exports `f(i64) -> i64`. The snapshot names `f`, which is
/// true and is the only thing it was ever asked. The caller is forged to pass a
/// `bool` and to declare a `bool` result — self-consistent, so nothing inside it
/// is wrong. Until this slice the closure was admitted and the disagreement
/// reached the engine.
#[test]
fn a_forged_caller_agreeing_with_a_hostile_snapshot_is_refused() {
    let dependency = dependency();
    let mut caller = caller(&dependency, CALLS);
    assert_eq!(
        launch(
            vec![image(&dependency), image(&caller)],
            &ResolutionSnapshot::default()
        ),
        "accepted",
        "the undamaged closure launches"
    );

    // One `bool`, used as the operand and claimed as the result.
    let boolean = match caller.types.iter().position(|ty| *ty == TypeDef::Bool) {
        Some(found) => found,
        None => {
            caller.types.push(TypeDef::Bool);
            caller.types.len() - 1
        }
    };
    let (function, block, at) = first_imported_call(&caller);
    caller.functions[function].values.push(boolean);
    let slot = caller.functions[function].values.len() - 1;
    let instruction = &mut caller.functions[function].blocks[block].instructions[at];
    instruction.ty = boolean;
    if let Some(result) = instruction.result {
        caller.functions[function].values[result] = boolean;
    }
    if let Op::Call { operands, .. } =
        &mut caller.functions[function].blocks[block].instructions[at].op
    {
        operands[0] = tos_ir::Operand::Value(slot);
    }
    // The export table follows the bodies, so the caller stays canonical in
    // every way but the one this is about.
    reproject(&mut caller);

    let snapshot = claiming("set.up", &dependency.header.content_id, &["f"]);
    assert_eq!(
        launch(vec![image(&dependency), image(&caller)], &snapshot),
        "V2010_TYPE",
        "a signature the artifact does not declare was accepted because a declaration agreed"
    );
}

/// **Forged closure B.** The declaration names an export the artifact does not
/// have, and the caller calls it.
///
/// This is the case the old check could not even fail: `export_surface` was the
/// authority, so a name in the snapshot *was* the module exporting it.
#[test]
fn a_call_to_an_export_the_artifact_does_not_have_is_refused() {
    let dependency = dependency();
    let mut caller = caller(&dependency, CALLS);
    let (function, block, at) = first_imported_call(&caller);
    if let Op::Call {
        target: CallTarget::Imported { name, .. },
        ..
    } = &mut caller.functions[function].blocks[block].instructions[at].op
    {
        *name = String::from("ghost");
    }

    let snapshot = claiming("set.up", &dependency.header.content_id, &["f", "ghost"]);
    assert_eq!(
        launch(vec![image(&dependency), image(&caller)], &snapshot),
        "V2012_IMPORT",
        "a declared export the module does not have was accepted"
    );
}

/// The bytes offered for a dependency are hashed before an export is read.
///
/// **The provider is changed between the two reads**, which is the only way to
/// separate this obligation from every other one: position 0 verifies as the
/// real `set.up`, and when the caller's verification asks for those bytes again
/// the source hands over a different module under the same name. ADR-0071 §5's
/// rule is exactly this comparison, and what it protects here is the signature
/// a caller is checked against.
#[test]
fn a_substituted_dependency_image_is_refused_before_its_exports_are_read() {
    /// A closure source that answers position 0 differently the second time.
    struct Flipping {
        first: tos_residency::ImageSnapshot,
        second: tos_residency::ImageSnapshot,
        caller: tos_residency::ImageSnapshot,
        served: core::cell::Cell<usize>,
    }

    impl tos_residency::ClosureSource for Flipping {
        fn count(&self) -> usize {
            2
        }

        fn image(&self, position: usize) -> Option<tos_residency::ImageSnapshot> {
            if position == 1 {
                return Some(self.caller.clone());
            }
            let served = self.served.get();
            self.served.set(served + 1);
            Some(if served == 0 {
                self.first.clone()
            } else {
                self.second.clone()
            })
        }
    }

    let dependency = dependency();
    let caller = caller(&dependency, CALLS);
    // A different artifact, wearing the same module name: one more export, so
    // the bytes differ and a declaration could not tell the two apart.
    let other = lowered(
        "set.up",
        "set/up.tos",
        &format!(
            "{ENVELOPE} pub fn f(value: i64) -> i64 {{ return value; }} \
             pub fn g(value: i64) -> i64 {{ return value; }}"
        ),
        &[],
    );

    let source = Flipping {
        first: image(&dependency),
        second: image(&other),
        caller: image(&caller),
        served: core::cell::Cell::new(0),
    };
    let resolution = |_: usize| ResolutionSnapshot::default();
    match tos_residency::launch(
        &source,
        &resolution,
        &tos_verifier::Limits::default(),
        1,
        "main",
    ) {
        Ok(_) => panic!("a substituted dependency image lent its exports to a caller"),
        Err(tos_residency::Failure::Verifier { module: 1, finding }) => {
            assert_eq!(finding.code, "V2012_IMPORT");
            assert!(
                finding
                    .detail
                    .contains("not the artifact this launch verified"),
                "{finding:?}"
            );
        }
        Err(other) => panic!("the launch failed differently: {other:?}"),
    }
}

/// A closure whose modules are not dependency-first is refused (ADR-0071 §1).
#[test]
fn a_caller_before_its_dependency_is_refused() {
    let dependency = dependency();
    let caller = caller(&dependency, CALLS);
    let images = vec![image(&caller), image(&dependency)];
    match tos_pipeline::Prepared::launch_images(
        images,
        &ResolutionSnapshot::default(),
        0,
        "main",
        RESIDENCY,
    ) {
        Ok(_) => panic!("a caller was verified before the module it imports"),
        Err(tos_residency::Failure::Verifier { module: 0, finding }) => {
            assert_eq!(finding.code, "V2012_IMPORT");
            assert!(
                finding.detail.contains("has not been verified earlier"),
                "{finding:?}"
            );
        }
        Err(other) => panic!("the launch failed differently: {other:?}"),
    }
}

/// And a cycle, which the same rule refuses without a second algorithm.
#[test]
fn a_two_module_cycle_is_refused_without_a_cycle_check() {
    let dependency = dependency();
    let mut caller = caller(&dependency, CALLS);
    // Make the dependency import its own caller: whichever of the two is
    // verified first names a module that has not been verified.
    let mut cyclic = dependency.clone();
    cyclic.header.resource_envelope.imports = 4;
    cyclic.imports.push(tos_ir::Import {
        module_name: String::from("set.down"),
        module_content_id: caller.header.content_id.clone(),
        binding: String::from("down"),
    });
    caller.header.resource_envelope.imports = 4;
    assert_eq!(
        launch(
            vec![image(&cyclic), image(&caller)],
            &ResolutionSnapshot::default()
        ),
        "V2012_IMPORT"
    );
}

// ------------------------------------------------- imported source checking

/// The verdict of the dependency-aware source check on a caller.
fn checked(body: &str) -> String {
    let dependency = dependency();
    let interface = LoweringInterface::of(&dependency);
    let text = format!(
        "module set.down version 1.0 profile bootstrap; import set.up as up; {ENVELOPE} {body}"
    );
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let imports = [ResolvedImport {
        name: "set.up",
        interface: &interface,
    }];
    match check_imported_calls(&source, &schema, &imports).first() {
        None => String::from("accepted"),
        Some(diagnostic) => String::from(diagnostic.code()),
    }
}

#[test]
fn a_correct_imported_call_checks() {
    assert_eq!(checked(CALLS), "accepted");
}

#[test]
fn an_imported_call_of_the_wrong_arity_is_an_arity_mismatch() {
    assert_eq!(
        checked("pub fn main() -> i64 { return up.f(1i64, 2i64); }"),
        "E1217_CALL_ARITY_MISMATCH"
    );
}

#[test]
fn an_imported_call_with_a_wrong_argument_type_is_refused() {
    assert_eq!(
        checked("pub fn main() -> i64 { return up.f(true); }"),
        "E1215_ARGUMENT_TYPE_MISMATCH"
    );
}

/// Two exact integer types keep the numeric code, as everywhere else.
#[test]
fn an_imported_call_with_a_wrong_integer_type_is_the_numeric_code() {
    assert_eq!(
        checked("pub fn main() -> i64 { return up.f(1i32); }"),
        "E1210_INTEGER_TYPE_MISMATCH"
    );
}

/// The import resolved and the module is known, so a member it does not export
/// is an unresolved value name rather than a type disagreement.
#[test]
fn a_call_to_a_member_the_module_does_not_export_is_an_unknown_value_name() {
    assert_eq!(
        checked("pub fn main() -> i64 { return up.no_such_function(1i64); }"),
        "E1202_UNKNOWN_VALUE_NAME"
    );
}

/// An unsuffixed literal takes the type the export declares, exactly as it does
/// at a local call (`docs/40` §3).
#[test]
fn an_unsuffixed_literal_takes_the_imported_parameter_type() {
    let dependency = dependency();
    let caller = caller(&dependency, "pub fn main() -> i64 { return up.f(2); }");
    assert!(
        caller
            .constants
            .iter()
            .any(|constant| matches!(constant, tos_ir::Constant::Int(tos_ir::IntKind::I64, 2))),
        "the literal is an i64: {:?}",
        caller.constants
    );
    assert_eq!(
        launch(
            vec![image(&dependency), image(&caller)],
            &ResolutionSnapshot::default()
        ),
        "accepted"
    );
}

// ------------------------------------------- source is not needed at admission

/// The whole production path, with the source gone before admission.
///
/// Build, bundle, drop everything the build held, admit the bundle, and run.
/// The imported call is checked against the dependency image inside the bundle,
/// which the launch verified a moment earlier — no typed surface is stored in
/// the declaration and none is needed.
#[test]
fn a_bundle_admits_and_runs_with_no_source_and_no_typed_declaration() {
    let bytes = {
        let dependency = dependency();
        let caller = caller(&dependency, CALLS);
        let mut room = vec![0u8; 8 * 1024 * 1024];
        let mut backing = tos_bundle::SliceBacking::new(&mut room);
        let mut writer = tos_bundle::BundleWriter::new(&mut backing);
        for module in [&dependency, &caller] {
            let image = tos_image::encode(module).0;
            let exports: Vec<&str> = module
                .exports
                .iter()
                .map(|signature| signature.name.as_str())
                .collect();
            writer
                .module(
                    &tos_bundle::ModuleClaim {
                        name: &module.header.module_name,
                        content_id: &module.header.content_id,
                        exports,
                        capabilities: Vec::new(),
                    },
                    &image,
                )
                .expect("the bundle has room");
        }
        let length = writer
            .finish(1, "set/down.tos")
            .expect("the bundle finishes");
        room.truncate(length);
        room
    };

    let bundle = tos_bundle::Bundle::parse(&bytes).expect("the bundle parses");
    let mut trace = tos_pipeline::Silent;
    match tos_pipeline::admit_bundle(&bundle, "main", &mut trace, RESIDENCY) {
        tos_pipeline::Preparation::Ready(mut prepared) => {
            let outcome = prepared
                .run(Vec::new(), &mut tos_engine::Unreachable)
                .expect("the entry runs")
                .expect("the run completes");
            assert_eq!(
                outcome.value,
                tos_engine::Value::Int(tos_ir::IntKind::I64, 2)
            );
        }
        tos_pipeline::Preparation::Refused(refusal) => {
            panic!("the bundle was refused: {refusal:?}")
        }
    }
}

// ------------------------------------------------------------------ helpers

fn first_imported_call(module: &Module) -> (usize, usize, usize) {
    for (f, function) in module.functions.iter().enumerate() {
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, instruction) in block.instructions.iter().enumerate() {
                if matches!(
                    instruction.op,
                    Op::Call {
                        target: CallTarget::Imported { .. },
                        ..
                    }
                ) {
                    return (f, b, i);
                }
            }
        }
    }
    panic!("the fixture contains an imported call");
}

/// Keeps `Module::exports` the canonical public projection of `functions`
/// (`docs/43` §2) after a forgery has changed a signature.
fn reproject(module: &mut Module) {
    module.exports = module
        .functions
        .iter()
        .filter(|function| function.signature.visibility == tos_ir::Visibility::Public)
        .map(|function| function.signature.clone())
        .collect();
}

// ------------------------------------------- the export table is a projection

// `Module::exports` is exactly the canonical public projection of
// `Module::functions` (`docs/43` §2). Until this slice the verifier checked
// only that it was sorted, while `Closure::export_of` resolved a call against
// the resident module's `functions` — so an authenticated export table could
// describe a function nobody executes. Each case damages one field of a module
// that verified a moment earlier.

fn verdict(module: &Module) -> String {
    match tos_verifier::verify(
        module,
        &ResolutionSnapshot::default(),
        &tos_verifier::Limits::default(),
    ) {
        Ok(_) => String::from("accepted"),
        Err(finding) => String::from(finding.code),
    }
}

/// A module with two public functions and one private one.
fn projected() -> Module {
    lowered(
        "set.surface",
        "set/surface.tos",
        &format!(
            "{ENVELOPE} pub fn a(value: i64) -> i64 {{ return value; }} \
             pub fn b(flag: bool) -> bool {{ return flag; }} \
             fn hidden(value: i64) -> i64 {{ return value; }}"
        ),
        &[],
    )
}

#[test]
fn the_projection_of_an_undamaged_module_verifies() {
    assert_eq!(verdict(&projected()), "accepted");
}

#[test]
fn an_export_with_a_changed_parameter_type_is_refused() {
    let mut module = projected();
    let boolean = module
        .types
        .iter()
        .position(|ty| *ty == TypeDef::Bool)
        .expect("the fixture declares a bool");
    module.exports[0].parameters[0].ty = boolean;
    assert_eq!(verdict(&module), "V2004_TABLE_ORDER");
}

#[test]
fn an_export_with_a_changed_result_is_refused() {
    let mut module = projected();
    let boolean = module
        .types
        .iter()
        .position(|ty| *ty == TypeDef::Bool)
        .expect("the fixture declares a bool");
    module.exports[0].result = boolean;
    assert_eq!(verdict(&module), "V2004_TABLE_ORDER");
}

/// A mode is not compared at an imported call, and is compared here: what the
/// export table describes must be the function execution enters.
#[test]
fn an_export_with_a_changed_pass_mode_is_refused() {
    let mut module = projected();
    module.exports[0].parameters[0].mode = tos_ir::PassMode::SharedBorrow;
    assert_eq!(verdict(&module), "V2004_TABLE_ORDER");
}

#[test]
fn an_export_with_a_changed_async_flag_is_refused() {
    let mut module = projected();
    module.exports[0].is_async = true;
    assert_eq!(verdict(&module), "V2004_TABLE_ORDER");
}

#[test]
fn an_export_with_changed_effects_is_refused() {
    let mut module = projected();
    module.exports[0]
        .effects
        .push(String::from("system.time.Clock"));
    assert_eq!(verdict(&module), "V2004_TABLE_ORDER");
}

#[test]
fn a_missing_public_export_is_refused() {
    let mut module = projected();
    module.exports.remove(0);
    assert_eq!(verdict(&module), "V2004_TABLE_ORDER");
}

#[test]
fn an_extra_export_is_refused() {
    let mut module = projected();
    let extra = module.exports[0].clone();
    module.exports.push(extra);
    assert_eq!(verdict(&module), "V2004_TABLE_ORDER");
}

#[test]
fn a_duplicated_export_is_refused() {
    let mut module = projected();
    module.exports[1] = module.exports[0].clone();
    assert_eq!(verdict(&module), "V2004_TABLE_ORDER");
}

/// A private function published through the export table.
#[test]
fn a_private_function_exported_is_refused() {
    let mut module = projected();
    let hidden = module
        .functions
        .iter()
        .find(|function| function.signature.name == "hidden")
        .expect("the fixture declares a private function")
        .signature
        .clone();
    module.exports.push(hidden);
    assert_eq!(verdict(&module), "V2004_TABLE_ORDER");
}

// ------------------------------------------------ the closure edge ceiling

// ADR-0090: a resolved closure holds at most 1024 unique caller-to-dependency
// relationships. The bound exists because the number of authenticated reopens a
// launch performs is exactly that number, and every other accepted ceiling left
// it able to reach 32 256. The frontend refuses a source set above it with
// `E1609_IMPORT_EDGE_LIMIT`; the launch does not take that refusal's word for
// it and counts the edges it is about to pay for.

/// The envelope the ceiling fixtures declare, wide enough to admit their fan.
const WIDE: &str = "resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, \
     workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 255]";

/// A two-layer closure: `providers` leaves, then one caller per entry of `fan`,
/// importing and **calling** that many of them.
///
/// Two layers rather than a chain because the edge count is then exactly
/// `fan.iter().sum()` and can be aimed at a number, and because every caller
/// reaches back into the same small set of dependencies — which is what makes
/// the uniqueness of an edge a *pair* rather than a property of either end.
fn layered(providers: usize, fan: &[usize]) -> Vec<Module> {
    let mut modules: Vec<Module> = Vec::with_capacity(providers + fan.len());
    for index in 0..providers {
        let text = format!(
            "module set.p{index} version 1.0 profile bootstrap; {WIDE} \
             pub fn f(value: i64) -> i64 {{ return value; }} \
             pub fn main() -> i64 {{ return 0i64; }}"
        );
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("the fixture parses");
        modules.push(
            lower_module_in_set(
                &source,
                &schema,
                &context("", &format!("set/p{index}.tos"), &text),
                &[],
            )
            .expect("the fixture lowers"),
        );
    }
    for (caller, width) in fan.iter().enumerate() {
        assert!(
            *width <= providers,
            "a caller cannot import more than exist"
        );
        let head: String = (0..*width)
            .map(|d| format!("import set.p{d} as d{d}; "))
            .collect();
        let calls: String = (0..*width)
            .map(|d| {
                if d == 0 {
                    format!("d{d}.f({d}i64)")
                } else {
                    format!(" + d{d}.f({d}i64)")
                }
            })
            .collect();
        let body = if *width == 0 {
            String::from("return 0i64;")
        } else {
            format!("return {calls};")
        };
        let text = format!(
            "module set.c{caller} version 1.0 profile bootstrap; {head}{WIDE} \
             pub fn main() -> i64 {{ {body} }}"
        );
        let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
        let schema = Parser::parse_schema(&source)
            .into_accepted()
            .expect("the fixture parses");
        let carried: Vec<(String, LoweringInterface)> = (0..*width)
            .map(|d| (format!("set.p{d}"), LoweringInterface::of(&modules[d])))
            .collect();
        let imports: Vec<ResolvedImport<'_>> = carried
            .iter()
            .map(|(name, interface)| ResolvedImport {
                name: name.as_str(),
                interface,
            })
            .collect();
        modules.push(
            lower_module_in_set(
                &source,
                &schema,
                &context("", &format!("set/c{caller}.tos"), &text),
                &imports,
            )
            .expect("the fixture lowers"),
        );
    }
    modules
}

/// The unique `(caller identity, resolved dependency identity)` pairs a closure
/// actually contains, counted from the lowered artifacts.
///
/// **Counted, never claimed.** A fixture that says how many edges it has is a
/// label; this walks the artifacts the launch will be handed, deduplicates each
/// caller's bindings the way ADR-0090 §2a defines an edge, and returns what is
/// there. Caller identities are asserted distinct, so two modules cannot
/// collapse into one row and hide an edge.
fn unique_edges(modules: &[Module]) -> usize {
    use std::collections::BTreeSet;
    let mut names: BTreeSet<&str> = BTreeSet::new();
    for module in modules {
        assert!(
            names.insert(module.header.module_name.as_str()),
            "two modules of the fixture share the identity {}",
            module.header.module_name
        );
    }
    let mut edges: BTreeSet<(&str, &str)> = BTreeSet::new();
    for module in modules {
        for import in &module.imports {
            edges.insert((
                module.header.module_name.as_str(),
                import.module_name.as_str(),
            ));
        }
    }
    edges.len()
}

/// A closure source that records every position it is asked for, in order.
///
/// The launch reads each module's image once to verify it, and the verifier
/// reads a dependency's image again for each authenticated reopen. So the
/// recording is the reopen ledger: what was paid, for whom, and in what order.
struct Recording {
    images: Vec<tos_residency::ImageSnapshot>,
    reads: core::cell::RefCell<Vec<usize>>,
}

impl tos_residency::ClosureSource for Recording {
    fn count(&self) -> usize {
        self.images.len()
    }

    fn image(&self, position: usize) -> Option<tos_residency::ImageSnapshot> {
        self.reads.borrow_mut().push(position);
        self.images.get(position).cloned()
    }
}

impl Recording {
    fn of(modules: &[Module]) -> Recording {
        Recording {
            images: modules.iter().map(image).collect(),
            reads: core::cell::RefCell::new(Vec::new()),
        }
    }
}

/// **Exactly 1024 unique edges launches.** 8 providers and 128 callers of all
/// eight: the ceiling itself, not a number near it.
#[test]
fn a_closure_of_exactly_one_thousand_and_twenty_four_edges_launches() {
    let modules = layered(8, &[8; 128]);
    assert_eq!(
        unique_edges(&modules),
        1024,
        "the closure counted, not labelled"
    );
    assert_eq!(modules.len(), 136);

    let source = Recording::of(&modules);
    let resolution = |_: usize| ResolutionSnapshot::default();
    let entry = modules.len() - 1;
    tos_residency::launch(
        &source,
        &resolution,
        &tos_verifier::Limits::default(),
        entry,
        "main",
    )
    .expect("a closure at the ceiling is inside it");

    // And the ledger agrees with the ADR's own claim about what the ceiling
    // bounds: one image read per module to verify it, plus exactly one
    // authenticated reopen per edge.
    let reads = source.reads.borrow();
    assert_eq!(reads.len(), modules.len() + 1024);
}

/// **Exactly 1025 unique edges is refused, before the crossing edge is paid.**
///
/// The same 128 callers, and then one more importing a single provider — the
/// 1025th relationship in the closure, and the one that crosses. It calls what
/// it imports, so a reopen is what would come next.
///
/// Exactly 1025 edges puts the crossing at the final module by construction:
/// any module after it would carry edges of its own and the total would no
/// longer be 1025. What is proved instead is sharper than position. The reopen
/// ledger shows the launch paid 1024 authenticated reopens and not 1025: after
/// the crossing caller's own image was read, nothing was read at all, so the
/// dependency it crosses on was never reopened for it. The topology quota
/// refused the relationship before the semantic work that relationship asks
/// for.
#[test]
fn exactly_one_edge_past_the_ceiling_is_refused_before_its_reopen() {
    let mut fan = vec![8usize; 128];
    fan.push(1);
    let modules = layered(8, &fan);
    assert_eq!(
        unique_edges(&modules),
        1025,
        "the closure counted, not labelled"
    );
    assert_eq!(modules.len(), 137);
    let crossing = modules.len() - 1;
    // The crossing caller does import, and does call, the dependency it would
    // otherwise reopen — so "no reopen" below is a refusal, not a vacancy.
    assert_eq!(modules[crossing].imports.len(), 1);
    assert_eq!(modules[crossing].imports[0].module_name, "set.p0");

    let source = Recording::of(&modules);
    let resolution = |_: usize| ResolutionSnapshot::default();
    match tos_residency::launch(
        &source,
        &resolution,
        &tos_verifier::Limits::default(),
        crossing,
        "main",
    ) {
        Ok(_) => panic!("a closure of 1025 edges was admitted"),
        Err(tos_residency::Failure::Verifier { module, finding }) => {
            assert_eq!(module, crossing);
            assert_eq!(finding.code, "V2001_LIMIT");
            assert_eq!(finding.location, "resolved closure direct dependency edges");
            assert!(
                finding.detail.contains("1025") && finding.detail.contains("1024"),
                "{finding:?}"
            );
        }
        Err(other) => panic!("the launch failed differently: {other:?}"),
    }

    let reads = source.reads.borrow();
    // 137 modules verified, 1024 reopens paid — one per edge below the ceiling
    // and none for the edge that crosses it.
    assert_eq!(reads.len(), modules.len() + 1024);
    assert_eq!(
        *reads.last().expect("the launch read something"),
        crossing,
        "the last read was the crossing caller's own image, so nothing was reopened for it"
    );
}

/// Duplicate bindings still cost one edge at the ceiling itself.
///
/// The same 1024-edge closure, with the last caller binding each of its eight
/// dependencies twice. Sixteen declarations, eight relationships: counted by
/// declaration this closure would be 1032 and refused, and it launches.
#[test]
fn duplicate_bindings_do_not_consume_the_ceiling_twice() {
    let mut modules = layered(8, &[8; 127]);
    let head: String = (0..8)
        .map(|d| format!("import set.p{d} as d{d}; import set.p{d} as e{d}; "))
        .collect();
    let calls: String = (0..8)
        .map(|d| {
            let lead = if d == 0 { "" } else { " + " };
            format!("{lead}d{d}.f({d}i64) + e{d}.f({d}i64)")
        })
        .collect();
    let text = format!(
        "module set.c127 version 1.0 profile bootstrap; {head}{WIDE} \
         pub fn main() -> i64 {{ return {calls}; }}"
    );
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let carried: Vec<(String, LoweringInterface)> = (0..8)
        .map(|d| (format!("set.p{d}"), LoweringInterface::of(&modules[d])))
        .collect();
    let imports: Vec<ResolvedImport<'_>> = carried
        .iter()
        .map(|(name, interface)| ResolvedImport {
            name: name.as_str(),
            interface,
        })
        .collect();
    modules.push(
        lower_module_in_set(
            &source,
            &schema,
            &context("", "set/c127.tos", &text),
            &imports,
        )
        .expect("the fixture lowers"),
    );

    let doubled = &modules[modules.len() - 1];
    assert_eq!(doubled.imports.len(), 16, "sixteen declarations");
    assert_eq!(unique_edges(&modules), 1024, "eight relationships");

    let source = Recording::of(&modules);
    let resolution = |_: usize| ResolutionSnapshot::default();
    let entry = modules.len() - 1;
    tos_residency::launch(
        &source,
        &resolution,
        &tos_verifier::Limits::default(),
        entry,
        "main",
    )
    .expect("a duplicate binding is one relationship");

    // Sixteen bindings, eight reopens: the relationship is what is paid for.
    let reads = source.reads.borrow();
    assert_eq!(reads.len(), modules.len() + 1024);
}

/// Two bindings of one module are one relationship and cost one edge.
#[test]
fn two_bindings_of_one_module_are_one_edge() {
    let dependency = dependency();
    let interface = LoweringInterface::of(&dependency);
    let text = format!(
        "module set.down version 1.0 profile bootstrap; \
         import set.up as first; import set.up as second; {ENVELOPE} \
         pub fn main() -> i64 {{ return first.f(1i64) + second.f(2i64); }}"
    );
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let imports = [ResolvedImport {
        name: "set.up",
        interface: &interface,
    }];
    assert!(check_imported_calls(&source, &schema, &imports).is_empty());
    let caller = lower_module_in_set(
        &source,
        &schema,
        &context("", "set/down.tos", &text),
        &imports,
    )
    .expect("the fixture lowers");
    // Two declarations, and the artifact records them; what the ceiling counts
    // is the relationship, so the launch pays for one authenticated reopen.
    assert_eq!(caller.imports.len(), 2);

    let modules = [dependency, caller];
    let source = Recording::of(&modules);
    let resolution = |_: usize| ResolutionSnapshot::default();
    tos_residency::launch(
        &source,
        &resolution,
        &tos_verifier::Limits::default(),
        1,
        "main",
    )
    .expect("two bindings of one dependency launch");

    // **And it pays for one.** Two modules verified, one reopen: the second
    // binding reaches the surface the first one reconstructed. This is the
    // sentence ADR-0090 §2a rests its edge definition on, and without it the
    // ceiling would bound relationships while the launch paid per declaration —
    // up to 255 authenticated reopens for one edge of budget.
    let reads = source.reads.borrow();
    assert_eq!(reads.len(), 3);
    assert_eq!(*reads, vec![0, 1, 0]);
}
