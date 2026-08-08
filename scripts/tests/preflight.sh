#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# CLI/orchestration regression tests for scripts/preflight.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PREFLIGHT="$ROOT/scripts/preflight.sh"

if [[ ! -f "$PREFLIGHT" ]]; then
    echo "FAIL: missing scripts/preflight.sh" >&2
    exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/repo/scripts" "$TMP/repo/tools" \
    "$TMP/repo/source/host-tools/qemu-test" "$TMP/bin"
cp "$PREFLIGHT" "$TMP/repo/scripts/preflight.sh"

make_log_script() {
    local path=$1 name=$2
    cat > "$path" <<EOF
#!/bin/sh
printf '%s\n' '$name' >> "\$PREFLIGHT_TEST_LOG"
exit 0
EOF
    chmod +x "$path"
}

make_log_script "$TMP/repo/tools/build-specification.py" spec
make_log_script "$TMP/repo/tools/build-release-manifest.py" release
make_log_script "$TMP/repo/scripts/check-spdx.sh" spdx
make_log_script "$TMP/repo/scripts/check-dco.sh" dco
make_log_script "$TMP/repo/source/host-tools/qemu-test/run.sh" qemu-success
make_log_script "$TMP/repo/source/host-tools/qemu-test/negative-suite.sh" qemu-negative

cat > "$TMP/bin/python3" <<'EOF'
#!/bin/sh
script=$1
shift
exec "$script" "$@"
EOF
chmod +x "$TMP/bin/python3"

cat > "$TMP/bin/cargo" <<'EOF'
#!/bin/sh
printf 'cargo %s\n' "$*" >> "$PREFLIGHT_TEST_LOG"
exit 0
EOF
chmod +x "$TMP/bin/cargo"

if ! sh "$PREFLIGHT" --help | grep -q -- '--full'; then
    echo "FAIL: --help does not describe --full" >&2
    exit 1
fi
if sh "$PREFLIGHT" --unknown >/dev/null 2>&1; then
    echo "FAIL: unknown option was accepted" >&2
    exit 1
fi

export PREFLIGHT_TEST_LOG="$TMP/default.log"
(cd "$TMP/repo" && PATH="$TMP/bin:$PATH" sh scripts/preflight.sh) >/dev/null
cat > "$TMP/default.expected" <<'EOF'
spec
release
spdx
dco
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo clippy -p tos-uefi-loader --target x86_64-unknown-uefi -- -D warnings
cargo clippy -p tos-nucleus --target x86_64-unknown-none -- -D warnings
EOF
if ! cmp -s "$TMP/default.expected" "$TMP/default.log"; then
    echo "FAIL: default gate order differs" >&2
    diff -u "$TMP/default.expected" "$TMP/default.log" >&2 || true
    exit 1
fi

export PREFLIGHT_TEST_LOG="$TMP/full.log"
(cd "$TMP/repo" && PATH="$TMP/bin:$PATH" sh scripts/preflight.sh --full) >/dev/null
cat > "$TMP/full.tail.expected" <<'EOF'
cargo run --release -p tos-tests-fuzz -- 200000
cargo build --release -p tos-capsule-tool
cargo build --release -p tos-uefi-loader --target x86_64-unknown-uefi
cargo build --release -p tos-nucleus --target x86_64-unknown-none
qemu-success
qemu-negative
EOF
tail -n 6 "$TMP/full.log" > "$TMP/full.tail"
if ! cmp -s "$TMP/full.tail.expected" "$TMP/full.tail"; then
    echo "FAIL: full-only gate order differs" >&2
    diff -u "$TMP/full.tail.expected" "$TMP/full.tail" >&2 || true
    exit 1
fi

echo "preflight-tests: PASS"
