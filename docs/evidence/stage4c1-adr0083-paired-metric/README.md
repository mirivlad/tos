<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0083 — the corrected paired metric, measured

- Status: **evidence, 2026-09-05.** ADR-0083 is **Proposed — not Project
  Architect-approved.** No threshold is active, no gate is switched, the old
  ADR-0026 gate is preserved
- Supersedes the first round of this evidence, whose numerator measured one
  validation pass against a denominator modelling two. That mismatch, and the
  `≤ 0.40` proposed from it, were rejected
- Harness: `source/host-tools/qemu-test/paired-measurement.sh`,
  `paired-equivalence.sh`, `paired-report.py`, `paired-interval.py`
- Trees: `09a35c4` (pre-ownership + corrected harness) and the same with the
  ADR-0082 ownership repairs cherry-picked (`831c2e0`, from `6fc0bf5`)

## 1. Two defects, both now removed

| defect | symptom | repair |
|---|---|---|
| separately linked numerator and denominator | an inert layout change moved the ratio 40–100% | one artifact, two runtime-selected modes, identical image digest enforced |
| numerator and denominator described different logical workloads | ratio sat near 0.35 for arithmetic reasons | `FULL_EXACT` is now the two-validator workload; `UNAVOIDABLE_CRYPTO` is its crypto subset |

The denominator's accepted model is **unchanged** — two parser-crypto passes,
two whole-capsule mirror digests, one boot-text digest, `101203397` bytes over
`2007` invocations. It was not narrowed to make the number approach one.

## 2. The sanity property holds

`FULL_EXACT` now contains everything `UNAVOIDABLE_CRYPTO` represents plus the
structural validation and lookup, so the semantic centre must be at or above 1.

```
TCG p95 ratio     pooled mean 1.0076   min 0.9549   max 1.0746   stdev 4.0%
TCG median ratio  pooled mean 0.9977   min 0.9826   max 1.0109   stdev 1.2%
native            pooled mean 0.9988   min 0.9900   max 1.0146   stdev 1.0%
```

The previous form sat at 0.35. It now sits at 1.00.

## 3. The trees remain indistinguishable

| tree | TCG p95 ratios | mean |
|---|---|---|
| pre-ownership | 1.0177, 0.9823, 1.0116 | 1.0039 |
| Stage 4C | 1.0045, 0.9549, 1.0746 | 1.0113 |

Between-tree mean difference **0.0075**, against a pooled stdev of 0.0401. The
Stage 4C layout perturbation that moved the old cross-artifact metric by 1.9×
does not move this one beyond its noise.

Native likewise: base 0.9900/0.9900/0.9958, Stage 4C 1.0054/0.9968/1.0146.

## 4. Absolute figures and accounting

| tree | series | image sha256 | full p95 | crypto p95 | full median | crypto median |
|---|---|---|---|---|---|---|
| base | 1 | `2fea89a9b6a2f29e…` | 3642.1 | 3578.9 | 3555.6 | 3517.4 |
| base | 2 | `2fea89a9b6a2f29e…` | 3632.4 | 3697.8 | 3530.5 | 3589.9 |
| base | 3 | `2fea89a9b6a2f29e…` | 3654.4 | 3612.7 | 3508.5 | 3515.9 |
| s4c | 1 | `243f5cb79a9e5f34…` | 3635.8 | 3619.5 | 3509.4 | 3571.6 |
| s4c | 2 | `243f5cb79a9e5f34…` | 3692.8 | 3867.1 | 3595.7 | 3565.7 |
| s4c | 3 | `243f5cb79a9e5f34…` | 3530.3 | 3285.2 | 3235.4 | 3226.0 |

Milliseconds. Both members of every series share the image digest shown; the
reporter refuses otherwise. Crypto accounting is `101203397` bytes / `2007`
hashes in every run.

## 5. Production equivalence

```
fresh production parses        2          files validated   1000
fresh whole-capsule digests    2          canonical path    /system/boot/init.tos
lookup taken from              second     boot-text digest  c5488fcd6918f1df…
capsule digest                 6dabadc666f46a75…             crypto  101203397 / 2007
common boundary in both modes  TOS.TEST.PAIRED.START
pass 1 scoped out of pass 2    yes
no duplicated algorithm        10 cfg sites, production calls only
```

`sha256`, `parse`, `boot_file` and the parser crypto replay are the production
implementations; the measurement code supplies only the order.

**The gate caught the same mistake twice during development**: building
`--features test-paired-measurement` without `CARGO_TARGET_DIR` overwrites the
production nucleus, and every other check passes when it does.

## 6. Two findings the threshold must account for

**The structural overhead is below the measurement's resolution.** The pooled
p95 ratio is 1.0076 with a 4.0% standard deviation, and in two of six series the
numerator came out *below* the denominator. Stage 1 validation over this fixture
is overwhelmingly cryptographic: hashing 16 MiB across 1000 files dominates
parsing and lookup so completely that the structural remainder is around or
under one percent. The metric can therefore **bound** structural overhead but
cannot resolve it, and a threshold should be read as a ceiling rather than as a
figure tracking a real trend.

**The p95-of-ratio is three times noisier than the median-of-ratio** — 4.0%
against 1.2% — because it divides two independently drawn tail estimates, so
both tails' noise enters the quotient. The accepted discipline names nearest-rank
p95 and this evidence retains it, but the measured difference is offered because
the ruling admits a change where the experiment demonstrates a specific defect.

## 7. Threshold proposal

**Proposed, for review, not adopted.** The conformance figure stays the p95
ratio, keeping the accepted discipline:

```
same_artifact_full_exact_p95 / same_artifact_unavoidable_crypto_p95

    <= 1.15   regression requires explanation
    <= 1.30   blocking
```

**Interpretation** — and this is the interpretation ADR-0026 always claimed and
its construction could not deliver: the complete Stage 1 logical validation
costs no more than 30% above the unavoidable cryptographic subset **of that same
workload**, measured in one artifact, over one interval, from one boundary.
Because the measured centre is 1.00, the two lines are exactly the existing
policy — a 15% structural regression requires explanation, 30% blocks — with no
translation of units needed.

| | headroom over worst observed (1.0746) | σ over pooled mean |
|---|---|---|
| 1.15 | 7.0% | 3.55 |
| 1.30 | 21.0% | 7.29 |

- **leaves measured headroom**: the blocking line sits 21% above the worst of
  six series rather than on it;
- **accounts for repeatability**: 7.29 σ at the pooled 4.0% stdev;
- **detects a structural regression**: the centre is 1.00, so the lines are
  directly the 15%/30% policy; a 30% increase in non-cryptographic validation
  work breaches;
- **does not encode linker or TCG page placement**: the displacement that moved
  the old metric by 40–100% moves this one by 0.0075 between trees, because both
  halves share the layout;
- **preserves the existing policy** rather than replacing it.

**`1.30` is not carried over from ADR-0026 and is not the same decision.** That
number bounded a quotient of two artifacts over two incomparable intervals,
where it meant nothing checkable. It is proposed again here only because the
repaired metric's centre is 1.00 and 30% is the policy the corpus already
states; it is derived from this distribution, not inherited.

**A caveat this proposal must carry.** Because structural overhead is under the
noise floor (§6), neither line is sensitive to small real regressions. They
bound catastrophe, not drift. If the project wants a metric that resolves
structural cost, the fixture would have to shift work away from hashing — a
larger change than this ADR should make.

## 8. What is not claimed

- no gate is switched; the old ADR-0026 gate is preserved and still active;
- `main` is untouched at `1c3bb49`;
- the Stage 4C ownership repairs are unmerged at `6fc0bf5` and unmodified;
- this ratio is **not** production boot latency. It measures the Stage 1 logical
  validation workload against its own cryptographic subset. Ordinary production
  boot wall-clock and segment decomposition remain separate observational
  evidence;
- KVM is not obtainable on this host — `/dev/kvm` exists, the nucleus fails
  identically on both trees with `TOS.RUN.UNSTARTABLE reason=no-address-space`.
