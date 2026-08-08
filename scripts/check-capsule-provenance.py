#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate the accepted Stage 1 capsule provenance sidecar v1."""

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path


FORMAT = "tos-capsule-provenance-v1"
FORMAT_UUID = bytes.fromhex("2c4f78b39d1e4b0a9f2c1a5c8e0d6f71")
HEX256 = re.compile(r"[0-9a-f]{64}")
OID = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})")


class ProvenanceError(Exception):
    pass


def fail(field: str, message: str) -> None:
    raise ProvenanceError(f"{field}: {message}")


def require_object(value: object, field: str) -> dict:
    if not isinstance(value, dict):
        fail(field, "must be an object")
    return value


def require_list(value: object, field: str) -> list:
    if not isinstance(value, list):
        fail(field, "must be an array")
    return value


def require_string(value: object, field: str) -> str:
    if not isinstance(value, str):
        fail(field, "must be a string")
    return value


def require_hex256(value: object, field: str) -> str:
    value = require_string(value, field)
    if not HEX256.fullmatch(value):
        fail(field, "must be 64 lowercase hexadecimal characters")
    return value


def u16(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 2], "little")


def u32(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 4], "little")


def u64(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 8], "little")


def bounded(data: bytes, offset: int, size: int, field: str) -> bytes:
    if offset < 0 or size < 0 or offset + size > len(data):
        fail(field, "lies outside capsule bytes")
    return data[offset : offset + size]


def parse_capsule(data: bytes) -> dict:
    if len(data) < 184:
        fail("capsule", "is shorter than the v1 header")
    if data[:8] != b"TOSCAPSU" or data[8:24] != FORMAT_UUID or u16(data, 24) != 1:
        fail("capsule", "is not a capsule v1 artifact")
    path_offset, path_count = u64(data, 40), u32(data, 48)
    file_offset, file_count = u64(data, 56), u32(data, 64)
    payload_offset = u64(data, 72)
    if path_count != file_count:
        fail("capsule", "path/file count differs")
    materials = []
    name_start = path_offset + path_count * 16
    for index in range(path_count):
        path_entry = path_offset + index * 16
        name_offset, name_length, file_index = (
            u32(data, path_entry),
            u32(data, path_entry + 4),
            u32(data, path_entry + 8),
        )
        if file_index != index:
            fail("capsule", "file table is not in canonical path order")
        try:
            path = bounded(data, name_start + name_offset, name_length, "capsule path").decode("utf-8")
        except UnicodeDecodeError as error:
            fail("capsule path", f"is not UTF-8: {error}")
        file_entry = file_offset + index * 64
        content_offset, content_length = u64(data, file_entry), u64(data, file_entry + 8)
        content = bounded(data, payload_offset + content_offset, content_length, "capsule content")
        digest = data[file_entry + 16 : file_entry + 48]
        if hashlib.sha256(content).digest() != digest:
            fail("capsule content", "does not match its file-table digest")
        materials.append((path, digest.hex(), content))
    return {
        "format_version": u16(data, 24),
        "architecture": u32(data, 88),
        "builder_version": u32(data, 92),
        "identity_kind": data[96],
        "oid_algorithm": data[97],
        "oid_length": data[98],
        "identity_value": data[100:132],
        "notice": bounded(data, u64(data, 136), u64(data, 144), "licence notice"),
        "materials": materials,
    }


def spdx_identifiers(data: bytes, field: str) -> list[str]:
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as error:
        fail(field, f"is not UTF-8: {error}")
    marker = "SPDX-License-Identifier:"
    values = []
    for line in text.splitlines():
        if marker not in line:
            continue
        value = line.split(marker, 1)[1].strip().removesuffix("-->").strip()
        if value:
            values.append(value)
    return sorted(set(values))


def source_spdx(data: bytes, field: str) -> str:
    values = spdx_identifiers(data, field)
    if not values:
        fail(field, "has no SPDX-License-Identifier")
    return values[0]


def git(root: Path, *args: str) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(root), *args], capture_output=True, check=False
    )
    if result.returncode != 0:
        fail("source_identity.source_commit", result.stderr.decode("utf-8", "replace").strip())
    return result.stdout


def validate_identity(record: dict, capsule: dict, root: Path) -> None:
    identity = require_object(record.get("source_identity"), "source_identity")
    kind = require_string(identity.get("kind"), "source_identity.kind")
    if kind == "git-commit":
        if capsule["identity_kind"] != 1:
            fail("source_identity.kind", "does not match capsule header")
        commit = require_string(identity.get("source_commit"), "source_identity.source_commit")
        if not OID.fullmatch(commit):
            fail("source_identity.source_commit", "must be a full lowercase Git OID")
        algorithm = require_string(identity.get("oid_algorithm"), "source_identity.oid_algorithm")
        expected_length = 20 if algorithm == "sha1" else 32 if algorithm == "sha256" else None
        if expected_length is None or identity.get("oid_length") != expected_length:
            fail("source_identity", "has an unsupported Git OID algorithm/length")
        raw_oid = require_string(identity.get("raw_oid"), "source_identity.raw_oid")
        if raw_oid != commit or len(raw_oid) != expected_length * 2:
            fail("source_identity.raw_oid", "must equal source_commit at its declared length")
        if capsule["oid_algorithm"] != (1 if algorithm == "sha1" else 2) or capsule["oid_length"] != expected_length:
            fail("source_identity", "OID metadata does not match capsule header")
        if capsule["identity_value"][:expected_length].hex() != raw_oid or any(
            capsule["identity_value"][expected_length:]
        ):
            fail("source_identity.raw_oid", "does not match canonical capsule header padding")
        git(root, "cat-file", "-e", f"{commit}^{{commit}}")
    elif kind == "detached-source-set":
        if capsule["identity_kind"] != 2 or capsule["oid_algorithm"] != 0 or capsule["oid_length"] != 0:
            fail("source_identity", "does not match detached capsule header")
        if identity.get("digest_algorithm") != "sha256":
            fail("source_identity.digest_algorithm", "must be sha256")
        digest = require_hex256(identity.get("digest"), "source_identity.digest")
        if digest != capsule["identity_value"].hex():
            fail("source_identity.digest", "does not match capsule header")
    else:
        fail("source_identity.kind", "must be git-commit or detached-source-set")


def validate(record: dict, capsule_bytes: bytes, root: Path) -> None:
    if record.get("format") != FORMAT or record.get("schema_version") != 1:
        fail("format", "must be tos-capsule-provenance-v1 schema_version 1")
    capsule = parse_capsule(capsule_bytes)
    artifact = require_object(record.get("artifact"), "artifact")
    if require_hex256(artifact.get("sha256"), "artifact.sha256") != hashlib.sha256(capsule_bytes).hexdigest():
        fail("artifact.sha256", "does not match capsule bytes")
    capsule_format = require_object(artifact.get("capsule_format"), "artifact.capsule_format")
    if capsule_format != {"uuid": "2c4f78b3-9d1e-4b0a-9f2c-1a5c8e0d6f71", "version": capsule["format_version"]}:
        fail("artifact.capsule_format", "does not match capsule header")
    if artifact.get("architecture_spec_version") != "0.2.1" or capsule["architecture"] != 0x000201:
        fail("artifact.architecture_spec_version", "does not match capsule header")
    if artifact.get("builder") != {"implementation": "tos-capsule-tool", "version": capsule["builder_version"]}:
        fail("artifact.builder", "does not match capsule header")
    if artifact.get("target") != {
        "architecture": "x86_64",
        "loader_abi": "x86_64-unknown-uefi",
        "nucleus_boot_abi": {
            "minimum": {"major": 1, "minor": 0},
            "maximum": {"major": 1, "minor": 0},
        },
    }:
        fail("artifact.target", "is not the accepted Stage 1 target ABI")

    validate_identity(record, capsule, root)
    materials = require_list(record.get("materials"), "materials")
    if len(materials) != len(capsule["materials"]):
        fail("materials", "does not cover every capsule file")
    previous = ""
    for index, (entry, expected) in enumerate(zip(materials, capsule["materials"])):
        entry = require_object(entry, f"materials[{index}]")
        path, digest, content = expected
        if entry.get("role") != "canonical-source" or entry.get("capsule_path") != path:
            fail(f"materials[{index}]", "does not match canonical capsule path")
        if path <= previous:
            fail("materials", "is not in ascending canonical path order")
        previous = path
        if require_hex256(entry.get("content_sha256"), f"materials[{index}].content_sha256") != digest:
            fail(f"materials[{index}].content_sha256", "does not match capsule content")
        if entry.get("spdx_expression") != source_spdx(content, f"materials[{index}]"):
            fail(f"materials[{index}].spdx_expression", "does not match source bytes")
        if record["source_identity"]["kind"] == "git-commit":
            repository_path = require_string(entry.get("repository_path"), f"materials[{index}].repository_path")
            blob = git(root, "cat-file", "blob", f"{record['source_identity']['source_commit']}:{repository_path}")
            if blob != content:
                fail(f"materials[{index}].repository_path", "does not name the recorded Git blob")
        elif "repository_path" in entry:
            fail(f"materials[{index}].repository_path", "must be absent for detached identity")

    notice = require_object(record.get("licence_notice"), "licence_notice")
    if require_hex256(notice.get("sha256"), "licence_notice.sha256") != hashlib.sha256(capsule["notice"]).hexdigest():
        fail("licence_notice.sha256", "does not match embedded notice tail")
    identifiers = require_list(notice.get("spdx_identifiers"), "licence_notice.spdx_identifiers")
    if identifiers != spdx_identifiers(capsule["notice"], "licence_notice"):
        fail("licence_notice.spdx_identifiers", "does not match the embedded notice")
    if not identifiers or identifiers != sorted(set(identifiers)):
        fail("licence_notice.spdx_identifiers", "must be a non-empty sorted unique list")
    for entry in materials:
        if entry["spdx_expression"] not in identifiers:
            fail("licence_notice.spdx_identifiers", "does not cover every source material")
    build = require_object(record.get("build"), "build")
    expected_mode = record["source_identity"]["kind"]
    if build != {
        "identity_mode": expected_mode,
        "licence_notice_included": True,
        "reproducibility_grade": "R0",
    }:
        fail("build", "does not state the accepted Stage 1 build facts")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--capsule", required=True, type=Path)
    parser.add_argument("--manifest", required=True, type=Path)
    args = parser.parse_args()
    try:
        record = json.loads(args.manifest.read_text(encoding="utf-8"))
        validate(require_object(record, "manifest"), args.capsule.read_bytes(), args.root.resolve())
    except (OSError, json.JSONDecodeError, ProvenanceError) as error:
        print(f"capsule-provenance: FAIL: {error}", file=sys.stderr)
        return 1
    print("capsule-provenance: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
