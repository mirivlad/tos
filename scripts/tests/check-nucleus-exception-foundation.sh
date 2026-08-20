#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Check the mechanically inspectable parts of the bounded Stage 1 exception foundation.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SOURCE="$ROOT/source/nucleus/src/exception.rs"
[ -f "$SOURCE" ] || { echo "missing nucleus exception foundation: $SOURCE" >&2; exit 1; }
grep -Fq 'const DF_IST_INDEX: u8 = 1;' "$SOURCE" >/dev/null
grep -Fq 'const EXCEPTION_VECTOR_COUNT: usize = 32;' "$SOURCE" >/dev/null
grep -Fq 'entry.set(handler, if vector == 8 { DF_IST_INDEX } else { 0 });' "$SOURCE" >/dev/null
grep -Fq 'write_unaligned(addr_of_mut!(TSS.ist[0]), stack_top);' "$SOURCE" >/dev/null
grep -Fq 'load_task_register(TSS_SELECTOR);' "$SOURCE" >/dev/null
echo 'nucleus-exception-foundation: PASS'
