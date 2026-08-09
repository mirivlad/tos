<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0026: Stage 1 validation-performance metric

- Status: Proposed
- Date: 2026-08-09
- Change level: **Level 2** — revises the Stage 1 performance-conformance
  metric only if accepted; it does not change capsule v1, BootInfo v1, the
  validation algorithm, or either trust boundary
- Project Architect approval: pending

## Context

The current Tier 2 Stage 1 reference-platform rule in
`docs/35_PERFORMANCE_CONTRACTS.md` requires a 1,000-file / exactly-16-MiB
capsule to validate and locate `/system/boot/init.tos` in no more than 250 ms
p95 on the declared QEMU CI profile.  ADR-0025 accepted an exact ordinary
q35/qemu64/one-vCPU/256-MiB/TCG measurement profile for that rule and recorded
that its first P1 result failed it.  ADR-0025 did not authorize a metric
change.

The Project Architect directed a further, exact-work investigation after the
native implementation also failed the initial estimate.  The result is that
the 250 ms value is an **empirically falsified initial reference estimate**:
it was a deliberately loose first-stage guard against accidental quadratic
validation, but was not based on a measurement of the required cryptographic
work.  It must not be silently deleted or relabelled as passing.

The investigation uses the same deterministic detached capsule throughout:

- 1,000 canonical files and exactly 16,777,216 payload bytes;
- capsule SHA-256
  `d0a61d16997492190f258159f599ae80ca26472856316b7035ceaf98c416da55`;
- workload manifest SHA-256
  `91711071612f350595cbc05b898e1f00550308999b69b5bfba508d4758c38855`;
- detached-source-set identity
  `8415f94824d06f8f68798d7ddf54a37a08a6b1fcae6699e83c3774533f8783cc`.

For each timed sample, the full path is the ordinary production logical work:

1. loader plain whole-capsule SHA-256 for the BootInfo mirror;
2. fresh loader parser validation (whole capsule, every file and detached
   identity);
3. fresh nucleus plain whole-capsule SHA-256 and mirror comparison;
4. fresh nucleus parser validation of the same bytes;
5. canonical `/system/boot/init.tos` lookup and the normal nucleus boot-text
   digest.

No parser output or digest is transferred between logical validators.  The
native runner invokes the production parser/hash implementation directly.  The
QEMU baseline uses the same loader/capsule/ESP/OVMF/q35 profile and an isolated
test-only nucleus artifact; its normal production nucleus hash is checked
unchanged before and after the feature build.

The unavoidable-crypto measurement executes exactly the digest operations in
the list above from fresh `tos-hash` state, comparing every resulting parser
digest to the encoded capsule value.  Its setup parse supplies only a borrowed
structural view and none of that parse's digest results enters either timed
logical validator.  Thus it is not a cached validation result or a benchmark
copy of the cryptographic implementation.

### Exact crypto accounting

For this fixture, both the native and QEMU baseline report exactly
**101,203,198 SHA-256 input bytes** and **2,007 SHA-256 invocations** per
boot.  They consist of:

- 2,000 per-file content digests (1,000 for each independent parser);
- two detached source-identity digests;
- two parser whole-capsule digests;
- two existing loader/nucleus whole-capsule BootInfo-mirror digests; and
- one post-lookup canonical boot-text digest.

The byte count includes every input to those hashes: four capsule traversals,
two full payload traversals, the two ADR-0018 domain/path/digest streams and
the boot-text bytes.  It is therefore the lower bound imposed by the current
accepted semantics, not an optional extra workload.

### P1 measurements at `73d7b423d4e534e405a6abbe7c842e1902cbf099`

Each series used three warm-ups and 21 measured samples; p95 is nearest-rank
20 and p99 is nearest-rank 21.  Raw JSONL, reports, fixture, sidecar and QEMU
serial/event logs are retained under ignored `source/target/` evidence paths
and are reproducible with the commands in the Evidence section.

The evidence host was an Intel Xeon E5-2680 v4 at 2.40 GHz running
`Linux-6.5.0-1mx-ahs-amd64-x86_64-with-glibc2.41`, built with
`rustc 1.97.1 (8bab26f4f 2026-07-14)`.  QEMU evidence used QEMU 10.0.11,
OVMF code SHA-256
`624e06de18b4fa535e90db7160d00d3d07d206422b89999bf1e27d920264e4e0`,
OVMF vars SHA-256
`79091dd4ab5e91d7febac74b02dc7f7ec8891a40150cad37c8836105d833cce0`,
and the declared q35/qemu64/one-vCPU/256-MiB guest.  TCG was selected by
omitting `-enable-kvm`; KVM was an explicitly requested research run.

| Profile | Full exact work (median / p95 / p99 ms) | Unavoidable crypto (median / p95 / p99 ms) | Full / crypto p95 | Crypto share of full p95 |
|---|---:|---:|---:|---:|
| Native release research | 624.801 / 658.231 / 664.737 | 619.957 / 622.467 / 623.287 | 1.057 | 94.567% |
| q35/qemu64/TCG functional profile | 2681.217 / 2766.213 / 2772.794 | 2333.338 / 2395.122 / 2398.142 | 1.155 | 86.585% |
| q35/qemu64/KVM research only | 780.988 / 826.389 / 839.925 | 696.552 / 701.056 / 721.422 | 1.179 | 84.834% |

The KVM row is comparison evidence only: it is neither a replacement CI
profile nor a conformance result.  The ordinary qemu64/TCG p95 is 4.202 times
the native p95 for this corrected exact workload.  Its absolute latency is
materially affected by CPU emulation, while the native result independently
shows that the original 250 ms estimate is not met even without TCG.

The remaining p95 time after unavoidable crypto is 35.763 ms native,
371.091 ms TCG and 125.333 ms KVM.  Crypto consequently accounts for at least
84.834% of full p95 in all three independently measured profiles.  This is
evidence that the measured non-crypto validation architecture is bounded and
small relative to the semantics-required work; it is not evidence that any
validation may be removed.

The fresh TCG full-path serial decomposition at p95 is: loader validation
1497.409 ms; loader post-validation 49.233 ms; handoff transition 0.200 ms;
nucleus validation 1243.351 ms; canonical lookup 0.512 ms; and
post-validation-to-halt 23.147 ms.  These are host-monotonic serial-arrival
intervals, not guest instrumentation or a new Boot ABI event.

## Proposed decision

If accepted, replace the initial absolute 250 ms Stage 1 reference-platform
budget with a paired functional and relative-conformance contract:

1. **Hard architectural budgets remain unchanged.**  Parsing remains bounded
   multi-pass; there is no attacker-dependent recursion or premature
   attacker-proportional allocation; canonical lookup remains bounded;
   ADR-0021 limits and traversal constraints remain in force; and loader and
   nucleus each perform their independent validation.  This ADR does not
   authorize unsafe SHA, handwritten assembly, mandatory SHA extensions, an
   external crypto dependency, a capsule-format change, a fused trust
   boundary, or fewer than two validations.
2. **q35/qemu64/TCG remains mandatory functional conformance.**  It runs the
   exact ordinary production boot path with the existing event ordering,
   structured failures, fixture and serial evidence.  It records full-path
   median/p95/p99 and segment decomposition, but its wall-clock result is no
   longer asserted as representative physical-CPU latency.
3. **Native exact work remains mandatory archived evidence.**  A declared
   native release/reference environment runs the same two fresh validations
   and canonical lookup.  It records the full and unavoidable-crypto absolute
   samples, environment and build identities; it has no invented absolute
   latency threshold in this decision.
4. **The primary Stage 1 validation metric is a p95 ratio.**  On the declared
   mandatory q35/qemu64/TCG profile, `full_exact_p95 /
   unavoidable_crypto_p95` MUST be no more than **1.30**.  Both series must
   have the exact fixture/source/provenance/accounting identity, three warmups
   and 21 fresh measurements.  The baseline may not reuse a digest or parser
   result from either logical validator.
5. **The 1.30 threshold has a semantic interpretation.**  It caps measured
   non-cryptographic validation overhead at 30% of the mandatory digest cost.
   It is not fitted to one passing sample: the independent P1 p95 ratios span
   1.057, 1.155 and 1.179.  The cap is 10.3 percentage points above the largest
   research observation, while still requiring at least 76.923% of the
   measured p95 to be attributable to unavoidable crypto.  The existing 15%
   explanation / 30% block regression policy also applies to retained ratio
   and absolute series baselines.
6. **Evidence remains P2 for closure.**  CI retains raw samples, reports,
   serial/event logs, fixture, checked provenance sidecar, source/build/QEMU/
   firmware/host identities and a segment decomposition.  A local P1 result
   does not close F-18.

This proposal evaluates the cost of the validation architecture after
subtracting neither a check nor a byte that accepted semantics make
unavoidable.  It therefore detects pathological structural overhead without
pretending that one emulator's scalar-SHA wall clock is a physical CPU budget.

## Exact proposed Tier 2 amendment

`docs/35_PERFORMANCE_CONTRACTS.md` remains unchanged until this ADR is
accepted.  If accepted, replace only its current Stage 1
“Reference-platform budget” paragraph with the following text; no other
Stage 1 hard budget or the document-wide regression policy changes.

```diff
 Reference-platform budget:

- a capsule fixture containing 1,000 files and 16 MiB total payload validates and locates `/system/boot/init.tos` in no more than 250 ms p95 in release mode under the declared QEMU CI profile.
+ - the mandatory q35/qemu64/one-vCPU/256-MiB/TCG functional profile runs the
+   exact ordinary production boot path for a capsule fixture containing 1,000
+   files and exactly 16 MiB total payload. It retains raw 3-warmup/21-sample
+   median/p95/p99 wall-clock data, serial/event logs and segment decomposition;
+   its wall-clock latency is a retained regression metric, not a physical-CPU
+   absolute-latency assertion;
+ - a declared native release/reference profile records the same exact two fresh
+   validations and canonical `/system/boot/init.tos` lookup, including raw
+   3-warmup/21-sample median/p95/p99 data and environment/build identities;
+ - each profile also measures the unavoidable SHA-256 baseline with the same
+   fixture/source/provenance identity: two parser whole-capsule traversals, two
+   loader/nucleus BootInfo-mirror whole-capsule traversals, two cumulative
+   per-file traversals, two detached-identity traversals where applicable and
+   the post-lookup boot-text digest. No result may be cached or shared between
+   logical validators; and
+ - on the mandatory qemu64/TCG profile,
+   full-exact-validation-p95 / unavoidable-crypto-p95 is no more than 1.30.
+   This relative gate constrains validation-architecture overhead without
+   weakening the required validations or hard architectural budgets.
```

The former 250 ms sentence is retained in ADR-0025 and this ADR as historical
evidence of the falsified initial estimate; it is not erased from history.

## Architecture impact statement

- **Invariants and canonical representation:** I-01, I-02, I-09, I-10 and
  I-18 remain unchanged.  Canonical text, capsule bytes, source identity and
  provenance sidecars are unchanged.
- **Trusted base and dependencies:** no production code, unsafe block,
  assembly, CPU feature or external dependency is introduced.  The baseline
  feature remains test-only and uses production `tos-hash`/capsule logic.
- **Source-to-runtime, recovery and rollback:** the loader/nucleus boundary,
  independent validations, recovery model and rollback remain exactly as
  before.
- **Threat model:** hostile bytes remain fail-closed at both validation
  boundaries.  No error precedence, resource bound, parser property or
  canonical lookup rule is relaxed.
- **Performance contract and compatibility:** this is a Level-2 proposed
  revision of the Stage 1 metric only.  q35/qemu64/TCG remains the mandatory
  functional compatibility profile; KVM is research-only.  The 250 ms rule is
  explicitly falsified rather than silently weakened.
- **Licence and patent:** the proposal imports no code or dependency and has
  no licence or patent effect.
- **Evidence:** production SHA known-answer/streaming tests, capsule/vector
  negatives, precedence and fuzz tests, normal QEMU exit 33, exception exits
  73, fixture/provenance checks, and P2 full/crypto raw series would enforce
  the accepted decision.

## Consequences and review boundary

Until accepted, ADR-0025 and the existing `docs/35` 250 ms requirement remain
authoritative.  F-18 remains a BLOCKER.  This proposal does not mark its local
ratios as passing Stage 1 and does not authorize F-21 or Stage 1.5 work.

If later evidence materially fails the relative bound, TOS must profile and
explain the residual overhead.  It may not make the result pass by changing
the QEMU CPU/acceleration profile, deleting validation work or importing an
unreviewed accelerated implementation.

## Evidence reproduction

From a clean checkout of the source commit being measured:

```sh
bash source/host-tools/qemu-test/stage1-native-performance.sh --out source/target/stage1-native-crypto
bash source/host-tools/qemu-test/stage1-performance.sh --out source/target/stage1-tcg-crypto
bash source/host-tools/qemu-test/crypto-baseline.sh --out source/target/stage1-tcg-crypto-baseline
# Optional research only; never substitutes for the preceding TCG commands.
bash source/host-tools/qemu-test/stage1-performance.sh --accel kvm --out source/target/stage1-kvm-crypto
bash source/host-tools/qemu-test/crypto-baseline.sh --accel kvm --out source/target/stage1-kvm-crypto-baseline
```

The report helper checks the 3+21 shape, matching source/workload/provenance
identity and exact byte/hash accounting before it emits each ratio.  P2 CI
would retain the named reports and their raw JSONL rather than copying them
into a mutable document.
