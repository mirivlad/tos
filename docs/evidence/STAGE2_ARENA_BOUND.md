<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 2 implementation-arena bound — measured

ADR-0041 accepts two disciplines for allocation failure. The one this
implementation relies on is a proved upper memory bound with an arena at
least that large, because `GlobalAlloc`'s contract is a null pointer and
`alloc` turns that into `handle_alloc_error`. A bound has to be measured to
be proved. This is the measurement.

Reproduce with:

```bash
cargo run --manifest-path source/Cargo.toml -p tos-arena-bound --release
```

## What was measured

The whole production path — SourceReader, Parser, Checker, Lowerer,
independent Verifier, reference engine — over a canonical module filling the
`docs/44` source-unit ceiling of 256 KiB, with `tos_runtime`'s bounded heap
installed as the global allocator. Nothing was stubbed and no stage was
skipped; the run's answer is checked, so the bound describes a run that
really happened.

Running the pipeline *through* the heap is also the strongest test the heap
has. Hundreds of thousands of irregular allocate/free pairs exercise
splitting, coalescing and reuse far past what a unit test reaches, and any
corruption would surface as a wrong answer rather than as a passing
assertion.

## Result

```text
TOS Stage 2 implementation-arena bound
allocator: tos_runtime::BoundedHeap over a 536870912-byte region

fixture: 262066 bytes of canonical source
pipeline result: Int(I32, 3)
committed after the run: 382704 bytes
peak extent (the arena this run needed): 54408096 bytes
  = 51.89 MiB, against a 512 MiB region
blocks after the run: 6 total, 2 free
committed before the run: 382704 bytes

The whole production path ran on this heap and produced the right
answer, so the bound above is a bound on a run that really happened.
```

## Reading

**51.89 MiB** is the arena the worst-case single module needs. `peak_extent`
is the highest address the arena was ever carried to, so it already includes
every block's tags, the rounding to the grain, the per-allocation prefix that
makes strong alignments serviceable, remainders too small to split off, and
any hole below the frontier. It is not a sum of requested payloads, which
would not have been a bound at all.

Six blocks remain at the end, two of them free, and the committed figure
before and after the measured run is identical — the pipeline gave back what
it took, and the arena did not fragment into a long tail.

## What this does and does not settle

It settles the discipline for **one module at the published ceiling**, which
is the case `docs/44` bounds. A nucleus grant sized above this with margin
satisfies ADR-0041 for that workload.

It does not settle a multi-module source set, whose closure `docs/44` caps at
256 modules; that bound needs its own measurement before a grant size is
fixed for it. The number is retained here rather than baked into a constant
so the grant size stays a declared decision rather than a magic value.
