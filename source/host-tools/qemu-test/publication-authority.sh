#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Possession of the publication endpoint is what permits publication, and this
# gate proves it by taking it away.
#
# **The property, as `CAPABILITY_V1` §6 states it after ADR-0095.** The right to
# publish `block.device.v1` is a capability naming a **dedicated publication
# object** whose identity fixes what may be published through it. The registry
# holds `receive` on it; an authorised publisher holds `call`. A process that can
# name no such endpoint cannot publish through it, and no name written into a
# message is authority.
#
# **So the negative is one endowment short of the positive and identical
# otherwise.** The claimant's source is an authorised publisher's source: it
# requests the publication endpoint under the same binding and calls it the
# moment it runs. Nothing is stubbed, no string is compared, and the registry has
# no branch involved — it never sees the claimant at all. The claimant is refused
# by the startup check because nothing answered its request, which is
# `Refusal::CapabilityDenied` and is in the audit record with the binding and the
# interface named (`PROCESS_IDENTITY_V1` §7.3).
#
# **And the mutation is the point** (ADR-0095 §5.3). Running again with
# `publish_call` endowed to the claimant under the same binding — one line of the
# launcher, nothing else: not the module, not its source, not the registry — must
# make the negative stop proving denial. The claimant starts, publishes, and the
# registry receives. A negative that survived that mutation would have been
# testing something else, and this gate performs both runs rather than asserting
# the first alone.
#
# The positive half of ADR-0095 §5.1 is `name-service.sh` and
# `block-service.sh`; this gate does not repeat it.
#
#   bash host-tools/qemu-test/publication-authority.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-publication-authority}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/publication-authority"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-publication-authority"
WORK="$OUT/source"

# The launcher: the registry created, the claimant created, and the launcher's own
# receiving name let go.
EXPECTED_INIT="i64:7"
# The registry, denied run: it waited on the publication endpoint and the liveness
# rule ended the wait, so nothing was published.
EXPECTED_REGISTRY_EMPTY="i64:2"
# And in the mutation: a registration arrived and was acknowledged.
EXPECTED_REGISTRY_SERVED="i64:1"
# The claimant, in the mutation only: the registration was accepted.
EXPECTED_CLAIMANT="i64:64"
# What the refusal must say. The binding is what the source called it and the
# interface is what was wanted; ADR-0051's evidence line asks for both.
DENIAL='^TOS\.RUN\.REFUSED stage=execute reason=capability-denied binding=publish interface=system\.ipc\.Endpoint$'

fail() {
    echo "publication-authority: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

# **The nucleus is the block-service one, unchanged.** Its launcher constant
# already endows the boot process with names for a publication endpoint, a lookup
# endpoint, a service endpoint and an inbox; which of them travel to which child
# is the canonical textual launcher's decision, and that is the only thing these
# two runs differ in. No nucleus feature was added for this gate.
before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
(cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release \
    -p tos-nucleus --target x86_64-unknown-none --features test-block-service)
if [ -n "$before" ]; then
    after="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
    [ "$before" = "$after" ] ||
        fail "the production nucleus changed while building the isolated test artifact"
fi

# The mutation, as a one-line edit to the launcher's claimant plan. Written here
# rather than kept as a second fixture on purpose: a second copy of the launcher
# could drift from the first, and then the two runs would differ in more than the
# endowment.
GRANT_LINE='            if (endow_for_launch(inbox_full, builder, RIGHT_SEND, "own") != OK) {'
MUTATED='            if (endow_for_launch(publish_call, builder, RIGHT_CALL, "publish") != OK) {
                return Err(-2002i64);
            }
            if (endow_for_launch(inbox_full, builder, RIGHT_SEND, "own") != OK) {'

prepare() {
    # $1 = "denied" | "granted"
    rm -rf "$WORK"
    mkdir -p "$WORK"
    cp "$FIXTURE"/*.tos "$WORK/"
    if [ "$1" = granted ]; then
        python3 - "$WORK/init.tos" "$GRANT_LINE" "$MUTATED" <<'PY'
import pathlib
import sys

path, anchor, replacement = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
text = path.read_text()
if anchor not in text:
    raise SystemExit("the launcher no longer has the line the mutation edits")
# And the capability the mutation endows has to be imported to be named.
text = text.replace(
    "import capability system.ipc.Endpoint as inbox_full;",
    "import capability system.ipc.Endpoint as inbox_full;\n"
    "import capability system.ipc.Endpoint as publish_call;",
    1,
)
text = text.replace(
    "fn claimant_plan() -> Result<system.process.LaunchPlan, i64>\n"
    "    uses [process, inbox_full, system.ipc.Endpoint]",
    "fn claimant_plan() -> Result<system.process.LaunchPlan, i64>\n"
    "    uses [process, inbox_full, publish_call, system.ipc.Endpoint]",
    1,
)
text = text.replace(
    "          uses [process, memory, publish_full, inbox_full, system.ipc.Endpoint]",
    "          uses [process, memory, publish_full, publish_call, inbox_full,\n"
    "          system.ipc.Endpoint]",
    1,
)
text = text.replace(
    "    uses [process, memory, publish_full, inbox_full, system.ipc.Endpoint]",
    "    uses [process, memory, publish_full, publish_call, inbox_full,\n"
    "          system.ipc.Endpoint]",
    1,
)
text = text.replace("const RIGHT_SEND: u64 = 1u64;",
                    "const RIGHT_SEND: u64 = 1u64;\nconst RIGHT_CALL: u64 = 4u64;", 1)
text = text.replace(anchor, replacement, 1)
path.write_text(text)
PY
    fi
    printf '/system/boot/init.tos\t%s/init.tos\n' "$WORK" > "$OUT/manifest-$1.txt"
    printf '/system/registry/nameservice.tos\t%s/nameservice.tos\n' "$WORK" \
        >> "$OUT/manifest-$1.txt"
    printf '/system/service/claimant.tos\t%s/claimant.tos\n' "$WORK" \
        >> "$OUT/manifest-$1.txt"
    "$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
        --out "$OUT/$1.bin" --meta "$OUT/$1.meta.json" "$OUT/manifest-$1.txt" > /dev/null
}

boot() {
    # $1 = "denied" | "granted"; forbids differ, because a refused startup is the
    # expected outcome of one run and a defect in the other.
    local forbid="TOS.EXCEPTION TOS.PANIC TOS.RUN.TRAP"
    [ "$1" = granted ] && forbid="$forbid TOS.RUN.REFUSED"
    mkdir -p "$OUT/$1"
    bash "$HERE/run.sh" \
        --out "$OUT/$1" \
        --capsule "$OUT/$1.bin" \
        --nucleus "$TARGET/x86_64-unknown-none/release/tos-nucleus" \
        --stage4-block-device \
        --expect 33 \
        --require "TOS.NUCLEUS.ENTRY TOS.RUN.COMPLETED TOS.HALT" \
        --forbid "$forbid" \
        > /dev/null
}

# --- run one: the authority is withheld ----------------------------------------
prepare denied
boot denied
LOG="$OUT/denied/events.log"
count() { grep -c "$1" "$LOG" || true; }

grep -q '^TOS\.RUN\.BEGIN .* modules=3$' "$LOG" ||
    fail "the denied boot did not run a set of three modules"

# **The refusal, and what it names.** Before the claimant's first instruction, so
# nothing it contains had a chance to decide anything.
[ "$(count "$DENIAL")" = 1 ] ||
    fail "the claimant was not refused by name and interface: $(grep REFUSED "$LOG" || echo 'no refusal at all')"

# And it is the *only* thing refused: a boot in which something else also failed
# would be a boot proving something else.
[ "$(count '^TOS\.RUN\.REFUSED ')" = 1 ] ||
    fail "more than one module was refused in the denied boot"

# **Nothing was published**, said by the registry rather than by an absence: it
# holds `receive` on the publication endpoint, it waited, and the liveness rule
# ended the wait.
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_REGISTRY_EMPTY\$")" = 1 ] ||
    fail "the registry did not report an empty wait: $(grep COMPLETED "$LOG")"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_REGISTRY_SERVED\$")" = 0 ] ||
    fail "the registry served a registration in the denied boot"

# The launcher did its whole job, so the denial is not a launcher that gave up.
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_INIT\$")" = 1 ] ||
    fail "the launcher did not create both children and release its own name: $(grep COMPLETED "$LOG")"

# The claimant ran no instruction, so it reported nothing.
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_CLAIMANT\$")" = 0 ] ||
    fail "something reported the claimant's success value in the denied boot"

# --- run two: the same everything, plus the capability -------------------------
prepare granted
boot granted
LOG="$OUT/granted/events.log"

grep -q '^TOS\.RUN\.BEGIN .* modules=3$' "$LOG" ||
    fail "the granted boot did not run a set of three modules"

# **No refusal at all**, which is the mutation landing: the same module, the same
# binding, the same registry, one capability more.
[ "$(count '^TOS\.RUN\.REFUSED ')" = 0 ] ||
    fail "the claimant was still refused after being endowed the publication endpoint"

# It published, and the registry received.
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_CLAIMANT\$")" = 1 ] ||
    fail "the endowed claimant did not report a successful registration: $(grep COMPLETED "$LOG")"
[ "$(count '^TOS\.RUN\.INTERFACE operation=endpoint_call_carrying status=0$')" = 1 ] ||
    fail "the endowed claimant's publication call did not succeed"
[ "$(count "^TOS\\.RUN\\.COMPLETED value=$EXPECTED_REGISTRY_EMPTY\$")" = 0 ] ||
    fail "the registry still reported an empty wait after a registration arrived"

echo "PUBLICATION-AUTHORITY PASS: possession of the publication endpoint is the authority"
echo "  two boots of one capsule differing in exactly one endowment"
echo "  denied: the claimant requests the publication endpoint under the binding"
echo "  an authorised publisher uses, nothing answers it, and it is refused"
echo "  before its first instruction —"
echo "    TOS.RUN.REFUSED stage=execute reason=capability-denied"
echo "    binding=publish interface=system.ipc.Endpoint"
echo "  and the registry, which holds \`receive\` on that endpoint, reports that"
echo "  its wait ended with nothing to take, so nothing was published"
echo "  granted: the same module, the same source, the same binding, the same"
echo "  registry, plus \`call\` on that endpoint — it starts, publishes, and the"
echo "  registry receives. No refusal anywhere in the boot"
echo "  so the denial was the missing capability and nothing else: not a string"
echo "  comparison, not a source convention, not a registry policy branch"
echo "  CAPABILITY_V1 §6 as ADR-0095 amended it, and ADR-0095 §5.2 and §5.3"
echo "  NOT claimed: the positive path, which is name-service.sh and"
echo "  block-service.sh; a second published interface, which ADR-0095 §6"
echo "  leaves undecided; or anything about the device"
