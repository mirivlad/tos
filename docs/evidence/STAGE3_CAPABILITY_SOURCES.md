<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 — an operation acts on authority it was given at runtime

The Project Architect's ruling on `STAGE3_LAUNCH_PLANS.md` §6, implemented. It
is a consistency repair, not a new capability: TOS Core V1 has always admitted
capability values and capability-derived authority, and `tos-ir/v1` could not
represent an operation whose required capability came from one. ADR-0078 repairs
the representation so the accepted semantics are representable.

## 1. What changed, and what did not

`Op::Capability` carried `import: usize` and `further_imports: Vec<usize>`. It
now carries `capabilities: Vec<CapabilitySource>`, where a source is
`Import(index)` or `Value(operand)`.

Not changed: TOS Core source version (`1.0`), the capability type constructor,
the semantic schema version (`tos-ir/v1`), any ABI operation, any status, any
accepted ceiling, and the meaning of the import path — an import-supplied
position is the explicit `Import` case and resolves through the same table by
the same index.

Explicitly not done, because the ruling re-rejected both: `import` was **not**
reinterpreted as a licensing field, and **no** import-anchored capability was
introduced that the ABI does not act on. There is no `AnyCapability`, no raw or
scalar handle, no sentinel index, and no `tos-ir/v2`.

## 2. General over every position

The ruling required this and it is the part worth checking hardest, because a
repair applied to the operation's own capability alone would have moved the
contradiction one position along.

Operation 19 requires two capabilities: authority over the process a child is
created under, and the `MemoryAuthority` its footprint is charged to. In the
proof the first is an import and the **second** is a scoped budget operation 16
produced:

```text
process_create_funded  [Import(0), Value(_)]
```

The verifier's checks are written per position and run per position; nothing in
them special-cases index zero except the one thing that genuinely belongs to it,
which is the accepted interface ID the instruction carries.

## 3. What the verifier proves, against the artifact

Per position, and never on the frontend's word:

| Check | Refusal |
|---|---|
| `Import` names an index inside the module's own table | `V2013_CAPABILITY` |
| `Value` names an operand whose type is `TypeDef::Capability` | `V2013_CAPABILITY` |
| position zero's interface is the instruction's accepted interface ID | `V2013_CAPABILITY` |
| every position's interface is an effect the enclosing function declares | `V2033_UNSAFE` |
| no source appears twice (ADR-0063, now over sources) | `V2013_CAPABILITY` |

The fourth is what makes the exact nominal interface checkable at a position
with no import declaration to compare against: `docs/42` §2's "enclosing `uses`
effect" requirement, applied per position rather than to the first alone.

The negatives are proved by **damaging the artifact**, not the source — a
verifier that only ever sees what this frontend emits proves nothing about a
frontend somebody else wrote:

- a `Constant` in a capability position — the shape a frontend that had learned
  to write handles would emit — is refused as "not of any capability type";
- a capability of the wrong nominal interface in a capability position is
  refused;
- one source standing in for two is refused.

## 4. The storage encoding

`ENCODING_VERSION` 3 → **4**. The bytes of one instruction changed, so the
version changed: an older reader must refuse a version-4 image rather than read
a source tag as an index.

Version 3 remains readable and decodes as `Import` throughout. That is bounded
and canonical rather than a guess — an import index was the only source version
3 could write — so admitting it adds no way for an old image to mean something
new. `READABLE_ENCODING_VERSIONS` is the whole of the allowance, and anything
else is `UnknownEncodingVersion`, which the proof exercises with version 99.

`SCHEMA_VERSION` stays `1`.

## 5. The eight items, and where each is proved

`source/tests/integration/tests/capability_source.rs` (host) and
`source/host-tools/qemu-test/runtime-authority.sh` (QEMU, against the real
nucleus).

| # | Required | Where |
|---|---|---|
| 1 | operation 19 returns a child `system.process.Control` | type table asserts `TypeDef::Capability("system.process.Control")` inside the `Result` |
| 2 | that capability is the **own** capability of `process_terminate` | the artifact's source at position 0 is `Value`, and the QEMU boot performs it |
| 3 | lower → TOSIMAGE → decode → verifier → engine → **ABI** | the verifier runs over the *decoded* module; the ABI link is the QEMU gate |
| 4 | operation 16 returns a scoped `system.memory.Authority` | type table, and `TOS.RUN.INTERFACE operation=capability_attenuate_scoped status=0` |
| 5 | that authority is what `endow_for_launch` acts through | the instruction's recorded interface is `system.memory.Authority`, the value's own |
| 6 | attenuation and release of runtime authority representable | both performed, both `status=0` |
| 7 | forged scalar, wrong nominal interface | both refused by the verifier |
| 8 | a non-first position runtime-sourced | operation 19's second capability |

## 6. The QEMU boot

Two canonical modules. `/system/boot/init.tos` is the supervisor;
`/system/boot/worker.tos` asks for nothing, so what is under test is the
supervisor's authority and not a second question in the same boot.

The launcher's constant grants exactly two things — `create | terminate` over
the process itself, and the root's remainder to spend — and the gate asserts
that **nothing else was requested**, so every other capability in the run is one
an operation produced:

```text
TOS.RUN.REQUEST binding=process interface=system.process.Control object=3 wanted=3
TOS.RUN.REQUEST binding=memory  interface=system.memory.Authority object=6 wanted=6
TOS.RUN.INTERFACE operation=capability_attenuate_scoped status=0
TOS.RUN.INTERFACE operation=launch_plan_create          status=0
TOS.RUN.INTERFACE operation=endow_for_launch            status=0
TOS.RUN.INTERFACE operation=launch_plan_seal            status=0
TOS.RUN.INTERFACE operation=process_create_funded       status=0
TOS.RUN.INTERFACE operation=capability_attenuate        status=0
TOS.RUN.INTERFACE operation=capability_release          status=0
TOS.RUN.INTERFACE operation=process_terminate           status=0
TOS.RUN.COMPLETED value=i64:1
```

Every status is one ring 0 produced. The nucleus's own lines carry the rest: the
child was charged the grant the module named, it was ended by the process that
created it, and `plans_live=0` at reclamation.

**One thing the boot taught, which the host test could not.** The first version
released the child's wider handle *after* terminating it, and the nucleus
answered `E_NO_CAPABILITY`. That is `CAPABILITY_V1` §3 working — a capability's
lifetime is bounded by its object, so a handle to a process that has ended
resolves to nothing and there is nothing left to release. The vector now
releases while the child is alive, which is also the more honest supervisory
act: narrowing your own authority over something you made, and dropping the
wider name.

## 7. Gates

`scripts/preflight.sh` — **36 of 36**, with `capability_source.rs` added to the
host tests.

One gate is new: `runtime-authority` in the QEMU profile. Nothing was weakened,
and every import-only gate is unchanged — the two that report on capability
operations, `interface_schema` and `interface_reach`, assert the same facts
against `CapabilitySource::Import`, which is what an import now is.

Two existing verifier findings changed wording because the checks became
per-position: "is performed under" → "is performed through", and the effects
finding now names which position failed. Both still fire, with the same codes,
on the same damaged artifacts.

## 8. What this unblocks

Section H's remaining work, which was waiting on exactly this: a supervisor can
now terminate a child, release and refine authority it obtained at runtime, and
endow a child with a **scoped** memory authority rather than a name for its own
whole budget. None of the restart state machine, the dependency states, the
journal or the T1 lifecycle needs anything further from the IR.
