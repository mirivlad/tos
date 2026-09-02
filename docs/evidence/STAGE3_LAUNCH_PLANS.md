<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 3 — launch plans, and an operation that returns authority

What this records: the mechanism half of the Project Architect's Stage-3 closure
instruction of 2026-09-03. Section A's narrow proof was run first and is green;
sections B and C are implemented at the ABI and integrated into operations 19
and 20. One architecture finding came out of the work and is stated in §6 — it
is why this document exists rather than the whole closure candidate.

## 1. Section A's narrow proof, exactly as specified

The instruction was to run one path before building anything:

```text
extern operation -> Result<nominal capability, i64>
    -> lower -> TOSIMAGE -> independent verify -> engine execution
```

It is green, and it needed no new `tos-ir/v1` variant and no new TOS Core type
constructor. The Architect's diagnosis was right to within one line: the
limitation was in the frontend, not the IR.

Three frontend changes, and nothing else:

| Where | What it did before | What it does now |
|---|---|---|
| `boundary::type_text` | admitted only `TypeSyntax::Name`, so a schema could declare no result that was not a bare name | renders a constructed type canonically, so `Result<system.process.LaunchPlanBuilder, i64>` is a type a schema can declare |
| `lower::resolve_type` | resolved an interface path written as a type to a nominal record that merely shared its name | resolves it to `TypeDef::Capability(path)` — the constructor the IR has carried since it was written |
| `types::resolves` | admitted a dotted type name only if this module imported it as a capability | admits any interface an accepted schema declares, because naming a type is not holding one |

`source/tests/integration/tests/capability_result.rs` checks the five stages
separately rather than end to end, because "it ran" is the weakest of the things
that had to be true:

- the type table records `TypeDef::Capability("system.process.LaunchPlanBuilder")`
  inside a `TypeDef::Result`, and no operand anywhere holds a capability;
- `tos_image::encode`/`parse` round-trips it unchanged;
- the **decoded** module verifies, so it is the image that carried it and not
  the in-memory value;
- the engine delivers `Ok(capability)` into the arm that matched;
- and a refusal delivers the status into the other arm.

**No closed-IR prohibition was found on this path.** Section A's STOP condition
did not fire.

## 2. Operations 21, 22 and 23

ADR-0077 is the decision; this is what was built.

- **21 `launch_plan_create`** — process authority with `create`, returns an
  affine builder in `rdx`. Creation authority is required for a thing that
  creates nothing, so that a process which may not create children cannot
  occupy the bounded plan table with policy for them.
- **22 `launch_plan_endow`** — **the capability being delegated** in `rdi`, the
  builder in `rsi`, the rights asked for in `r10`, and the binding's length in
  `rdx` with its bytes at `LAUNCH_ENDOW_BINDING`. One selector for every kind of
  authority there is. Rights are intersected with what the caller holds, so
  widening is unexpressible rather than refused.
- **23 `launch_plan_seal`** — consuming, in the shape `region_freeze`
  established: same capability slot, generation advanced, same object, no
  reference taken or dropped, no free slot required.

The plan table is `MAX_PLANS = MAX_PROCESSES` entries in
`source/nucleus/src/plan.rs`. A plan holds a reference of its own on everything
its entries name, released exactly once when the plan is destroyed — by an
explicit `capability_release`, or by `capability::clear` walking a dead
process's table. Affinity makes "exactly once" structural rather than counted.

Regions, replies and plans are refused as entries, each for the reason ADR-0077
§3 gives.

## 3. Operations 19 and 20 rebuilt on the sealed plan

Both take the sealed plan as an input capability and derive the child's
endowment from it inside the one transaction that already charges the child's
footprint.

| | before | now |
|---|---|---|
| 19 `rdx` | module path length | the sealed plan |
| 19 `r10` | endowment count | module path length |
| 20 `r10` | endowment count | the sealed plan |

A builder is refused where a sealed plan belongs. **The plan is not consumed by
a successful creation**, which is the point: a restart is the same policy applied
to a new instance.

**The `CREATE_ENDOWMENT` area of the argument region is removed, not
deprecated.** `tos_launch::CreateEndowment`, `CREATE_ENDOWMENT` and
`ENDOWMENT_ENTRY_BYTES` are gone and `CREATE_SELF_BINDING` is re-anchored at
offset 0. A table that is still read is a second way to endow a child, and the
Architect's instruction was explicit that there must not be two.

This is a register reshuffle on two operations, and it is free exactly once:
neither 19 nor 20 was ever declared by `SYSTEM_INTERFACE_V1`, and neither has a
caller outside this repository's runtime image. `SYSTEM_ABI_V1` §7 records that,
and records that it is the last such change either operation gets.

## 4. The typed bridge

`SYSTEM_INTERFACE_V1` §5 now says an operation returns **the value it produced**,
with `Result<T, i64>` as the refusal model. §4.1 admits a third kind of value
parameter — a nominal capability type — and one requirement form with no
declared right.

The host bridge in `runtime-image` is a table, one row per operation, saying
which register each declared argument goes in and where the result comes from:

```rust
Performed {
    interface: "system.process.Control",
    name: "process_create_funded",
    operation: PROCESS_CREATE_FUNDED,
    capabilities: &[Reg::Rdi, Reg::Rsi],
    values: &[
        Slot::Held(Reg::Rdx),
        Slot::Text { length: Reg::R10, at: CREATE_MODULE, maximum: 256 },
        Slot::Number(Reg::R9),
        Slot::Number(Reg::R8),
    ],
    result: Produced::Authority,
}
```

Above that table a module names an operation and receives a value; below it
there are registers, an argument region and fixed offsets. A `string` past its
declared maximum is refused **before the call is made**, with the status §4.1
assigns — the bound is the schema's, never the host's.

`endow_for_launch` is declared once per interface whose capabilities may be a
startup endowment (`system.ipc.Endpoint`, `system.memory.Authority`,
`system.process.Control`), with the same name and the same ABI selector. Which
declaration a call reaches is decided by the interface of its first argument,
which §4.1 already makes the operation's own capability. A module that endows two
kinds of authority declares two `extern` items of that name; the frontend keys
operations by name **and** interface so the second does not silently take the
first's schema entry.

**No `AnyCapability`, no raw handle, no erased capability value** exists anywhere
in TOS Core, and the artifact records the exact nominal interface of every value.

## 5. What a textual supervisor can now write

`capability_result.rs::a_plan_is_written_through_three_interfaces_and_creates_a_process`
lowers, verifies and executes this:

```tos
match (launch_plan_create(process)) {
    Ok(builder) => {
        let inboxed: i64 = endow_for_launch(inbox, builder, 2u64, "inbox");
        let funded: i64 = endow_for_launch(memory, builder, 128u64, "memory");
        ...
        match (launch_plan_seal(process, builder)) {
            Ok(plan) => {
                match (process_create_funded(
                    process, memory, plan, "system/boot/init.tos", 56623104u64, 0u64
                )) { ... }
            }
        }
    }
}
```

The scripted host records what actually crossed:

```text
system.process.Control    launch_plan_create      [0x10]
system.ipc.Endpoint       endow_for_launch        [0x11, 0x101]
system.memory.Authority   endow_for_launch        [0x12, 0x101]
system.process.Control    launch_plan_seal        [0x10, 0x101]
system.process.Control    process_create_funded   [0x10, 0x12, 0x102]
```

The builder the first call produced (`0x101`) is the second argument of both
endowments and of the seal; the sealed plan (`0x102`) is what the creation is
given. Nothing else could have carried it — there is no handle in the source and
no capability the module could have named. The endowment is reached **through the
endpoint** and **through the authority**, so the interface recorded on each
instruction is the one being delegated. The three names the policy chose crossed
as values, in order.

That is a heterogeneous typed endowment, decided in text, with each capability
keeping its exact nominal type.

## 6. The finding: an operation cannot act on a capability it was given at runtime

This is why this document is not the closure candidate.

`tos-ir/v1`'s `Op::Capability` names the operation's **own** capability as an
import index — `import: usize` into `module.capability_imports` — and the
verifier requires that index to be in range and to match the interface the
instruction declares. Capabilities beyond the first travel as operands, which is
how a plan reaches 22, 23, 19 and 20 with no IR change at all.

But an operation whose *first* capability is one an operation produced has
nowhere to put it. That blocks, from text:

- `process_terminate` on a **child** — the child's `system.process.Control`
  comes from operation 19 as a value, and `process_terminate`'s own interface is
  that same interface;
- `capability_release` and `capability_attenuate` on anything obtained at
  runtime;
- `endow_for_launch` of a runtime-obtained capability — which is what
  `capability_attenuate_scoped` produces, so a supervisor cannot endow a child
  with a **scoped** memory authority, only with a name for its own.

Every operation built in this round is shaped so the question does not arise: a
plan is written and sealed *through* the authority that made it, and a child is
created *through* the parent's own authority. That is not a workaround — it is
the honest subset, and the schema says so in §4.1 rather than leaving it to be
discovered.

The three ways past it were considered and none is a frontend change:

1. make `import` optional, or add a per-position source discriminator — a change
   to `tos-ir/v1`'s instruction schema and to the image format;
2. let `import` mean "the import that *licensed* the reach" while an operand
   names the object acted on — no shape change, but it silently changes the
   meaning of an existing IR field, so an older verifier would misread which
   capability a newer artifact acts on. Rejected;
3. give each affected operation a second, import-anchored capability the ABI does
   not use — a schema that declares an authority the system never checks.
   Rejected.

Section A's instruction was "STOP with the precise prohibition. Do not work
around it." This is the precise prohibition, in the same class and one level
deeper than the one that instruction anticipated: not in the type schema, which
admits everything needed, but in the instruction schema.

## 6a. The plan as a holder, proved at the nucleus

ADR-0077 §3 says a plan takes a reference of its own on everything its entries
name, so a creator may release its own handle and the plan goes on holding it.
That is a claim about the accounting, and it needed an observation rather than an
assertion.

"Did the sixty megabytes come back?" is not a question a parent with room to
spare can answer — it says yes either way. So `supervisor.sh` drains the parent
authority in reservations of exactly that size until it refuses, and then asks
the same question twice:

```text
released=0        the creator releases its own handle for the funding node
stale=-1          and that handle stops resolving
held_by_plan=-6   the drained parent cannot reserve: the node has not returned
                  ← capability_release(plan)
returned=0        and now it can
```

The same request, refused and then granted, with only the plan's release in
between. Nothing else changed, and no other name for that node existed.

## 7. Gates

`scripts/preflight.sh` — **36 of 36**.
`scripts/preflight.sh --profile qemu` — **40 of 42**. The two are
`stage3-observer-conformance` and `stage3-ipc-conformance`, both exiting 2 with
*"the selected QEMU has no ADR-0066 observer-build.json"* before booting
anything. Unchanged from before this round and independent of it.

Three QEMU gates say something new:

- **`supervisor.sh`** — one sealed plan served every creation in the funding
  scenario and none consumed it; the plan-as-holder pair of §6a;
- **`bundle-launch.sh`** — a plan that has not been sealed is refused as an
  input to operation 20 (`unsealed=-1`), while the builder itself was made
  successfully (`unsealed_plan=0`); one sealed plan stood behind both targets
  and the hostile one, unchanged by any of them;
- and every boot's reclamation line now carries `plans_live=`, which is `0` at
  the end of every one of them.

**One register error was caught by a gate rather than by review.** The first
version of the bridge table put `endpoint_send`'s payload length in `rdx`; the
ABI puts it in `rsi`. Every send would have been a zero-length send that
succeeded. `supervisor-text.sh` failed on it immediately, because that gate's
whole point is that the policy's two figures produce two *different* answers —
one accepted and one past the inline bound — and a system that ignored the
length gave the same answer twice.

Nothing was weakened. `check-interface-schema.sh` gained the three new object
kinds, a right spelled `none`, and a rewritten extraction for the host table's
new form; it still holds the document and the frontend table together operation
by operation, kind by kind, requirement by requirement, and ABI number by ABI
number — 10 operations, 6 kinds, 10 assignments, 12 requirements.
`check-abi-operations.sh` — 23 operation numbers, contract and implementation
agree.

The physical account is untouched by this round: no new frame is charged, a plan
is nucleus metadata in a statically reserved table, and the reserve contract
(1452 total, 1451 runtime baseline, 1 permanent backing root) is unchanged.

**One assertion did change, and it is a decision rather than a weakening.**
`memory-account.sh` asserted `root >= MAX_PROCESSES × the ordinary charge`. The
Project Architect's section E fixed what `MAX_PROCESSES` means — the bounded
number of process *slots*, not a reservation — because keeping that sum as an
invariant made every page of code growth in the runtime image an architecture
STOP. The per-process charge has moved from `14 356` frames to `14 357` and the
root from `57 424` to `57 415`, so four ordinary processes no longer fit, and
nothing is wrong: a free slot beside an authority that cannot pay is `E_LIMIT`,
which is correct behaviour.

`RUNTIME_GRANT` stays at `54 MiB`, `MAX_PROCESSES` stays at 4, the reference
machine is not enlarged. What the gate does now is **report** how many ordinary
processes one root funds — 3 — and assert the topologies the system is built to
run:

```text
one ordinary process costs 14357 frames; the root funds 3
supervisor + target: 28714 frames, margin 28701
build worker topology: 28701 frames (112.11 MiB) left for bundle backing
```

That last number is the one section D asked for, measured rather than assumed:
`112.11 MiB` of headroom for bundle backing with a resident supervisor and a
transient worker both at the ordinary grant, against a largest measured
Capsule-v1 bundle of roughly `50.5 MiB`. ADR-0069 §7 is amended to carry the
decision and to say plainly that its four-process measurement is evidence about
one build.

## 8. What is not in this round

Named plainly, because the Architect's section I listed them and they are not
here:

- the textual canonical supervisor and `/system/policy/` supervision source;
- the service supervision state machine, the restart window, the dependency
  `BLOCKED` state and the terminal `FAILED` latch;
- the central operator journal;
- the T1 build-worker lifecycle measured against Capsule v1;
- `CreatedProcess` and `ChildEnding` as schema-declared nominal record types —
  operation 19 currently returns `Result<system.process.Control, i64>`, so the
  child's capability crosses and its instance id does not;
- the `MAX_PROCESSES` and ADR-0069 §7 wording amendments of section E.

None of them is blocked by §6 except the parts that need to act on a
runtime-obtained capability: terminating a child, and endowing a scoped memory
authority. The rest is work, not a question.
