<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0083: Repairing the Stage 1 validation-performance metric after TCG layout falsified the cross-artifact ratio

- Status: **Proposed — not Project Architect-approved.** It proposes no
  threshold, and must not be marked accepted before the repaired metric's
  evidence has been reviewed
- Date: 2026-09-05
- Decision level: **2** — it replaces the construction of an accepted
  conformance metric and its gate, and amends ADR-0026 and
  `docs/35_PERFORMANCE_CONTRACTS.md`. It changes no invariant, no ABI, no
  language contract and no production code path
- Related: ADR-0025 and **ADR-0026** (the metric this repairs, superseded in its
  semantic interpretation and preserved in its history), ADR-0065 (what a green
  run means), `docs/35_PERFORMANCE_CONTRACTS.md`,
  `docs/evidence/stage4c1-adr0026-investigation/` (the falsification), ADR-0082
  (the Stage 4C work that exposed it, itself unaffected)

## 1. What happened, in order

ADR-0026 was accepted in good faith from the evidence available in Stage 1. Its
ratio was measured, its samples were real, and nothing about the round that
produced it was careless.

Stage 4C then supplied a controlled falsification of its **construct validity**
— not of its numbers. Implementing the ADR-0082 ownership repairs moved the
ratio from ~1.11 to 2.09–2.27 against a 1.30 bound, in a workload that
provably executes none of the new code. The investigation
(`docs/evidence/stage4c1-adr0026-investigation/`) established, by controlled
experiment rather than by inference:

- executed validation work is identical — same capsule bytes and digest, same
  files validated, same SHA byte and invocation accounting, same ordered event
  sequence, same memory account, and no `pci_function_claim` at all;
- the hot validation and hashing implementation is unchanged apart from
  address-relative relocation;
- an **inert layout displacement** — executing nothing, adding no reachable
  work, and leaving the raw image byte-for-byte the same length — moves the
  ratio from ~1.11 to 1.546, across the conformance boundary, while native
  execution is unmoved at 0.999;
- appended inert growth that does **not** displace the hot path does not
  reproduce it, at any size from 64 bytes to a page-crossing 4096;
- the effect is confined to nucleus validation;
- the metric's own repeat noise on an unchanged binary is ±3%, against a 30%
  budget and an effect of 40–100%.

The explanatory mechanism — a guest 4 KiB page boundary falling inside
`Sha256::compress_block`, which a TCG translation block may not span — is
useful and is recorded, but the ruling that accepted this evidence does not make
it a normative dependency. **The construct-validity failure follows from the
controlled experiment itself**, whatever the emulator's internal reason.

## 2. Why the quotient could not cancel it

A ratio exists to divide out what is common. This one could not, for two
independent structural reasons.

**Two artifacts.** The numerator was measured in the production nucleus and the
denominator in a *separately linked* `test-crypto-baseline` nucleus — 179312 and
134216 bytes, different layouts, translated independently. Nothing about the
emulator's layout sensitivity is shared between them, so nothing about it
cancels.

**Two incomparable intervals**, found while building the repair and recorded
here because it is a second defect of the same metric and not a detail of the
first:

```text
numerator     TOS.BOOT.ENTRY                  ->  TOS.BOOTTEXT.PATH
denominator   TOS.TEST.CRYPTO.BASELINE.START  ->  TOS.TEST.CRYPTO.BASELINE.DONE
```

They do not begin at the same instant and do not cover the same component. The
numerator carries the entire UEFI loader phase — roughly 1370 ms of the ~2740 ms
measured, itself largely hashing, performed by a **different binary** that this
metric neither links nor varies. The denominator carries none of it. Their
quotient was therefore not a ratio of two comparable quantities even before
layout was considered.

## 3. What is superseded, and what is not

**Superseded**: the semantic interpretation attached to the quotient — that

```text
production_full_exact_p95 / separately_linked_unavoidable_crypto_p95 <= 1.30
```

caps non-cryptographic validation overhead at 30% of unavoidable cryptographic
cost. That reading requires the ratio to move when and only when
non-cryptographic validation work changes, and §1 shows it does not.

**Not superseded, and not to be rewritten**:

- ADR-0026 itself, which stays in the record as the decision it was;
- the Stage 1 closure record, which is untouched;
- the P1 and P2 evidence already collected, which remains valid historical
  evidence of *what was measured*;
- `1.30`, which remains the historical threshold **of the superseded metric**.
  It is not silently carried into the replacement, and §7 forbids assuming it.

The old gate is not deleted. Stage 4C does not become green by removing the
thing that failed.

## 4. D1 — one artifact, two runtime-selected modes

**Decided (architecture approved for implementation and measurement; threshold
not approved).**

```text
one measurement-only nucleus image — one ELF, one SHA-256
        │
        ├── FULL_EXACT
        │     the production validation path, reached by falling through
        │     into the same `nucleus_main` body: capsule hashing, parser
        │     validation, detached identity, canonical lookup, boot-text digest
        │
        └── UNAVOIDABLE_CRYPTO
              exactly the accepted unavoidable cryptographic work, the same
              hashing implementation over the same bytes, with no parser or
              hash result carried over from the other mode
```

Both series come from the same bytes, so linker layout, function placement, code
addresses, static data placement and the TCG translation environment are shared
and cancel in the quotient. That is the whole of the repair's mechanism.

**Both series start at `TOS.NUCLEUS.ENTRY`**, so both cover the same component
of the same image and the loader — a different binary, doing its own hashing —
is outside both sides rather than inside one.

## 5. D2 — the selector

Mode selection must not require separately linked artifacts, so it is read at
run time from a measurement-only value published through the emulator's
firmware-configuration interface.

| Requirement | How it is met |
|---|---|
| fixed before the timed interval | set on the emulator's command line, before the machine starts |
| does not alter the executable image | the same bytes boot in both modes; the harness proves the digests are equal |
| recorded in retained evidence | the **guest** states which series it is on its own log, so a harness that mislabelled a sample is caught by the guest rather than by arithmetic |
| neither mode reuses the other's work | the modes are separate boots of a machine that retains nothing between them |
| warmups cannot enter measured samples | phase is recorded per sample and the reporter counts them separately |

It adds **no device**, so the machine profile is identical between the two
series; adding one would reintroduce a difference of exactly the kind this
repair removes. It creates no authority, no capability and no public ABI, and it
is never present in a production build. Absent, it means `FULL_EXACT` — so the
artifact boots exactly as production does, and a harness that forgot to select a
mode measures the numerator twice and reports a ratio near one, which is a
visible mistake rather than a silent swap of the two series.

## 6. D3 — production equivalence, proved rather than asserted

The measurement artifact is not the production nucleus, so equivalence is
mechanical:

- **no duplicated algorithm.** The feature adds a selector and a branch *into*
  the crypto baseline. `FULL_EXACT` does not take that branch and falls through
  into the same body, calling the same production implementations. A gate bounds
  the feature's footprint in ring 0 and requires that the selector mention none
  of the work it measures;
- **the same reported work.** The production nucleus and the measurement
  nucleus in `FULL_EXACT` are booted over the same capsule and must agree on
  capsule digest, fixture identity, files validated, canonical lookup target,
  boot-text digest, and the **ordered event sequence** — differing only by the
  one line naming which series the measurement artifact is.

The memory account is deliberately not compared: the measurement artifact is a
larger image, so it occupies one frame more and admits one fewer to the pool.
That follows from it being a different binary and is not a difference in
validation work.

**The ordinary production functional QEMU boot gate is separate and unchanged.**
The paired benchmark does not replace functional production boot testing.

## 7. D4 — the reporter refuses

> No conformance ratio is computed unless the two series report **exactly
> equal** image digests.

This is stated as a decision rather than left to implementation because it is
the property that makes the repair a repair. A reporter that computed a ratio
across two artifacts would be reproducing the falsified construct with better
paperwork.

Retained beside every report, as **diagnostic identity and never as a
threshold**: ELF size, `.text` address and size, and the hot hashing symbol's
address. Those are what the old metric was accidentally measuring; recording
them makes a future movement attributable instead of mysterious.

## 8. D5 — the measurement profile is unchanged

`q35`, `qemu64`, one vCPU, 256 MiB, TCG, the same OVMF identity, the same
deterministic 1,000-file capsule, 3 warmups, 21 measured samples, nearest-rank
p95/p99, raw samples retained. Nothing about the accepted discipline is relaxed;
what changes is what is being divided by what.

Native same-artifact measurement is required as comparison and archive evidence.
KVM remains optional research evidence and is not required while the recorded
host cannot boot TOS under it — `/dev/kvm` is present and the nucleus fails
identically on both trees with `TOS.RUN.UNSTARTABLE reason=no-address-space`.

## 9. The threshold is deliberately absent

**This ADR proposes no number, and `1.30` is not carried over.**

A threshold is proposed only after the repaired metric has been measured — three
complete TCG paired series from clean rebuilds of each tree, three native paired
series, the same-artifact digests, raw 3+21 samples, medians, p95, p99, segment
decomposition and repeatability statistics — and it is brought back for review
rather than adopted here.

When it is proposed it must: have a semantic interpretation; leave measured
headroom rather than equal the worst sample; account for measured repeatability;
still detect a meaningful structural validation regression; not encode TCG
page-placement artifacts; and preserve the existing rule that a >15% regression
requires explanation and >30% blocks unless an ADR changes the contract.

**A finding the measurement has already produced, which the threshold must
confront.** With comparable intervals in one artifact, `FULL_EXACT` measures
about **0.34** of `UNAVOIDABLE_CRYPTO`. The denominator's accepted model — two
parser-crypto replays, two whole-capsule mirror digests and a boot-text digest,
101,203,397 bytes over 2007 invocations — is roughly three times the
cryptographic work the nucleus performs in the interval the numerator covers. A
budget phrased as "overhead ≤ 30% of unavoidable crypto" is therefore satisfied
by a wide margin for a reason that has nothing to do with overhead. **Whether
the denominator's model, the numerator's scope, or the budget's phrasing is what
needs adjusting is a question for review, and this ADR does not decide it** — the
accepted definition of unavoidable cryptographic work is not something an
implementation may quietly redefine to make a number come out.

## 10. Gate transition

Until this ADR is accepted:

- the old ADR-0026 gate is preserved as historical and superseded evidence, and
  is not deleted to make Stage 4C pass;
- the paired harness computes and reports but applies no verdict;
- the ownership-repair branch is not merged while it would leave required CI
  red.

After approval of the metric **and** a threshold, atomically in one change:

- the old cross-artifact `≤ 1.30` gate becomes a historical
  regression/reproduction tool and stops being active conformance;
- the same-artifact paired metric becomes Stage 1 validation-performance
  conformance;
- ADR-0026, `docs/35_PERFORMANCE_CONTRACTS.md`, the preflight inventory and CI
  change together.

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none. No ABI, no language
  contract, no capability, no production code path.
- **Trusted-base impact:** none in production. The nucleus gains a
  measurement-only module, compiled only under a feature no production build
  selects, which reads two emulator ports and branches.
- **Source-to-runtime impact:** none.
- **Threat-model impact:** none. The selector confers nothing and is absent from
  production images.
- **Compatibility profile:** unchanged. The measurement discipline of ADR-0025
  and ADR-0026 is retained; their semantic interpretation of the quotient is
  superseded.
- **Evidence impact:** existing P1/P2 evidence remains valid as a record of what
  was measured. The Stage 1 closure record is untouched.

## 11. Conformance evidence

1. both series report the same image SHA-256, and the reporter refuses when they
   do not;
2. `FULL_EXACT` and a production boot agree on capsule digest, fixture identity,
   files validated, canonical lookup, boot-text digest and ordered event
   identity;
3. the selector's ring-0 footprint stays a selector and a branch, and reimplements
   none of the measured work;
4. the guest names its own series on every sample;
5. three complete TCG paired series from clean rebuilds of each tree;
6. three native paired series;
7. raw 3+21 samples, medians, p95, p99 and repeatability retained for every
   series;
8. segment decomposition for `FULL_EXACT`;
9. **the pre-ownership tree and the Stage 4C ownership-repair tree are no longer
   materially distinguished** when their Stage 1 executed work is unchanged. If
   they still are, the construct is still confounded and this ADR is not ready.
