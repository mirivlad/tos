<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# R008 — forged capability IR

The production IR test generator must create an otherwise valid `tos-ir/v1`
module for `conformance.forged_capability`, then replace a capability operand
with a `u64` constant while preserving a frontend-supplied "type checked" flag.
The independent verifier MUST reject this object as `V2013_CAPABILITY`; it must
not trust the flag or a host compiler result. This Markdown description is used
until the implementation supplies a bounded binary/text IR fixture generator.
