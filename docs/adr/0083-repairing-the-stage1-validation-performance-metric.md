<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0083: Repairing the Stage 1 validation-performance metric after TCG layout falsified the cross-artifact ratio

- Status: **Accepted (Project Architect-approved, 2026-09-06).** The measurement
  construction and one blocking conformance threshold are approved:

  ```text
  same_artifact_full_exact_p95 / same_artifact_unavoidable_crypto_p95 <= 1.30
  ```

  §9 states it, §10 records the gate transition that made it active
- Date: 2026-09-05. Accepted 2026-09-06
- Project Architect approval: Vladimir Tomashevskiy, 2026-09-06, on the
  construction implemented at `09a35c4` and the evidence measured at `d580fe9`
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
is never present in a production build.

**An absent selector means `FULL_EXACT`, which is the safe default and nothing
more.** It is not a claim that the artifact then boots as production does — it
does not; it runs the numerator's measured workload and halts. The earlier form
of `FULL_EXACT` fell through into the ordinary production boot and that sentence
was true of it; `09a35c4` replaced it and the sentence with it. What the default
buys is that a harness which forgot to select a mode measures the numerator
twice and reports a ratio near one, rather than silently swapping the two
series.

**The protection against measuring the wrong series is the guest's own report**,
not the default: every sample carries `TOS.TEST.PAIRED.MODE ... asserted_by=nucleus`,
and the harness fails the series when a sample it asked for one mode ran the
other.

## 6. D3 — workload equivalence, proved rather than asserted

The measurement artifact is not the production nucleus, so equivalence is
mechanical. **What is proved is the workload, not the event sequence.** An
earlier form of `FULL_EXACT` fell through into the ordinary production boot, and
for that form equality of the complete ordered production event sequence was the
right proof. `09a35c4` replaced it: `FULL_EXACT` is now the two-validator
logical workload of §4, which is deliberately not the shape of one production
boot. Ordered-event equality would now be the wrong question, and asking it
would fail a correct artifact.

`source/host-tools/qemu-test/paired-equivalence.sh` therefore boots production
and both measurement modes over one fixture and proves:

- **the shape of the numerator's workload**, reported by the guest that
  performed it rather than read out of the source: `parses=2`,
  `capsule_digests=2`, `lookup_from=second`;
- **pass 1 is scoped out of pass 2** — the first parsed view is bound inside a
  block yielding only its file count, so no parsed object and no digest can
  cross between the two passes. This is a property of scope rather than of
  output, so it is checked in the source;
- **the values are production's values** for the same fixture: files validated,
  canonical lookup path, boot-text digest and capsule digest all equal what the
  production nucleus reports on the same capsule;
- **the denominator's accounting is the accepted one**, exactly: `101203397`
  bytes over `2007` invocations;
- **both modes share the boundary** `TOS.TEST.PAIRED.START` after an identical
  untimed prefix;
- **no algorithm is duplicated.** The selector module may mention none of the
  work it measures, the feature's ring-0 footprint is bounded, and the
  orchestration must be seen calling the production `sha256`, `parse`,
  `boot_file` and parser crypto replay.

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

## 9. The threshold — accepted, from the corrected distribution

**Accepted 2026-09-06.** One blocking conformance line, and no second absolute
line:

```text
same_artifact_full_exact_p95 / same_artifact_unavoidable_crypto_p95 <= 1.30
```

**Interpretation** — the one ADR-0026 always claimed and its construction could
not deliver: the complete exact Stage 1 logical validation costs at most 30%
more than the unavoidable cryptographic subset **of that same logical
workload**, in one artifact, over one interval, from one boundary.

Six complete TCG series — three clean rebuilds of each tree — and six native
series, all in `docs/evidence/stage4c1-adr0083-paired-metric/`:

```text
TCG p95 ratio     pooled mean 1.0076   min 0.9549   max 1.0746   stdev 4.0%
TCG median ratio  pooled mean 0.9977   min 0.9826   max 1.0109   stdev 1.2%
native            pooled mean 0.9988   min 0.9900   max 1.0146   stdev 1.0%
between trees     mean difference 0.0075, against a pooled stdev of 0.0401
```

The sanity property of §4 holds: the centre is 1.00, where the mismatched form
sat at 0.35. The blocking line sits **21% above the worst of the six series**,
which is the headroom it was chosen for.

**`1.30` is not carried over, and is not the same decision.** The old number
bounded a quotient of two artifacts over two incomparable intervals, where it
meant nothing checkable. The numerical coincidence is exactly that: this
distribution centres on 1.00, 30% is the budget the corpus already states for a
hard-gated metric, and the line is derived from the evidence above rather than
inherited from the superseded construction.

### There is deliberately no second absolute line

An earlier draft of this section proposed `> 1.15` as an absolute
"explanation required" threshold beside the blocking one. **That is rejected**,
because it conflates two different quantities:

1. **absolute structural overhead** relative to the unavoidable cryptographic
   subset — governed by the hard `1.30` conformance budget here;
2. **regression** relative to a retained accepted baseline — governed by
   `docs/35_PERFORMANCE_CONTRACTS.md`, "Regression policy": above 15% requires
   explanation, above 30% blocks unless an ADR changes the contract.

The repository's percentages apply to the **retained baseline** (§10), not to
the mathematical constant `1.0`. A metric whose centre happens to sit near one
does not make those two readings the same statement, and writing them as one
line would have made a future 15% regression argument about arithmetic instead
of about the baseline.

### The conformance statistic stays the nearest-rank p95

The median ratio is retained beside it as **diagnostic and regression evidence**
and is not the conformance statistic. The measured noise difference below is a
recorded limitation, not a reason to substitute one for the other: the
separation between the observed distribution and the `1.30` budget is large
enough for what this gate is for.

The σ figures are **descriptive of six series and are not a normative
statistical guarantee.** Six is too few to make a distributional claim, and
none is made.

### Two limitations this threshold carries

**The structural overhead is below the measurement's resolution.** In two of six
series the numerator came out *below* the denominator, and the pooled mean is
1.0076 with a 4.0% stdev. Stage 1 validation over the current 1,000-file /
16-MiB fixture is overwhelmingly cryptographic: hashing dominates parsing and
lookup so completely that the structural remainder is around or under one
percent. **This gate therefore detects catastrophic validation-architecture
overhead and does not resolve small structural drift.** The line bounds
catastrophe; it is not a figure tracking a real trend.

Making the metric resolve structural cost would need a fixture that shifts work
away from hashing. That is deliberately **not** done here: it is a later
performance-research item, and it blocks neither this ADR nor Stage 4C.

**The p95-of-ratio is three times noisier than the median-of-ratio** — 4.0%
against 1.2% — because it divides two independently drawn tail estimates, so
both tails' noise enters the quotient. §8's accepted discipline names nearest-rank
p95 and this ADR keeps it, for the reason above: the distance between the
observed distribution and `1.30` is large compared with either noise figure.

## 10. Gate transition

Performed atomically on acceptance, in one change:

- **the old ADR-0026 cross-artifact ratio stops being active conformance.** It
  is not deleted. `stage1-performance-historical.sh` still measures the
  production TCG series, the separately linked crypto series and the native
  series, still records the quotient, and asserts nothing about it. Stage 4C
  does not become green by removing the thing that failed; it becomes green
  because the thing that failed was measuring the linker;
- **the same-artifact paired p95 ratio becomes Stage 1 validation-performance
  conformance**, as `stage1-paired-conformance.sh`.

The active gate fails when:

1. the two series' image SHA-256 differ;
2. either series retains the wrong sample count;
3. the guest-reported mode disagrees with the series the harness asked for;
4. the workload-equivalence or accounting gates of §6 fail;
5. the p95 ratio exceeds `1.30`.

**Retained separately, and not replaced by any of this:** the ordinary
production functional QEMU boot gates; production absolute boot timing as
observational and regression evidence; the native paired series; and the
historical ADR-0026 evidence.

### Retained baseline

The accepted corrected distribution of §9 is the retained baseline. Both figures
are recorded per run — **the p95 ratio, which is conformance, and the median
ratio, which is diagnostic** — and the repository's regression policy applies to
both *relative to that baseline*, not relative to 1.0.

```text
retained baseline, TCG, 2026-09-06, from d580fe9

  p95 ratio      1.0076   (six series, 0.9549 – 1.0746)
  median ratio   0.9977   (six series, 0.9826 – 1.0109)
  native p95     0.9988   (six series, 0.9900 – 1.0146)
```

## Architecture impact statement

- **Change level:** 2. **Invariants affected:** none. No ABI, no language
  contract, no capability, no production code path.
- **Trusted-base impact:** none in production. The measurement feature is absent
  from every production artifact, which is why the impact is none — not because
  the feature is small. **It is no longer merely a selector and a branch.** It
  is a selector module that reads two emulator ports, plus measurement-only
  orchestration of the accepted logical workload: the order in which the
  production `sha256`, `parse`, `boot_file` and parser crypto replay are called,
  the scoping that keeps the two passes independent, and the events a gate reads
  the shape from. It implements none of the work it sequences, and §6's gate
  proves that mechanically rather than by inspection.
- **Source-to-runtime impact:** none.
- **Threat-model impact:** none. The selector confers nothing and is absent from
  production images.
- **Compatibility profile:** unchanged. The measurement discipline of ADR-0025
  and ADR-0026 is retained; their semantic interpretation of the quotient is
  superseded.
- **Evidence impact:** existing P1/P2 evidence remains valid as a record of what
  was measured. The Stage 1 closure record is untouched.

## 11. Conformance evidence

Every run of the active gate:

1. both series report the same image SHA-256, and the reporter refuses to
   compute a quotient when they do not;
2. **the workload-equivalence gates of §6** — `parses=2`, `capsule_digests=2`,
   `lookup_from=second`, pass 1 scoped out of pass 2, files/path/boot-text
   digest/capsule digest equal to production's on the same fixture, the
   denominator's exact `101203397`/`2007` accounting, one common boundary, and
   no duplicated algorithm;
3. the guest names its own series on every sample, and the harness fails the
   series when a sample ran a mode other than the one requested;
4. raw 3+21 samples with medians, p95 and p99 retained for both series;
5. the p95 ratio against `1.30`, and the median ratio recorded beside it as
   diagnostic.

Collected once for acceptance, and retained:

6. three complete TCG paired series from clean rebuilds of each tree;
7. three native paired series;
8. **the pre-ownership tree and the Stage 4C ownership-repair tree are no longer
   materially distinguished** when their Stage 1 executed work is unchanged —
   0.0075 apart, where the old cross-artifact metric separated them by about
   1.9×. This is the construct-validity test the repair had to pass.

Segment decomposition belongs to the production boot series, which retains it,
and not to this metric: `FULL_EXACT` is a measured logical workload rather than
a boot with phases.
