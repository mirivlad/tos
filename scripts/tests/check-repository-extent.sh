#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The repository extent's writer and its independent checker, held against each
# other and against ordinary Git.
#
# **This gate tests the gate's own tooling** (`preflight.sh`'s `selftest`
# profile), which is why it runs without QEMU and without a nucleus. ADR-0102
# §10b puts three obligations on the host tool, and a tool that met none of them
# would still produce a green boot — the canonical reader would simply read
# whatever it was handed. So they are asserted here:
#
#   deterministic       the same commit twice produces the same bytes
#   independent         a second reader that shares the format's constants and
#                       none of the writer's encoding reaches the same verdict
#   git-readable        ordinary `git cat-file` reads every stream it emits
#
# And the **refusal vocabulary** (§10a) is exercised end to end on the host:
# nineteen deliberately broken extents, each refused with the class ADR-0102
# §11c names for it. That is not the conformance evidence — canonical TOS Core
# reading a real device is — but it is what makes the QEMU gate's assertions
# about a *second* implementation's agreement rather than about one.
#
#   bash scripts/tests/check-repository-extent.sh [OUT_DIR]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TOOLS="$ROOT/source/host-tools/repository-extent"
OUT="${1:-$ROOT/source/target/selftest-repository-extent}"
CAPSULE_TOOL="$ROOT/source/target/release/tos-capsule-tool"

fail() {
    echo "repository-extent-selftest: FAIL: $*" >&2
    exit 1
}

rm -rf "$OUT"
mkdir -p "$OUT"
[ -x "$CAPSULE_TOOL" ] || (cd "$ROOT/source" && cargo build --release -p tos-capsule-tool)

# **The fixture repository is an alternates view of the real one**, never a copy
# and never the real object store written into. Every object the mutations write
# lands here and the repository under test is untouched; every object of the
# real history is readable through the alternate.
FIXTURE="$OUT/repo"
git init -q "$FIXTURE"
printf '%s\n' "$ROOT/.git/objects" > "$FIXTURE/.git/objects/info/alternates"
# **From the commit, not from the working tree.** The object mutations build
# their root commit on `HEAD`, and the capsule tool verifies every manifest file
# against `commit:<path>`. Taking this one from the working tree made the gate
# fail whenever the tree was dirty — which says nothing about the tool and
# everything about when the gate happened to run.
git -C "$ROOT" show "HEAD:README.md" > "$FIXTURE/README.md"
printf '/system/boot/init.tos\tREADME.md\n' > "$FIXTURE/off-path.txt"

HEAD_COMMIT="$(git -C "$ROOT" rev-parse HEAD)"

# --- the ordinary case ---------------------------------------------------------
printf '/system/boot/init.tos\tsource/system/boot/init.tos\n' > "$OUT/manifest.txt"
(cd "$ROOT" && "$CAPSULE_TOOL" --git-commit "$HEAD_COMMIT" \
    --licence "$ROOT/source/system/boot/NOTICES.txt" \
    --out "$OUT/capsule.bin" "$OUT/manifest.txt") > /dev/null

python3 "$TOOLS/provision.py" --capsule "$OUT/capsule.bin" --git-dir "$ROOT/.git" \
    --out "$OUT/extent.img"
python3 "$TOOLS/provision.py" --capsule "$OUT/capsule.bin" --git-dir "$ROOT/.git" \
    --out "$OUT/extent-again.img"
cmp -s "$OUT/extent.img" "$OUT/extent-again.img" ||
    fail "the same commit provisioned twice produced two different extents"

python3 "$TOOLS/verify.py" --extent "$OUT/extent.img" --capsule "$OUT/capsule.bin" \
    --git-dir "$ROOT/.git" --git-loose-check "$OUT/loose.git" ||
    fail "the independent checker does not agree that this extent is the commit's"

# **The capsule is the authority, and this proves it is being read.** A tool that
# quietly used `HEAD` would pass every check above; what catches it is a capsule
# naming an older commit, whose extent must then be that older commit's.
OLDER="$(git -C "$ROOT" rev-parse 'HEAD^{commit}^')"
(cd "$ROOT" && "$CAPSULE_TOOL" --git-commit "$OLDER" \
    --licence "$ROOT/source/system/boot/NOTICES.txt" \
    --out "$OUT/older.bin" "$OUT/manifest.txt") > /dev/null 2>&1 || true
if [ -f "$OUT/older.bin" ]; then
    python3 "$TOOLS/provision.py" --capsule "$OUT/older.bin" --git-dir "$ROOT/.git" \
        --out "$OUT/older.img" > /dev/null
    cmp -s "$OUT/extent.img" "$OUT/older.img" &&
        fail "two different commits provisioned the same bytes; the capsule is not being read"
    python3 "$TOOLS/verify.py" --extent "$OUT/older.img" --capsule "$OUT/older.bin" \
        --git-dir "$ROOT/.git" > /dev/null ||
        fail "the older commit's extent is not that commit's"
    python3 "$TOOLS/verify.py" --extent "$OUT/extent.img" --capsule "$OUT/older.bin" \
        --expect REPO_MISSING > /dev/null ||
        fail "an extent of the wrong commit was not refused as REPO_MISSING"
    checked=1
else
    # `source/system/boot/init.tos` did not exist, or differed, at that commit.
    echo "repository-extent-selftest: the previous commit does not carry this landing path;"
    echo "  the capsule-is-the-authority case is NOT MEASURED on this history"
    checked=0
fi

# --- every refusal §11c names, each its own case -------------------------------
extent_case() {
    python3 "$TOOLS/provision.py" --capsule "$OUT/capsule.bin" --git-dir "$ROOT/.git" \
        --out "$OUT/mutant.img" --mutate "$1" > /dev/null ||
        fail "provisioning the $1 mutation failed"
    python3 "$TOOLS/verify.py" --extent "$OUT/mutant.img" --capsule "$OUT/capsule.bin" \
        --expect "$2" > /dev/null || fail "the $1 mutation was not refused with $2"
    cases=$((cases + 1))
}

object_case() {
    root="$(python3 "$TOOLS/provision.py" --root "$HEAD_COMMIT" --git-dir "$FIXTURE/.git" \
        --out "$OUT/mutant.img" --mutate "$1" --print-root)" ||
        fail "provisioning the $1 mutation failed"
    (cd "$FIXTURE" && "$CAPSULE_TOOL" --git-commit "$root" --out "$OUT/mutant.bin" \
        off-path.txt) > /dev/null ||
        fail "no capsule could be built naming the $1 mutation's root commit"
    python3 "$TOOLS/verify.py" --extent "$OUT/mutant.img" --capsule "$OUT/mutant.bin" \
        --expect "$2" > /dev/null || fail "the $1 mutation was not refused with $2"
    cases=$((cases + 1))
}

cases=0
extent_case object-byte       REPO_OID
extent_case zlib-header       REPO_FORMAT
extent_case len-nlen          REPO_FORMAT
extent_case adler             REPO_FORMAT
extent_case trailing          REPO_FORMAT
extent_case padding           REPO_FORMAT
extent_case duplicate-oid     REPO_FORMAT
extent_case unsorted          REPO_FORMAT
extent_case object-count      REPO_BOUNDS
extent_case object-length     REPO_BOUNDS
extent_case missing-blob      REPO_MISSING
extent_case dynamic-huffman   REPO_UNSUPPORTED
extent_case oid-algorithm     REPO_UNSUPPORTED
extent_case other-commit      REPO_MISSING
object_case git-object-header      REPO_FORMAT
object_case tree-entry-truncated   REPO_FORMAT
object_case duplicate-component    REPO_FORMAT
object_case wrong-kind             REPO_KIND
object_case missing-component      REPO_MISSING

# **A detached capsule names no commit at all**, and linkage is refused rather
# than attempted: `source_identity_kind` is the first thing §9 step 3 depends on.
(cd "$ROOT" && "$CAPSULE_TOOL" --detached --licence "$ROOT/source/system/boot/NOTICES.txt" \
    --out "$OUT/detached.bin" "$OUT/manifest.txt") > /dev/null
python3 "$TOOLS/verify.py" --extent "$OUT/extent.img" --capsule "$OUT/detached.bin" \
    --expect REPO_UNSUPPORTED > /dev/null ||
    fail "a detached capsule was not refused as having no repository identity"
cases=$((cases + 1))

rm -rf "$OUT/loose.git" "$OUT/repo"
echo "repository-extent-selftest: PASS ($cases refusal case(s), deterministic bytes," \
     "ordinary git reads every stream, capsule-is-the-authority checked=$checked)"
