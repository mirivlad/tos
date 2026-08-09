#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Reject drift inside the proposed TOS Core V1 documentation contract."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


def require(condition: bool, message: str, failures: list[str]) -> None:
    if not condition:
        failures.append(message)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    grammar_path = root / "docs/39_TOS_CORE_V1_SOURCE_AND_GRAMMAR.md"
    types_path = root / "docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md"
    concurrency_path = root / "docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md"
    modules_path = root / "docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md"
    ir_path = root / "docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md"
    conformance_path = root / "docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md"
    expectations_path = root / "docs/language/conformance/v1/EXPECTATIONS.md"
    guide_path = root / "docs/language/TOS_CORE_V1_GUIDE.md"
    tutorial_path = root / "docs/language/LEARNING_TOS_CORE.md"
    grammar = grammar_path.read_text(encoding="utf-8")
    types = types_path.read_text(encoding="utf-8")
    concurrency = concurrency_path.read_text(encoding="utf-8")
    modules = modules_path.read_text(encoding="utf-8")
    ir = ir_path.read_text(encoding="utf-8")
    conformance = conformance_path.read_text(encoding="utf-8")
    expectations = expectations_path.read_text(encoding="utf-8")
    guide = guide_path.read_text(encoding="utf-8")
    tutorial = tutorial_path.read_text(encoding="utf-8")
    failures: list[str] = []

    for document in [grammar, types, concurrency, modules, ir, conformance]:
        require(
            "Status: **Proposed Stage 2 contract — not implementation authority**" in document,
            "a numbered Stage 2 contract lost its proposed/not-implemented status",
            failures,
        )

    inventory = re.search(
        r"<!-- stage2-word-inventory:start -->\n```text\n(.*?)\n```\n<!-- stage2-word-inventory:end -->",
        grammar,
        flags=re.DOTALL,
    )
    require(inventory is not None, "missing machine-readable word inventory", failures)
    words: set[str] = set()
    if inventory is not None:
        for line in inventory.group(1).splitlines():
            if ":" not in line:
                continue
            _, values = line.split(":", 1)
            words.update(values.split())

    grammar_body = re.search(
        r"## 5\. Complete V1 grammar\n\n```ebnf\n(.*?)\n```",
        grammar,
        flags=re.DOTALL,
    )
    require(grammar_body is not None, "missing complete EBNF grammar block", failures)
    if grammar_body is not None:
        terminals = set(re.findall(r'"([A-Za-z_][A-Za-z0-9_]*)"', grammar_body.group(1)))
        missing = sorted(terminals - words)
        require(not missing, f"grammar words absent from inventory: {', '.join(missing)}", failures)
        require('"nil"' not in grammar_body.group(1), "nil remains a V1 grammar terminal", failures)
        require("tuple_type" in grammar_body.group(1), "tuple_type missing from grammar", failures)
        require('"slice" "<" type ">"' in grammar_body.group(1), "slice<T> missing from grammar", failures)
        require('if_expression   = "if" "(" expression ")" block' in grammar_body.group(1), "if is not a value-producing expression", failures)
        require('while_stmt      = "while" "(" expression ")" block' in grammar_body.group(1), "while control head is not parenthesized", failures)
        require('match_expression = "match" "(" expression ")" "{" match_arm_list? "}"' in grammar_body.group(1), "match is not a value-producing expression", failures)
        require('expression      = if_expression | match_expression | logical_or ;' in grammar_body.group(1), "if/match are absent from expression grammar", failures)
        require('call_suffix     = "(" argument_list? ")" ;' in grammar_body.group(1), "generic call/constructor-call syntax missing", failures)
        require("enum_init" not in grammar_body.group(1), "enum constructor remains a competing parse", failures)
        require('predeclared_function' in grammar_body.group(1), "checked conversion functions lack grammar representation", failures)
        require("field_init_list" in grammar_body.group(1), "record initializer lacks a separated field list", failures)
        require('field_init      = identifier ":" expression ;' in grammar_body.group(1), "record field unexpectedly owns a separator", failures)

    require("TaskResult<T>" in types, "TaskResult<T> semantics missing", failures)
    require("`AtomicU64`, and `ConversionError` are non-generic typed runtime contracts" in types, "fixed-arity predeclared types missing", failures)
    require("`Result<T,E>` takes two" in types, "constructed type arity inventory missing", failures)
    require("cancel alone does not discharge" in concurrency, "cancel/join lifecycle remains ambiguous", failures)
    require("join Task<T> -> TaskResult<T>" in concurrency, "join Task<T> result missing", failures)
    require("TaskResult<Result<T,E>>" in concurrency, "join Task<Result<T,E>> result missing", failures)
    require("TaskResult<T>" in ir, "IR task result typing missing", failures)
    require("`to_i8` through `to_i64` and `to_u8` through `to_u64`" in types, "checked conversion source contract missing", failures)
    require("`convert<T>(x)`" not in types, "unexpressible generic conversion notation remains", failures)
    require(re.search(r"Copy is\s+automatic and structural", types) is not None, "aggregate Copy rule is not explicit and automatic", failures)
    require(
        all(
            re.search(pattern, types) is not None
            for pattern in [r"tuple is `Copy`", r"array is `Copy`", r"record or enum\s+is `Copy`"]
        ),
        "aggregate Copy coverage is incomplete",
        failures,
    )
    require("`nil` is not a TOS Core V1 value" in modules, "module contract still implies a nil value", failures)
    require("`Option` (not `nil`)" in conformance, "conformance contract does not exclude nil", failures)
    require("TaskResult<T>" in guide and "TaskResult<T>" in tutorial, "programmer documentation misses TaskResult lifecycle", failures)

    required_vectors = [
        "accept/type-forms.tos",
        "accept/control-heads.tos",
        "accept/task-cancellation.tos",
        "accept/control-values.tos",
        "accept/call-and-constructor.tos",
        "accept/checked-conversion.tos",
        "accept/copy-aggregates.tos",
        "reject/if-identifier-control-head.tos",
        "reject/while-identifier-control-head.tos",
        "reject/match-identifier-control-head.tos",
        "reject/record-field-separator.tos",
        "reject/unjoined-task.tos",
        "reject/duplicate-record-field.tos",
        "reject/nil-absence.tos",
        "reject/unchecked-conversion.tos",
        "reject/noncopy-aggregate.tos",
    ]
    for vector in required_vectors:
        require((root / "docs/language/conformance/v1" / vector).is_file(), f"missing vector: {vector}", failures)
        require(vector in expectations, f"missing expectation: {vector}", failures)

    def vector_text(relative: str) -> str:
        path = root / "docs/language/conformance/v1" / relative
        return path.read_text(encoding="utf-8") if path.is_file() else ""

    control_values = vector_text("accept/control-values.tos")
    require("let value = if (ready)" in control_values and "pub fn tail_if" in control_values, "if value conformance cases missing", failures)
    require("let value = match (signal)" in control_values and "pub fn tail_match" in control_values, "match value conformance cases missing", failures)
    call_vector = vector_text("accept/call-and-constructor.tos")
    require(all(token in call_vector for token in ["zero()", "add(start, 2i32)", "Ok(", "Err(", "Pair(total, 3i32)"]), "call/constructor conformance coverage incomplete", failures)
    conversion_vector = vector_text("accept/checked-conversion.tos")
    require("to_u8(value)" in conversion_vector, "checked conversion conformance case missing", failures)
    copy_vector = vector_text("accept/copy-aggregates.tos")
    require(all(token in copy_vector for token in ["let tuple", "let array", "let pair", "let choice"]), "aggregate Copy conformance coverage incomplete", failures)

    vector_root = root / "docs/language/conformance/v1"
    for vector in sorted(vector_root.glob("accept/*.tos")) + sorted(vector_root.glob("reject/*.tos")):
        rel = vector.relative_to(vector_root).as_posix()
        require(rel in expectations, f"conformance input lacks expectation: {rel}", failures)

    data_example = (root / "docs/language/examples/data.tos").read_text(encoding="utf-8")
    first_example = (root / "docs/language/examples/first.tos").read_text(encoding="utf-8")
    require("-> (i32, i32)" in data_example, "tuple example unexpectedly changed", failures)
    require("match (axis)" in data_example, "canonical data example no longer has tail match", failures)
    require("if (answer == 42i32)" in first_example, "canonical first example no longer has tail if", failures)

    allowed_unparenthesized = {
        "reject/if-identifier-control-head.tos",
        "reject/while-identifier-control-head.tos",
        "reject/match-identifier-control-head.tos",
    }
    for source_dir in [
        root / "docs/language/examples",
        root / "docs/language/conformance/v1/accept",
        root / "docs/language/conformance/v1/reject",
    ]:
        for source in sorted(source_dir.glob("*.tos")):
            for line_no, line in enumerate(source.read_text(encoding="utf-8").splitlines(), start=1):
                rel = source.relative_to(vector_root).as_posix() if source.is_relative_to(vector_root) else ""
                if re.match(r"^\s*(if|while|match)\s+[^ (]", line) and rel not in allowed_unparenthesized:
                    failures.append(
                        f"unparenthesized control head in canonical source: {source.relative_to(root)}:{line_no}"
                    )

    language_docs = "\n".join([grammar, types, concurrency, modules, ir, conformance, guide, tutorial])
    require(not re.search(r"\b(Semaphore|Event|Barrier|Latch|AtomicBool|AtomicU32|AtomicU64)\s*<", language_docs), "zero-arity predeclared type is used as generic", failures)
    require(
        re.search(r"ordinary function calls and tuple-variant\s+constructors use the same Call form", types, flags=re.IGNORECASE) is not None,
        "call/constructor semantic unification missing",
        failures,
    )

    if failures:
        for failure in failures:
            print(f"stage2-language-contract: FAIL: {failure}")
        return 1
    print("stage2-language-contract: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
