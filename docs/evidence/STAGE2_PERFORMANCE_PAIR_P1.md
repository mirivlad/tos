<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Stage 2 performance — normative pair at commit 46911ef

Evidence level: **P1** (locally measured, docs/35). Verdict: **3 PASS**.
Procedure: the normative one — 3 warmups, 21 samples, median/p95/p99, one
commit, one set of fixtures, both halves.
Fixtures: emitted by `tos-core-performance --emit-fixture`, so both halves
measure the **same bytes**. Frontend fixture 262 114 bytes, content
`sha256:40ea301db1e52502190794049cecf65ddc40a76cde07a98181cca9a1aa433a98`.
Reference platform: ADR-0040 — q35, qemu64, 1 vCPU, 256 MiB, TCG, OVMF, through
the real freestanding Stage 2 path.

## The pair

| metric | native p95 | reference p95 | budget | verdict |
|---|---|---|---|---|
| frontend, 256 KiB module | 120 943 us | 1 212 216 us | 1 500 000 us (ADR-0045) | **PASS** |
| engine, 1e6 operations | 207 873 us | 3 655 641 us | ratio ≤ 22x (ADR-0043) | **PASS** (17.6x) |
| quota rejection | — | 469 738 us | ≤ 2x accepted | **PASS** (0.388) |

Reference medians: frontend 1 138 712 us, engine 3 603 540 us, rejection
447 965 us. All raw samples are in the harness output and `reference.json`.

Taken **after** the ADR-0046 pattern work and the ADR-0047 match lowering, so it
describes the implementation being offered for closure rather than one that
predates it. Every metric is inside its accepted budget, and the frontend figure
sits within the spread these runs have shown.

## A fact the Architect should have about the engine threshold

ADR-0043's 22x was accepted as "about +30% on the measured figure", where the
measured figure was 16.8x. The ratio has since drifted:

```text
before the per-instruction clone fix   333 743 / 5 541 378 = 16.6x
after it                               209 128 / 3 628 441 = 17.3x
normative pair at f05e7c8              217 473 / 4 154 811 = 19.1x
normative pair at cf806de              201 501 / 3 993 138 = 19.8x
normative pair at fca219a              205 880 / 4 147 373 = 20.1x
normative pair at 46911ef              207 873 / 3 655 641 = 17.6x
```

**The drift is not monotone.** It ran 16.6, 17.3, 19.1, 19.8, 20.1, then back to
17.6, which leaves about 20% headroom against 22x at the current figure.

An earlier version of this record attributed the last move to the match-lowering
change, on the reasoning that source order stopped the lowerer building a
variant map. **That explanation was wrong and is withdrawn.** The engine metric
is measured from the last `TOS.RUN.STAGE` — the announcement that execution is
beginning — to the first result event, so lowering has already finished before
the span opens. A change to `lower_match` cannot move a number that starts after
lowering ends.

What the series shows instead is the honest limit of P1 evidence: single-machine
run-to-run variation of this size, in both directions, means **no trend can be
read from these points**. The measured figures stand; the causal story does not.
ADR-0043's 22x is not reopened on this basis, and a P2 or CI record with repeated
runs is what would turn this range into something a trend could be read from.

## Where the frontend's 104 ms goes

```text
read       0.4 ms     transport validity (ASCII fast path)
parse      5.7 ms
check     26.1 ms     ten slices; typing derived once, not three times
lower     23.5 ms     IR construction and a 12 058-entry source map
verify    47.5 ms
  module digest       34.4 ms
  source-map digest   12.3 ms
  the nine checks      1.2 ms
```

**The independent verifier's verification costs 1.2 ms; its identity binding
costs 46.6 ms.** The verifier is not slow — hashing the module's identity is,
and the stream it hashes is 5 975 526 bytes for a 262 114-byte module because
every count is a 16-byte `u128` and the source map repeats six module-level
identity strings in each of its 12 058 entries.

`docs/evidence/STAGE2_MODULE_DIGEST.md` decomposes it and **ADR-0044 (Proposed)**
carries the question; by Architect direction it is a documented future
improvement rather than a Stage 2 blocker. **ADR-0045 (Accepted)** revised the
frontend budget from the original 500 ms research estimate to the measured
1500 ms this pair is judged against.

## History — superseded measurements, kept as history

Nothing below describes the current implementation. It is retained because the
budgets that now apply were decided on it.

The first normative pair measured the engine at **16.6x against the then-current
10x budget** and the frontend at **2.98x over the then-current 500 ms**. It was
taken before the engine's per-instruction clone, the transport-validation fast
path, the verifier's eager finding locations and the checker's triple typing
derivation were fixed, and before ADR-0043 and ADR-0045 revised the two budgets
on that evidence. Earlier revisions of this file are in the git history and are
not rewritten.
