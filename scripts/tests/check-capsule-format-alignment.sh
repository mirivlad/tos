#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail

doc=source/interfaces/boot/CAPSULE_FORMAT_V1.md

grep -Fq 'content_offset` are **not** required' "$doc"
if sed -n '/^16\. /,/^17\. /p' "$doc" | grep -q misaligned; then
    echo 'FAIL: rule 16 contradicts ADR-0017' >&2
    exit 1
fi

echo 'capsule-format-alignment: PASS'
