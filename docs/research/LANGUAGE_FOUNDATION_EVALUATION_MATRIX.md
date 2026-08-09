<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Language foundation evaluation matrix

**Status:** non-normative research template required by ADR-0015. Its blocking
requirements implement the Tier 2 language and execution requirements in
`docs/05_TOS_CORE_LANGUAGE.md` and `docs/06_EXECUTION_AND_IR.md`; it does not
independently amend them.

## Decision to be made

Choose the foundation that will implement the TOS bootstrap profile and support the long-term TOS Core role without making a hidden binary/runtime ecosystem the true operating-system contract.

Candidate classes:

- **A — Bespoke TOS Core:** grammar, type system, IR lowering and reference runtime designed for TOS.
- **B — TOS surface over an existing formal core:** TOS source remains canonical while a rigorously specified lower core provides execution semantics.
- **C — Adapted existing language:** an existing language is restricted or extended to satisfy TOS contracts.
- **D — Existing language unchanged:** accepted only if it satisfies all blocking requirements without semantic fiction.

## Blocking requirements

A candidate is rejected if it cannot demonstrate:

1. canonical human-readable source remains authoritative;
2. deterministic parse and lowering from declared inputs;
3. bounded bootstrap implementation and resource accounting;
4. explicit capability imports that cannot be forged by ordinary code;
5. typed memory/region model suitable for services and drivers;
6. source maps through every derived stage;
7. independent verification before execution;
8. no ambient host filesystem/network/time access during lowering;
9. no undocumented C/host ABI becoming the real system ABI;
10. compatible licence and acceptable patent/dependency profile;
11. recovery implementation small enough to audit and fuzz;
12. multiple execution backends cannot disagree silently on semantics;
13. one process can use multiple runnable execution contexts for genuine
    simultaneous multicore work, rather than only separate processes or IPC;
14. safe shared-memory concurrency has defined data-race, synchronization,
    atomic and memory-order semantics rather than undefined behavior or
    undocumented host-runtime behavior;
15. parallel workers, tasks, stacks, shared regions and synchronization
    resources can be bounded and accounted for.

A candidate also fails the multicore requirement if its async runtime only
multiplexes tasks on one execution context, if it requires separate OS
processes and IPC for ordinary CPU parallelism, or if it has no viable path
from the selected semantics to real simultaneous multicore execution.

## Comparative criteria

For each candidate record evidence, not adjectives:

- normative specification size and maturity;
- trusted implementation size and transitive dependencies;
- parser/type-checker/verifier complexity;
- memory safety and unsafe boundary;
- asynchronous, structured-concurrency and structured-parallelism semantics;
- multicore execution model and task-to-thread/core mapping;
- cost of parallel task creation and scalability with worker count;
- safe shared-memory model, synchronization, atomic and memory-order semantics;
- scheduler independence and future affinity/NUMA/topology compatibility;
- deterministic behavior;
- interrupt/IPC/DMA expression;
- resource metering/preemption;
- diagnostics and source maps;
- boot-profile reducibility;
- frontend extensibility;
- performance profile;
- self-hosting path;
- tool support value versus architectural cost;
- licensing and contribution compatibility;
- implementation and maintenance effort.

## Required prototype exercises

Each serious candidate must implement or model the same exercises:

1. parse a malformed module corpus with stable diagnostics;
2. declare and enforce a PCI/MMIO/IRQ/DMA capability set;
3. lower a small block-driver state machine into typed IR;
4. reject an undeclared privileged operation;
5. enforce a bounded loop/fuel policy in bootstrap mode;
6. produce source maps through an optimized execution path;
7. invalidate a cache after one source/dependency change;
8. run the same semantic conformance vectors in two engines or interpreter modes;
9. build in a documented recovery-sized configuration;
10. report trusted-base and dependency inventory;
11. run a deterministic CPU-bound partitioned workload with one worker,
    two workers and a reasonable N-worker configuration, recording the same
    logical result and actual simultaneous host-core execution when the
    candidate runtime supports it;
12. demonstrate safe handling or rejection of unsynchronized mutable sharing;
13. exercise atomics/synchronization, structured join and cancellation;
14. demonstrate bounded worker/task resource behavior;
15. where a reference/interpreter mode exists, run the same concurrency
    semantics in that mode and record any intentional serialized execution.

For every multicore exercise, record hardware, operating system,
compiler/runtime version, worker count, exact commands, raw measurements and
observed result. No candidate receives credit for a speedup claim without
evidence of actual simultaneous execution where its runtime claims to support
it.

## Decision output

The Stage 1.5 report must contain:

- candidates evaluated;
- evidence repository/commits;
- blocking failures;
- measured results;
- trusted-base comparison;
- language and IR boundary;
- selected option;
- rejected alternatives;
- migration consequences;
- accepted selection ADR.

This matrix does not presuppose that a bespoke language wins. It prevents convenience from masquerading as architecture.

## Completed 2026-08-09 evaluation

| Candidate | Blocking result | Evidence |
|---|---|---|
| A — bespoke TOS Core | PASS, proposed selection | `stage15/finalists/bespoke-tos-core.md`; common corpus and 1/2/4-worker records |
| B — TOS surface over WebAssembly Threads formal core | FAIL | Wasm Threads requires host-created threads; a TOS surface would have to recreate task, capability, resource and source semantics. See `stage15/screening.md`, W1/W2. |
| C — adapted restricted Rust | PASS, runner-up | `stage15/finalists/adapted-rust.md`; actual E0451/E0499 negatives and common worker records |
| D — unchanged Rust, Pony, Go | FAIL | Ambient/unsafe/resource boundary; actor-only parallelism; or unsafe-race/capability failures respectively. See `stage15/screening.md`. |

Both passing finalists demonstrate deterministic serial and parallel result,
observed multicore overlap, static/data-race negative handling,
atomics/synchronization, structured join/cancellation and bounded
tasks/workers. The proposed winner is chosen for semantic/TCB/recovery fit, not
speedup.
