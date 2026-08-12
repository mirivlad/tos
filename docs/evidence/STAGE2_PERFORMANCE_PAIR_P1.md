<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
# Stage 2 performance — normative pair at commit f05e7c8

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
| frontend, 256 KiB module | 124 379 us | **1 225 348 us** | 500 000 us | **FAIL** (2.45x over) |
| engine, 1e6 operations | 217 473 us | 4 154 811 us | ratio ≤ 22x (ADR-0043) | **PASS** (19.1x) |
| quota rejection | — | 511 207 us | ≤ 2x accepted | **PASS** (0.417) |

Reference medians: frontend 1 195 871 us, engine 3 974 852 us, rejection
487 687 us. All raw samples are in the harness output and `reference.json`.

## A fact the Architect should have about the engine threshold

ADR-0043's 22x was accepted as "about +30% on the measured figure", where the
measured figure was 16.8x. The ratio has since drifted:

```text
before the per-instruction clone fix   333 743 / 5 541 378 = 16.6x
after it                               209 128 / 3 628 441 = 17.3x
this normative pair                    217 473 / 4 154 811 = 19.1x
```

The drift is upward as the native half gets faster, which is what a fixed guest
overhead does to a ratio: shrinking the denominator raises the quotient. **The
real headroom against 22x is therefore about 13%, not 30%.** The gate passes and
the margin is thinner than the number was chosen to give. That is stated here
rather than left for someone to rediscover; whether 22x should move is the
Architect's, and this ADR-0043 is already accepted at 22x.

## Frontend: still FAIL, and where it goes

```text
read       0.4 ms     transport validity (ASCII fast path)
parse      5.7 ms
check     26.1 ms     ten slices; typing now derived once, not three times
lower     23.5 ms
verify    48.3 ms     of which 35.3 ms is the module digest
```

The single largest item in the whole frontend is **SHA-256 over the canonical
hash stream** — 33 ms of a ~104 ms total. The stream is 5 975 526 bytes for a
262 114-byte module, a 22.8x expansion caused by encoding every count and length
as a 16-byte `u128`. `docs/evidence/STAGE2_MODULE_DIGEST.md` has the
decomposition and **ADR-0044 (Proposed)** the question, because the stream is the
module's identity rather than an internal representation.

Nothing here says 500 ms is unreachable. It says the largest identified cost is
now understood, has a known shape, and needs a decision that is not the
implementation's to make.

## History

The previous normative pair (engine 16.6x against the then-current 10x budget,
frontend 2.98x over) is retained in the git history of this file and is not
rewritten. It was taken before the engine's per-instruction clone, the
transport-validation fast path, the verifier's eager finding locations and the
checker's triple typing derivation were fixed.
