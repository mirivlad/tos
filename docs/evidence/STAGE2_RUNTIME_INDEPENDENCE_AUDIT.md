<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Stage 2 runtime independence — HISTORICAL, superseded

> **Status: superseded.** This audit describes the state *before* ADR-0041 and
> its conclusion — that Stage 2 could not close — was correct when written and
> is no longer true. It is kept because it is what produced ADR-0041, and
> rewriting it to look as though the gap never existed would delete the reason
> the decision was made.
>
> The gap it found is closed by:
>
> - **ADR-0041 `RuntimeMemoryGrantV1`** — the nucleus grants one bounded region
>   and the runtime never discovers memory;
> - **a bounded reclaiming allocator** with real free, bidirectional coalescing
>   and a search whose cost does not grow with the arena
>   (`docs/evidence/STAGE2_ALLOCATOR_SEARCH.md`);
> - **`no_std` production crates** on the `x86_64-unknown-none` target, gated by
>   both a source gate and a build gate;
> - **a real guest Stage 2 pipeline**: the capsule's canonical boot module goes
>   through reader, parser, checker, resolution, lowering, the independent
>   verifier and the bounded engine inside QEMU, verified by
>   `host-tools/qemu-test/stage2-runtime.sh`.
>
> The original text follows unchanged.

---


# Stage 2 runtime-independence audit

`docs/44` states the contract this audit is against: Rust may implement the
Stage 2 components, but **rustc, LLVM, libc, the C ABI and host threads are not
recovery or runtime dependencies**. The Stage 2 crates are written against Rust
`std` today, so that claim needed checking rather than asserting.

This is a factual audit taken before any rewriting. The result is short: the
production code is already free of host runtime facilities, the freestanding
target is already in use elsewhere in the repository, and **one thing is
genuinely missing — a heap allocator**. That last item is an architectural
boundary, so this audit stops there and presents options rather than choosing.

## 1. What each crate actually uses

Measured over production code only. `tos-core`'s test module (`lib.rs` from the
`#[cfg(test)]` marker onward) is excluded, because a test harness runs on the
host by construction and is not part of any runtime path.

| Crate | `std` facilities used in production code |
|---|---|
| `tos-core` | `vec::Vec`, `string::String`/`ToString`, `boxed::Box`, `collections::{BTreeMap, BTreeSet}`, `mem::{take, replace}`, `format!`, `vec!` |
| `tos-ir` | `vec::Vec`, `string::String`/`ToString`, `format!` |
| `tos-verifier` | `vec::Vec`, `string::String`/`ToString`, `collections::{BTreeMap, BTreeSet}`, `format!` |
| `tos-engine` | `vec::Vec`, `string::String`/`ToString`, `collections::BTreeMap`, `format!`, `vec!` |
| `tos-cache` | `vec::Vec`, `string::String`/`ToString`, `collections::BTreeMap`, `format!` |
| `tos-hash` | already `#![no_std]` |
| `tos-serial` | already `#![no_std]` |

**Every one of those lives in `alloc` or `core`.** `Vec`, `String`, `ToString`,
`Box`, `BTreeMap`, `BTreeSet`, `format!` and `vec!` are `alloc`; `mem::take` and
`mem::replace` are `core`.

## 2. What is absent

Searched across all five Stage 2 crates' production code:

```text
std::fs        none
std::io        none
std::env       none
std::net       none
std::thread    none
std::time      none
std::process   none
std::sync      none
```

The only `std::fs` and `std::panic` uses anywhere in `tos-core` are at
`lib.rs:1554`, `lib.rs:1919` and `lib.rs:2877` — all inside the test module that
begins at `lib.rs:735`. No production path reads a file, consults a clock, spawns
a thread, touches the environment or opens a socket.

That is not an accident of implementation. It follows from decisions already
taken: the source reader takes bytes rather than a path, module resolution reads
only a declared source set (`docs/42` section 1), the verifier takes a declared
snapshot rather than discovering one, the engine's determinism forbids consulting
a clock, and the cache defines identity without defining storage.

## 3. Evidence from the built artifacts

```text
$ ldd source/target/release/tos-core-performance
        linux-vdso.so.1
        libgcc_s.so.1 => /lib/x86_64-linux-gnu/libgcc_s.so.1
        libc.so.6 => /lib/x86_64-linux-gnu/libc.so.6
        /lib64/ld-linux-x86-64.so.2

$ ldd source/target/x86_64-unknown-none/release/tos-nucleus
        not a dynamic executable
```

The first line is the finding: a **host binary** linking these crates does depend
on libc, and a measurement or execution taken through it is a host execution. The
second line is the counterweight: `x86_64-unknown-none` already produces a
freestanding artifact in this repository, under the existing
`source/.cargo/config.toml` configuration and the Stage 1 QEMU gates.

Section 1 says why the first line is a property of the *link*, not of the code:
nothing in the production code asks for anything libc provides.

## 4. The one real gap: allocation

`alloc` requires a `#[global_allocator]`. The nucleus is `#![no_std]` with a
`#[panic_handler]` and **does not use `alloc` at all**, so no allocator exists
anywhere in the repository, and no accepted document names one.

This is the architectural boundary. It is not a porting detail:

- `docs/41` section 6 makes `allocation` one of the ten declared resource limits,
  so an allocator in a TOS runtime is not free-floating — it is accountable
  against a module's envelope, and allocation must fail before it has an effect
  when the envelope is spent.
- Who owns the heap in Stage 2, before the Stage 3 process substrate exists, is
  not settled by any accepted document.
- The nucleus/runtime interface that would carry a memory grant does not exist,
  and inventing one silently would be exactly the hidden ABI this audit is meant
  to prevent.

Per the mandate, this audit **stops here** and presents options rather than
choosing one.

### Option A — a bounded arena granted by the nucleus

The nucleus already receives a memory map through `BootInfo` and already
reserves identity-mapped pools. It grants the runtime one bounded region at
start-up; the runtime installs a `#[global_allocator]` over that region, backed
by a bump or free-list allocator inside `tos-runtime`.

- The grant is a size and a base — the same shape as `BootInfo`'s existing
  handoff, so no new ABI concept appears.
- Allocation is naturally accountable: the arena's size is the ceiling, and a
  module's declared `allocation` limit is checked against it before an effect.
- Exhaustion is a defined failure, not a host `abort`.
- Cost: one new interface item in the boot handoff, and an allocator to write
  and test. Both are ordinary bounded work.

### Option B — no allocator; convert the Stage 2 crates to fixed-capacity storage

Replace `Vec`/`String`/`BTreeMap` with bounded, caller-provided storage sized
from the `docs/44` hard limits, and stay on `core` alone.

- No allocator, no heap, no grant interface.
- Every table already has a published ceiling, so the sizes exist.
- Cost: it rewrites essentially all of `tos-core`, `tos-ir`, `tos-verifier` and
  `tos-engine`, and it makes worst-case memory the *always* case, which for a
  256 KiB module against the published ceilings is very large. It also trades a
  well-understood data-structure style for a bespoke one, in the components that
  most need to stay reviewable.

### Option C — allocator provided by a Stage 3 process substrate; Stage 2 defers

Declare that a freestanding Stage 2 runtime waits for the Stage 3 substrate to
own memory, and that Stage 2 closes with the frontend, verifier and engine
proven only as host-hosted components.

- Honest and cheap now.
- But it leaves the `docs/44` contract unproven at Stage 2 closure, and Stage 2's
  identity question is whether *actual language semantics execute* — with a host
  runtime under them, that execution is a host execution. This option is recorded
  for completeness and is not recommended.

### Recommendation

**Option A.** It matches the existing architecture rather than adding a new
concept, it makes allocation accountable in the way `docs/41` section 6 already
requires, and its cost is bounded and ordinary. Option B pays a very large and
permanent complexity cost to avoid an interface the system needs anyway. Option C
does not discharge the contract.

Option A needs one narrow ADR — what the nucleus grants, in what shape, and how
`allocation` accounting binds to it. That ADR is not written here, because
choosing among A, B and C is the Project Architect's decision and writing the
ADR would presume it.

## 5. What the conversion costs once allocation is settled

Assuming Option A or an equivalent:

- `#![no_std]` plus `extern crate alloc` in the five crates, and `std::` rewritten
  to `alloc::`/`core::`. Section 1 shows every path has a direct equivalent, so
  this is mechanical, not a redesign.
- The `format!` uses (127 of them) are `alloc::format!` — same macro.
- Diagnostics, findings and traps already carry structured data rather than
  formatted host strings, so nothing depends on `std`'s error machinery.
- Test code keeps `std`: a test harness is a host program, and `#[cfg(test)]`
  keeps it out of the runtime artifact.
- The build target is `x86_64-unknown-none`, which the repository already builds
  and gates.
- A `#[panic_handler]` is needed for the freestanding artifact. The engine
  already never relies on panics or unwinding for semantics — every dynamic
  failure is a `Trap` value — so the handler is a halt, not a control-flow
  mechanism.

**No new hidden ABI appears** in any of this: no C ABI, no libc, no WASI, no
Linux personality, no host shim. The only new interface is the memory grant of
Option A, which is a declared item of the existing boot handoff.

## 6. Findings

1. The Stage 2 production code uses **no** host runtime facility. Every `std`
   path it uses is in `alloc` or `core`.
2. The host binary's libc dependency is a property of linking for a host target,
   not of the code, and it is real: a measurement or execution taken through that
   binary is a host execution and cannot close a Stage 2 gate.
3. The freestanding target already exists, is configured, and is gated.
4. **The one genuine gap is a heap allocator**, and with it the question of who
   owns memory in Stage 2 before Stage 3. No accepted mechanism exists. This is
   an architectural decision and is presented, not taken.
5. Until it is taken, the Stage 2 runtime-independence claim of `docs/44` is
   **not** discharged, the ADR-0040 reference measurement cannot be taken on the
   real path, and Stage 2 cannot be candidate-complete.
