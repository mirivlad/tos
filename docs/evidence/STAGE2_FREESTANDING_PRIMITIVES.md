<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Freestanding memory primitives: the hypothesis, and what was actually wrong

Evidence level: **P1** (locally measured, docs/35).
Artifact audited: `target/x86_64-unknown-none/release/tos-nucleus`, the real
Stage 2 freestanding binary, built with `TOS_NUCLEUS_ELF=1` so the ELF container
and its symbol table survive. Nothing else about the build differs — same
objects, same linker script, same code.

## The hypothesis

`docs/evidence/STAGE2_PERFORMANCE_PAIR_P1.md` recorded a leading hypothesis for
the frontend's cost on the reference platform: that `x86_64-unknown-none` takes
`memcpy`, `memmove`, `memset` and `memcmp` from `compiler_builtins`, whose
portable implementations move a byte at a time, while a host build gets glibc's
vectorised ones.

It was labelled a hypothesis because it was untested. It is now tested.

## The hypothesis is **refuted**

The primitives in the actual binary are word-oriented, not byte-at-a-time:

```text
memcpy    align to 8, then `rep movsq`, then a `rep movsb` tail
memset    broadcast the byte to a word, align, `rep stosq`, tail
memmove   same shape, with the direction chosen from the operand order
memcmp    two 8-byte loads per iteration, 16 bytes at a time
```

`memcpy` is fifteen instructions and contains no byte loop. There is nothing to
replace, and implementing TOS primitives to fix this would have been work
directed at a defect that does not exist.

Writing them anyway would also have been a change to the freestanding substrate
made on a guess — which is the failure mode the audit existed to prevent, and
the reason it ran before any implementation.

## What was actually wrong

The static call profile of the same binary points somewhere else. Counting call
sites by target:

```text
 999  GlobalHeap::dealloc
 756  GlobalHeap::alloc
 433  alloc::raw_vec::handle_error
 234  core::str::from_utf8
```

Allocation dominates, and the reason it dominates was in the lowerer:

```rust
fn intern(&mut self, definition: TypeDef) -> TypeId {
    let key = alloc::format!("{definition:?}");        // every single time
    if let Some(&existing) = self.type_index.get(&key) { return existing; }
    ...
}
```

Every type reference in a module allocated a `String`, ran the whole `core::fmt`
machinery to render a debug representation, and then compared strings. Lowering
a module at the published 256 KiB ceiling interns tens of thousands of times.

The index is now keyed on the definition itself. Structural equality is also the
*correct* key: two definitions are the same type exactly when they are equal,
whereas a debug rendering is a presentation that merely happened to be
injective.

## Why it was invisible natively

This is the finding worth keeping.

```text
native frontend, 262 114-byte fixture     163 ms  ->  152 ms   (-7%)
reference frontend, same fixture       >900 000 ms  ->  1 492 ms  (>600x)
```

Natively the change is 7% and would never have justified attention. On the
reference platform it is the difference between a measurement and a timeout.

The two environments price the same code differently: `core::fmt` is dynamic
dispatch and per-fragment writes, and a per-intern allocation goes to glibc's
malloc natively and to the bounded heap in the guest. Both are cheap on a host
and expensive under TCG, and they compounded.

**A profile taken on the host would have pointed at the wrong thing.** The
defect was found by auditing the artifact that was actually slow, on the
platform it was slow on.

## Where that leaves the platform factor

```text
engine    reference / native  = 16.8x
frontend  reference / native  = 1 492 ms / 152 ms = 9.8x
```

They now agree within a factor of two, where before they differed by three
orders of magnitude. That is what "no remaining implementation pathology" looks
like from the outside: the platform costs what the platform costs, uniformly.

The frontend is still over its 500 ms budget — 1.49 s — and that is now a
question about the budget and the ordinary cost of the work, not about a defect.
`docs/evidence/STAGE2_PERFORMANCE_PAIR_P1.md` and ADR-0043 carry it.
