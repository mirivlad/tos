<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
# The `TOS.RUN.*` events the Stage 2 reference runtime emits

Status: **not a normative contract.** This records what the implementation
emits today. It is deliberately *not* placed under `source/interfaces/`: a
versioned interface contract there carries Tier 2 authority under
`docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md` only once the Project Architect has
accepted it, and this vocabulary has not been accepted.
**ADR-0042 (Proposed)** asks for it to be promoted to an accepted Tier 2
interface contract at `source/interfaces/runtime/RUNTIME_OBSERVABILITY_V1.md`.
Until that decision, nothing here binds any consumer, and the descriptions of
stability below state the producer's current discipline rather than a guarantee
anyone may rely on.

Producer: `source/crates/tos-pipeline` (`render::events`) and the component
driving it
Consumers: the serial boot log, host test harnesses,
`source/host-tools/qemu-test/stage2-runtime.sh`

## 1. Role

Boot ABI v1 (`interfaces/boot/BOOT_ABI_V1.md`) is the loader-to-nucleus
handoff: what the machine handed over and whether the capsule is what it
claims. It says nothing about *running* anything, because at the time it was
fixed nothing ran.

This contract covers the other question: what the TOS Core reference runtime
did with the canonical source it was given. It is a separate interface with a
separate vocabulary, because the two answer to different components and change
for different reasons. Boot ABI v1 is unchanged by this document.

## 2. What an event is

One line, terminated by CRLF on a serial transport, beginning with a stable
identifier matching `^TOS\.RUN\.[A-Z0-9_]+`, followed by space-separated
`key=value` fields. A value never contains whitespace: the producer substitutes
`_` for any whitespace or control character in a value that can carry free text
(a detail string, a rendered value), so a field can neither invent a field nor
split an event. Codes, digests, identifiers and numbers are never altered.

Identifiers and the fields listed as required below are stable. An
implementation MAY append further `key=value` fields after the required ones;
it MUST NOT remove one, reorder the required ones, or change what one means.

## 3. Progress

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.BEGIN` | `path=` `bytes=` `entry=` `grant_base=0x<hex>` `grant_length=` `grant_version=` | A run is starting over the named source with the named memory grant. |
| `TOS.RUN.STAGE` | `name=<read\|parse\|check\|resolve\|lower\|verify\|execute>` | The named stage is being **entered**. |

`TOS.RUN.STAGE` is emitted before the stage runs, not after. A stage that never
returns is then named by the last event in the log, which is the only way a
hang or a trap inside a stage identifies itself from outside.

The stages appear in exactly this order, and the order is the reference path:
transport validity, grammar, checking, module-name-to-path agreement, lowering
to `tos-ir/v1`, independent verification, execution. A log in which they appear
in any other order, or in which one is missing before a later one, is not a
conforming run.

## 4. Outcome

Exactly one outcome event ends a run, and it is always the last of the run's
events.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.COMPLETED` | `value=` | The entry function returned. `value` is the rendered result. |
| `TOS.RUN.TRAP` | `code=` `at=` `detail=` | A defined dynamic failure ended the run. `at` is `path:line:column-line:column`, or `<unmapped>` when the IR carried no source map entry. |
| `TOS.RUN.REFUSED` | `stage=` and stage-specific fields | A stage refused to hand its output on. |

`TOS.RUN.REFUSED` carries the stage that refused and its reason:

| `stage=` | Further required fields |
|---|---|
| `read` | `code=` `byte=` |
| `parse`, `check`, `resolve` | `count=`, preceded by `count` × `TOS.RUN.DIAGNOSTIC` |
| `lower` | `construct=` `bytes=<start>..<end>` |
| `verify` | `code=` `at=` `detail=` |
| `execute` | `reason=` |

`TOS.RUN.DIAGNOSTIC` carries one frontend diagnostic in the docs/41 section 7
model: `<CODE> severity= stage= bytes=<start>..<end> at=<line>:<column>`, then
the module identity when the diagnostic carries one, then the diagnostic's
structured fields in their own order. The byte span is the normative locator;
line and column are derived from it.

A refusal by `parse`, `check` or `resolve` is a statement about the *program*.
A refusal by `lower` is a statement about the *lowerer* — the source is valid
and checked and this implementation cannot yet represent one of its constructs.
A refusal by `verify` is a statement about the *IR the frontend emitted*, which
is why it is never merged with the frontend's own diagnostics.

## 5. Verification and accounting

Emitted before the outcome on a completed run.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.VERIFIED` | `module=` `digest=sha256:<64 hex>` `verifier=` | The independent verifier issued a receipt for this exact module. |
| `TOS.RUN.ACCOUNTING` | `fuel=` `depth=` `tasks=` `allocation=` `cleanup=` `workers=`, each `used/limit` | What the run consumed against what the module declared. |

`digest` is the complete `tos-ir/v1` module digest, and the engine ran the
module that digest names — a receipt for another module is refused before any
instruction executes (docs/43 section 5). A consumer that wants to know whether
verification really happened checks this event, not the absence of a refusal.

Every `used` in `TOS.RUN.ACCOUNTING` is bounded by its `limit`; a run that
exceeded one would have been stopped before the effect, so a log showing
otherwise is a defect and not a permitted state.

## 6. Implementation resources

Emitted after the outcome, by the component that supplied the memory.

| Identifier | Required fields | Meaning |
|---|---|---|
| `TOS.RUN.MEMORY` | `granted=` `peak=` `committed=` `blocks=` `free=` | The arena the run needed, against the region it was granted. |
| `TOS.RUN.STACK` | `used=` `capacity=` | Stack the run actually used, measured, against the region it ran on. |
| `TOS.RUN.UNSTARTABLE` | `reason=` | The runtime could not be started at all. No stage ran. |

These are **implementation** figures and are never a statement about the module.
A module's `resource [allocation: ...]` is its own declared budget, enforced by
the engine and reported in `TOS.RUN.ACCOUNTING`; `TOS.RUN.MEMORY` is what the
implementation running it required. Exhausting one is not the other (ADR-0041).

`peak` is the highest address the arena was ever carried to, so it includes
block metadata, grain rounding, remainders too small to split off, and holes
below the frontier. It never falls when memory is freed: a bound must err
upward. `committed` is the live figure in whole blocks.

`TOS.RUN.STACK used` is measured, not estimated: the unused stack is painted
with an address-derived pattern before the run and read back after it.

## 7. Relationship to the boot log

When a runtime is driven from the nucleus, these events appear on the same
serial transport as the Boot ABI v1 events, between `TOS.IDENTITY` and
`TOS.HALT`. Boot ABI v1's own identifiers keep their relative order and their
meanings; nothing in this contract changes, removes or reinterprets any of them.

Boot ABI v1 section 7 does not say whether identifiers belonging to another
vocabulary may be interleaved with its success sequence, and it directs a
consumer to treat "an unknown non-success `TOS.*` failure or result" as a
failed boot. Whether `TOS.RUN.COMPLETED` falls under that sentence is not
settled by any accepted document. **ADR-0042 (Proposed)** states the question,
and this section describes what the implementation does today rather than
claiming the point is decided.

The nucleus fails the boot closed when the canonical boot module does not
complete, using the result code it already uses for capsule content it rejects
after handoff. Boot ABI v1 has no result code meaning "the canonical boot module
did not execute"; ADR-0042 raises that too.
