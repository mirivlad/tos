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
    language_role_path = root / "docs/05_TOS_CORE_LANGUAGE.md"
    execution_role_path = root / "docs/06_EXECUTION_AND_IR.md"
    types_path = root / "docs/40_TOS_CORE_V1_TYPES_EVALUATION_AND_MEMORY.md"
    concurrency_path = root / "docs/41_TOS_CORE_V1_CONCURRENCY_RESOURCES_AND_DIAGNOSTICS.md"
    modules_path = root / "docs/42_TOS_CORE_V1_MODULES_CAPABILITIES_AND_VERSIONING.md"
    ir_path = root / "docs/43_TOS_CORE_V1_IR_AND_VERIFIER.md"
    conformance_path = root / "docs/44_TOS_CORE_V1_CONFORMANCE_AND_IMPLEMENTABILITY.md"
    adr_path = root / "docs/adr/0028-tos-core-v1-semantics-and-ir-contract.md"
    unicode_adr_path = root / "docs/adr/0029-tos-core-v1-unicode-normalization-baseline.md"
    expectations_path = root / "docs/language/conformance/v1/EXPECTATIONS.md"
    guide_path = root / "docs/language/TOS_CORE_V1_GUIDE.md"
    tutorial_path = root / "docs/language/LEARNING_TOS_CORE.md"
    grammar = grammar_path.read_text(encoding="utf-8")
    language_role = language_role_path.read_text(encoding="utf-8")
    execution_role = execution_role_path.read_text(encoding="utf-8")
    types = types_path.read_text(encoding="utf-8")
    concurrency = concurrency_path.read_text(encoding="utf-8")
    modules = modules_path.read_text(encoding="utf-8")
    ir = ir_path.read_text(encoding="utf-8")
    conformance = conformance_path.read_text(encoding="utf-8")
    adr = adr_path.read_text(encoding="utf-8")
    unicode_adr = unicode_adr_path.read_text(encoding="utf-8")
    expectations = expectations_path.read_text(encoding="utf-8")
    guide = guide_path.read_text(encoding="utf-8")
    tutorial = tutorial_path.read_text(encoding="utf-8")
    failures: list[str] = []

    for document in [grammar, types, concurrency, modules, ir, conformance]:
        require(
            "Status: **Accepted Tier 2 contract — production implementation in progress**" in document,
            "a numbered Stage 2 contract is not accepted for production implementation",
            failures,
        )
    require("- Status: Accepted" in adr, "ADR-0028 is not accepted", failures)
    require("Project Architect approval:" in adr, "ADR-0028 lacks Project Architect approval record", failures)
    require("accepted ADR-0028" in language_role, "language role document still treats ADR-0028 as proposed", failures)
    require("Stage 2 Part B production\n> implementation is authorized." in execution_role, "execution role document still prohibits accepted Part B", failures)
    require("- Status: Accepted (Project Architect-approved)" in unicode_adr, "ADR-0029 is not accepted", failures)
    require("Unicode Standard:                 17.0.0" in unicode_adr, "ADR-0029 lacks the Unicode 17.0.0 baseline", failures)
    require("UAX #15, Revision 57" in unicode_adr, "ADR-0029 lacks the UAX #15 revision", failures)
    require("Unicode 17.0.0" in grammar and "UAX #15 Revision 57" in grammar, "source grammar lacks the fixed Unicode baseline", failures)
    require("Unicode 17.0.0 / UAX #15 Revision 57" in modules, "module/cache contract lacks the fixed Unicode baseline", failures)
    require("UCD-17.0.0/UAX15-r57/NFC" in ir, "IR header lacks the fixed Unicode baseline", failures)
    require("NormalizationTest.txt-derived" in conformance, "conformance contract lacks Unicode-data coverage", failures)
    for lexical_case in ["L005", "L006", "L007", "L008", "L009", "L010", "L011"]:
        require(lexical_case in expectations, f"missing Unicode normalization expectation: {lexical_case}", failures)

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
        require('resource_decl   = "resource" "[" resource_limit_list? "]" ;' in grammar_body.group(1), "resource declaration does not use a list", failures)
        require('record_decl     = "record" identifier "[" field_decl_list? "]" ;' in grammar_body.group(1), "record declaration does not use a list", failures)
        require('enum_decl       = "enum" identifier "[" variant_decl_list? "]" ;' in grammar_body.group(1), "enum declaration does not use a list", failures)
        require('block           = "{" statement* "}" ;' in grammar_body.group(1), "executable block still has an implicit tail value", failures)
        require("tail_expression" not in grammar_body.group(1), "implicit tail expression remains in grammar", failures)
        require('if_stmt         = "if" "(" expression ")" block' in grammar_body.group(1), "if statement form missing", failures)
        require('while_stmt      = "while" "(" expression ")" block' in grammar_body.group(1), "while control head is not parenthesized", failures)
        require('match_stmt      = "match" "(" expression ")" "{" match_branch* "}" ;' in grammar_body.group(1), "match statement form missing", failures)
        require('match_branch    = pattern "=>" block ;' in grammar_body.group(1), "match branches are not executable blocks", failures)
        require('expression      = logical_or ;' in grammar_body.group(1), "if/match remain value-producing expressions", failures)
        require('call_suffix     = "(" call_arguments? ")" ;' in grammar_body.group(1), "unified call argument syntax missing", failures)
        require('named_argument  = identifier ":" expression ;' in grammar_body.group(1), "named record constructor argument syntax missing", failures)
        require("record_init" not in grammar_body.group(1), "record data construction still uses braces", failures)
        require('predeclared_function' in grammar_body.group(1), "checked conversion functions lack grammar representation", failures)
        require("named_argument_list" in grammar_body.group(1), "record constructor lacks a separated named argument list", failures)
        require("| block ;" not in grammar_body.group(1), "executable block remains a primary expression", failures)
        require('closure         = "fn" "(" closure_parameters? ")" block ;' in grammar_body.group(1), "closure parameters do not use the V1 parenthesized form", failures)
        require('array_type      = "array" "<" type "," const_expression ">" ;' in grammar_body.group(1), "fixed array type still uses a semicolon separator", failures)
        require('variant_decl    = identifier ( "(" type_list? ")" )?' in grammar_body.group(1), "enum variant grammar missing", failures)

    require("TaskResult<T>" in types, "TaskResult<T> semantics missing", failures)
    require("`AtomicU64`, and `ConversionError` are non-generic typed runtime contracts" in types, "fixed-arity predeclared types missing", failures)
    require("`Result<T,E>` takes two" in types, "constructed type arity inventory missing", failures)
    require("`array<T, N>` takes one type argument and one compile-time `size` constant" in types, "fixed-array type arity contract missing", failures)
    require("cancel alone does not discharge" in concurrency, "cancel/join lifecycle remains ambiguous", failures)
    require("join Task<T> -> TaskResult<T>" in concurrency, "join Task<T> result missing", failures)
    require("TaskResult<Result<T,E>>" in concurrency, "join Task<Result<T,E>> result missing", failures)
    require("TaskResult<T>" in ir, "IR task result typing missing", failures)
    require("nearest enclosing return scope" in types, "return-scope semantics missing from type/evaluation contract", failures)
    require("return scope" in ir, "return-scope lowering/verifier rule missing", failures)
    require("`to_i8` through `to_i64` and `to_u8` through `to_u64`" in types, "checked conversion source contract missing", failures)
    require("`convert<T>(x)`" not in types, "unexpressible generic conversion notation remains", failures)
    require("User records and enums are always affine/non-Copy in V1" in types, "user aggregate Copy rule is not explicit", failures)
    require(
        all(
            re.search(pattern, types) is not None
            for pattern in [r"tuple is `Copy`", r"array is `Copy`"]
        ),
        "aggregate Copy coverage is incomplete",
        failures,
    )
    require("`nil` is not a TOS Core V1 value" in modules, "module contract still implies a nil value", failures)
    require("`Option` (not `nil`)" in conformance, "conformance contract does not exclude nil", failures)
    require("TaskResult<T>" in guide and "TaskResult<T>" in tutorial, "programmer documentation misses TaskResult lifecycle", failures)

    # ADR-0032: docs/44 is the authoritative diagnostic registry that docs/41
    # points at, and it may not drift away from the conformance expectations or
    # from the codes docs/39 names.
    registry_block = re.search(
        r"<!-- stage2-diagnostic-registry:start -->\n(.*?)\n<!-- stage2-diagnostic-registry:end -->",
        conformance,
        flags=re.DOTALL,
    )
    require(registry_block is not None, "missing machine-readable diagnostic registry", failures)
    registry: dict[str, str] = {}
    if registry_block is not None:
        stage = ""
        for line in registry_block.group(1).splitlines():
            heading = re.match(r"### .*\(stage `(\w+)`\)", line)
            if heading is not None:
                stage = heading.group(1)
                continue
            row = re.match(r"\|\s*`(E1\d{3}_[A-Z0-9_]+)`\s*\|\s*(.+?)\s*\|$", line)
            if row is None:
                continue
            code, condition = row.group(1), row.group(2)
            require(code not in registry, f"duplicate registry entry: {code}", failures)
            require(stage != "", f"registry entry outside a stage section: {code}", failures)
            require(len(condition) >= 20, f"registry entry lacks a condition: {code}", failures)
            registry[code] = stage
        require(len(registry) >= 21, "diagnostic registry is implausibly small", failures)
        # A family folded into the registry must be complete for the checks the
        # implementation actually performs (docs/44 section 7).
        for code, stage in [
            ("E1201_ASSIGN_TO_IMMUTABLE", "type"),
            ("E1202_UNKNOWN_VALUE_NAME", "type"),
            ("E1203_UNKNOWN_TYPE_NAME", "type"),
            ("E1204_TYPE_ARGUMENT_ARITY", "type"),
            ("E1205_DUPLICATE_RECORD_FIELD", "type"),
            ("E1206_MISSING_RECORD_FIELD", "type"),
            ("E1207_UNKNOWN_RECORD_FIELD", "type"),
            ("E1210_INTEGER_TYPE_MISMATCH", "type"),
            ("E1211_INDEX_TYPE_MISMATCH", "type"),
            ("E1212_INVALID_AS_CONVERSION", "type"),
            ("E1220_NONEXHAUSTIVE_MATCH", "type"),
            ("E1221_MISSING_RETURN", "type"),
            ("E1222_RETURN_TYPE_MISMATCH", "type"),
            ("E1225_INVALID_DEFER", "type"),
            ("E1601_UNSUPPORTED_LANGUAGE_VERSION", "type"),
            ("E1602_UNSUPPORTED_LANGUAGE_MINOR", "type"),
            ("E1603_MODULE_PATH_MISMATCH", "type"),
            ("E1604_IMPORT_NOT_FOUND", "type"),
            ("E1606_IMPORT_CYCLE", "type"),
            ("E1607_PRIVATE_PUBLIC_TYPE", "type"),
            ("E1301_USE_AFTER_MOVE", "ownership"),
            ("E1302_CONFLICTING_BORROW", "ownership"),
            ("E1303_MUTATE_WHILE_BORROWED", "ownership"),
            ("E1304_INVALID_TASK_CAPTURE", "ownership"),
            ("E1305_INVALID_CLOSURE_CAPTURE", "ownership"),
            ("E1401_UNJOINED_TASK", "type"),
            ("E1410_INVALID_ATOMIC_ORDER", "type"),
            ("E1501_UNDECLARED_CAPABILITY_EFFECT", "effect"),
            ("E1502_FORGED_CAPABILITY", "effect"),
            ("E1801_FFI_NOT_AVAILABLE", "effect"),
            ("E1802_UNSAFE_RATIONALE_REQUIRED", "effect"),
            ("E1700_RESOURCE_DECLARATION_REQUIRED", "resource"),
            ("E1702_PROFILE_NOT_SUPPORTED", "resource"),
            ("E1703_DUPLICATE_RESOURCE_DECLARATION", "resource"),
            ("E1704_UNKNOWN_RESOURCE_LIMIT", "resource"),
        ]:
            require(registry.get(code) == stage, f"{code} is not registered at stage {stage}", failures)

    require(
        "parse error" not in expectations,
        "a conformance expectation still says 'parse error' instead of a stable code",
        failures,
    )
    # Frontend codes the source reader, lexer and parser can raise must be in
    # the registry. Later-stage families stay with their owning contract until
    # the stage that raises them is implemented (docs/44 section 7), so they are
    # checked for a definition instead.
    owning_contracts = "\n".join([types, concurrency, modules, ir, grammar])
    for document, label in [(expectations, "expectation"), (grammar, "grammar")]:
        for code in sorted(set(re.findall(r"`(E1\d{3}_[A-Z0-9_]+)`", document))):
            if re.match(r"E1[01]\d{2}_", code) or code in registry:
                require(code in registry, f"{label} cites unregistered frontend code: {code}", failures)
            else:
                require(
                    f"`{code}`" in owning_contracts,
                    f"{label} cites a later-stage code no contract defines: {code}",
                    failures,
                )
    for code in sorted(set(re.findall(r"`(V2\d{3}_[A-Z0-9_]+)`", expectations))):
        require(f"`{code}`" in ir, f"expectation cites an unspecified verifier code: {code}", failures)
    for code in ["E1013_UNEXPECTED_CHARACTER", "E1105_CONTROL_HEAD_PARENS_REQUIRED", "E1106_LIST_SEPARATOR_REQUIRED"]:
        require(code in registry, f"registry is missing required code: {code}", failures)
    require(
        registry.get("E1013_UNEXPECTED_CHARACTER") == "lex",
        "E1013_UNEXPECTED_CHARACTER is not a lexical diagnostic",
        failures,
    )
    for code in ["E1100_EXPECTED_MODULE_HEADER", "E1101_EXPECTED_IDENTIFIER", "E1107_UNEXPECTED_TOKEN"]:
        require(registry.get(code) == "parse", f"{code} is not registered as a parse diagnostic", failures)
    diagnostics_adr_path = root / "docs/adr/0032-parser-diagnostics-and-recovery.md"
    diagnostics_adr = diagnostics_adr_path.read_text(encoding="utf-8")
    require("- Status: Accepted" in diagnostics_adr, "ADR-0032 is not accepted", failures)
    pattern_adr = (root / "docs/adr/0033-pattern-name-resolution.md").read_text(encoding="utf-8")
    require("- Status: Accepted" in pattern_adr, "ADR-0033 is not accepted", failures)
    require(
        'pattern_path    = pattern_name ( "." identifier )* ;' in grammar,
        "docs/39 lacks the ADR-0033 qualified constructor-pattern path",
        failures,
    )
    require(
        "A bare identifier that exactly names a variant of the expected enum type" in types,
        "docs/40 lacks the ADR-0033 pattern resolution rule",
        failures,
    )
    arity_adr = (root / "docs/adr/0034-type-name-and-arity-diagnostics.md").read_text(encoding="utf-8")
    require("- Status: Accepted" in arity_adr, "ADR-0034 is not accepted", failures)
    require(
        "parse/type error" not in types,
        "docs/40 still leaves the type-argument arity stage ambiguous",
        failures,
    )
    require(
        "transitive public type surface" in modules,
        "docs/42 lacks the transitive public type surface rule",
        failures,
    )
    require(
        "The number of type arguments is a static type property" in types,
        "docs/40 lacks the ADR-0034 arity stage decision",
        failures,
    )
    require(
        "### Constructed-type boundary" in grammar,
        "docs/39 lacks the ADR-0034 constructed-type boundary",
        failures,
    )
    for vector in [
        "accept/task-valid-capture.tos",
        "reject/task-capture-then-use.tos",
        "accept/closure-valid-captures.tos",
        "reject/mutate-while-borrowed.tos",
        "reject/closure-borrow-capture.tos",
        "accept/visibility-exported-surface.tos",
        "accept/visibility-imported-surface.tos",
        "accept/visibility-private-internals.tos",
        "reject/visibility-private-in-public.tos",
        "reject/visibility-private-transitively.tos",
        "reject/index-type-mismatch.tos",
        "reject/integer-type-mismatch.tos",
        "reject/return-type-mismatch.tos",
        "reject/type-unknown-local.tos",
        "reject/type-unknown-qualified.tos",
        "reject/type-option-arity.tos",
        "reject/type-result-arity.tos",
        "reject/type-unknown-before-arity.tos",
        "accept/pattern-local-variants.tos",
        "accept/pattern-bindings.tos",
        "accept/pattern-qualified-import.tos",
        "reject/pattern-unknown-qualified-variant.tos",
        "reject/pattern-nonexhaustive-variants.tos",
    ]:
        require((root / "docs/language/conformance/v1" / vector).is_file(), f"missing vector: {vector}", failures)
        require(vector in expectations, f"missing expectation: {vector}", failures)
    require(
        re.search(
            r"or at the `\}` that closes a top-level declaration\s+body and returns delimiter nesting to zero",
            grammar,
        )
        is not None,
        "docs/39 lacks the ADR-0032 declaration-recovery boundary",
        failures,
    )

    required_vectors = [
        "accept/type-forms.tos",
        "accept/control-heads.tos",
        "accept/task-cancellation.tos",
        "accept/explicit-control-return.tos",
        "accept/async-explicit-return.tos",
        "accept/named-record-constructor.tos",
        "accept/named-enum-variant.tos",
        "accept/return-scopes.tos",
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
        "reject/implicit-tail-return.tos",
        "reject/missing-nonunit-return.tos",
        "reject/old-resource-braces.tos",
        "reject/old-enum-braces.tos",
        "reject/old-record-braces.tos",
        "reject/old-record-construction-braces.tos",
        "reject/comma-match-branches.tos",
        "reject/duplicate-record-constructor-field.tos",
        "reject/missing-record-constructor-field.tos",
        "reject/unchecked-conversion.tos",
        "reject/noncopy-aggregate.tos",
        "reject/standalone-block-expression.tos",
        "reject/old-array-semicolon-type.tos",
    ]
    for vector in required_vectors:
        require((root / "docs/language/conformance/v1" / vector).is_file(), f"missing vector: {vector}", failures)
        require(vector in expectations, f"missing expectation: {vector}", failures)

    def vector_text(relative: str) -> str:
        path = root / "docs/language/conformance/v1" / relative
        return path.read_text(encoding="utf-8") if path.is_file() else ""

    control_values = vector_text("accept/explicit-control-return.tos")
    require("if (ready)" in control_values and "return" in control_values, "if explicit-return conformance case missing", failures)
    require("match (signal)" in control_values and "=> {" in control_values, "match block conformance case missing", failures)
    call_vector = vector_text("accept/call-and-constructor.tos")
    require(all(token in call_vector for token in ["zero()", "add(start, 2i32)", "Ok(", "Err(", "Pair(total, 3i32)"]), "call/constructor conformance coverage incomplete", failures)
    conversion_vector = vector_text("accept/checked-conversion.tos")
    require("to_u8(value)" in conversion_vector, "checked conversion conformance case missing", failures)
    copy_vector = vector_text("accept/copy-aggregates.tos")
    require(all(token in copy_vector for token in ["let tuple", "let array"]), "aggregate Copy conformance coverage incomplete", failures)
    enum_vector = vector_text("accept/named-enum-variant.tos")
    require("Rgb(red:" in enum_vector, "named-field enum construction conformance case missing", failures)
    return_scope_vector = vector_text("accept/return-scopes.tos")
    require("spawn async" in return_scope_vector and "fn (value: i32)" in return_scope_vector, "nested task/closure return-scope conformance case missing", failures)

    vector_root = root / "docs/language/conformance/v1"
    for vector in sorted(vector_root.glob("accept/*.tos")) + sorted(vector_root.glob("reject/*.tos")):
        rel = vector.relative_to(vector_root).as_posix()
        require(rel in expectations, f"conformance input lacks expectation: {rel}", failures)

    data_example = (root / "docs/language/examples/data.tos").read_text(encoding="utf-8")
    first_example = (root / "docs/language/examples/first.tos").read_text(encoding="utf-8")
    require("-> (i32, i32)" in data_example, "tuple example unexpectedly changed", failures)
    require("match (axis)" in data_example and "return Point(" in data_example, "canonical data example does not use explicit match return", failures)
    require("if (answer == 42i32)" in first_example and "return Ok(answer);" in first_example, "canonical first example does not use explicit if return", failures)

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

    for source_dir in [
        root / "docs/language/examples",
        root / "docs/language/conformance/v1/accept",
    ]:
        for source in sorted(source_dir.glob("*.tos")):
            source_text = source.read_text(encoding="utf-8")
            require("resource {" not in source_text, f"old resource-brace syntax in canonical source: {source.relative_to(root)}", failures)
            require(not re.search(r"\b(record|enum)\s+[A-Za-z_][A-Za-z0-9_]*\s*\{", source_text), f"old declaration-brace syntax in canonical source: {source.relative_to(root)}", failures)
            require("uses {" not in source_text, f"old effect-brace syntax in canonical source: {source.relative_to(root)}", failures)
            require(not re.search(r"\b(?:return|=)\s+[A-Z][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)?\s*\{", source_text), f"old record-construction braces in canonical source: {source.relative_to(root)}", failures)
            for function in re.finditer(r"(?:async\s+)?fn\s+\w+\([^)]*\)\s*->(?!\s*unit\b)[^{]+\{(.*?)\n\}", source_text, flags=re.DOTALL):
                require("return " in function.group(1), f"non-unit canonical function lacks explicit return: {source.relative_to(root)}", failures)

    language_docs = "\n".join([grammar, types, concurrency, modules, ir, conformance, guide, tutorial])
    require(not re.search(r"\b(Semaphore|Event|Barrier|Latch|AtomicBool|AtomicU32|AtomicU64)\s*<", language_docs), "zero-arity predeclared type is used as generic", failures)
    require(
        re.search(r"ordinary function calls and tuple-variant\s+constructors use the same Call form", types, flags=re.IGNORECASE) is not None,
        "call/constructor semantic unification missing",
        failures,
    )
    require("Five syntax rules to remember" in tutorial, "tutorial lacks the punctuation model", failures)
    require("[]` — lists, data, and declarations" in tutorial, "tutorial punctuation model omits declarative lists", failures)
    require("fn (value: i32) { ... }" in tutorial, "tutorial does not teach parenthesized closure syntax", failures)

    if failures:
        for failure in failures:
            print(f"stage2-language-contract: FAIL: {failure}")
        return 1
    print("stage2-language-contract: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
