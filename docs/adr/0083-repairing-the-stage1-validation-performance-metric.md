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

## 4. D1 — one artifact, two runtime-selected modes, one logical workload

**Decided (architecture approved for implementation and measurement; threshold
not approved).**

```text
one measurement-only nucleus image — one ELF, one SHA-256
        │
        ├── FULL_EXACT
        │     two fresh complete logical validation passes
        │     two fresh plain whole-capsule mirror digests
        │     no parse or digest result shared between passes
        │     canonical /system/boot/init.tos lookup on the second pass
        │     fresh boot-text digest
        │
        └── UNAVOIDABLE_CRYPTO
              the cryptographic subset of exactly that workload
              two fresh parser-crypto passes
              two fresh whole-capsule mirror digests
              one fresh boot-text digest
```

Both series come from the same bytes, so linker layout, function placement, code
addresses, static data placement and the TCG translation environment are shared
and cancel. That is the first half of the repair.

**The second half is that the two describe the same logical operation.** An
earlier form measured *one* validation pass against a denominator modelling
*two*, and a ratio near a third was the arithmetic of that mismatch rather than a
statement about structural overhead. A quotient meant to read as

> structural validation cost, relative to the cryptographic work that same
> validation necessarily performs

must compare the whole operation with the crypto subset **of that operation**.

`FULL_EXACT` is the sequence the native runner already models in
`validate_twice_and_lookup()`:

```text
digest_1 = sha256(capsule_bytes)
{ parse_1 = parse(capsule_bytes) }        scoped, so nothing crosses
digest_2 = sha256(capsule_bytes)          require digest_2 == digest_1
parse_2  = parse(capsule_bytes)
boot     = parse_2.boot_file()            require /system/boot/init.tos
boot_digest = sha256(boot.content)
```

and `UNAVOIDABLE_CRYPTO` is `validate_unavoidable_crypto_twice()`. Both call the
production `sha256`, `parse`, `boot_file` and parser crypto replay; the
measurement code supplies the order and nothing else.

**The denominator's accepted model is not narrowed.** Two parser passes, two
whole-capsule mirrors, one boot-text digest — `101203397` bytes over `2007`
invocations for the current fixture. Redefining accepted "unavoidable
cryptographic work" to make a ratio approach one is not an implementation's to
do.

### The boundary

Both modes emit **`TOS.TEST.PAIRED.START`** at the same point, after an
identical untimed prefix that includes one common setup parse. The ordinary
boot, the loader and the setup are therefore outside both intervals rather than
inside one, and whatever they did to translation and cache state is common to
both. The setup parse's digests enter neither timed workload; both recompute
from `cap_bytes`.

`TOS.NUCLEUS.ENTRY` is not a ratio boundary, and `TOS.BOOTTEXT.PATH` is not the
numerator's end — the boot-text digest is inside the numerator, as this document
always described it. `FULL_EXACT` ends at `TOS.TEST.PAIRED.FULL.DONE`.

### What this metric is not

**It is not the wall-clock latency of the production loader+nucleus boot.** It
is an architectural validation-efficiency figure: the cost of the exact Stage 1
logical validation workload relative to its own unavoidable cryptographic
subset. Ordinary production boot timing, its segment decomposition and its
retained regression history remain separate observational evidence, and the
ordinary functional boot gate is unchanged.

That separation is deliberate and is better than pretending two separately
linked production components can form one layout-cancelling quotient — which is
precisely what the old construction attempted.

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

## 9. The threshold — proposed from the corrected distribution

**Proposed for review. Not adopted, and not mine to accept.**

Six complete TCG series — three clean rebuilds of each tree — and six native
series, all in `docs/evidence/stage4c1-adr0083-paired-metric/`:

```text
TCG p95 ratio     pooled mean 1.0076   min 0.9549   max 1.0746   stdev 4.0%
TCG median ratio  pooled mean 0.9977   min 0.9826   max 1.0109   stdev 1.2%
native            pooled mean 0.9988   min 0.9900   max 1.0146   stdev 1.0%
between trees     mean difference 0.0075, against a pooled stdev of 0.0401
```

The sanity property of §4 holds: the centre is 1.00, where the mismatched form
sat at 0.35.

```text
same_artifact_full_exact_p95 / same_artifact_unavoidable_crypto_p95

    <= 1.15   regression requires explanation
    <= 1.30   blocking
```

**Interpretation** — the one ADR-0026 always claimed and its construction could
not deliver: the complete Stage 1 logical validation costs no more than 30%
above the unavoidable cryptographic subset **of that same workload**, in one
artifact, over one interval, from one boundary. Because the centre is 1.00 the
two lines are exactly the corpus's existing policy, with no change of units.

| | headroom over worst observed (1.0746) | σ over pooled mean |
|---|---|---|
| 1.15 | 7.0% | 3.55 |
| 1.30 | 21.0% | 7.29 |

**`1.30` is not carried over, and is not the same decision.** The old number
bounded a quotient of two artifacts over two incomparable intervals, where it
meant nothing checkable. It is proposed again only because this distribution
centres on 1.00 and 30% is the policy the corpus already states — derived from
the evidence, not inherited from the superseded metric.

### Two caveats this proposal carries

**The structural overhead is below the measurement's resolution.** In two of six
series the numerator came out *below* the denominator, and the pooled mean is
1.0076 with a 4.0% stdev. Stage 1 validation over this fixture is overwhelmingly
cryptographic: hashing 16 MiB across 1000 files dominates parsing and lookup so
completely that the structural remainder is around or under one percent. These
lines therefore **bound** structural cost; they do not resolve it, and they will
not detect drift. Making the metric resolve structural cost would mean a fixture
that shifts work away from hashing, which is a larger change than this ADR
should make.

**The p95-of-ratio is three times noisier than the median-of-ratio** — 4.0%
against 1.2% — because it divides two independently drawn tail estimates, so
both tails' noise enters the quotient. §8's accepted discipline names nearest-rank
p95 and this proposal keeps it; the measured alternative is recorded because the
ruling admits a change where the experiment demonstrates a specific defect, and
this is a measured one.

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
