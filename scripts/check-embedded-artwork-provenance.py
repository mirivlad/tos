#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check provenance for separately licensed artwork embedded by Stage 1."""

import argparse
import hashlib
import json
from pathlib import Path


EXPECTED_DIGEST = "0641c303b486e1615250c09c1f597a25a2dab2e478062cb2d7ba4d9879bbea6f"
EXPECTED = {
    "format": "TOS.embedded-artwork-provenance.v1",
    "canonical_source": "assets/mascot/tos_ascii-art2.txt",
    "canonical_source_spdx": "CC-BY-SA-4.0",
    "canonical_source_commit": "21975bba71b2be32d6222efbf0dcb4d43488bb0e",
    "consumer": "source/nucleus/src/framebuffer.rs",
    "consumer_spdx": "GPL-3.0-or-later",
    "embedding": "exact-include-bytes-ascii-grid-v1",
    "licence_notice_retained": True,
    "attribution_record": "assets/mascot/README.md",
    "adapted_material_treatment": "CC-BY-SA-4.0-to-GPLv3-one-way-compatibility-if-adapted",
}


def fail(message: str) -> int:
    print(f"embedded-artwork-provenance: FAIL: {message}")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    root = parser.parse_args().root.resolve()
    record_path = root / "assets/mascot/pyro-stage1-provenance.json"
    try:
        record = json.loads(record_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        return fail(f"cannot read provenance record: {error}")

    if record.get("record_spdx_license") != "CC-BY-SA-4.0":
        return fail("record SPDX licence is not CC-BY-SA-4.0")
    if record.get("format") != EXPECTED["format"]:
        return fail("unsupported provenance record format")
    embeddings = record.get("embeddings")
    if not isinstance(embeddings, list) or len(embeddings) != 1:
        return fail("expected exactly one Pyro embedding")
    entry = embeddings[0]
    if not isinstance(entry, dict):
        return fail("embedding is not an object")
    for key, expected in EXPECTED.items():
        if key == "format":
            continue
        if entry.get(key) != expected:
            return fail(f"unexpected {key}")
    if entry.get("canonical_source_sha256") != EXPECTED_DIGEST:
        return fail("recorded canonical source digest mismatch")

    source = root / EXPECTED["canonical_source"]
    try:
        source_bytes = source.read_bytes()
    except OSError as error:
        return fail(f"cannot read canonical source: {error}")
    if hashlib.sha256(source_bytes).hexdigest() != EXPECTED_DIGEST:
        return fail("canonical source digest mismatch")
    if not source_bytes.startswith(b"# SPDX-License-Identifier: CC-BY-SA-4.0\n"):
        return fail("canonical source SPDX header is missing")

    attribution = root / EXPECTED["attribution_record"]
    try:
        attribution_text = attribution.read_text(encoding="utf-8")
    except OSError as error:
        return fail(f"cannot read attribution record: {error}")
    if "`assets/mascot/tos_ascii-art2.txt`" not in attribution_text:
        return fail("attribution record does not inventory canonical source")

    consumer = root / EXPECTED["consumer"]
    try:
        consumer_text = consumer.read_text(encoding="utf-8")
    except OSError as error:
        return fail(f"cannot read consumer: {error}")
    include = 'include_bytes!("../../../assets/mascot/tos_ascii-art2.txt")'
    if include not in consumer_text:
        return fail("consumer does not embed the canonical artwork source")

    print("embedded-artwork-provenance: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
