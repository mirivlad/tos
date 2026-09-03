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
if ! sh "$LAUNCHER" --help | grep -q -- '--stage3'; then
    echo "FAIL: launcher help does not describe --stage3" >&2
    exit 1
fi
# The Stage 3 demonstration must be the canonical supervision scenario, not a
# second one written to look like it. What makes that checkable is that the
# launcher delegates to the same script the `qemu_supervision` gate runs.
if ! grep -q 'host-tools/qemu-test/supervision.sh' "$LAUNCHER"; then
    echo "FAIL: --stage3 does not run the canonical supervision scenario" >&2
    exit 1
fi
if ! grep -q 'qemu-test/supervision.sh' "$ROOT/scripts/preflight.sh"; then
    echo "FAIL: the supervision scenario is no longer a gate" >&2
    exit 1
fi
# And it renders the accepted transport with the accepted reader rather than a
# second selection rule of its own.
if ! grep -q 'scripts/tos-journal.py' "$LAUNCHER"; then
    echo "FAIL: --stage3 does not use the accepted journal reader" >&2
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
[ "${PWD##*/}" = source ] || {
    echo "rustup target check was not run from source/" >&2
    exit 9
}
if [ "${RUN_TOS_MISSING_TARGET-}" = 1 ]; then
    printf '%s\n' x86_64-unknown-uefi
else
    printf '%s\n' x86_64-unknown-uefi x86_64-unknown-none
fi
EOF
chmod +x "$TMP/bin/rustup"

cat > "$TMP/bin/qemu-system-x86_64" <<'EOF'
#!/bin/sh
if [ "$1" = -display ] && [ "$2" = help ]; then
    printf '%b\n' "${RUN_TOS_DISPLAY_HELP-gtk\\nsdl}"
    exit 0
fi
echo "unexpected QEMU invocation: $*" >&2
exit 9
EOF
chmod +x "$TMP/bin/qemu-system-x86_64"

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
        | grep -Fxq 'harness --out target/run-tos/interactive --interactive --display gtk'; then
    echo "FAIL: interactive mode did not delegate to the shared harness" >&2
    cat "$TMP/interactive.log" >&2
    exit 1
fi

export RUN_TOS_TEST_LOG="$TMP/sdl.log"
(cd "$TMP/repo" && PATH="$TMP/bin:$PATH" DISPLAY=:1 RUN_TOS_DISPLAY_HELP='none\nsdl' \
    sh ./run-tos.sh) >/dev/null
if ! tail -n 1 "$TMP/sdl.log" \
        | grep -Fxq 'harness --out target/run-tos/interactive --interactive --display sdl'; then
    echo "FAIL: SDL fallback was not delegated to the shared harness" >&2
    cat "$TMP/sdl.log" >&2
    exit 1
fi

if (cd "$TMP/repo" && PATH="$TMP/bin:$PATH" DISPLAY=:1 RUN_TOS_DISPLAY_HELP='none\ncurses' \
        sh ./run-tos.sh) >"$TMP/no-backend.out" 2>&1; then
    echo "FAIL: interactive mode accepted QEMU without GTK or SDL" >&2
    exit 1
fi
for line in \
    'run-tos: QEMU has no graphical display backend' \
    'run-tos: on Debian/MX install qemu-system-gui' \
    'run-tos: use ./run-tos.sh --check for headless verification'
do
    if ! grep -Fxq "$line" "$TMP/no-backend.out"; then
        echo "FAIL: no-backend error omitted: $line" >&2
        cat "$TMP/no-backend.out" >&2
        exit 1
    fi
done

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

# --- the Stage 3 mode delegates as designed --------------------------------
cat > "$TMP/repo/source/host-tools/qemu-test/supervision.sh" <<'EOF'
#!/usr/bin/env bash
printf 'supervision %s\n' "$*" >> "$RUN_TOS_TEST_LOG"
mkdir -p "$1"
: > "$1/serial.log"
exit 0
EOF
chmod +x "$TMP/repo/source/host-tools/qemu-test/supervision.sh"
mkdir -p "$TMP/repo/scripts" "$TMP/repo/source/tests/vectors/supervision"
cat > "$TMP/repo/scripts/tos-journal.py" <<'EOF'
import sys
print("journal " + " ".join(sys.argv[1:]))
EOF

export RUN_TOS_TEST_LOG="$TMP/stage3.log"
(cd "$TMP/repo" && PATH="$TMP/bin:$PATH" sh ./run-tos.sh --stage3) > "$TMP/stage3.out" 2>&1
cat > "$TMP/stage3.expected" <<'EOF'
cargo build --release -p tos-capsule-tool
cargo build --release -p tos-uefi-loader --target x86_64-unknown-uefi
cargo build --release -p tos-nucleus --target x86_64-unknown-none
cargo build --release -p tos-runtime-image --target x86_64-unknown-none
supervision TMPREPO/source/target/run-tos/stage3
EOF
sed -i "s|TMPREPO|$TMP/repo|" "$TMP/stage3.expected"
if ! cmp -s "$TMP/stage3.expected" "$TMP/stage3.log"; then
    echo "FAIL: --stage3 does not build/delegate as designed" >&2
    diff -u "$TMP/stage3.expected" "$TMP/stage3.log" >&2 || true
    exit 1
fi
# The production runtime image is built, not inherited: the shared target
# directory also holds the evidence builds' feature-gated images, and booting
# whichever was there last would demonstrate something other than the system.
if ! grep -Fq 'cargo build --release -p tos-runtime-image --target x86_64-unknown-none' \
        "$TMP/stage3.log"; then
    echo "FAIL: --stage3 did not build the production runtime image" >&2
    exit 1
fi
# It names where the evidence is, and where the policy a user would edit lives.
for needle in \
    'source/target/run-tos/stage3/serial.log' \
    'source/tests/vectors/supervision' \
    '/system/policy/services.tos' \
    '/system/boot/init.tos'
do
    if ! grep -Fq "$needle" "$TMP/stage3.out"; then
        echo "FAIL: --stage3 output does not mention: $needle" >&2
        cat "$TMP/stage3.out" >&2
        exit 1
    fi
done
# And it renders both views through the accepted reader.
if ! grep -Fq -- '--story' "$TMP/stage3.out"; then
    echo "FAIL: --stage3 did not render the supervision story" >&2
    cat "$TMP/stage3.out" >&2
    exit 1
fi

# Stage 3 is headless by default: no graphical session is required for it, and
# demanding one would make the demonstration unrunnable over ssh.
export RUN_TOS_TEST_LOG="$TMP/stage3-headless.log"
if ! (cd "$TMP/repo" && PATH="$TMP/bin:$PATH" DISPLAY= WAYLAND_DISPLAY= \
        sh ./run-tos.sh --stage3) >/dev/null 2>&1; then
    echo "FAIL: --stage3 required a graphical session" >&2
    exit 1
fi

# With --interactive it asks for the display the same way the Stage 1 mode does,
# and passes it through to the same harness.
export RUN_TOS_TEST_LOG="$TMP/stage3-display.log"
(cd "$TMP/repo" && PATH="$TMP/bin:$PATH" DISPLAY=:1 \
    sh ./run-tos.sh --stage3 --interactive) >/dev/null 2>&1
if ! grep -Fq -- '--interactive --display gtk' "$TMP/stage3-display.log"; then
    echo "FAIL: --stage3 --interactive did not pass the display through" >&2
    cat "$TMP/stage3-display.log" >&2
    exit 1
fi
if (cd "$TMP/repo" && PATH="$TMP/bin:$PATH" DISPLAY= WAYLAND_DISPLAY= \
        sh ./run-tos.sh --stage3 --interactive) >"$TMP/stage3-nodisplay.out" 2>&1; then
    echo "FAIL: --stage3 --interactive accepted a missing graphical session" >&2
    exit 1
fi
if ! grep -q 'graphical session' "$TMP/stage3-nodisplay.out"; then
    echo "FAIL: --stage3 --interactive error is not actionable" >&2
    cat "$TMP/stage3-nodisplay.out" >&2
    exit 1
fi

# A trailing option that is not --interactive is still an error.
if (cd "$TMP/repo" && PATH="$TMP/bin:$PATH" sh ./run-tos.sh --stage3 --nonsense) \
        >/dev/null 2>&1; then
    echo "FAIL: --stage3 accepted an unknown trailing option" >&2
    exit 1
fi

echo "run-tos-tests: PASS"
