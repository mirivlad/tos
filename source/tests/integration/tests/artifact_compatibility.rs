// SPDX-License-Identifier: GPL-3.0-or-later
//! Existing artifacts are byte-identical across ADR-0085 (§12, §14, §16.1).
//!
//! The amendment's compatibility claims are stated as consequences of "no
//! encoding path is touched", and that is the thing to check rather than the
//! conclusions. So this pins what an unchanged module actually produces:
//!
//! - the **module digest**, which is what cache, provenance and residency
//!   identity are built on;
//! - the **image bytes**, by length and by content hash, which is where a
//!   renumbered TOSIMAGE tag would show up and a digest would not — the
//!   canonical stream has discriminants of its own, so a tag renumbered
//!   consistently in encoder and parser would round-trip and rehash the same.
//!
//! **The constants were taken from before the slice, not from after it.** Each
//! was read by running the same four modules at `28c661f` — the last commit
//! before ADR-0085's implementation began — and compared with the value the
//! present tree produces. They matched, which is the claim; recording them here
//! is what keeps them matching.
//!
//! ADR-0085 §11's tag table is covered by the fourth module, which names every
//! type in it. `capability_interface_digest` is over **imported interface
//! paths**, not over the interface schema's text, so moving
//! `SYSTEM_INTERFACE_V1` to version 2 and `PLATFORM_INTERFACE_V1` to version 3
//! moves no module's identity — the modules below import what they always
//! imported.

use tos_ir::Module;

/// A 1.0 module reaching an interface through an import.
const V1_0: &str = "\
module system.test.compat version 1.0 profile full;
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

/// A 1.1 module using ADR-0080's direct-interface effect form.
const V1_1: &str = "\
module system.test.compat version 1.1 profile full;
import capability system.process.Control as control;

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

extern fn capability_release(cap: system.process.Control) -> i64 uses [system.process.Control];

pub fn main() -> i64 uses [system.process.Control] {
    return capability_release(control);
}
";

/// A 1.2 module holding ADR-0081's device memory and a DMA region.
const V1_2: &str = "\
module system.test.compat version 1.2 profile full;

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
    imports: 0
]

pub fn main(window: MmioRegionMut, area: DmaRegion<mut u64>) -> u64 {
    return mmio_read_le_u64(window, 0B);
}
";

/// Every type in ADR-0085 §11's tag table, in one artifact.
///
/// `Region` 19, `DmaRegion` 20, `Capability` 29, `RegionMut` 34,
/// `DmaRegionMut` 35, `MmioRegion` 36, `MmioRegionMut` 37. If any of them were
/// added, removed or renumbered, this module's image bytes would move — which
/// is the check the ADR asks a reader to make rather than to take on trust.
const EVERY_TAG: &str = "\
module system.test.compat version 1.2 profile full;
import capability system.ipc.Endpoint as endpoint;

resource [
    fuel: 4096,
    stack: 8KiB,
    allocation: 4KiB,
    tasks: 1,
    workers: 1,
    sync: 0,
    shared: 0B,
    cleanup: 0,
    recursion: 4,
    imports: 1
]

extern fn endpoint_send(cap: system.ipc.Endpoint, length: u64) -> i64 uses [endpoint];

pub fn every_tag(
    plain: Region<u64>,
    plain_mut: Region<mut u64>,
    dma: DmaRegion<u64>,
    dma_mut: DmaRegion<mut u64>,
    window: MmioRegion,
    window_mut: MmioRegionMut
) -> i64 uses [endpoint] {
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
        "the module checks clean: {diagnostics:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("compat"),
            path: String::from("system/test/compat.tos"),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

/// The digest, the image length and the image's own hash, as one line.
fn identity(text: &str) -> (String, usize, String) {
    let module = lower(text);
    let (bytes, _) = tos_image::encode(&module);
    let mut hex = [0u8; 64];
    tos_hash::hex(&tos_hash::sha256(&bytes), &mut hex);
    (
        tos_ir::digest::module_digest(&module),
        bytes.len(),
        String::from(std::str::from_utf8(&hex).expect("hex is ASCII")),
    )
}

#[test]
fn an_existing_module_keeps_its_digest_and_its_exact_image() {
    for (text, digest, length, image) in [
        (
            V1_0,
            "sha256:fcf587a81484709281d9cc0e5138222c82263b02d08e3addbc249b995787d89b",
            490,
            "48e998c7ee4e7c72fe1cb05900c338227d7076c86bad8bcc8e3287b084c2a0bd",
        ),
        (
            V1_1,
            "sha256:9f5097e3d062367848de9043ab9ff7ad973b9c9baa175afe68ddc91c86c75d70",
            489,
            "39a40a141a54d8468505a133f0a709e4657ce16a1aeeadaae5fd36dc53654d6a",
        ),
        (
            V1_2,
            "sha256:b29a565ade7b17bc94ea97ac7536cbee8b7a2fac438a577a3f42667a1cab917e",
            471,
            "7ba2d78d35912a0be895d8e6646dffe7beb262397899a32aada4476ee56b1d0e",
        ),
        (
            EVERY_TAG,
            "sha256:56aee62dad0bbe1a067c58258891b40dfc6af240fe82e8206c8e0d35a9a53995",
            597,
            "a5c64690e6f45e27fcb5d100ee42decbf65ea776047a34790033b0d139469552",
        ),
    ] {
        assert_eq!(
            identity(text),
            (String::from(digest), length, String::from(image))
        );
    }
}

/// Every type of §11's table survives a round trip through the image format.
///
/// The byte pin above catches a renumbering; this catches a tag that stopped
/// being decoded as what it was encoded as, which is the other half of "no tag
/// is reinterpreted".
#[test]
fn every_tagged_type_decodes_as_what_it_was_encoded_as() {
    use tos_ir::TypeDef;
    let module = lower(EVERY_TAG);
    let (bytes, _) = tos_image::encode(&module);
    // Stated rather than defaulted: `ParseLimits` deliberately has no
    // `Default`, because a caller that has not said what it will accept has not
    // said anything.
    let parsed = tos_image::parse(
        &bytes,
        &tos_image::ParseLimits {
            table_entries: 65_536,
            modules: 256,
            fields: 1024,
            parameters: 128,
            blocks_per_function: 4096,
            instructions_per_block: 65_536,
            source_map_entries: 262_144,
        },
    )
    .expect("the image parses");
    assert_eq!(parsed.types, module.types);
    // And the seven are all really in there, so the assertion above is about
    // something rather than vacuously true of a table that lost them.
    for wanted in [
        "Region",
        "RegionMut",
        "DmaRegion",
        "DmaRegionMut",
        "MmioRegion",
        "MmioRegionMut",
        "Capability",
    ] {
        assert!(
            module.types.iter().any(|ty| matches!(
                (wanted, ty),
                ("Region", TypeDef::Region(_))
                    | ("RegionMut", TypeDef::RegionMut(_))
                    | ("DmaRegion", TypeDef::DmaRegion(_))
                    | ("DmaRegionMut", TypeDef::DmaRegionMut(_))
                    | ("MmioRegion", TypeDef::MmioRegion)
                    | ("MmioRegionMut", TypeDef::MmioRegionMut)
                    | ("Capability", TypeDef::Capability(_))
            )),
            "{wanted} is not in the fixture's type table"
        );
    }
}
