#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# CLI/delegation regression tests for the human Stage 1 QEMU launcher.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LAUNCHER="$ROOT/run-tos.sh"
HARNESS="$ROOT/source/host-tools/qemu-test/run.sh"

if [[ ! -f "$LAUNCHER" ]]; then
    echo "FAIL: missing run-tos.sh" >&2
    exit 1
fi
if ! sh "$LAUNCHER" --help | grep -q -- '--check'; then
    echo "FAIL: launcher help does not describe --check" >&2
    exit 1
fi
if sh "$LAUNCHER" --unknown >/dev/null 2>&1; then
    echo "FAIL: launcher accepted an unknown option" >&2
    exit 1
fi
if ! bash "$HARNESS" --help | grep -q -- '--interactive'; then
    echo "FAIL: existing QEMU harness has no documented interactive mode" >&2
    exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/repo/source/host-tools/qemu-test" "$TMP/bin"
cp "$LAUNCHER" "$TMP/repo/run-tos.sh"

cat > "$TMP/repo/source/host-tools/qemu-test/run.sh" <<'EOF'
#!/usr/bin/env bash
printf 'harness %s\n' "$*" >> "$RUN_TOS_TEST_LOG"
exit 0
EOF
chmod +x "$TMP/repo/source/host-tools/qemu-test/run.sh"

cat > "$TMP/bin/cargo" <<'EOF'
#!/bin/sh
printf 'cargo %s\n' "$*" >> "$RUN_TOS_TEST_LOG"
exit 0
EOF
chmod +x "$TMP/bin/cargo"

cat > "$TMP/bin/rustup" <<'EOF'
#!/bin/sh
if [ "${RUN_TOS_MISSING_TARGET-}" = 1 ]; then
    printf '%s\n' x86_64-unknown-uefi
else
    printf '%s\n' x86_64-unknown-uefi x86_64-unknown-none
fi
EOF
chmod +x "$TMP/bin/rustup"

export RUN_TOS_TEST_LOG="$TMP/check.log"
(cd "$TMP/repo" && PATH="$TMP/bin:$PATH" sh ./run-tos.sh --check) >/dev/null
cat > "$TMP/check.expected" <<'EOF'
cargo build --release -p tos-capsule-tool
cargo build --release -p tos-uefi-loader --target x86_64-unknown-uefi
cargo build --release -p tos-nucleus --target x86_64-unknown-none
harness --out target/run-tos/check --expect 33
EOF
if ! cmp -s "$TMP/check.expected" "$TMP/check.log"; then
    echo "FAIL: --check does not build/delegate as designed" >&2
    diff -u "$TMP/check.expected" "$TMP/check.log" >&2 || true
    exit 1
fi

export RUN_TOS_TEST_LOG="$TMP/interactive.log"
(cd "$TMP/repo" && PATH="$TMP/bin:$PATH" DISPLAY=:1 \
    sh ./run-tos.sh) >/dev/null
if ! tail -n 1 "$TMP/interactive.log" \
        | grep -Fxq 'harness --out target/run-tos/interactive --expect 33 --interactive'; then
    echo "FAIL: interactive mode did not delegate to the shared harness" >&2
    cat "$TMP/interactive.log" >&2
    exit 1
fi

if (cd "$TMP/repo" && PATH="$TMP/bin:$PATH" RUN_TOS_MISSING_TARGET=1 \
        sh ./run-tos.sh --check) >"$TMP/missing.out" 2>&1; then
    echo "FAIL: missing Rust target was accepted" >&2
    exit 1
fi
if ! grep -q 'x86_64-unknown-none' "$TMP/missing.out"; then
    echo "FAIL: missing-target error did not name x86_64-unknown-none" >&2
    cat "$TMP/missing.out" >&2
    exit 1
fi

if (cd "$TMP/repo" && PATH="$TMP/bin:$PATH" DISPLAY= WAYLAND_DISPLAY= \
        sh ./run-tos.sh) >"$TMP/display.out" 2>&1; then
    echo "FAIL: interactive mode accepted a missing graphical session" >&2
    exit 1
fi
if ! grep -q 'graphical session' "$TMP/display.out"; then
    echo "FAIL: missing-display error is not actionable" >&2
    cat "$TMP/display.out" >&2
    exit 1
fi

echo "run-tos-tests: PASS"
