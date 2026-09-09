// SPDX-License-Identifier: GPL-3.0-or-later
//! One DMA region, from the operation that makes it to the two ways of reaching
//! it (Stage 4C-2; ADR-0084, ADR-0085 §9, §8a; ADR-0081 §2).
//!
//! The whole path the accepted decisions specify, with only the **nucleus**
//! replaced: source, frontend representation rule, lowering, IR, the
//! independent verifier, the engine, and the runtime bridge's own mapping table
//! — `tos_launch::DeviceMappings`, the real one the runtime image uses, not a
//! model of it.
//!
//! What the host below stands in for is the nucleus: it mints a handle for a
//! successful allocation and writes down the handles it is asked to act on. It
//! does **not** model the nucleus's object-kind, right, generation or liveness
//! checks — those refusals belong to the nucleus and are not evidence a double
//! can give.
//!
//! **Why the handles are inspected rather than inferred.** ADR-0085 §9's claim
//! is that a region is *one* handle reached two ways. Two calls both succeeding
//! is consistent with two handles, so the numbers are recorded and compared.

use std::collections::BTreeMap;

use tos_verifier::{verify, Limits, ResolutionSnapshot};

/// A module that allocates a region, writes and reads through it, and asks the
/// nucleus for its device-visible address — all on **one** binding.
const MODULE: &str = "\
module system.test.dma version 1.3 profile full;
import capability platform.pci.FunctionConfig as device;
import capability system.memory.Authority as budget;

resource [
    fuel: 65536, stack: 16KiB, allocation: 4KiB, tasks: 1, workers: 1,
    sync: 0, shared: 0B, cleanup: 0, recursion: 8, imports: 2
]

extern fn dma_region_allocate(
    function: platform.pci.FunctionConfig,
    authority: system.memory.Authority,
    bytes: size
) -> Result<DmaRegion<mut u8>, i64> uses [device, budget];

extern fn dma_device_address(region: platform.dma.Region, offset: size)
    -> Result<u64, i64> uses [platform.dma.Region];

extern fn capability_release(region: platform.dma.Region) -> i64
    uses [platform.dma.Region];

pub fn main() -> i64 uses [device, budget, platform.dma.Region] {
    let made: Result<DmaRegion<mut u8>, i64> = dma_region_allocate(device, budget, 4096B);
    match (made) {
        Ok(region) => {
            region[0B] = 200u8;
            let first: u8 = region[0B];
            let last: u8 = region[4095B];
            let address: Result<u64, i64> = dma_device_address(region, 0B);
            let ended: i64 = capability_release(region);
            return ended;
        }
        Err(status) => {
            return status;
        }
    }
}
";

fn lower(text: &str) -> tos_ir::Module {
    let source = tos_core::SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = tos_core::Parser::parse_schema(&source)
        .into_accepted()
        .expect("the module parses");
    let found = tos_core::Checker::check(&source, &schema);
    assert!(
        !found
            .iter()
            .any(|d| d.severity() == tos_core::Severity::Error),
        "the module checks clean: {found:?}"
    );
    tos_core::lower_module(
        &source,
        &schema,
        &tos_core::ModuleContext {
            source_set: String::from("dma-region-path"),
            path: String::from("system/test/dma.tos"),
            content_id: tos_pipeline::content_id(source.bytes()),
            dependency_digest: tos_pipeline::list_digest(&[]),
            capability_interface_digest: tos_pipeline::list_digest(&[]),
        },
    )
    .expect("the module lowers")
}

/// What the region actually covers, in the process's own address space.
///
/// Real memory, so a write followed by a read proves the bridge moved bytes
/// rather than answering from a table.
const EXTENT: usize = 4096;

/// The handle this host mints for the one region it makes. Any number does; it
/// is written down and compared rather than assumed.
const REGION_HANDLE: u64 = 0x51;

struct Nucleus {
    /// The bytes the region is, kept alive for as long as the run is.
    backing: Vec<u8>,
    /// The real bridge table, keyed by handle exactly as the runtime image
    /// keys it.
    mappings: tos_launch::DeviceMappings,
    /// Which handle each operation was performed on, in order.
    reached: Vec<(String, u64)>,
    /// Which handle each indexed access named, in order.
    accessed: Vec<u64>,
    /// What a `capability_release` answers, so the successful and the failed
    /// lifecycle are the same test with one number changed.
    release: i64,
}

const OK: i64 = 0;
const E_NO_CAPABILITY: i64 = -6;

impl Nucleus {
    fn new(release: i64) -> Nucleus {
        Nucleus {
            backing: vec![0u8; EXTENT],
            mappings: tos_launch::DeviceMappings::new(),
            reached: Vec::new(),
            accessed: Vec::new(),
            release,
        }
    }

    /// The handles the nucleus side saw, by operation.
    fn handles(&self, operation: &str) -> Vec<u64> {
        self.reached
            .iter()
            .filter(|(name, _)| name == operation)
            .map(|(_, handle)| *handle)
            .collect()
    }
}

impl tos_engine::System for Nucleus {
    fn granted(&mut self, request: tos_engine::Request<'_>) -> Option<tos_engine::Handle> {
        // Two startup grants, of two ordinary importable interfaces. Neither is
        // the region: `platform.dma.Region` is not startup-importable, and
        // `E1503` refuses a module that asks.
        match request.interface {
            "platform.pci.FunctionConfig" => Some(tos_engine::Handle::new(0x11)),
            "system.memory.Authority" => Some(tos_engine::Handle::new(0x22)),
            _ => None,
        }
    }

    fn reach(
        &mut self,
        call: tos_engine::Reach<'_>,
    ) -> Result<tos_engine::Value, tos_engine::Trap> {
        let first = match call.arguments.first() {
            Some(tos_engine::Value::Capability(handle)) => handle.get(),
            _ => 0,
        };
        self.reached.push((String::from(call.operation), first));
        match call.operation {
            // The successful handoff, in the order ADR-0084 §8 requires: the
            // handle, then the window, then the mapping — and only then a value
            // the module can hold. A failure would return `Err` having done
            // none of it.
            "dma_region_allocate" => {
                let base = self.backing.as_ptr() as u64;
                let record = tos_launch::MmioMapRecord {
                    base,
                    length: EXTENT as u64,
                };
                assert!(
                    self.mappings.remember(REGION_HANDLE, record, true),
                    "the bridge remembers the window it was just handed"
                );
                Ok(tos_engine::Value::Variant {
                    index: 0,
                    payload: vec![tos_engine::Value::Capability(tos_engine::Handle::new(
                        REGION_HANDLE,
                    ))],
                })
            }
            // The nucleus resolves the offset; what comes back is data.
            "dma_device_address" => {
                if self.mappings.mapping(first).is_none() {
                    // A handle the process no longer holds: the nucleus answers
                    // `E_NO_CAPABILITY` by generation, and this stands in for
                    // that one fact alone.
                    return Ok(tos_engine::Value::Variant {
                        index: 1,
                        payload: vec![tos_engine::Value::Int(
                            tos_ir::IntKind::I64,
                            E_NO_CAPABILITY.into(),
                        )],
                    });
                }
                Ok(tos_engine::Value::Variant {
                    index: 0,
                    payload: vec![tos_engine::Value::Int(tos_ir::IntKind::U64, 0xd_a000)],
                })
            }
            // **The retirement is the bridge's, on success only** (ADR-0085
            // §8a) — the same rule the runtime image applies, over the same
            // table.
            "capability_release" => {
                if self.release == OK {
                    self.mappings.retire(first);
                }
                Ok(tos_engine::Value::Int(
                    tos_ir::IntKind::I64,
                    self.release.into(),
                ))
            }
            other => Err(tos_engine::Trap::new(
                "RUNTIME_INTERFACE_UNREACHABLE",
                std::format!("{other} is not an operation this host performs"),
                0,
            )),
        }
    }

    /// The bridge's own path, over the real table.
    fn access(
        &mut self,
        access: tos_engine::Access,
    ) -> Result<tos_engine::Value, tos_engine::Trap> {
        self.accessed.push(access.region.get());
        let width = access.element.width();
        let Some((at, _)) = self.mappings.extent(
            access.region.get(),
            access.index,
            width,
            access.value.is_some(),
        ) else {
            return Err(tos_engine::Trap::new(
                "RUNTIME_DEVICE_REFUSED",
                String::from("a region access this process holds no mapping for"),
                0,
            ));
        };
        let offset = (at - self.backing.as_ptr() as u64) as usize;
        match access.value {
            None => Ok(tos_engine::Value::Int(
                tos_ir::IntKind::U8,
                self.backing[offset].into(),
            )),
            Some(tos_engine::Value::Int(_, number)) => {
                self.backing[offset] = number as u8;
                Ok(tos_engine::Value::Unit)
            }
            Some(_) => Err(tos_engine::Trap::new(
                "RUNTIME_DEVICE_REFUSED",
                String::from("a region write does not match the element it names"),
                0,
            )),
        }
    }

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
}

fn run(release: i64) -> (i64, Nucleus) {
    let module = lower(MODULE);
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("the artifact verifies");
    let mut system = Nucleus::new(release);
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
        .expect("both requests are granted")
        .expect("the run completes");
    let tos_engine::Value::Int(_, produced) = outcome.value else {
        panic!("the entry returns an i64");
    };
    (produced as i64, system)
}

/// **§16.6 — one binding, one handle, one mapping.**
///
/// The same source binding is indexed *and* passed to two capability
/// operations, and every one of them reaches the boundary carrying the **same**
/// number the allocation returned. There is no conversion instruction between
/// them and no second authority anywhere.
#[test]
fn one_binding_reaches_the_boundary_as_one_handle() {
    let (produced, system) = run(OK);
    assert_eq!(produced, OK);

    // The allocation answered with this handle...
    assert_eq!(system.handles("dma_region_allocate"), vec![0x11]);
    // ...and every later use of the binding carried exactly it.
    assert_eq!(system.handles("dma_device_address"), vec![REGION_HANDLE]);
    assert_eq!(system.handles("capability_release"), vec![REGION_HANDLE]);
    // Three indexed accesses — a write, and two reads — all on the same one.
    assert_eq!(
        system.accessed,
        vec![REGION_HANDLE, REGION_HANDLE, REGION_HANDLE]
    );

    // And the bridge's table held exactly one entry, under that handle, for the
    // life of the region: a second mapping would be a second authority for one
    // allocation.
    let mut seen: BTreeMap<u64, usize> = BTreeMap::new();
    for handle in system.accessed.iter().chain(
        system
            .reached
            .iter()
            .filter(|(name, _)| name != "dma_region_allocate")
            .map(|(_, handle)| handle),
    ) {
        *seen.entry(*handle).or_default() += 1;
    }
    assert_eq!(
        seen.keys().copied().collect::<Vec<_>>(),
        vec![REGION_HANDLE]
    );
}

/// The bridge moves bytes, and moves them through the mapping rather than
/// through anything the engine holds.
///
/// `region[0B] = 200u8` then `region[0B]` reads 200 back out of the process's
/// own memory: an indexed access to a DMA region is an **ordinary** checked
/// memory access (ADR-0081 §2), not a device observation, and this is what it
/// being one looks like.
#[test]
fn an_indexed_write_is_read_back_out_of_the_regions_own_bytes() {
    let (_, system) = run(OK);
    assert_eq!(system.backing[0], 200);
    // And nothing outside the element was touched.
    assert!(system.backing[1..].iter().all(|byte| *byte == 0));
}

/// **§16.8 — after a successful release, both stale paths refuse**, and they
/// refuse differently.
///
/// The nucleus path answers `E_NO_CAPABILITY` because the handle no longer
/// resolves; the indexed path is refused by the bridge, from its own table,
/// **before any address is formed**. Neither relies on a page fault.
#[test]
fn a_successful_release_closes_both_paths() {
    let (produced, mut system) = run(OK);
    assert_eq!(produced, OK);

    // The mapping is gone the moment the release returned, which is what
    // "before control returns to TOS Core" means at this boundary.
    assert_eq!(system.mappings.mapping(REGION_HANDLE), None);
    assert_eq!(system.mappings.extent(REGION_HANDLE, 0, 1, false), None);

    // The operation path, asked again through the same handle.
    let stale = tos_engine::System::reach(
        &mut system,
        tos_engine::Reach {
            interface: "platform.dma.Region",
            operation: "dma_device_address",
            arguments: &[tos_engine::Value::Capability(tos_engine::Handle::new(
                REGION_HANDLE,
            ))],
            source: 0,
        },
    )
    .expect("a stale operation answers rather than trapping");
    assert_eq!(
        stale,
        tos_engine::Value::Variant {
            index: 1,
            payload: vec![tos_engine::Value::Int(
                tos_ir::IntKind::I64,
                E_NO_CAPABILITY.into()
            )],
        }
    );

    // The indexed path, asked again through the same handle.
    let refused = tos_engine::System::access(
        &mut system,
        tos_engine::Access {
            region: tos_engine::Handle::new(REGION_HANDLE),
            index: 0,
            element: tos_engine::Element::Int(tos_ir::IntKind::U8),
            value: None,
        },
    )
    .expect_err("a stale indexed access is refused");
    assert_eq!(refused.code, "RUNTIME_DEVICE_REFUSED");
}

/// **§16.9 — a failed release preserves the mapping**, and the binding still
/// works afterwards.
///
/// The object did not end, so the entry is still the process's. The run is the
/// same source; only what the nucleus answered changed.
#[test]
fn a_failed_release_leaves_both_paths_alive() {
    let (produced, mut system) = run(E_NO_CAPABILITY);
    assert_eq!(produced, E_NO_CAPABILITY);

    assert!(system.mappings.mapping(REGION_HANDLE).is_some());
    let served = tos_engine::System::access(
        &mut system,
        tos_engine::Access {
            region: tos_engine::Handle::new(REGION_HANDLE),
            index: 0,
            element: tos_engine::Element::Int(tos_ir::IntKind::U8),
            value: None,
        },
    )
    .expect("an access after a failed release is served");
    assert_eq!(served, tos_engine::Value::Int(tos_ir::IntKind::U8, 200));
}

/// **§11 — the exact extent, from source.**
///
/// The module above already reads `region[4095B]`, the last byte of a 4096-byte
/// region, and the run completes. This is the next position, and it is refused:
/// no clamping, no wrapping, and no value standing for an access that did not
/// happen.
#[test]
fn an_index_past_the_extent_is_refused_from_source() {
    let past = MODULE.replace("region[4095B]", "region[4096B]");
    let module = lower(&past);
    verify(&module, &ResolutionSnapshot::default(), &Limits::default())
        .expect("the artifact verifies — the extent is the host's, not the type's");
    let mut system = Nucleus::new(OK);
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
    let trap = prepared
        .run(Vec::new(), &mut system)
        .expect("both requests are granted")
        .expect_err("an access past the extent does not complete");
    assert_eq!(trap.code, "RUNTIME_DEVICE_REFUSED");
    // **Refused before the access**, which is what the region's own bytes say:
    // the write at position 0 landed, and nothing else in the region moved.
    assert_eq!(system.backing[0], 200);
    assert!(system.backing[1..].iter().all(|byte| *byte == 0));
    // And the release never ran, so the mapping is still the process's — the
    // refusal ended the run rather than unwinding through the lifecycle.
    assert!(system.mappings.mapping(REGION_HANDLE).is_some());
}
