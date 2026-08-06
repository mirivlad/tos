<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Observability and debugging

## Source-aware operation

Every diagnostic event should be traceable to the exact source that produced it. A running component exposes:

- system commit;
- source path;
- source content ID;
- frontend content ID;
- runtime engine;
- capability grant digest;
- process generation.

## Structured logs

Logs contain stable event identifiers and structured fields. Human text is additional presentation, not the only machine-readable content.

Example:

```text
event=driver.virtio.block.queue_timeout
commit=8f1c...
source=/system/drivers/virtio/block.tos
source_id=blob:3a7d...
device=pci:00:04.0
queue=1
elapsed_ms=5000
```

## Crash reports

A crash report includes:

- exception or panic code;
- source span;
- stack trace mapped to source;
- process and supervisor identity;
- granted capabilities;
- recent IPC events under privacy policy;
- system commit and overlay status;
- relevant device identity;
- restart decision.

## Live inspection

The system shell should support commands conceptually equivalent to:

```text
system process show <id>
system source locate <id>
system diff --running
system capabilities <id>
system trace <service>
system driver inspect <device>
system commit health <commit>
```

## Debugging text modules

The reference interpreter supports:

- breakpoints by source path and line;
- step into/over/out;
- typed local-variable inspection;
- capability and handle inspection;
- IPC message tracing;
- deterministic replay where inputs are captured;
- source revision comparison during hot replacement.

## Boot diagnostics

Boot emits machine-readable stage codes over serial and stores a bounded boot journal when storage becomes available.

A failed candidate boot records the last completed stage without modifying the candidate commit.

## Performance observability

The runtime attributes CPU time, allocations, IPC wait, cache hits, and JIT activity to source modules and source spans where possible.

## Audit trail

Security-sensitive events use an append-only audit service with external sealing or remote forwarding options. Audit data is mutable state, not committed system source.
