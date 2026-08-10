<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS Core V1 documentation-status matrix

All rows below describe **Accepted** semantics. `Specified` means a precise rule
exists in docs/39–44; `Not implemented` means no production parser/checker/
verifier/interpreter has executed it yet. This matrix prevents a reader from
mistaking guide/tutorial material for completed Stage 2 functionality.

| Feature | Normative specification | Guide | Tutorial / canonical example | Common mistake / diagnostic | Conformance vector | Implementation status |
|---|---|---|---|---|---|---|
| source normalization and module header | docs/39 §1–2; ADR-0029 | Guide: What a program is | [first.tos](examples/first.tos) | BOM/NFC/tab: E1002/E1004/E1010 | L001–L009, C004 | Bounded SourceReader and lexer implemented; parser/checker not implemented |
| declarations, functions, statements | docs/39 §5; docs/40 §4 | Guide: Values / Functions | [data.tos](examples/data.tos) | explicit non-unit return: E1221 | C001, C009, R018–R024 | Not implemented |
| fixed-width values and units | docs/39 §3/5; docs/40 §1–3 | Guide: Values | [values.tos](examples/values.tos), [checked-conversion.tos](conformance/v1/accept/checked-conversion.tos) | implicit/narrowing conversion: E1210/E1212 | C001, C011, R016 | Not implemented |
| records, tuples, enums, match | docs/39 §5; docs/40 §1/4 | Guide: Data | [data.tos](examples/data.tos), [explicit-control-return.tos](conformance/v1/accept/explicit-control-return.tos), [named-record-constructor.tos](conformance/v1/accept/named-record-constructor.tos), [named-enum-variant.tos](conformance/v1/accept/named-enum-variant.tos), [call-and-constructor.tos](conformance/v1/accept/call-and-constructor.tos) | missing variant: E1220; malformed head/list: E1105/E1106 | C006, C007, C009, C010, C014–C016, R009–R012, R014, R023–R028 | Not implemented |
| Option, Result, `?` | docs/40 §1/4 | Guide: Option and Result | [results.tos](examples/results.tos) | treating trap as Result | C004 | Not implemented |
| ownership and borrows | docs/40 §5 | Guide: Ownership | [ownership.tos](examples/ownership.tos), [copy-aggregates.tos](conformance/v1/accept/copy-aggregates.tos) | moved/conflicting value: E1301/E1302 | C012, R001, R002, R017 | Not implemented |
| imports and module identity | docs/39 §5; docs/42 §1 | Guide: Files/modules | [modules.tos](examples/modules.tos) | ambient/missing import: E1604 | C005, import graph cases | Not implemented |
| capability effects/nonforgeability | docs/40 §2; docs/42 §2 | Guide: Regions/capabilities | [capabilities.tos](examples/capabilities.tos) | scalar authority: E1502 | R003, R008 | Not implemented |
| resource envelope / metering | docs/41 §6 | Guide: Resources | [resources.tos](examples/resources.tos) | unmetered Bootstrap loop: E1701 | R005 | Not implemented |
| structured parallelism | docs/39 §5; docs/41 §1–2 | Guide: Async/parallel | [parallel-work.tos](examples/parallel-work.tos), [task-cancellation.tos](conformance/v1/accept/task-cancellation.tos) | abandoned child: E1401; cancellation still needs join | C002, C008, R013 + 1/2/N exercise | Not implemented |
| asynchronous tasks | docs/39 §5; docs/41 §1–2 | Guide: Async/parallel | [async-tasks.tos](examples/async-tasks.tos) | Bootstrap use: E1702; match `TaskResult<T>` explicitly | async/profile generated cases | Not implemented |
| safe sharing / synchronization | docs/40 §6; docs/41 §3–4 | Guide: Shared data | [parallel-work.tos](examples/parallel-work.tos) | mutable task capture: E1304 | R004 + sync cases | Not implemented |
| atomics / happens-before | docs/41 §5 | Guide: Atomics | [atomic-publication.tos](examples/atomic-publication.tos) | invalid ordering: E1410 | C003, R007 | Not implemented |
| Bootstrap vs Full | docs/42 §3 | Guide: Bootstrap/Full | tutorial chapters 15, 19 | silent downgrade: E1702 | R006 | Not implemented |
| diagnostics and source maps | docs/41 §7; docs/43 §6 | Guide: Diagnostics | [EXPECTATIONS.md](conformance/v1/EXPECTATIONS.md) | unbound output map: V2040 | all primary-code cases | Not implemented |
| typed IR and independent verifier | docs/43 §1–5 | Guide: What a program is | [forged-ir.md](conformance/v1/reject/forged-ir.md) | frontend trust claim: V2013 | R008 + V20xx generated cases | Not implemented |
| unsafe / FFI boundary | docs/40 §7; docs/42 §5 | Guide: Bootstrap/Full | tutorial chapter 19 | absent interface: E1801 | profile/unsafe generated cases | Not implemented |
| realistic bounded module shape | docs/39–42 | Guide throughout | [counter-service.tos](examples/counter-service.tos) | source request is not a grant | source-map/resource generated cases | Not implemented |
