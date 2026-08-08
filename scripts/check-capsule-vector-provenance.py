#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate the versioned capsule-v1 fixture provenance manifest.

The checker is the machine-enforced schema for
``tos-capsule-vector-provenance-v1``.  It intentionally uses only Python's
standard library: this is a release/provenance gate, not a new TOS runtime or
build dependency.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path, PurePosixPath
from typing import Any


FORMAT = "tos-capsule-vector-provenance-v1"
SCHEMA_VERSION = 1
VECTOR_DIRECTORY = "source/tests/vectors/capsule-v1"
MANIFEST_PATH = f"{VECTOR_DIRECTORY}/provenance.json"
SCHEMA_PATH = f"{VECTOR_DIRECTORY}/provenance.schema.json"
ALLOWED_SPDX = {
    "GPL-3.0-or-later",
    "Apache-2.0",
    "CC-BY-SA-4.0",
    "GPL-3.0-or-later OR Apache-2.0",
}
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
OID_RE = {"sha1": re.compile(r"^[0-9a-f]{40}$"), "sha256": SHA256_RE}


def git(root: Path, *args: str) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["git", "-C", str(root), *args],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def tracked(root: Path, path: str) -> bool:
    return git(root, "ls-files", "--error-unmatch", "--", path).returncode == 0


def tracked_bins(root: Path) -> set[str]:
    result = git(root, "ls-files", "--", f"{VECTOR_DIRECTORY}/*.bin")
    if result.returncode != 0:
        raise RuntimeError(result.stderr.decode(errors="replace").strip())
    return {
        Path(line).name
        for line in result.stdout.decode().splitlines()
        if line.endswith(".bin")
    }


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(64 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def is_repository_path(value: Any) -> bool:
    if not isinstance(value, str) or not value or value.startswith("/"):
        return False
    path = PurePosixPath(value)
    return ".." not in path.parts and path.as_posix() == value


def is_sha256(value: Any) -> bool:
    return isinstance(value, str) and SHA256_RE.fullmatch(value) is not None


def git_blob_sha256(root: Path, commit: str, path: str) -> str | None:
    result = git(root, "show", f"{commit}:{path}")
    if result.returncode != 0:
        return None
    return hashlib.sha256(result.stdout).hexdigest()


def check_git_commit(root: Path, value: Any, where: str, errors: list[str]) -> str | None:
    if not isinstance(value, dict):
        errors.append(f"{where}: source_commit must be an object")
        return None
    algorithm = value.get("algorithm")
    oid = value.get("value")
    if value.get("kind") != "git" or algorithm not in OID_RE:
        errors.append(f"{where}: source_commit must be a git sha1/sha256 identity")
        return None
    if not isinstance(oid, str) or OID_RE[algorithm].fullmatch(oid) is None:
        errors.append(f"{where}: source_commit value is not a full {algorithm} OID")
        return None
    if git(root, "cat-file", "-e", f"{oid}^{{commit}}").returncode != 0:
        errors.append(f"{where}: source_commit does not name a local commit")
        return None
    return oid


def check_generator(root: Path, record: Any, where: str, errors: list[str]) -> None:
    if not isinstance(record, dict):
        errors.append(f"{where}: generator must be an object")
        return
    path = record.get("path")
    if not is_repository_path(path):
        errors.append(f"{where}: generator path is not a repository-relative path")
        return
    if not isinstance(record.get("version"), int) or record["version"] < 1:
        errors.append(f"{where}: generator version must be a positive integer")
    if record.get("spdx") not in ALLOWED_SPDX:
        errors.append(f"{where}: generator SPDX is missing or unsupported")
    source_commit = record.get("source_commit")
    if not isinstance(source_commit, str) or not any(
        pattern.fullmatch(source_commit) for pattern in OID_RE.values()
    ):
        errors.append(f"{where}: generator source_commit must be a full Git OID")
        return
    if git(root, "cat-file", "-e", f"{source_commit}^{{commit}}").returncode != 0:
        errors.append(f"{where}: generator source_commit does not name a local commit")
        return
    digest = record.get("sha256")
    if not is_sha256(digest):
        errors.append(f"{where}: generator sha256 must be 64 lowercase hex")
        return
    actual = git_blob_sha256(root, source_commit, path)
    if actual is None:
        errors.append(f"{where}: generator path is absent from generator source_commit")
    elif actual != digest:
        errors.append(f"{where}: generator sha256 does not match generator source_commit")


def check_inputs(root: Path, record: Any, commit: str | None, where: str, errors: list[str]) -> None:
    if not isinstance(record, list) or not record:
        errors.append(f"{where}: inputs must be a non-empty array")
        return
    for index, item in enumerate(record):
        item_where = f"{where}[{index}]"
        if not isinstance(item, dict):
            errors.append(f"{item_where}: input must be an object")
            continue
        path = item.get("repository_path")
        if not is_repository_path(path):
            errors.append(f"{item_where}: repository_path is not repository-relative")
            continue
        if not isinstance(item.get("role"), str) or not item["role"].strip():
            errors.append(f"{item_where}: role must be a non-empty string")
        capsule_path = item.get("capsule_path")
        if capsule_path is not None and (
            not isinstance(capsule_path, str) or not capsule_path.startswith("/")
        ):
            errors.append(f"{item_where}: capsule_path must be an absolute capsule path or null")
        digest = item.get("sha256")
        if not is_sha256(digest):
            errors.append(f"{item_where}: sha256 must be 64 lowercase hex")
        spdx = item.get("spdx")
        if not isinstance(spdx, list) or not spdx or any(value not in ALLOWED_SPDX for value in spdx):
            errors.append(f"{item_where}: spdx must be a non-empty list from the project licence matrix")
        if commit is not None and is_sha256(digest):
            actual = git_blob_sha256(root, commit, path)
            if actual is None:
                errors.append(f"{item_where}: input is absent from source_commit")
            elif actual != digest:
                errors.append(f"{item_where}: input sha256 does not match source_commit")


def check_container(record: Any, where: str, errors: list[str]) -> None:
    if not isinstance(record, dict):
        errors.append(f"{where}: container_licensing must be an object")
        return
    if record.get("status") != "mixed-material-generated":
        errors.append(f"{where}: container_licensing.status must be mixed-material-generated")
    if record.get("spdx_expression") is not None:
        errors.append(f"{where}: container_licensing.spdx_expression must be null")


def check_derivation(record: Any, names: dict[str, str], where: str, errors: list[str]) -> None:
    if record is None:
        return
    if not isinstance(record, dict):
        errors.append(f"{where}: derivation must be null or an object")
        return
    base = record.get("base_vector")
    base_digest = record.get("base_sha256")
    recipe = record.get("transformation_recipe")
    if not isinstance(base, str) or base not in names:
        errors.append(f"{where}: derivation base_vector must name another manifest vector")
    if not is_sha256(base_digest):
        errors.append(f"{where}: derivation base_sha256 must be 64 lowercase hex")
    elif isinstance(base, str) and base in names and names[base] != base_digest:
        errors.append(f"{where}: derivation base_sha256 does not match base_vector")
    if not isinstance(recipe, dict) or not isinstance(recipe.get("kind"), str) or not recipe["kind"]:
        errors.append(f"{where}: derivation transformation_recipe must have a kind")
    elif not isinstance(recipe.get("operations"), list) or not recipe["operations"]:
        errors.append(f"{where}: derivation transformation_recipe must have non-empty operations")
    else:
        for index, operation in enumerate(recipe["operations"]):
            if not isinstance(operation, dict) or not isinstance(operation.get("op"), str) or not operation["op"]:
                errors.append(f"{where}: derivation operation {index} must name an op")


def check_schema(root: Path) -> list[str]:
    schema_path = root / SCHEMA_PATH
    if not tracked(root, SCHEMA_PATH):
        return [f"{SCHEMA_PATH}: provenance schema must be tracked"]
    try:
        schema = json.loads(schema_path.read_text(encoding="utf-8"))
        properties = schema["properties"]
        if (
            schema["record_spdx_license"] != "GPL-3.0-or-later"
            or properties["format"]["const"] != FORMAT
            or properties["schema_version"]["const"] != SCHEMA_VERSION
        ):
            raise KeyError("schema metadata does not match checker contract")
    except (OSError, json.JSONDecodeError, KeyError, TypeError) as error:
        return [f"{SCHEMA_PATH}: invalid provenance schema: {error}"]
    return []


def check_manifest(root: Path, manifest_path: Path) -> list[str]:
    errors = check_schema(root)
    relative_manifest = manifest_path.relative_to(root).as_posix()
    if not tracked(root, relative_manifest):
        return errors + [f"{relative_manifest}: provenance manifest must be tracked"]
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return errors + [f"{relative_manifest}: cannot read JSON: {error}"]
    if not isinstance(manifest, dict):
        return errors + [f"{relative_manifest}: top-level value must be an object"]
    if manifest.get("record_spdx_license") not in ALLOWED_SPDX:
        errors.append(f"{relative_manifest}: record_spdx_license is missing or unsupported")
    if manifest.get("format") != FORMAT:
        errors.append(f"{relative_manifest}: format must be {FORMAT}")
    if manifest.get("schema_version") != SCHEMA_VERSION:
        errors.append(f"{relative_manifest}: schema_version must be {SCHEMA_VERSION}")
    vectors = manifest.get("vectors")
    if not isinstance(vectors, list):
        return errors + [f"{relative_manifest}: vectors must be an array"]

    names: dict[str, str] = {}
    for index, entry in enumerate(vectors):
        where = f"{relative_manifest}: vectors[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{where}: entry must be an object")
            continue
        vector = entry.get("vector")
        digest = entry.get("sha256")
        if not isinstance(vector, str) or not vector.endswith(".bin") or "/" in vector:
            errors.append(f"{where}: vector must be a fixture filename")
            continue
        if vector in names:
            errors.append(f"{where}: duplicate vector entry {vector}")
        elif is_sha256(digest):
            names[vector] = digest
        else:
            errors.append(f"{where}: sha256 must be 64 lowercase hex")

    fixture_names = tracked_bins(root)
    if set(names) != fixture_names:
        for name in sorted(fixture_names - set(names)):
            errors.append(f"{relative_manifest}: missing provenance entry for {name}")
        for name in sorted(set(names) - fixture_names):
            errors.append(f"{relative_manifest}: entry names untracked fixture {name}")

    for index, entry in enumerate(vectors):
        where = f"{relative_manifest}: vectors[{index}]"
        if not isinstance(entry, dict):
            continue
        vector = entry.get("vector")
        digest = entry.get("sha256")
        if isinstance(vector, str) and vector in fixture_names and is_sha256(digest):
            actual = sha256_file(root / VECTOR_DIRECTORY / vector)
            if actual != digest:
                errors.append(f"{where}: fixture sha256 does not match {vector}")
        if entry.get("generated_artifact") is not True:
            errors.append(f"{where}: generated_artifact must be true")
        check_container(entry.get("container_licensing"), where, errors)
        status = entry.get("provenance_status")
        if status == "verified":
            commit = check_git_commit(root, entry.get("source_commit"), where, errors)
            check_generator(root, entry.get("generator"), where, errors)
            check_inputs(root, entry.get("inputs"), commit, where, errors)
        elif status == "unverifiable-legacy":
            if not isinstance(entry.get("legacy_reason"), str) or not entry["legacy_reason"].strip():
                errors.append(f"{where}: unverifiable-legacy requires legacy_reason")
            if entry.get("generator") is not None or entry.get("source_commit") is not None:
                errors.append(f"{where}: unverifiable-legacy must not claim generator or source_commit")
            if entry.get("inputs") != []:
                errors.append(f"{where}: unverifiable-legacy inputs must be an empty array")
        else:
            errors.append(f"{where}: provenance_status must be verified or unverifiable-legacy")
        check_derivation(entry.get("derivation"), names, where, errors)
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    manifest = args.manifest.resolve() if args.manifest else root / MANIFEST_PATH
    try:
        errors = check_manifest(root, manifest)
    except (OSError, RuntimeError, ValueError) as error:
        errors = [str(error)]
    if errors:
        for error in errors:
            print(f"check-capsule-vector-provenance: {error}", file=sys.stderr)
        return 1
    print("check-capsule-vector-provenance: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
