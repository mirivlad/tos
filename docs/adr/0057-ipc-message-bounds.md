<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# ADR-0057: The three numbers `IPC_V1` says it declares

- Status: **Accepted (option A)** (Project Architect-approved)
- Date: 2026-08-19
- Decision level: 2 — it states the values of bounds an accepted contract
  declares itself to fix; no message shape, right or guarantee changes
- Project Architect approval: Vladimir Tomashevskiy, 2026-08-19

## The gap, stated once

`IPC_V1` §3 defines a message as inline bytes plus transferred capabilities plus
transferred regions, and bounds all three:

| Property | Bound, as the contract states it |
|---|---|
| Inline payload | "fixed maximum, declared by this contract version, small enough to copy without allocation" |
| Transferred capabilities | "fixed maximum per message" |
| Transferred regions | "fixed maximum per message" |

It then explains why the inline maximum is a constant of the contract rather
than a per-endpoint parameter — "a per-endpoint size makes the nucleus's fast
path depend on data it must first go and read" — and requires as evidence that
"a message larger than the inline maximum is refused, not truncated" (§7.1).

**The contract never states any of the three numbers.** A version that says it
declares a constant and does not is a version nothing can conform to, and a
refusal test against an unstated bound tests the implementation's opinion.

## What the accepted documents already constrain

- **Small enough to copy without allocation.** The nucleus does not allocate in
  the transfer path, and ADR-0049 §5 forbids unbounded work with interrupts
  masked. The copy is bounded by this number and happens on the fast path.
- **docs/35 Stage 3** budgets the IPC round trip. Whatever is chosen has to be
  copyable twice — sender to nucleus, nucleus to receiver — inside that budget.
- **`CAPABILITY_V1` §4** makes transfer of a linear capability atomic with the
  receiver's acquisition. Each transferred capability costs a table slot in the
  receiver and a validation in the sender, so the per-message maximum bounds the
  work one call can impose on another process's table.
- **`IPC_V1` §5** already says anything larger than the inline maximum travels
  as a region, so the number is not a limit on what can be communicated. It is
  the boundary between the path that copies and the path that maps.

## Options

Each option is the same three-part decision at a different point on one
trade-off: a larger inline payload serves more messages without a region, and
costs more copying on the path that runs with interrupts masked.

### A — 256 bytes inline, 4 capabilities, 2 regions

Sized so that a message and its transfer table fit in a single cache-friendly
copy, and so that the whole of a message is smaller than one page by a wide
margin. Four capabilities is enough for a request that hands over an endpoint, a
reply endpoint and a region handle with one spare; two regions is enough for a
request-and-response pair of buffers.

Costs: a service whose ordinary request is a path, a name or a small record will
sometimes exceed 256 bytes and pay for a region on a call that is morally small.

### B — 512 bytes inline, 8 capabilities, 4 regions

Twice A on every axis. Fewer ordinary messages spill into a region, at twice the
copy on the masked path and twice the receiver-table pressure one call can
create.

Costs: the largest message is 512 bytes copied twice per round trip inside the
docs/35 budget, which has to be measured rather than assumed; and eight
capabilities per message means one call can consume eight of a receiver's table
slots, so the table has to be sized against that.

### C — 4096 bytes inline, 8 capabilities, 4 regions

One page. Attractive because it makes the inline path cover nearly everything
and makes the region path rare.

Costs: it stops being "small enough to copy without allocation" in any
meaningful sense — 4 KiB copied twice with interrupts masked is precisely the
unbounded-in-practice work ADR-0049 §5 exists to prevent — and it makes the
region path so rare that it will be under-tested exactly when it matters.

## Recommendation

**A**, with the numbers stated as a contract constant and the region path
exercised by the first service that needs it.

256 bytes is small enough that the copy on the masked path is not a scheduling
event, and the contract's own words — "small enough to copy without allocation"
— read as a much smaller number than a page. The right way to discover that A is
too small is a measured service that pays for a region on messages that should
not have needed one; the wrong way is to pick B or C now because they might save
that discovery, since a bound that is loose from the start never gets tightened.

If A is accepted, `IPC_V1` §3 states the three values in its table, and §7 gains
the refusal evidence for each of the three rather than only for the inline size:
a message with too many capabilities and one with too many regions are refused
the same way, and neither is silently truncated to the bound.

## Boundary

Phase 4 Task 2 cannot be written without this: the size of the inline buffer is
the size of a nucleus structure, and the refusal test has nothing to compare
against. It is independent of ADR-0055 and ADR-0056 — the numbers are the same
whatever fills the table and whichever register names a handle.
