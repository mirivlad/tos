#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Every declared feature of the two freestanding binaries type-checks.
#
# **Why this gate exists.** Stage 4B added a method to the `System` trait and
# missed one implementation — `Marked`, behind `test-measurement-call`. Nothing
# local compiled that feature, because the two gates that select it exit early
# on a host without an ADR-0066 observer QEMU, so a compile error travelled all
# the way to CI while every local run was green. A feature nothing builds is a
# feature nothing checks.
#
# It type-checks rather than builds: what is being caught is a code path that
# stopped compiling, and `cargo check` catches that at a fraction of the cost.
# Features are checked one at a time, because several are alternative launcher
# constants and enabling them together would be a configuration no boot has.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)/source"
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}/featcheck"
failures=0
checked=0

features_of() {
    python3 - "$1" <<'PY'
import pathlib, re, sys
body = pathlib.Path(sys.argv[1]).read_text().split("[features]", 1)[1]
for line in body.splitlines():
    if line.startswith("["):
        break
    match = re.match(r"^([a-z][a-z0-9-]*) *=", line)
    if match:
        print(match.group(1))
PY
}

for package in tos-runtime-image tos-nucleus; do
    case "$package" in
        tos-runtime-image) manifest="$ROOT/runtime-image/Cargo.toml" ;;
        tos-nucleus)       manifest="$ROOT/nucleus/Cargo.toml" ;;
    esac
    while IFS= read -r feature; do
        [ -n "$feature" ] || continue
        checked=$((checked + 1))
        if ! (cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo check -q \
                -p "$package" --target x86_64-unknown-none \
                --features "$feature" 2>&1); then
            echo "check-feature-builds: FAIL: $package --features $feature" >&2
            failures=$((failures + 1))
        fi
    done < <(features_of "$manifest")
done

[ "$failures" -eq 0 ] || {
    echo "check-feature-builds: FAIL ($failures of $checked configuration(s))" >&2
    exit 1
}
echo "check-feature-builds: OK ($checked feature configuration(s) type-check)"
