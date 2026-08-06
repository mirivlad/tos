<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Architecture conformance tests

TOS requires tests for its identity, not only for functions.

## Canonical-source tests

- delete every bytecode/native cache and confirm regeneration from source;
- mutate source and reject the old cache;
- runtime introspection reports path, hash, commit, frontend and engine;
- no textual module requires an undeclared host compiler.

## Repository identity tests

- boot two commits and prove `/system` differs exactly as declared;
- fail a candidate and return to last-known-good without mutating it;
- active commit, boot record and process source identities agree;
- mutable state cannot dirty `/system`;
- claimed Git profile passes its exact suite.

## Owner-control tests

- authorize an owner key or explicit experimental branch through recovery;
- boot modified source without vendor secrets on an official developer profile;
- warnings do not become irreversible lockout;
- restore a previous commit from recovery media.

## Trusted-base tests

- dependency inventory contains only approved nucleus components;
- no network stack, rich Git service, general language runtime or ordinary driver enters nucleus accidentally;
- parsers reject malformed input without panic and within quota;
- capabilities are required for every privileged primitive.

## Stage identity tests

Every stage report maps to `docs/37_STAGE_IDENTITY_GATES.md` and has automated evidence where possible.

Mandatory examples:

- Stage 1 official capsule commit exists and source hash matches;
- Stage 1.5 selection ADR exists before normative parser implementation;
- Stage 2 runtime/cache trace terminates at canonical source;
- Stage 3 textual service exercises real capability enforcement;
- Stage 4 device I/O disappears if the textual driver is removed;
- Stage 5 `/system` bytes are resolved from the active commit tree;
- Stage 6 self-edit workflow does not call undocumented host tools.

## Threat-model tests

- each new trust boundary has negative tests;
- malformed input work remains within quotas;
- protected refs reject unauthorized mutation;
- DMA tests detect out-of-grant attempts where the platform supports enforcement;
- rollback and state migration failures preserve recovery;
- evidence level is recorded honestly.

## Performance tests

- benchmark environment and source commit recorded;
- hard budgets on copies, allocations and crossings are asserted where instrumentable;
- reference-platform thresholds and percentiles checked;
- reference/oracle implementation remains outside accepted runtime architecture;
- regressions follow `docs/35_PERFORMANCE_CONTRACTS.md`.

## Licence and provenance tests

- SPDX scan has no unknown source file;
- third-party inventory resolves every dependency;
- generated artifact maps to canonical inputs;
- DCO sign-off exists for merged commits;
- prohibited licence combinations detected.

## Documentation integrity tests

- source manifest paths exist and are unique;
- every accepted ADR is listed;
- generated consolidated specification is byte-identical to generator output;
- generated header identifies version and source-manifest digest;
- no direct edit to generated file is accepted without a source change.

## Compatibility honesty tests

A claim is tied to a profile and test set. Parsing syntax alone does not count as running a language. G1 object reading does not count as G4 remotes or full Git. A passing profile publishes exactly what was tested.
