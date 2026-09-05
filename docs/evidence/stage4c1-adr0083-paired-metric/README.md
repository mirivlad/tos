<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0083 — the repaired metric, measured

- Status: **evidence, 2026-09-05.** ADR-0083 is **Proposed — not Project
  Architect-approved.** No threshold is active, no gate is changed, and the old
  ADR-0026 gate is preserved
- Harness: `source/host-tools/qemu-test/paired-measurement.sh`,
  `paired-equivalence.sh`, `paired-report.py`, `paired-interval.py`
- Trees compared: `e20dbb6` (pre-ownership + harness) and the same with the
  ADR-0082 ownership repairs cherry-picked (`6fc0bf5`)

## 1. The success criterion

The ruling's test of the repair was: *the repaired metric should no longer
materially distinguish the pre-ownership tree from the Stage 4C tree, when their
Stage 1 executed work is unchanged.*

| metric | pre-ownership | Stage 4C | separation |
|---|---|---|---|
| **old** cross-artifact ratio | 1.114 – 1.174 | 2.095 – 2.271 | **~1.9×, across the bound** |
| **new** same-artifact ratio | 0.340 – 0.365 | 0.338 – 0.356 | **overlapping** |

```
between-tree mean difference   0.0058   (1.6% of the pooled mean)
within-tree spread             0.0246 (base) / 0.0174 (Stage 4C)
```

**The between-tree difference is smaller than the spread within either tree.**
The two trees are no longer operationally distinguishable, which is what the
repair had to achieve.

## 2. TCG paired series — three clean rebuilds per tree

Every series rebuilds the measurement artifact from scratch, so a series is a
draw from the build as well as from the machine. Both members of a series come
from the **same image digest**; the reporter refuses to compute a ratio
otherwise.

| tree | series | image sha256 | ratio p95 | ratio median | full p95 | crypto p95 |
|---|---|---|---|---|---|---|
| base | 1 | `5256170985416d2c…` | 0.3403 | 0.3360 | 1227.8 ms | 3608.1 ms |
| base | 2 | `5256170985416d2c…` | 0.3649 | 0.3650 | 1287.9 ms | 3529.0 ms |
| base | 3 | `5256170985416d2c…` | 0.3539 | 0.3368 | 1181.9 ms | 3339.5 ms |
| s4c | 1 | `af6ff783d94148da…` | 0.3476 | 0.3361 | 1204.8 ms | 3466.5 ms |
| s4c | 2 | `af6ff783d94148da…` | 0.3384 | 0.3368 | 1149.3 ms | 3396.9 ms |
| s4c | 3 | `af6ff783d94148da…` | 0.3558 | 0.3370 | 1201.2 ms | 3375.7 ms |

Pooled: mean **0.3502**, min 0.3384, max 0.3649, stdev 0.0101 (**2.9%**).
Worst observed / pooled mean = **1.042**.

Raw 3+21 samples, per-sample event logs and arrival timestamps are retained per
series; `tcg-paired-matrix.tsv` is the summary and two full reports are included
verbatim.

## 3. Native paired series

The native harness was **already same-artifact** — one `tos-stage1-performance`
binary with `--mode full|crypto` — which is why its ratio never moved during the
construct-validity investigation while the guest's did.

| tree | ratios | mean |
|---|---|---|
| base | 1.0019, 1.0084, 0.9950 | 1.0018 |
| Stage 4C | 0.9878, 1.0217, 1.0180 | 1.0092 |

Also indistinguishable between trees (difference 0.007).

**The native and TCG ratios are not the same number and are not expected to
be.** The native harness's full mode models both the loader's and the nucleus's
validation passes; the guest's `FULL_EXACT` covers the nucleus interval only,
because the loader is a different binary this metric neither links nor varies.
Native is comparison and archive evidence, not a second conformance figure.

## 4. Segment decomposition — `FULL_EXACT`

Medians over 48 retained samples per tree:

| phase | base | Stage 4C |
|---|---|---|
| nucleus validation (`NUCLEUS.ENTRY` → `CAPSULE.OK`) | 1182.91 ms | 1102.07 ms |
| canonical lookup (→ `BOOTTEXT.PATH`) | 0.55 ms | 0.53 ms |
| boot-text digest (→ `BOOTTEXT.DIGEST`) | 0.76 ms | 0.74 ms |
| detached identity (→ `IDENTITY`) | 0.97 ms | 0.93 ms |

Validation dominates; the lookup, digest and identity steps together are under
2.3 ms, about 0.2% of the interval.

## 5. Production equivalence

```
capsule sha256            6dabadc666f46a755c91845bfa896d4ed3fb2b3aeb707ba89da33461bb07959f
fixture identity          18819f894e8f0888a7ddf932cdc4b97a0a823fa65ae6f596826888477d16d89b
files validated           1000
canonical lookup          /system/boot/init.tos
boot-text digest          c5488fcd6918f1dfc484e8c4a7d6f871d84dd8ab16079ea1fd326edcfb441dfa
ordered event identity    90583768709338b562a43d425259785a
no duplicated algorithm   5 cfg sites, selector reimplements nothing
unavoidable-crypto model  bytes=101203397 hashes=2007
```

The ordered event identity `90583768709338b5…` is the same sequence hash the
construct-validity investigation recorded for the production boot, so
`FULL_EXACT` is the production path by the same measure used there.

**The gate earned itself immediately**: it caught an ad-hoc build during
development that had written a paired-feature nucleus into the production target
path, which every other check would have passed.

## 6. The finding a threshold must confront

`FULL_EXACT` measures about **0.35** of `UNAVOIDABLE_CRYPTO`. The accepted
denominator model — two parser-crypto replays, two whole-capsule mirror digests
and a boot-text digest, 101,203,397 bytes over 2007 invocations — is roughly
three times the cryptographic work the nucleus performs in the interval the
numerator covers.

So a budget phrased as *"non-cryptographic overhead ≤ 30% of unavoidable
cryptographic cost"* is satisfied with a factor of three to spare, for a reason
that has nothing to do with overhead. A threshold of `≤ 1.30` on this metric
would pass any conceivable Stage 1 regression and would detect nothing.

**This is not something an implementation may fix by redefining the accepted
model.** Which of three things needs adjusting is a decision:

1. the **denominator's model** — if "unavoidable crypto" should mean the crypto
   the nucleus actually performs in the measured interval, rather than a model
   of the whole boot's crypto;
2. the **numerator's scope** — if the interval should cover the loader too, which
   would require the loader to become part of the same artifact, which it cannot
   be;
3. the **budget's phrasing** — if the quantity worth bounding is not a ratio to
   crypto cost at all.

## 7. Threshold proposal

Because of §6, a numeric threshold on the current denominator would be
meaningless whatever value it took. The proposal is therefore conditional and is
brought for decision rather than adopted:

**If the denominator's model is left as accepted**, the distribution supports:

```
same_artifact_full_exact_p95 / same_artifact_unavoidable_crypto_p95  <=  0.40
```

- **semantic interpretation**: the nucleus's complete validation costs no more
  than 40% of the accepted unavoidable-crypto model, measured in one artifact
  over one interval;
- **headroom**: worst observed 0.3649, so the bound sits 9.6% above the worst
  sample rather than on it;
- **repeatability**: pooled stdev 2.9%; the bound is ≈ 3.4 σ above the pooled
  mean;
- **detects a structural regression**: a 15% increase in validation work moves
  the pooled mean to ~0.403 and breaches it; the existing rule that >15%
  requires explanation and >30% blocks is preserved and now measures validation
  work rather than translation luck;
- **does not encode a TCG artifact**: the page-placement displacement that moved
  the old metric by 40–100% moves this one by less than the noise, because both
  halves share the layout.

**If the denominator's model is narrowed** to the crypto actually performed in
the measured interval, the ratio moves near 1 and the threshold must be
re-derived from a fresh distribution. That is the better metric in the long run
and is a larger change than this ADR should make unreviewed.

**`1.30` is not carried over in either case.** It belongs to the superseded
construction.

## 8. What is not claimed

- no gate is switched; the old ADR-0026 gate is preserved and still active;
- `main` is untouched at `1c3bb49`;
- the Stage 4C ownership repairs are unmerged at `6fc0bf5` and unmodified;
- KVM is not obtainable on this host — `/dev/kvm` exists but the nucleus fails
  identically on both trees with `TOS.RUN.UNSTARTABLE reason=no-address-space`.
