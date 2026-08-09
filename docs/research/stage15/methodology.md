<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1.5 evaluation methodology

## Authority and scope

The pass/fail criteria are Tier 2 `docs/05_TOS_CORE_LANGUAGE.md` and
`docs/06_EXECUTION_AND_IR.md`, applied through the Tier 4 matrix required by
accepted ADR-0015. This methodology is an evidence protocol; it does not add a
language contract.

Every prototype is a deliberately incomplete research model. It must state its
omissions instead of presenting a convenient demonstration as a future TOS
runtime.

## Common semantic corpus

`prototypes/common/cases.json` contains the shared 13-case corpus. It covers
stable malformed diagnostics; declared and undeclared MMIO authority; typed
block-driver state transitions; bootstrap fuel; source-map retention through a
derived computation; cache invalidation; serial/parallel equivalence;
unsynchronized mutable sharing; release/acquire publication; structured
cancellation; task quotas; and partitioned deterministic reduction.

The text snippets in the corpus are semantic labels, not accepted `.tos`
syntax. A finalist translates them into its experimental representation and
records the mapping. This avoids rewarding a candidate merely because a
particular illustrative syntax resembles its existing grammar.

## Multicore workload

The workload partitions the integer interval `[0, 1_048_576)` into 64 fixed,
non-overlapping ranges. Each partition computes a deterministic wrapping
64-bit sum of squares; a fixed-order reduction combines the 64 partials. The
logical result is independent of worker count. A worker trace records start and
finish monotonic timestamps per partition; `overlap=true` requires two
different workers to have overlapping CPU-bound intervals, not merely an
increased thread count or a shorter elapsed time.

The exercise uses one worker, two workers and a reasonable N-worker count. It
does not claim linear speedup. The same semantics must execute in a serialized
reference mode if the candidate presents one.

## Measurement protocol

Every retained measurement uses the exact command recorded in its JSON file,
three warmups and 21 measured samples unless the record says it is a smoke
test. The record contains elapsed nanoseconds, logical result digest, worker
count, overlap observation, host CPU count and platform. Final reports add
`rustc -Vv`, operating-system release, CPU topology, RAM, candidate version and
the exact source commit.

Measurements compare only the same workload on the same host. Parser,
lowering, verification, cold-start, execution and memory figures are reported
as separate dimensions; no synthetic aggregate score chooses the winner.

## Equivalence and negative evidence

A parallel result is conforming only when it has the expected logical digest
and the corresponding serialized/reference mode has the same digest. A
candidate that cannot express a corpus case must report a blocking failure, not
silently omit the case. Invalid mutable sharing must be rejected statically or
have defined safe behavior; a data-race detector after execution alone is not
sufficient.

Cache identity evidence changes one declared dependency digest and proves a new
derived identity. Source-map evidence retains the canonical source span through
the prototype's derived/optimized path. The capability case must prove that an
ordinary integer/string cannot mint a hardware authority token.
