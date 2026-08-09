<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core language role and requirements

## Current status

“TOS Core” is the accepted bespoke TOS-owned native textual language foundation
under ADR-0027.

The syntax in this document remains illustrative. Stage 2 defines the complete
normative parser, grammar and runtime specification within ADR-0027's accepted
semantic and trust boundary.

This distinction is deliberate: TOS requires language properties, not a proprietary syntax for its own sake.

## Role

The selected foundation must be small enough to bootstrap and audit, yet complete enough to implement services, drivers, language frontends and eventually much of its own runtime.

Its priorities are:

- deterministic parsing;
- explicit types;
- structured errors;
- capability-safe system interaction;
- predictable and enforceable resource use;
- source-level observability;
- incremental loading;
- compatibility with a compact reference interpreter and later optimizers;
- independence from an undocumented host ABI.

Canonical native source files are expected to use UTF-8 and the `.tos` extension unless the selection ADR changes the surface-language decision.

## Language profiles

The selected language has two profiles sharing compatible syntax and semantics:

- **Bootstrap profile** — bounded allocation, no ambient dynamic module loading, minimal standard library, used during early boot and recovery. It MAY run on one worker/core, use deterministic serialized execution and restrict or prohibit parallel spawning.
- **Full profile** — structured asynchronous and parallel tasks, richer collections, dynamic service discovery, frontend APIs and user applications.

The bootstrap profile is a strict supported subset, not a temporary fake
language or a second concurrency semantics. Its restrictions are profile
restrictions on the same selected language foundation.

## Illustrative syntax

```tos
module drivers.virtio.block

import system.bus.pci
import system.capability
import system.driver
import system.memory.dma
import system.ipc

service VirtioBlock(device: capability.PciFunction) -> driver.BlockDevice {
    requires {
        pci.configure(device)
        irq.bind(device)
        dma.allocate(max_bytes: 16 MiB)
        publish("block.device")
    }

    let registers = pci.map_bar(device, 0)?
    let queues = setup_queues(registers)?

    loop {
        select {
            request = receive<BlockRequest>() => handle(request, queues),
            interrupt = await_irq(device) => complete_requests(interrupt, queues),
            stop = shutdown() => break,
        }
    }
}
```

This example expresses intent only. It must not be used as an accidental grammar.

## Blocking semantic requirements

The Stage 1.5 selection ADR MUST establish the semantic/trust boundary for:

- canonical source authority;
- type/effect, ownership/region and concurrency direction;
- verifier/IR/runtime relationship;
- bounded bootstrap and SMP-capable full-profile direction; and
- no safe-language data-race undefined behavior or hidden host-runtime ABI.

Stage 2 MUST define the complete normative specification within that boundary,
including:

- lexical grammar and Unicode normalization;
- complete syntactic grammar;
- static type rules;
- dynamic semantics;
- evaluation order;
- integer overflow behavior;
- memory ownership/borrowing/region behavior;
- error and panic behavior;
- concurrency and cancellation semantics;
- module resolution;
- capability import and transfer semantics;
- FFI/ABI boundary;
- deterministic lowering rules;
- source-map rules;
- resource accounting and preemption;
- unsafe-code boundary;
- versioning and compatibility policy.

## Required type categories

At minimum:

- fixed-width signed and unsigned integers;
- `bool`;
- Unicode `string` and raw `bytes`;
- tuples and records;
- tagged unions/enums;
- arrays and bounded slices;
- `Option<T>`;
- `Result<T, E>`;
- typed handles;
- capability types that cannot be forged from integers;
- duration and size literal types;
- functions and closures in the full profile;
- futures/tasks in the full profile.

## Memory model requirements

Ordinary modules must not receive unrestricted raw pointers.

Required mechanisms include:

- owned values;
- borrowed immutable or mutable regions with enforceable lifetime/alias rules;
- typed shared-memory handles granted by the nucleus;
- explicit DMA regions for drivers;
- unsafe operations confined to reviewed modules with declared invariants.

The bootstrap contract must not require a stop-the-world collector. An implementation may use arenas, reference counting or another internal strategy only if observable semantics and pause/resource limits are specified.

The selection ADR MUST also define the concurrency memory model: ownership,
immutable sharing, mutable sharing, transfer of values and tasks between
execution contexts, synchronization primitives, atomic types and memory
orderings, visibility/happens-before rules, interaction between atomic and
ordinary memory, shared memory regions and the unsafe concurrency boundary.
It MUST NOT rely on a particular Rust, C++ or host-runtime memory model merely
by implication.

Safe TOS Core code MUST NOT have undefined behavior from an unsynchronized
data race. The foundation MUST statically prevent unsafe unsynchronized mutable
sharing, provide defined runtime/type semantics for it, or combine those
methods. Ordinary safe code MUST NOT turn a race into arbitrary memory
corruption or undefined behavior.

The model MUST remain address-space independent. Ordinary safe code MUST NOT
assume a fixed virtual-address width, a fixed page-table layout or a fixed
process address-space size. Machine-sized indices and sizes MAY follow the
declared target ABI when semantically necessary; persistent and public
serialized formats use explicitly defined fixed-width types. Physical addresses
remain privileged system-level concepts rather than ordinary language integers.

## Errors and diagnostics

Recoverable failures use a typed result mechanism. Fatal invariant failure terminates the current process unless supervisor policy escalates it.

Every parser/runtime error includes:

- stable error code;
- module identity;
- source content ID;
- file path;
- byte span and line/column;
- causal chain;
- structured values safe to log.

## Modules

A module declares:

- canonical name;
- language and semantic version/profile;
- exports;
- imports with constraints;
- requested capabilities;
- runtime profile;
- deterministic source identity;
- optional tests and health probes.

Imports resolve against the active system commit and explicit overlays. Resolution cannot depend on ambient working directory, network, time or undeclared host state.

## Concurrency, parallelism and execution contexts

TOS Core distinguishes three related but different mechanisms:

- **asynchronous tasks** await IPC, IRQs, timers, I/O and other events without necessarily occupying a CPU;
- **parallel tasks** perform CPU-bound or independent work that MAY execute simultaneously on different CPU cores; and
- **low-level execution contexts/threads** are runtime or nucleus mechanisms for cases that require direct control.

An async event loop alone does not satisfy the TOS Core requirement.

In the full profile, one process MUST be able to have multiple runnable
execution contexts sharing its address space. Independent language-level work
MUST have a path to simultaneous execution on different CPU cores. Channels,
actors, IPC and queues MAY be important mechanisms, but they are not the only
way for a process to use multiple cores. Ordinary CPU-parallel work MUST NOT
require separate processes, serialization through IPC or manual queue
construction solely to obtain multicore execution.

The preferred safe-code model is structured concurrency and structured
parallelism. Conceptually, a scope may spawn parallel child work and then join
it; this is a semantic illustration, not accepted syntax. Parallel child tasks
belong to their scope, have a defined join and lifetime, define cancellation
behavior and cannot leave resources uncontrolled as orphans. Unscoped or
detached execution, if provided, is an explicit lower-level facility.

Program correctness MUST NOT depend on a CPU number, worker count or scheduler
interleaving. A correct program remains semantically correct on one, two or N
CPUs. The model MAY specify concurrency-related nondeterminism, but it MUST
define permitted outcomes. A correctly synchronized deterministic computation
MUST NOT change its logical result only because the runtime has a different
number of workers.

The language/runtime foundation MUST provide defined typed contracts, whether
as language features or standard/runtime APIs, for mutexes, reader/writer
synchronization where justified, semaphores or events, barriers or latches,
atomics, channels/message passing and task join/cancellation. Their semantics
MUST NOT depend on accidental host-runtime behavior.

Parallel execution does not grant unbounded CPU authority. The process/resource
model MUST be able to account for or limit total CPU time, runnable execution
contexts, parallel workers/tasks, stacks, memory, synchronization resources,
shared regions and cancellation cleanup cost. Spawning in a loop MUST NOT
implicitly create an unbounded number of kernel threads.

A reference or recovery interpreter MAY serialize parallel tasks for auditability
if it preserves the specified language semantics and conformance tests prove
that fact. At least one production-capable execution path MUST nevertheless
support genuine simultaneous multicore execution. All execution modes retain
the same language and memory semantics.

The selected foundation MUST leave an architectural path for later CPU affinity,
NUMA-aware scheduling and memory placement, heterogeneous cores and
topology-aware scheduling. Stage 1.5 does not define their final APIs.

## Metaprogramming

Unrestricted textual macros are excluded from the bootstrap profile. Any future macro system must be hygienic or equivalently attributable, preserve source maps and include generated expansion identity in cache keys.

## Standard-library boundary

Filesystems, networking, UI, Git operations and devices are services through versioned interfaces, not hidden language intrinsics.

## Selection process

ADR-0015 and `docs/research/LANGUAGE_FOUNDATION_EVALUATION_MATRIX.md` govern the comparison.

Candidate classes include:

- bespoke TOS Core;
- TOS source over an existing formal execution core;
- a restricted/extended existing language;
- an unchanged existing language only if every blocking requirement is met honestly.

Lua, Scheme, WebAssembly and other systems are research inputs, not pre-approved foundations. Wasm may be a backend while TOS text remains canonical.

## Licence of language assets

The official runtime and standard implementation are GPL-3.0-or-later. Public grammar schemas, frontend ABI definitions, bindings and conformance libraries may be Apache-2.0 when explicitly marked. The prose language specification is CC-BY-SA-4.0.
