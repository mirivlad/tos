<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0045: The Stage 2 frontend performance budget

- Status: **Proposed** (awaiting Project Architect decision)
- Date: 2026-08-12
- Decision level: 2 — a quantitative Stage 2 gate in docs/35
- Project Architect approval: *(none — this ADR is not accepted)*
- Relation to other ADRs: independent of ADR-0043 (engine ratio, Accepted at
  22x) and of ADR-0044 (module digest scheme, Proposed). Neither is a condition
  of this one, and this one is not an extension of either.

## The claim

The docs/35 Stage 2 frontend budget — **500 ms p95 on the ADR-0040 reference
platform** for `parse + check + lower + independent verify` of a 256 KiB
canonical module — was a research estimate written before a working Stage 2
implementation existed. It has done its job as a gate, and the evidence now says
it was empirically falsified.

## Final normative measurement

Commit `cf806de`. Procedure: 3 warmups, 21 samples, median/p95/p99, one commit,
one set of fixtures emitted by the harness that measures them natively, both
halves, real freestanding path on q35/qemu64/1 vCPU/256 MiB/TCG.
Fixture 262 114 bytes, `sha256:40ea301db1e52502190794049cecf65ddc40a76cde07a98181cca9a1aa433a98`.

| metric | native p95 | reference p95 | budget | verdict |
|---|---|---|---|---|
| frontend, 256 KiB | 116 701 us | **1 140 356 us** | 500 000 us | **FAIL** (2.28x) |
| engine, 1e6 ops | 201 501 us | 3 993 138 us | ≤ 22x | PASS (19.8x) |
| quota rejection | — | 522 418 us | ≤ 2x accepted | PASS (0.458) |

Frontend reference median 1 103 605 us; min 1 042 931, max 1 150 025.

## Where the time goes

Native stage decomposition of the same fixture:

```text
read       0.4 ms   transport validity
parse      5.7 ms   grammar
check     26.1 ms   ten independent slices
lower     23.5 ms   tos-ir/v1 construction
verify    47.5 ms   independent verification
```

One level down, and this is the finding that decides the ADR:

```text
verify                    47.5 ms
  module digest           34.4 ms   SHA-256 over a 5 975 526-byte canonical stream
  source-map digest       12.3 ms   re-serializes 4 strings x 12 058 entries
  the nine checks          1.2 ms   limits, schema, identity, table order,
                                    types/imports, control flow, ownership,
                                    tasks/sync/atomics/unsafe, source maps
```

**The independent verifier's verification costs 1.2 ms. Its identity binding
costs 46.6 ms.** The verifier is not slow; hashing the module's identity is.

Checker slices (median us): ownership 9 537, guards 4 893, typing 4 745,
visibility 2 907, names 2 541, types 1 604, concurrency 751, mutability 406,
exhaustiveness 389, returns 268. No slice dominates and none is doing another's
work since the triple typing derivation was removed.

## Implementation defects that were found and fixed

The gate earned its place. Six real defects, each general and each wrong for
every input, not just this benchmark:

1. the bounded heap searched every block on every allocation, making the
   frontend superlinear in its input (`STAGE2_ALLOCATOR_SEARCH.md`);
2. the lowerer rendered `format!("{:?}")` to intern every type — 7% natively and
   over 600x on the reference platform (`STAGE2_FREESTANDING_PRIMITIVES.md`);
3. the engine cloned every instruction it executed;
4. transport validation normalized the whole source to compare it with itself,
   when ASCII is NFC-stable by definition — 23.3 ms to 0.4 ms;
5. the verifier formatted a finding location for every entry it checked, on the
   success path;
6. `ownership` and `guards` each re-derived the whole typing analysis, so a
   module was typed three times to answer one question.

Between the first measurement and this one the reference frontend went from
**not completing in 900 seconds** to 1.14 s.

## Optimisations investigated and rejected

- **Freestanding memory primitives.** Hypothesised as byte-at-a-time; the real
  binary's `memcpy` is `rep movsq`, `memset` is `rep stosq`, `memcmp` compares
  16 bytes an iteration. Refuted against the artifact; no work done.
- **Streaming the digest instead of buffering it.** Measured *slower* — a
  compression function called with small fragments pays its per-call cost
  repeatedly, and a fixed intermediate window only added a copy. Reverted.
- **Caching the module digest in `Module`.** Rejected on correctness: three
  components derive it independently precisely so none takes another's word for
  which module a receipt describes.
- **Merging checker slices.** Not done. The slices are independent because each
  reports only what it can establish alone; merging them for speed would trade a
  correctness property for a benchmark.

## The remaining cost, and why it is the work

After the six fixes, the ~104 ms native frontend is:

- **~47 ms identity** — the module digest a receipt must bind to (docs/43
  section 5) and the source-map digest the receipt carries. Necessary work;
  its *encoding* is wasteful and that is ADR-0044's question, deliberately not
  answered here and explicitly not a Stage 2 blocker.
- **~26 ms checking** — types, ownership, effects, resources, concurrency,
  guards, exhaustiveness, returns, mutability, names, across ten slices that are
  separate on purpose.
- **~24 ms lowering** — deterministic IR construction and a 12 058-entry source
  map. Source-map fidelity is what makes a runtime failure name the text that
  caused it; it is not overhead to be trimmed.
- **~6 ms parsing** and **~0.4 ms transport validation**.

Reaching 500 ms would require either changing the identity contract prematurely
(ADR-0044, which the Architect has directed is not a Stage 2 blocker), or
sacrificing the independence of the checker's slices, or micro-optimising
against TCG — none of which improves TOS.

## Options

1. **Keep 500 ms.** Stage 2 cannot close, and the gate now measures the
   platform and the identity encoding rather than finding defects.
2. **Revise to a measured threshold.** Recommended.
3. **Change the reference platform** (KVM, a larger machine). Rejected:
   ADR-0040 chose TCG so the platform is reproducible on any host, and choosing
   a faster platform after seeing the number is what ADR-0040 exists to prevent.
4. **Measure a smaller module.** Rejected outright: the budget is written
   against the published source-unit ceiling, and measuring something else and
   reporting it as the same thing is what every evidence rule here prevents.

## Recommendation

**Option 2, with a threshold of 1500 ms p95.**

Not `current p95 + epsilon`. The reasoning:

- the measured p95 is 1 140 ms, so this is about **1.3x** it;
- within a single 21-sample run the frontend spans 1 043–1 150 ms, so ordinary
  variation is already ~10%, and a threshold inside 20% would fail on a machine
  slightly slower than this one;
- it sits above every frontend figure measured across the whole campaign except
  the very first (1.49 s, before three of the six fixes), which means **any of
  the six defects reappearing would break it** — including the cheapest, the NFC
  pass, worth 23 ms native and more under TCG;
- it leaves room for the frontend to grow as TOS Core gains checked rules,
  without reopening this ADR for ordinary measurement drift.

If ADR-0044 is later accepted and implemented, the frontend's largest single
cost falls and this threshold should be revisited **downward** — which is the
right direction for a budget to move and a reason to keep it in evidence.

## Relation to the docs/35 regression policy

A threshold catches a step change; it does not catch slow erosion. Retained
benchmark history against the docs/35 regression policy is the instrument for
that, and this ADR does not ask it to do a threshold's job — the mistake
ADR-0043 had to correct for the engine ratio.

## Consequences

If accepted, all three Stage 2 performance metrics pass on measured evidence and
the gate stops blocking closure. If rejected, Stage 2 does not close, and the
reason on record is a number written before the thing it measures existed.
