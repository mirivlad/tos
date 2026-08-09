<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1.5 language-foundation research evidence

**Status:** active Tier 4 research. This directory records the evidence needed
by accepted ADR-0015. It does not define a normative grammar, runtime, bytecode
format or Stage 2 implementation.

## Baseline and boundary

- Stage 1 formally closed at
  `9687d8acdef104f02536b7f7881ce4b77a1144d3`.
- The approved multicore requirements were published at
  `add6358b9372a5d45b329eedc84ec4bab7cdcabd`.
- Research begins from
  `345fa8c10a3da0715a3c24eb37327ff3277bedc7`.
- No artifact here is a production TOS Core parser, runtime, permanent IR,
  bytecode, standard library or Stage 2 code.

## Evidence layout

- `methodology.md` fixes the common corpus and measurement rules.
- `references.md` records primary external sources and the claims for which
  they are used.
- `screening.md` records broad candidate-class eliminations.
- `prototypes/` contains separately marked non-production experiments.
- `finalists/` records complete evidence for each candidate that survives
  screening.
- `measurements/` retains raw measurement records and their derivations.

The final decision remains a separate **Proposed** Level 3 ADR. Nothing in this
directory accepts it or starts Stage 2.
