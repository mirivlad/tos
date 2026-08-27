// SPDX-License-Identifier: GPL-3.0-or-later
//! The residency byte bound, measured against a real allocator.
//!
//! `tos-ir`'s own tests prove the accounting reaches every field of the schema.
//! They cannot prove the figure is an **upper** bound on what was actually
//! requested, because they compute both sides themselves. This test does not:
//! it counts the bytes a global allocator was asked for while a module is being
//! decoded, holds only the module, and compares the live figure against what
//! [`tos_ir::retained_bytes`] claims.
//!
//! ```text
//! retained_bytes(module) >= bytes still live after decoding it
//! ```
//!
//! That is the property ADR-0071 section 7 rests on. A residency limit enforced
//! against a figure below the truth is not a limit; it is a number that happens
//! to be smaller.
//!
//! As measured, the accounting is not merely above the truth but level with it:
//! across the all-variants module, two lowered modules and two decoded ones, the
//! reported figure exceeds the requested heap by exactly `size_of::<Module>()`
//! every time — the module's own inline shape, which is the one term in the sum
//! that is not a heap allocation. There is no slack being relied on.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use tos_core::{lower_module, Checker, ModuleContext, Parser, SourceReader};
use tos_ir::footprint::owned_payload_bytes;
use tos_ir::{retained_bytes, Module};

/// A global allocator that keeps a running total of live bytes, **per thread**.
///
/// Per thread because the test harness runs cases concurrently, and a
/// process-wide counter would report one test's allocations inside another's
/// measurement. The counter is a `const`-initialized thread local, so observing
/// it allocates nothing and the allocator cannot re-enter itself.
///
/// It measures **requested** sizes, not what the allocator rounded them up to:
/// what is being checked is that the accounting covers every allocation the
/// representation asked for, and per-block overhead is deliberately outside the
/// portable bound.
struct Counting;

thread_local! {
    /// Signed: a buffer allocated before the measurement began and freed inside
    /// it makes the balance go negative, which is honest rather than a wrap.
    static LIVE: Cell<isize> = const { Cell::new(0) };
}

fn record(delta: isize) {
    // `try_with`, because an allocation can outlive the thread local during
    // thread teardown and a measurement is not worth aborting a process for.
    let _ = LIVE.try_with(|live| live.set(live.get() + delta));
}

// SAFETY: every method forwards to `System`, which is a correct `GlobalAlloc`,
// with the same pointer and the same layout it was given. The bookkeeping added
// around each call touches only a `Cell<isize>` in thread-local storage and
// never the allocation itself, so the memory contract is exactly `System`'s.
unsafe impl GlobalAlloc for Counting {
    // SAFETY: `layout` is passed through unchanged to the system allocator,
    // which is what the caller's contract already permits.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() {
            record(layout.size() as isize);
        }
        pointer
    }

    // SAFETY: the caller guarantees `pointer` came from this allocator with this
    // `layout`; both are forwarded to `System` untouched, and the bookkeeping
    // runs before the free rather than through the pointer.
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record(-(layout.size() as isize));
        System.dealloc(pointer, layout);
    }

    // SAFETY: `pointer`, `layout` and `new_size` satisfy `realloc`'s contract by
    // the caller's guarantee and are forwarded unchanged.
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = System.realloc(pointer, layout, new_size);
        if !moved.is_null() {
            record(new_size as isize - layout.size() as isize);
        }
        moved
    }

    // SAFETY: as `alloc`, forwarded unchanged to the system allocator.
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc_zeroed(layout);
        if !pointer.is_null() {
            record(layout.size() as isize);
        }
        pointer
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn live() -> isize {
    LIVE.with(|live| live.get())
}

/// What holding `module` cost the allocator, and what the accounting says.
///
/// The producing closure runs inside the measurement so that everything it
/// allocated and dropped along the way nets out; what is left is what the
/// returned module is holding.
fn measured(produce: impl FnOnce() -> Module) -> (usize, usize) {
    let before = live();
    let module = produce();
    let after = live();
    let accounted = retained_bytes(&module);
    // The module is dropped after both figures are taken, so the measurement
    // covers a module that is genuinely still alive.
    drop(module);
    let requested = after - before;
    assert!(requested > 0, "producing a module allocated nothing");
    (requested as usize, accounted)
}

const ENVELOPE: &str = "resource [fuel: 100000, stack: 64KiB, allocation: 4KiB, tasks: 1, \
     workers: 1, sync: 0, shared: 0B, cleanup: 16, recursion: 8, imports: 0]";

fn content_id(bytes: &[u8]) -> String {
    let digest = tos_hash::sha256(bytes);
    let mut hex = [0u8; 64];
    tos_hash::hex(&digest, &mut hex);
    format!("sha256:{}", core::str::from_utf8(&hex).unwrap())
}

/// A module the production frontend actually produced.
fn lowered(body: &str) -> Module {
    let text = format!("module app.footprint version 1.0 profile bootstrap; {ENVELOPE} {body}");
    let source = SourceReader::read(text.as_bytes()).expect("transport-valid source");
    let schema = Parser::parse_schema(&source)
        .into_accepted()
        .expect("the fixture parses");
    let diagnostics = Checker::check(&source, &schema);
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == tos_core::Severity::Error),
        "the fixture checks clean"
    );
    let context = ModuleContext {
        source_set: "tos-tests-integration".to_string(),
        path: "app/footprint.tos".to_string(),
        content_id: content_id(source.bytes()),
        dependency_digest: content_id(b""),
        capability_interface_digest: content_id(b""),
    };
    lower_module(&source, &schema, &context).expect("the fixture lowers")
}

/// A body large enough that the module is dominated by its own tables rather
/// than by the constants every module carries.
fn wide_body() -> String {
    let mut text = String::new();
    for index in 0..64 {
        text.push_str(&format!(
            "pub fn f{index}(a: i64, b: i64) -> i64 {{ let mut total = a; \
             if (a > b) {{ total = a + b; }} else {{ total = a - b; }} return total; }} "
        ));
    }
    text.push_str("pub fn main() -> i64 { return f0(1i64, 2i64); }");
    text
}

/// The all-variants module: the accounting is above what was really requested.
#[test]
fn the_bound_holds_above_a_measured_all_variants_module() {
    let (requested, accounted) = measured(tos_ir::fixtures::every_variant);
    assert!(
        accounted >= requested,
        "accounting reports {accounted} bytes for a module that asked the allocator for {requested}"
    );
}

/// And above what a real lowered module really requested.
#[test]
fn the_bound_holds_above_a_measured_production_module() {
    for body in ["fn main() -> i64 { return 7; }".to_string(), wide_body()] {
        let (requested, accounted) = measured(|| lowered(&body));
        assert!(
            accounted >= requested,
            "accounting reports {accounted} bytes for a module that asked the allocator for \
             {requested}"
        );
    }
}

/// A module decoded from its image — the path residency actually takes.
///
/// This is the figure the byte bound is enforced against, so it is the one that
/// has to be an upper bound in the shape it is produced in. A parser that
/// over-reserves is not a defect here; a parser whose reservations the
/// accounting does not see would be.
#[test]
fn the_bound_holds_above_a_module_decoded_from_its_image() {
    let limits = tos_verifier::Limits::default();
    let parse_limits = tos_image::ParseLimits {
        table_entries: limits.table_entries,
        modules: limits.modules,
        fields: limits.fields,
        parameters: limits.parameters,
        blocks_per_function: limits.blocks_per_function,
        instructions_per_block: limits.instructions_per_block,
        source_map_entries: limits.source_map_entries,
    };

    for module in [tos_ir::fixtures::every_variant(), lowered(&wide_body())] {
        let (image, _) = tos_image::encode(&module);
        let (requested, accounted) = measured(|| {
            tos_image::parse(&image, &parse_limits).expect("an image this crate wrote parses")
        });
        assert!(
            accounted >= requested,
            "a decoded module asked for {requested} bytes and is accounted at {accounted}"
        );
        // And the same module, however it was produced, is bounded above the
        // payload it owns.
        let decoded = tos_image::parse(&image, &parse_limits).expect("parses");
        assert!(retained_bytes(&decoded) >= owned_payload_bytes(&decoded));
    }
}
