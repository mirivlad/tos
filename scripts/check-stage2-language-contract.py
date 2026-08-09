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
        require('if_stmt         = "if" "(" expression ")" block' in grammar_body.group(1), "if control head is not parenthesized", failures)
        require('while_stmt      = "while" "(" expression ")" block' in grammar_body.group(1), "while control head is not parenthesized", failures)
        require('match_stmt      = "match" "(" expression ")" "{" match_arm* "}"' in grammar_body.group(1), "match control head is not parenthesized", failures)
        require("field_init_list" in grammar_body.group(1), "record initializer lacks a separated field list", failures)
        require('field_init      = identifier ":" expression ;' in grammar_body.group(1), "record field unexpectedly owns a separator", failures)

    require("TaskResult<T>" in types, "TaskResult<T> semantics missing", failures)
    require("`Event`, `Semaphore`, `Barrier`, `Latch`, `AtomicBool`, `AtomicU32`, and" in types, "fixed-arity predeclared types missing", failures)
    require("`Result<T,E>` takes two" in types, "constructed type arity inventory missing", failures)
    require("cancel alone does not discharge" in concurrency, "cancel/join lifecycle remains ambiguous", failures)
    require("join Task<T> -> TaskResult<T>" in concurrency, "join Task<T> result missing", failures)
    require("TaskResult<Result<T,E>>" in concurrency, "join Task<Result<T,E>> result missing", failures)
    require("TaskResult<T>" in ir, "IR task result typing missing", failures)
    require("`nil` is not a TOS Core V1 value" in modules, "module contract still implies a nil value", failures)
    require("`Option` (not `nil`)" in conformance, "conformance contract does not exclude nil", failures)
    require("TaskResult<T>" in guide and "TaskResult<T>" in tutorial, "programmer documentation misses TaskResult lifecycle", failures)

    required_vectors = [
        "accept/type-forms.tos",
        "accept/control-heads.tos",
        "accept/task-cancellation.tos",
        "reject/if-identifier-control-head.tos",
        "reject/while-identifier-control-head.tos",
        "reject/match-identifier-control-head.tos",
        "reject/record-field-separator.tos",
        "reject/unjoined-task.tos",
        "reject/duplicate-record-field.tos",
        "reject/nil-absence.tos",
    ]
    for vector in required_vectors:
        require((root / "docs/language/conformance/v1" / vector).is_file(), f"missing vector: {vector}", failures)
        require(vector in expectations, f"missing expectation: {vector}", failures)

    vector_root = root / "docs/language/conformance/v1"
    for vector in sorted(vector_root.glob("accept/*.tos")) + sorted(vector_root.glob("reject/*.tos")):
        rel = vector.relative_to(vector_root).as_posix()
        require(rel in expectations, f"conformance input lacks expectation: {rel}", failures)

    example = (root / "docs/language/examples/data.tos").read_text(encoding="utf-8")
    require("-> (i32, i32)" in example, "tuple example unexpectedly changed", failures)

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

    if failures:
        for failure in failures:
            print(f"stage2-language-contract: FAIL: {failure}")
        return 1
    print("stage2-language-contract: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
