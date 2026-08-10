<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Accepted TOS Core V1 conformance expectations

## Source-level cases

| ID | Input | Profile | Expected result / primary code | Purpose |
|---|---|---|---|---|
| C001 | `accept/evaluation-order.tos` | Bootstrap | returns `12i32` | left-to-right call/evaluation order |
| C002 | `accept/bootstrap-parallel.tos` | Bootstrap | returns `Ok(190i32)` with serial child execution | structured join and serialized equivalence |
| C003 | `accept/release-acquire.tos` | Full | verifier-visible Release/Acquire operations | atomic publication contract |
| C004 | `../../examples/first.tos` | Bootstrap | returns `Ok(42i32)` | source header, resource envelope, Result |
| C005 | `../../examples/modules.tos` plus `../../examples/math.tos` | Bootstrap | returns `42i32` | deterministic declared import |
| C006 | `accept/type-forms.tos` | Full | accepts tuple, `slice<T>`, `TaskResult<T>`, and every fixed-arity predeclared synchronization/atomic type | grammar/type constructor parity |
| C007 | `accept/control-heads.tos` | Bootstrap | accepts parenthesized `if`/`while`/`match` heads plus empty and trailing-comma named record construction | deterministic control/call boundary |
| C008 | `accept/task-cancellation.tos` | Bootstrap | accepts `cancel` followed by consuming `join`; outcome is `Completed(i32)` or `Cancelled` under scheduling/cancellation rules | cancellation request is distinct from task consumption |
| C009 | `accept/explicit-control-return.tos` | Bootstrap | accepts statement `if`/`match` branches with explicit returns and no arm commas | explicit control-flow return model |
| C010 | `accept/call-and-constructor.tos` | Bootstrap | accepts zero/argument function calls, `Ok`, `Err`, and a user tuple variant through one Call form | deterministic call/constructor syntax |
| C011 | `accept/checked-conversion.tos` | Bootstrap | `to_u8(i32)` has type `Result<u8, ConversionError>` | explicit checked narrowing source form |
| C012 | `accept/copy-aggregates.tos` | Bootstrap | tuple and array values remain usable after assignment; nominal record/enum values remain affine | bounded aggregate Copy rule |
| C013 | `accept/async-explicit-return.tos` | Full | async function and spawned task return through explicit `return` | no implicit task tail value |
| C014 | `accept/named-record-constructor.tos` | Bootstrap | `Point(x: ..., y: ...)` supplies each exact field once | named record constructor arguments |
| C015 | `accept/named-enum-variant.tos` | Bootstrap | `Rgb(red: ..., green: ..., blue: ...)` supplies each exact field once | named-field enum construction through Call/Construct |
| C016 | `accept/return-scopes.tos` | Full | nested ordinary block, closure, and spawned task return to their own nearest scope | explicit return-scope boundary |
| C017 | `accept/pattern-local-variants.tos` | Bootstrap | accepts bare `Low`/`High` against `Signal` and against `Power`, plus qualified `Power.High` | expected type disambiguates a shared variant name, and a local variant may be written qualified |
| C018 | `accept/pattern-bindings.tos` | Bootstrap | `Low` binds where the expected type is `i32`; `Sample(amount)` destructures; `_` is a catch-all | a bare name binds when the expected type has no such variant, independently of capitalization |
| C019 | `accept/pattern-qualified-import.tos` | Bootstrap | accepts `upstream.Signal.Low` and `upstream.Signal.High` | an imported variant is reached as a qualified constructor path |
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
| R012 | `reject/record-field-separator.tos` | Bootstrap | `E1106_LIST_SEPARATOR_REQUIRED` | named record constructor fields require commas |
| R013 | `reject/unjoined-task.tos` | Bootstrap | `E1401_UNJOINED_TASK` | cancellation does not replace consuming join and every child is consumed |
| R014 | `reject/duplicate-record-field.tos` | Bootstrap | `E1205_DUPLICATE_RECORD_FIELD` | duplicate record field is a static named-field error |
| R015 | `reject/nil-absence.tos` | Bootstrap | `E1202_UNKNOWN_VALUE_NAME` | `nil` is an ordinary unbound identifier, not a second absence model |
| R016 | `reject/unchecked-conversion.tos` | Bootstrap | `E1212_INVALID_AS_CONVERSION` | narrowing/sign-changing conversion must use `to_*` |
| R017 | `reject/noncopy-aggregate.tos` | Bootstrap | `E1301_USE_AFTER_MOVE` | an aggregate containing `bytes` remains affine |
| R018 | `reject/implicit-tail-return.tos` | Bootstrap | `E1107_UNEXPECTED_TOKEN` at the closing `}` | a bare final expression is not an implicit return |
| R019 | `reject/missing-nonunit-return.tos` | Bootstrap | `E1221_MISSING_RETURN` | every normal non-unit path explicitly returns |
| R020 | `reject/old-resource-braces.tos` | Bootstrap | `E1107_UNEXPECTED_TOKEN` at `{` | resource declarations use `[]` |
| R021 | `reject/old-enum-braces.tos` | Bootstrap | `E1107_UNEXPECTED_TOKEN` at `{` | enum declarations use `[]` |
| R022 | `reject/old-record-braces.tos` | Bootstrap | `E1107_UNEXPECTED_TOKEN` at `{` | record declarations use `[]` |
| R023 | `reject/old-record-construction-braces.tos` | Bootstrap | `E1107_UNEXPECTED_TOKEN` at `{` | record values use named constructor arguments |
| R024 | `reject/comma-match-branches.tos` | Bootstrap | `E1101_EXPECTED_IDENTIFIER` at the `,` after a branch block | executable match branches do not use commas |
| R025 | `reject/duplicate-record-constructor-field.tos` | Bootstrap | `E1205_DUPLICATE_RECORD_FIELD` | named constructor field is exact-once |
| R026 | `reject/missing-record-constructor-field.tos` | Bootstrap | `E1206_MISSING_RECORD_FIELD` | named constructor cannot omit a field |
| R027 | `reject/standalone-block-expression.tos` | Bootstrap | `E1107_UNEXPECTED_TOKEN` at `{` | executable block is not an expression |
| R028 | `reject/old-array-semicolon-type.tos` | Bootstrap | `E1101_EXPECTED_IDENTIFIER` at `[` | fixed array type uses `array<T, N>` |
| R031 | `reject/pattern-unknown-qualified-variant.tos` | Bootstrap | `E1202_UNKNOWN_VALUE_NAME` at `Signal.Middle` | a qualified pattern path names a constructor and never degrades into a binding |
| R032 | `reject/pattern-nonexhaustive-variants.tos` | Bootstrap | `E1220_NONEXHAUSTIVE_MATCH` | bare variant names are patterns, not catch-all bindings, so the missing arm is detected |
| R033 | `reject/type-unknown-local.tos` | Bootstrap | `E1203_UNKNOWN_TYPE_NAME` at `Missing` | a type name resolving to nothing is rejected |
| R034 | `reject/type-unknown-qualified.tos` | Bootstrap | `E1203_UNKNOWN_TYPE_NAME` at `upstream.Missing` | the import resolves, so the missing type is a type-name error rather than an import error |
| R035 | `reject/type-option-arity.tos` | Bootstrap | `E1204_TYPE_ARGUMENT_ARITY` with `constructor=Option`, `expected_arity=1`, `actual_arity=2` | arity is a type property, not a parse decision |
| R036 | `reject/type-result-arity.tos` | Bootstrap | `E1204_TYPE_ARGUMENT_ARITY` with `constructor=Result`, `expected_arity=2`, `actual_arity=1` | `Result<T,E>` takes two type arguments |
| R037 | `reject/type-unknown-before-arity.tos` | Bootstrap | `E1203_UNKNOWN_TYPE_NAME` at `Missing` | an unresolved name precedes any arity finding (ADR-0034) |
| R029 | `reject/unexpected-character-at.tos` | Bootstrap | `E1013_UNEXPECTED_CHARACTER` at byte 287, line 7 column 12 | `@` begins no lexical form |
| R030 | `reject/unexpected-character-dollar.tos` | Bootstrap | `E1013_UNEXPECTED_CHARACTER` at byte 288, line 7 column 9 | `$` begins no lexical form |

R029 and R030 also fix the precedence between the two codes for a character that
cannot be tokenized: a non-ASCII scalar value outside a literal or comment is
`E1012_INVALID_IDENTIFIER`, and every other such character is
`E1013_UNEXPECTED_CHARACTER`. Both report the first byte of the character. These
vectors are ordinary `.tos` files rather than generator instructions, because
`@` and `$` are valid UTF-8 and the file can carry its own SPDX header.

Every code named in this document is defined in the diagnostic registry in
`docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md` section 7, and
`scripts/check-stage2-language-contract.py` fails if the two disagree.

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
| L005 | replace precomposed NFC `é` in an eligible comment with `65 CC 81` | `E1004_NOT_NFC` at the decomposed sequence under UCD 17.0.0/UAX #15 Rev. 57 |
| L006 | use precomposed NFC text in a comment and string literal | accepts unchanged under UCD 17.0.0/UAX #15 Rev. 57 |
| L007 | use canonically decomposed text in a string literal | `E1004_NOT_NFC` at the decomposed sequence |
| L008 | use a canonically out-of-order combining-mark sequence in a comment | `E1004_NOT_NFC` at that sequence |
| L009 | run UCD 17.0.0 `NormalizationTest.txt`-derived NFC positive/negative cases | accepts NFC and rejects non-NFC with `E1004_NOT_NFC`; generated test record retains input hashes |
| L010 | source input of 256 KiB + 1 byte | `E1000_SOURCE_LIMIT` at byte 262144 before UTF-8/NFC work |
| L011 | insert `00` NUL in otherwise valid UTF-8 source | `E1005_NUL_FORBIDDEN` at that byte |

## Required generated/IR cases

The production conformance runner must also generate one malformed IR object
per `V20xx` family in docs/43: oversized count, unknown schema, mismatched
source identity, noncanonical table order, type/CFG/import/capability/region/
resource/profile/task/synchronization/atomic/unsafe/source-map violation.
It must execute C002 under 1/2/N Full workers and serialized Bootstrap,
requiring identical logical output, actual Full overlap, bounded tasks/workers,
and the negative mutable-sharing case. Raw environment/measurement artifacts
follow docs/35 and docs/44.
