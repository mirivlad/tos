<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0042: Runtime events on the Boot ABI v1 serial log

- Status: **Proposed** (awaiting Project Architect decision)
- Date: 2026-08-12
- Decision level: 2 — settles how a versioned interface contract admits events
  belonging to another interface, and whether a boot has a result code for the
  canonical boot module failing to execute
- Project Architect approval: *(none — this ADR is not accepted)*

## Context

Stage 2 now runs the capsule's canonical boot module on the real boot path. The
reference runtime reports what it did in the `TOS.RUN.*` vocabulary described
in `docs/evidence/STAGE2_RUNTIME_EVENTS.md`, on the same serial
transport that carries the Boot ABI v1 events, between `TOS.IDENTITY` and
`TOS.HALT`.

Boot ABI v1 was fixed when nothing ran, and it does not settle what a
conforming consumer should now do. A third question follows from where the new
vocabulary is allowed to live.

### Gap 1: interleaving is neither permitted nor forbidden

`interfaces/boot/BOOT_ABI_V1.md` section 7 says a successful boot "emits the
following identifiers in this exact order", that "a consumer may rely on the
listed identifier order", and — in the failure section — that:

> A consumer MUST treat an unknown non-success `TOS.*` failure or result as
> failure, not as a successful boot.

Its explicit extension rule covers two things and only two: optional `key=value`
fields appended after a mandatory prefix, and new `TOS.BOOT.FAILI` reason
tokens. It says nothing about new *identifiers*.

**Minimal counterexample.** This is the log of a boot that succeeded in every
respect, abbreviated to the identifiers:

```text
TOS.BOOT.ENTRY
TOS.CAPSULE.OK
TOS.BOOT.HANDOFF
TOS.NUCLEUS.ENTRY
TOS.CAPSULE.OK
TOS.BOOTTEXT.PATH
TOS.BOOTTEXT.DIGEST
TOS.IDENTITY
TOS.RUN.BEGIN            <- not in the v1 vocabulary
TOS.RUN.COMPLETED        <- not in the v1 vocabulary
TOS.HALT
```

Two conforming readings disagree about it:

- **A.** Every listed identifier appears in the listed relative order, and
  `TOS.HALT ok=0x10` with result `0x10` is the terminal result. The boot
  succeeded.
- **B.** `TOS.RUN.COMPLETED` is an unknown `TOS.*` identifier that is not one of
  the listed success identifiers. Under the sentence quoted above it is an
  unknown non-success result and the consumer must treat the boot as failed.

Reading B rejects a boot that did strictly more than v1 describes and did it
correctly. Reading A is what every consumer in this repository implements today
(`host-tools/qemu-test/run.sh` checks the listed identifiers as an ordered
subsequence), but no accepted document says A is the right reading, and a v1
consumer written outside this repository could reasonably implement B.

### Gap 2: no result code for a boot module that did not execute

Boot ABI v1 section 2 fixes the result codes: `RESULT_HALT_OK` (`0x10`),
`RESULT_PANIC`, `RESULT_CAPSULE_INVALID`, `RESULT_ABI_INVALID`,
`RESULT_MEMORY_INVALID`, `RESULT_EXCEPTION`. None of them means "the capsule was
valid, the nucleus was healthy, and the canonical boot module did not execute".

That state is now reachable in three distinct ways that Boot ABI v1 cannot tell
apart: the frontend refused the module, the independent verifier refused the IR
the frontend emitted, or the engine trapped. Each is reported in full in the
`TOS.RUN.*` events, and each currently halts with `RESULT_CAPSULE_INVALID`.

That code is defensible — its meaning is "nucleus rejected capsule data after
handoff", and the canonical boot text is capsule data the nucleus rejected — but
it collapses "this program is wrong" into the same signal as "these bytes are
not a capsule", and a consumer that only reads the exit code cannot separate
them.

### Gap 3: the runtime vocabulary has no accepted home

`TOS.RUN.*` is an interface: a harness, an operator's tooling and a future
Stage 3 supervisor all read it. An accepted versioned interface contract lives
under `source/interfaces/` and carries Tier 2 authority
(`docs/38_NORMATIVE_DOCUMENT_HIERARCHY.md`), which requires an Architect
decision this vocabulary has not had. It therefore sits in
`docs/evidence/STAGE2_RUNTIME_EVENTS.md`, describing what the implementation
emits and binding nobody.

## Options

### For gap 1

1. **State that the success order is a required subsequence.** Add to Boot ABI
   v1 section 7 that identifiers defined by other interface contracts MAY be
   interleaved, that the listed identifiers must still appear in the listed
   relative order, and that the "unknown non-success" sentence applies to
   identifiers that report a *result of the boot*, not to identifiers belonging
   to another declared contract.
2. **Reserve a namespace.** Say that `TOS.RUN.*` — or, more generally, any
   identifier under a prefix Boot ABI v1 does not define — is outside this
   contract and carries no boot verdict.
3. **Forbid interleaving.** Require every event on the boot transport to belong
   to Boot ABI v1, which would mean either adding the runtime vocabulary to Boot
   ABI v1 or giving the runtime a separate transport.
4. **Bump to Boot ABI v2** and settle it there.

### For gap 2

1. **Add `RESULT_RUNTIME_REFUSED`** as a new stable result code, meaning the
   canonical boot module did not execute; the `TOS.RUN.*` events say which stage
   refused and why.
2. **Keep `RESULT_CAPSULE_INVALID`** and state in Boot ABI v1 that it also
   covers canonical boot content that did not execute.
3. **Halt with `RESULT_HALT_OK`** and leave the outcome to the event log.
   Rejected below.

### For gap 3

1. **Promote it** to `source/interfaces/runtime/RUNTIME_OBSERVABILITY_V1.md` as
   an accepted Tier 2 interface contract, listed in
   `docs/SPECIFICATION_SOURCES.txt`.
2. **Leave it descriptive** and let each consumer pin the producer's version
   instead of a contract.

## Recommendation

**Gap 1: option 1, with option 2's wording folded in.** It is the smallest
change that makes the existing implementations correct, it does not touch a
single mandatory field, identifier or result code, and it preserves the property
consumers actually rely on — that the v1 identifiers appear, in order, and that
the terminal result is unambiguous. Option 3 would push the runtime onto a
second transport for no gain, and reserving a prefix (option 2) is worth saying
explicitly so the next contract does not need this ADR again.

**Gap 2: option 1.** A new result code costs a constant and separates two states
that a consumer has a real reason to separate: a capsule that is not a capsule
is an integrity or supply problem, while a boot module that does not verify is a
source problem, and an operator's next action differs. Option 3 is rejected
outright: a boot whose canonical module did not execute is not a successful
boot, and reporting `0x10` for it would make the exit code lie about the state
of the system — exactly the failure mode "fail closed" exists to prevent.

**Gap 3: option 1.** The events are already load-bearing — a preflight gate
fails the build on their content — and an interface that gates a build while
binding nobody is a contract in everything but name. Option 2 makes every
consumer depend on an implementation version, which is the coupling
`source/interfaces/` exists to prevent.

## What the implementation does while this is Proposed

Nothing here is treated as decided.

- Boot ABI v1's normative text is **unchanged**. No identifier, field, result
  code or ordering statement has been edited.
- The runtime vocabulary is described in `docs/evidence/STAGE2_RUNTIME_EVENTS.md`,
  which states plainly that it is not a normative contract and records this
  question as open rather than answering it. Nothing was added under
  `source/interfaces/`, where placement would imply an authority it has not
  been granted.
- The nucleus fails the boot **closed** when the canonical boot module does not
  complete, using `RESULT_CAPSULE_INVALID` (gap 2, option 2's behaviour without
  gap 2, option 2's claim). If option 1 is chosen, this becomes a new code and
  the change is confined to one match arm and one constant.
- Consumers in this repository implement reading A, which they already did.

## Consequences

If gap 1 is settled as recommended, no code changes: the implementation already
matches. If it is settled the other way, the runtime needs a separate transport
or Boot ABI v2, and the QEMU harness's event checking changes with it.

If gap 2 is settled as recommended, `RESULT_RUNTIME_REFUSED` is added to
`crates/boot-protocol`, the nucleus's failure arm uses it, and the negative
QEMU suite gains a case for a boot module that does not verify. Until then a
consumer cannot distinguish that state from a malformed capsule by exit code
alone, and must read the event log — which is why the `TOS.RUN.*` events are
required rather than optional.

## Alternatives considered

**Say nothing and keep shipping.** Rejected. The ambiguity is real, an outside
v1 consumer could correctly reject a working boot, and a Stage 2 closure that
rested on a reading no document states would be resting on a habit.

**Fix Boot ABI v1 directly and note it in the gate record.** Rejected. The boot
ABI is a versioned public contract; editing its normative text without an
Architect decision is exactly the change this project does not make, however
small and however obviously right the edit looks.
