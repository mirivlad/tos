#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Regression test for the trusted Stage 1 unsafe-code coverage gate.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CHECKER="$ROOT/scripts/check-unsafe-safety.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/source/nucleus/src"
cat > "$TMP/source/nucleus/src/example.rs" <<'EOF'
// SPDX-License-Identifier: GPL-3.0-or-later

fn documented_operation() {
    // SAFETY: this fixture has no operands and the gate checks only the local rationale.
    unsafe {}
}

/// SAFETY: this fixture declares that callers uphold its synthetic contract.
#[allow(dead_code)]
unsafe fn documented_function() {}

fn documented_wrapped_operation() {
    // SAFETY: this fixture models formatter-wrapped assignment syntax.
    let _ =
        unsafe {};
}

// SAFETY: this fixture declares that the synthetic foreign symbol exists.
unsafe extern "C" {
    static DOCUMENTED_SYMBOL: u8;
}
EOF

python3 "$CHECKER" --root "$TMP"

cat >> "$TMP/source/nucleus/src/example.rs" <<'EOF'

fn undocumented_operation() {
    unsafe {}
}
EOF

if python3 "$CHECKER" --root "$TMP" >"$TMP/missing.log" 2>&1; then
    echo "FAIL: unsafe block without a local SAFETY rationale was accepted" >&2
    exit 1
fi
grep -Fq 'missing local SAFETY comment' "$TMP/missing.log" || {
    echo "FAIL: missing SAFETY rationale had no focused diagnosis" >&2
    cat "$TMP/missing.log" >&2
    exit 1
}

cat >> "$TMP/source/nucleus/src/example.rs" <<'EOF'

unsafe extern "C" {
    static UNDOCUMENTED_SYMBOL: u8;
}
EOF

if python3 "$CHECKER" --root "$TMP" >"$TMP/extern.log" 2>&1; then
    echo "FAIL: unsafe extern without a local SAFETY rationale was accepted" >&2
    exit 1
fi
grep -Fq 'unsafe extern declaration' "$TMP/extern.log" || {
    echo "FAIL: missing unsafe extern rationale had no focused diagnosis" >&2
    cat "$TMP/extern.log" >&2
    exit 1
}

echo 'unsafe-safety-coverage: PASS'
