<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Proposed TOS Core V1 conformance expectations

## Source-level cases

| ID | Input | Profile | Expected result / primary code | Purpose |
|---|---|---|---|---|
| C001 | `accept/evaluation-order.tos` | Bootstrap | returns `12i32` | left-to-right call/evaluation order |
| C002 | `accept/bootstrap-parallel.tos` | Bootstrap | returns `Ok(190i32)` with serial child execution | structured join and serialized equivalence |
| C003 | `accept/release-acquire.tos` | Full | verifier-visible Release/Acquire operations | atomic publication contract |
| C004 | `../../examples/first.tos` | Bootstrap | returns `Ok(42i32)` | source header, resource envelope, Result |
| C005 | `../../examples/modules.tos` plus `../../examples/math.tos` | Bootstrap | returns `42i32` | deterministic declared import |
| C006 | `accept/type-forms.tos` | Full | accepts tuple, `slice<T>`, `TaskResult<T>`, and every fixed-arity predeclared synchronization/atomic type | grammar/type constructor parity |
| C007 | `accept/control-heads.tos` | Bootstrap | accepts parenthesized `if`/`while`/`match` heads plus empty and trailing-comma record initialization | deterministic control/record boundary |
| C008 | `accept/task-cancellation.tos` | Bootstrap | accepts `cancel` followed by consuming `join`; outcome is `Completed(i32)` or `Cancelled` under scheduling/cancellation rules | cancellation request is distinct from task consumption |
| C009 | `accept/control-values.tos` | Bootstrap | accepts `if`/`match` in `let`, tail, and semicolon-free statement position | one value model for control expressions |
| C010 | `accept/call-and-constructor.tos` | Bootstrap | accepts zero/argument function calls, `Ok`, `Err`, and a user tuple variant through one Call form | deterministic call/constructor syntax |
| C011 | `accept/checked-conversion.tos` | Bootstrap | `to_u8(i32)` has type `Result<u8, ConversionError>` | explicit checked narrowing source form |
| C012 | `accept/copy-aggregates.tos` | Bootstrap | tuple, array, record, and enum values with Copy components remain usable after assignment | automatic structural aggregate Copy |
| R001 | `reject/use-after-move.tos` | Bootstrap | `E1301_USE_AFTER_MOVE` | affine ownership negative |
| R002 | `reject/borrow-escape.tos` | Bootstrap | `E1302_CONFLICTING_BORROW` | a mutable borrow cannot coexist with later borrow/use |
| R003 | `reject/forged-capability.tos` | Bootstrap | `E1502_FORGED_CAPABILITY` | scalar value cannot become authority |
| R004 | `reject/shared-mutable.tos` | Full | `E1304_INVALID_TASK_CAPTURE` | safe shared mutable data-race negative |
| R005 | `reject/unmetered-loop.tos` | Bootstrap | `E1701_UNMETERED_LOOP` | bounded Bootstrap work |
| R006 | `reject/full-profile-async.tos` | Bootstrap | `E1702_PROFILE_NOT_SUPPORTED` | no silent profile downgrade |
| R007 | `reject/invalid-atomic-order.tos` | Full | `E1410_INVALID_ATOMIC_ORDER` | atomics have typed order legality |
| R008 | `reject/forged-ir.md` | any | `V2013_CAPABILITY` | independent verifier ignores frontend claim |
| R009 | `reject/if-identifier-control-head.tos` | Bootstrap | `E1105_CONTROL_HEAD_PARENS_REQUIRED` | `if` head has an explicit syntactic boundary |
| R010 | `reject/while-identifier-control-head.tos` | Bootstrap | `E1105_CONTROL_HEAD_PARENS_REQUIRED` | `while` head has an explicit syntactic boundary |
| R011 | `reject/match-identifier-control-head.tos` | Bootstrap | `E1105_CONTROL_HEAD_PARENS_REQUIRED` | `match` head has an explicit syntactic boundary |
| R012 | `reject/record-field-separator.tos` | Bootstrap | `E1106_RECORD_FIELD_SEPARATOR_REQUIRED` | record fields require commas |
| R013 | `reject/unjoined-task.tos` | Bootstrap | `E1401_UNJOINED_TASK` | cancellation does not replace consuming join and every child is consumed |
| R014 | `reject/duplicate-record-field.tos` | Bootstrap | `E1205_DUPLICATE_RECORD_FIELD` | duplicate record field is a static named-field error |
| R015 | `reject/nil-absence.tos` | Bootstrap | `E1202_UNKNOWN_VALUE_NAME` | `nil` is an ordinary unbound identifier, not a second absence model |
| R016 | `reject/unchecked-conversion.tos` | Bootstrap | `E1212_INVALID_AS_CONVERSION` | narrowing/sign-changing conversion must use `to_*` |
| R017 | `reject/noncopy-aggregate.tos` | Bootstrap | `E1301_USE_AFTER_MOVE` | an aggregate containing `bytes` remains affine |

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
