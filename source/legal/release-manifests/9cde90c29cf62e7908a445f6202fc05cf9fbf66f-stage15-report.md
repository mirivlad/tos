<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Immutable Stage 1.5 language-foundation evidence record

## Identity

- Stage: Stage 1.5 — Language-foundation identity.
- Source evidence commit: `9cde90c29cf62e7908a445f6202fc05cf9fbf66f`.
- Accepted decision: ADR-0027, bespoke TOS Core.
- Architecture profile: canonical normalized UTF-8 `.tos`; disposable typed IR;
  independent verifier; bounded same-semantics bootstrap; SMP-capable full path.

## Accepted evidence

- completed candidate matrix, screening and primary-source bibliography;
- 13-case common corpus, negative capability/mutable-share evidence, atomics,
  structured join/cancel and bounded worker/task evidence;
- retained 3-warmup/21-sample one/two/four-worker raw data;
- measured-now versus architectural/Stage-2-only TCB, dependency and recovery
  analysis; and
- no hidden rustc/LLVM/libc/C ABI/host-runtime language contract.

## Gate result and limitation

`./scripts/preflight.sh --full` passed on the evidence package before closure.
Stage 1.5 selects the foundation only. Stage 2 production parser, verifier,
interpreter and complete normative language specification have **not** begun.
