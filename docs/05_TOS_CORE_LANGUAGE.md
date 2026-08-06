<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core language role and requirements

## Current status

“TOS Core” names the required native textual language role of the system. The final language foundation is **not yet selected**.

The syntax in this document is illustrative. No parser, grammar or runtime becomes normative until Stage 1.5 completes and a selection ADR is accepted under ADR-0015.

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

- **Bootstrap profile** — bounded allocation, no ambient dynamic module loading, minimal standard library, used during early boot and recovery.
- **Full profile** — structured asynchronous tasks, richer collections, dynamic service discovery, frontend APIs and user applications.

The bootstrap profile is a strict supported subset, not a temporary fake language.

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

The selection ADR must define or adopt:

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

## Concurrency

The required model is structured concurrency rather than unmanaged detached threads by default:

- tasks belong to a scope;
- cancellation propagates to children;
- resource handles close deterministically;
- drivers bind interrupts to explicit event streams;
- blocking operations are visible in the type/effect or API contract.

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
