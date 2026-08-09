<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Proposed TOS Core V1 conformance expectations

## Source-level cases

| ID | Input | Profile | Expected result / primary code | Purpose |
|---|---|---|---|---|
| C001 | `accept/evaluation-order.tos` | Bootstrap | returns `12i32` | left-to-right call/evaluation order |
| C002 | `accept/bootstrap-parallel.tos` | Bootstrap | returns `190i32` with serial child execution | structured join and serialized equivalence |
| C003 | `accept/release-acquire.tos` | Full | verifier-visible Release/Acquire operations | atomic publication contract |
| C004 | `../../examples/first.tos` | Bootstrap | returns `Ok(42i32)` | source header, resource envelope, Result |
| C005 | `../../examples/modules.tos` plus `../../examples/math.tos` | Bootstrap | returns `42i32` | deterministic declared import |
| R001 | `reject/use-after-move.tos` | Bootstrap | `E1301_USE_AFTER_MOVE` | affine ownership negative |
| R002 | `reject/borrow-escape.tos` | Bootstrap | `E1302_CONFLICTING_BORROW` | a mutable borrow cannot coexist with later borrow/use |
| R003 | `reject/forged-capability.tos` | Bootstrap | `E1502_FORGED_CAPABILITY` | scalar value cannot become authority |
| R004 | `reject/shared-mutable.tos` | Full | `E1304_INVALID_TASK_CAPTURE` | safe shared mutable data-race negative |
| R005 | `reject/unmetered-loop.tos` | Bootstrap | `E1701_UNMETERED_LOOP` | bounded Bootstrap work |
| R006 | `reject/full-profile-async.tos` | Bootstrap | `E1702_PROFILE_NOT_SUPPORTED` | no silent profile downgrade |
| R007 | `reject/invalid-atomic-order.tos` | Full | `E1410_INVALID_ATOMIC_ORDER` | atomics have typed order legality |
| R008 | `reject/forged-ir.md` | any | `V2013_CAPABILITY` | independent verifier ignores frontend claim |

## Byte/source transport cases

The following lexical cases are retained as generator instructions, because an
invalid UTF-8/NUL source cannot itself carry a valid textual SPDX declaration.
The implementation test harness materializes exactly these bytes in ignored
test output, then asserts the listed primary error and byte offset:

| ID | Bytes / transformation | Expected |
|---|---|---|
| L001 | prepend `EF BB BF` to `accept/evaluation-order.tos` | `E1002_BOM_FORBIDDEN` at byte 0 |
| L002 | replace the first ASCII source byte with `FF` | `E1001_INVALID_UTF8` at byte 0 |
| L003 | insert bare `0D` between two tokens | `E1003_BARE_CR` at that byte |
| L004 | replace one space outside a literal with `09` | `E1010_TAB_OUTSIDE_LITERAL` at that byte |
| L005 | replace precomposed NFC `é` in an eligible comment with `65 CC 81` | `E1004_NOT_NFC` at the decomposed sequence |

## Required generated/IR cases

The production conformance runner must also generate one malformed IR object
per `V20xx` family in docs/43: oversized count, unknown schema, mismatched
source identity, noncanonical table order, type/CFG/import/capability/region/
resource/profile/task/synchronization/atomic/unsafe/source-map violation.
It must execute C002 under 1/2/N Full workers and serialized Bootstrap,
requiring identical logical output, actual Full overlap, bounded tasks/workers,
and the negative mutable-sharing case. Raw environment/measurement artifacts
follow docs/35 and docs/44.
