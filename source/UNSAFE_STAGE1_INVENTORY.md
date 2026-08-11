<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1 trusted unsafe-code inventory

## Scope and mechanical rule

This inventory covers every Rust `unsafe { ... }`, `unsafe fn` and `unsafe
extern` declaration below `source/`, excluding generated `source/target/`
output. `scripts/check-unsafe-safety.py --root .` is the authoritative
mechanical inventory: it requires a contiguous, local `SAFETY:` rationale for
each operation (allowing only an immediately adjacent rustfmt-wrapped
assignment line or attributes). New unchecked operations therefore fail the
gate rather than relying on a reviewer's memory.

At the F-20 closure point the checker reports **89** operations:

| Location | Operations | Concrete precondition classes | Focused evidence |
|---|---:|---|---|
| `source/boot/uefi-loader/src/main.rs` | 70 | Live UEFI system/boot-service/protocol tables; non-null successful protocol outputs; bounded firmware entry-point bytes; UEFI pool/page allocations; checked memory-map offsets/lengths; fixed nucleus/stack/BootInfo handoff | UEFI layout assertions; ACPI/SMBIOS selection tests; FFI success+non-null regression; QEMU loader/handoff/negative paths |
| `source/nucleus/src/main.rs` | 8 | Loader-owned, identity-mapped and validated BootInfo, memory-map, capsule and framebuffer ranges; terminal QEMU port/HLT | BootInfo validation tests; capsule corruption/mismatch QEMU; normal exit 33 |
| `source/nucleus/src/exception.rs` | 7 | Nucleus-owned GDT/TSS/IDT/stub table, disabled interrupts, #DF IST, privileged CR2 read and isolated injected exceptions | IDT/TSS/IST mechanical check; #UD/#GP QEMU exit 73 |
| `source/nucleus/src/framebuffer.rs` | 1 | Validated ADR-0022 framebuffer range, checked pitch × height and best-effort rendering | RGBX/BGRX, pitch, clipping and absent-framebuffer host tests; graphical QEMU smoke |
| `source/crates/tos-serial/src/lib.rs` | 2 | Fixed COM1 I/O port range in the declared QEMU profile | Serial Boot ABI QEMU event-contract tests |
| `source/crates/boot-protocol/src/lib.rs` | 1 | Test-only byte view of a live local `repr(C)` BootInfo bounded by its exact size | 24 BootInfo unit tests |

No unsafe operation exists in the capsule parser/hash implementation. The
eighteen duplicated test-only BootInfo byte conversions that predated F-20 were
removed in favour of one bounded helper with one local rationale.

## Audit result

The audit did not introduce a new trusted dependency, public format, ABI field,
firmware trust claim or Stage 1.5 subsystem. UEFI pointers remain within the
existing firmware-to-loader boundary and malicious firmware remains accepted
threat-model non-goal T7. F-20 is therefore a Level 1 implementation hardening
of the existing ADR-0005, ADR-0022, ADR-0023 and Boot ABI v1 assumptions.

## Stage 2 addition: the reference runtime heap

`source/crates/tos-runtime` is the only Stage 2 component with `unsafe`. It
exists because a heap over a granted region is where raw addresses have to be
handled; every other Stage 2 crate is `#![forbid(unsafe_code)]`.

The unsafe surface is nine declarations and their blocks, and they share two
obligations:

- **the grant's promise** — `RuntimeMemoryGrantV1.base` addresses `length`
  readable, writable bytes owned by no one else for the heap's lifetime. This is
  the one thing the type cannot check for itself; everything else about a grant
  is validated in `adopt` before a byte is touched.
- **single-context use** — the reference runtime is single-threaded by
  construction, which is what lets `GlobalHeap` use a raw cell instead of a
  lock. That assumption is stated on the type, because it is the first thing
  that would have to change if a Full engine ever drove this allocator from more
  than one context.

Everything else is internal and checked: block walks stay inside the region,
splits stay inside the block they split, and `deallocate` re-checks that a
pointer lies in the arena before reading its tag.

Adversarial coverage is in `crates/tos-runtime/tests/heap.rs`: malformed grants
refused by reason, allocations proved disjoint and writable, reclaim after a
full-arena allocation, coalescing from both directions, a thousand
allocate-and-free rounds that must return the arena to its exact starting
layout, and exhaustion that refuses without damaging anything live.
