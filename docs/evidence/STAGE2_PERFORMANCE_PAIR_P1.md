<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Stage 2 performance — normative pair at commit fca219a

Evidence level: **P1** (locally measured, docs/35). Verdict: **1 FAIL, 2 PASS**.
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
| frontend, 256 KiB module | 119 468 us | 1 227 354 us | 1 500 000 us (ADR-0045) | **PASS** |
| engine, 1e6 operations | 205 880 us | 4 147 373 us | ratio ≤ 22x (ADR-0043) | **PASS** (20.1x) |
| quota rejection | — | 506 038 us | ≤ 2x accepted | **PASS** (0.412) |

Reference medians: frontend 1 187 729 us, engine 3 808 802 us, rejection
466 836 us. All raw samples are in the harness output and `reference.json`.

Taken **after** the ADR-0046 pattern work, so it describes the implementation
that is being offered for closure rather than one that predates it. No metric
regressed: the frontend moved from 1 140 356 to 1 227 354 us, well inside the
budget and inside the spread these figures have shown across runs.

## A fact the Architect should have about the engine threshold

ADR-0043's 22x was accepted as "about +30% on the measured figure", where the
measured figure was 16.8x. The ratio has since drifted:

```text
before the per-instruction clone fix   333 743 / 5 541 378 = 16.6x
after it                               209 128 / 3 628 441 = 17.3x
normative pair at f05e7c8              217 473 / 4 154 811 = 19.1x
normative pair at cf806de              201 501 / 3 993 138 = 19.8x
normative pair at fca219a              205 880 / 4 147 373 = 20.1x
```

The drift is upward as the native half gets faster, which is what a fixed guest
overhead does to a ratio: shrinking the denominator raises the quotient. **The
real headroom against 22x is now about 10%, not the 30% the threshold was
reasoned from.** The gate passes and the margin is thinner than the number was
chosen to give. The Architect has directed that 22x is not reopened on P1
single-machine variation; this is recorded so a future P2/CI record has
something to compare against. That is stated here
rather than left for someone to rediscover; whether 22x should move is the
Architect's, and this ADR-0043 is already accepted at 22x.

## Frontend: still FAIL, and where it goes

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
improvement rather than a Stage 2 blocker. **ADR-0045 (Proposed)** asks for the
500 ms research estimate itself to be revised on this evidence.

## History

The previous normative pair (engine 16.6x against the then-current 10x budget,
frontend 2.98x over) is retained in the git history of this file and is not
rewritten. It was taken before the engine's per-instruction clone, the
transport-validation fast path, the verifier's eager finding locations and the
checker's triple typing derivation were fixed.
