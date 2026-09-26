#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# The boot text this machine is running, found in the repository its capsule names.
#
# **ADR-0102.** Stage 4 owes the first half of the capsule-to-repository handoff: not a
# repository-backed `/system`, not refs, not writes — the *linkage*, which is the
# statement that the canonical text this machine booted is the
# `source/system/boot/init.tos` of the commit the boot chain verified. Every layer of it
# is canonical text: a block service speaking the accepted `block.device.v1`, and above
# it a reader holding SHA-1, SHA-256, a bounded stored-block inflater, the minimum Git
# parser and the extent format — and holding no part of the machine.
#
# **The fixture's launcher is the landing blob, and that is the whole design.** The
# capsule's `/system/boot/init.tos` is what the nucleus digests into
# `boot_content_sha256`, and the reader compares that digest against the repository's
# blob at `source/system/boot/init.tos`. So the commit this boot names is one whose
# `source/system/boot/init.tos` **is** this fixture's supervisor: a real Git commit,
# with the real repository's history behind it and its real sibling trees around it,
# built by the harness in an alternates view of the repository so that nothing is
# written into the repository under test. It is the owner-controlled path to boot
# modified source, exercised: the machine boots a modified `init.tos`, and the
# repository is required to contain exactly that modification at the commit named.
#
# **What each boot proves.**
#
# *The ordinary boot*, against the reference image with the extent written at sector 65:
#
#   block service        created first, ends last; the only holder of the bus
#   reader A             capacity, header, table, the commit, four trees, the blob,
#                        every object against the id it was located by, and
#                        SHA-256(blob) == boot_content_sha256
#   A ends and is collected
#   reader B             the same module from the same sealed plan, re-reading the
#                        same device, reaching the same witness
#
# *The refusal boots*, one per negative ADR-0102 §11c names, each differing from the
# ordinary boot in exactly one fact and each refused with the exact §10a class.
#
# **Claims this gate is built around.**
#
# *The reader starts from the capsule and not from a constant.* The commit id and the
# boot-content digest reach it through `system.boot.Identity` and operation 32, which
# report what the boot chain already validated. `other-commit` is the witness that the
# reading happens: an extent of a well-formed *different* commit is refused
# `REPO_MISSING`, because the id the reader was given is simply not in its table.
#
# *The extent is established before it is read* (§6a1). The too-small boot asks
# `CAPACITY`, is answered below 2113, and issues **no repository-sector READ at all** —
# proved from the block service's own counted requests, not from the reader's word.
#
# *Repository reading can never address a sector below 65 or at or above 2113* (§6b),
# and the counted sector addresses of every boot say so.
#
# *The reader holds no part of the machine.* The bus is requested by the supervisor and
# the block service and nobody else; `platform.pci.FunctionConfig`, `platform.irq.Source`
# and `platform.dma.Region` are requested by the block service alone.
#
# *A refusal is a verdict and a fault is not.* The reader's account is
# `class + 16 * proved`: the §10a class in the low four bits and what was established on
# the way to it above them. A **negative** account is an incomplete lower operation and
# never a repository class (§11e) — and no boot here produces one.
#
# **Not claimed:** cross-reboot persistence — the harness re-creates the disk image for
# every invocation, so no gate in this repository has ever shown a byte surviving a
# reboot. Nor refs, writes, packfiles, history traversal, a repository-backed `/system`,
# a present system commit id, or Stage 4 closure.
#
#   bash host-tools/qemu-test/repository-linkage.sh [OUT_DIR]
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
GITROOT="$(cd "$ROOT/.." && pwd)"
OUT="${1:-$ROOT/target/qemu-repository-linkage}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

FIXTURE="$ROOT/tests/vectors/repository-linkage"
TOOLS="$GITROOT/source/host-tools/repository-extent"
TOOL="$ROOT/target/release/tos-capsule-tool"
PRODUCTION="$ROOT/target/x86_64-unknown-none/release/tos-nucleus"
TARGET="$ROOT/target/test-repository-linkage"

REPOSITORY_FIRST_SECTOR=65

# --- the accounts, one distinct value each -------------------------------------
# The supervisor: eight children in six phases, every ending collected.
EXPECTED_SUPERVISOR="i64:511"
# ADR-0099's own accounts, unchanged, because ADR-0099's own modules are what run.
# The initializer formats sector 0; store generation A opens and serves the
# writer's two objects; the writer's three shape refusals and two `PUT`s; then —
# after every repository read of two reader generations — a **new** store
# generation opens the header from the device and the reader `GET`s object 2 and
# verifies all 512 bytes.
EXPECTED_FORMATTED="i64:15"
EXPECTED_STATE_A="i64:4544"
EXPECTED_WRITER="i64:126976"
EXPECTED_STATE_B="i64:14016"
EXPECTED_STORE_READER="i64:16646144"
# The reader, twice: every proof bit set and the class `REPO_OK`.
#   1 identity read        16 table valid        128 blob verified
#   2 identity is git      32 commit verified    256 linkage
#   4 capacity fits        64 trees verified
#   8 header valid
PROVED_ALL=511
EXPECTED_READER="i64:$((16 * PROVED_ALL))"
# The block service: Stage 4D's twenty device-side facts, the configuration found, the
# capacity stable under §2.5.1's generation protocol, and capacity and reads served.
# Stage 4D's twenty device-side facts, the configuration found and the capacity
# stable under §2.5.1's generation protocol; then `CAPACITY`, `READ` and `WRITE`
# served, and the liveness rule ending its receive once every client is gone.
#
# **The one write in this boot is the state store's header** — nothing on the
# repository path writes anything, and the reader holds no authority that could.
# The bit is here because the store's initializer is here, and it is the same bit
# that would appear if a repository reader had written, which is why the counted
# requests below say how many of each there were rather than only that some were.
BLOCK_DEVICE_ALL=1048575
DEVICE_CFG_FOUND=$((1 << 20))
CAPACITY_STABLE=$((1 << 21))
SERVED_CAPACITY=$((1 << 22))
SERVED_WRITE=$((1 << 23))
SERVED_READ=$((1 << 24))
CLIENTS_GONE=$((1 << 32))
EXPECTED_BLOCK="i64:$((BLOCK_DEVICE_ALL + DEVICE_CFG_FOUND + CAPACITY_STABLE + SERVED_CAPACITY \
    + SERVED_WRITE + SERVED_READ + CLIENTS_GONE))"

fail() {
    echo "repository-linkage: FAIL: $*" >&2
    exit 1
}

[ -x "$TOOL" ] || (cd "$ROOT" && cargo build --release -p tos-capsule-tool)

# Every test nucleus is built into a target directory of its own and the
# production digest is compared before and after. A gate that quietly rebuilt the
# shipped artifact would be evidence about a different nucleus.
before=""
[ -f "$PRODUCTION" ] && before="$(sha256sum "$PRODUCTION" | awk '{print $1}')"
nucleus_for() {
    feature="$1"
    target="$ROOT/target/$feature"
    (cd "$ROOT" && CARGO_TARGET_DIR="$target" cargo build --release \
        -p tos-nucleus --target x86_64-unknown-none --features "$feature" >/dev/null 2>&1) ||
        fail "the nucleus does not build with $feature"
    if [ -n "$before" ]; then
        [ "$before" = "$(sha256sum "$PRODUCTION" | awk '{print $1}')" ] ||
            fail "the production nucleus changed while building $feature"
    fi
    echo "$target/x86_64-unknown-none/release/tos-nucleus"
}
NUCLEUS="$(nucleus_for test-repository-linkage)"

# --- the fixture repository ----------------------------------------------------
#
# **An alternates view, never a copy and never the repository written into.** Every
# object the harness creates lands here; every object of the real history is readable
# through the alternate. `git gc` on the repository under test cannot see these and
# nothing here can reach it.
WORK="$OUT/repo"
rm -rf "$WORK"
git init -q "$WORK"
printf '%s\n' "$GITROOT/.git/objects" > "$WORK/.git/objects/info/alternates"
export GIT_AUTHOR_NAME="TOS fixture" GIT_AUTHOR_EMAIL="fixture@tos.invalid"
export GIT_COMMITTER_NAME="TOS fixture" GIT_COMMITTER_EMAIL="fixture@tos.invalid"
export GIT_AUTHOR_DATE="1000000000 +0000" GIT_COMMITTER_DATE="1000000000 +0000"

HEAD_COMMIT="$(git -C "$GITROOT" rev-parse HEAD)"

# **The depth guard, made reachable without widening the product.**
#
# `source/system/boot/init.tos` is four components deep and `MAX_TREE_DEPTH` is
# sixteen, so the production traversal cannot exceed its own bound — and a
# runtime-selectable path added to manufacture a depth negative would be surface
# invented for a test. Instead the bound is lowered *in a conformance copy of the
# reader* to two, so the same fixed traversal runs past it, and the gate proves
# the copy differs from the committed reader in exactly that one line. The
# production interface, the production reader and the production path are
# unchanged.
sed 's/^const MAX_TREE_DEPTH: size = 16B;$/const MAX_TREE_DEPTH: size = 2B;/' \
    "$FIXTURE/reader.tos" > "$OUT/reader-depth.tos"
differences="$(diff "$FIXTURE/reader.tos" "$OUT/reader-depth.tos" | grep -c '^[<>]' || true)"
[ "$differences" = 2 ] ||
    fail "the depth-limited reader differs from the committed one in $differences line(s);
       it must differ in exactly one, which diff reports as one removed and one added"
grep -q '^const MAX_TREE_DEPTH: size = 2B;$' "$OUT/reader-depth.tos" ||
    fail "the depth-limited reader does not carry the lowered bound"

# The three files this boot runs, at the repository paths they are committed to — and
# the launcher at the landing path, which is what makes the positive reachable.
place() {
    mkdir -p "$WORK/$(dirname "$1")"
    cp "$2" "$WORK/$1"
}
place source/system/boot/init.tos "$FIXTURE/init.tos"
# The launcher at its *own* repository path as well, which is what the refusal
# capsules take their boot module from: a capsule whose boot text came from
# somewhere other than the landing path is exactly what `REPO_LINKAGE` is about,
# and every object mutation needs one too because ordinary Git cannot walk to the
# landing path of a commit whose trees are deliberately malformed.
place source/tests/vectors/repository-linkage/init.tos "$FIXTURE/init.tos"
# **The accepted `state.store.v1` implementation, unchanged and not a copy of it.**
# ADR-0102 §11c requires that a `GET` of an object the store holds still succeeds
# after the repository reads, in the same boot — so what runs has to be ADR-0099's
# own initializer, store, writer and reader, and their accounts have to be the ones
# `state-store.sh` already asserts.
mkdir -p "$WORK/source/tests/vectors/state-store"
for accepted in initializer state writer reader; do
    git -C "$GITROOT" show "HEAD:source/tests/vectors/state-store/$accepted.tos" \
        > "$WORK/source/tests/vectors/state-store/$accepted.tos"
done
place source/tests/vectors/repository-linkage/identity.tos "$FIXTURE/identity.tos"
place source/tests/vectors/repository-linkage/denied.tos "$FIXTURE/denied.tos"
place source/tests/vectors/repository-linkage/carry.tos "$FIXTURE/carry.tos"
place source/tests/vectors/repository-linkage/block.tos "$FIXTURE/block.tos"
place source/tests/vectors/repository-linkage/reader.tos "$FIXTURE/reader.tos"

synthesise() {
    # $1 = the landing blob's source file; the other two are always the fixture's.
    export GIT_INDEX_FILE="$OUT/index"
    rm -f "$GIT_INDEX_FILE"
    git -C "$WORK" read-tree "$HEAD_COMMIT"
    for pair in "source/system/boot/init.tos:$1" \
                "source/tests/vectors/repository-linkage/init.tos:$FIXTURE/init.tos" \
                "source/tests/vectors/repository-linkage/identity.tos:$FIXTURE/identity.tos" \
                "source/tests/vectors/repository-linkage/denied.tos:$FIXTURE/denied.tos" \
                "source/tests/vectors/repository-linkage/carry.tos:$FIXTURE/carry.tos" \
                "source/tests/vectors/repository-linkage/reader-depth.tos:$OUT/reader-depth.tos" \
                "source/tests/vectors/repository-linkage/block.tos:$FIXTURE/block.tos" \
                "source/tests/vectors/repository-linkage/reader.tos:$FIXTURE/reader.tos"; do
        blob="$(git -C "$WORK" hash-object -w "${pair#*:}")"
        git -C "$WORK" update-index --add --cacheinfo "100644,$blob,${pair%%:*}"
    done
    tree="$(git -C "$WORK" write-tree)"
    unset GIT_INDEX_FILE
    git -C "$WORK" commit-tree "$tree" -p "$HEAD_COMMIT" -m "repository-linkage fixture"
}

# The ordinary boot's commit: the landing blob **is** the supervisor this boot runs.
LINKED="$(synthesise "$FIXTURE/init.tos")"
# And a second commit whose landing blob is the repository's own canonical boot module,
# which this boot does not run. Traversal succeeds, every object verifies, and the
# SHA-256 comparison is the one thing that fails: `REPO_LINKAGE` (§11c).
UNLINKED="$(synthesise "$GITROOT/source/system/boot/init.tos")"

manifest() {
    printf '/system/boot/init.tos\t%s\n' "$1" > "$WORK/manifest.txt"
    printf '/system/service/block.tos\tsource/tests/vectors/repository-linkage/block.tos\n' \
        >> "$WORK/manifest.txt"
    printf '/system/repository/reader.tos\tsource/tests/vectors/repository-linkage/reader.tos\n' \
        >> "$WORK/manifest.txt"
    printf '/system/state/initializer.tos\tsource/tests/vectors/state-store/initializer.tos\n' \
        >> "$WORK/manifest.txt"
    printf '/system/state/store.tos\tsource/tests/vectors/state-store/state.tos\n' \
        >> "$WORK/manifest.txt"
    printf '/system/state/writer.tos\tsource/tests/vectors/state-store/writer.tos\n' \
        >> "$WORK/manifest.txt"
    printf '/system/state/reader.tos\tsource/tests/vectors/state-store/reader.tos\n' \
        >> "$WORK/manifest.txt"
}

build_capsule() {
    # $1 = commit, $2 = the repository path the boot module is taken from,
    # $3 = output, $4 = the file that commit carries at the landing path.
    #
    # **The worktree is put back in step with the commit first.** The capsule
    # tool verifies every manifest file against `commit:<path>`, and these boots
    # deliberately use commits that differ at the landing path — which is the
    # whole of what `REPO_LINKAGE` is about. A worktree left holding the previous
    # boot's landing blob would make the tool refuse, and rightly.
    cp "${4:-$FIXTURE/init.tos}" "$WORK/source/system/boot/init.tos"
    manifest "$2"
    (cd "$WORK" && "$TOOL" --git-commit "$1" \
        --licence "$ROOT/system/boot/NOTICES.txt" --out "$3" manifest.txt) > /dev/null
}

build_capsule "$LINKED" source/system/boot/init.tos "$OUT/capsule.bin"

provisioned="$(python3 "$TOOLS/provision.py" --capsule "$OUT/capsule.bin" \
    --git-dir "$WORK/.git" --out "$OUT/extent.img")"
echo "$provisioned"
# **The device requests are derived from the extent, not chosen.** One `CAPACITY`
# and then one `READ` per sector the extent actually uses — the header, the three
# table sectors, and the sectors the object chain occupies — which is the number
# the provisioner reports. A count written here by hand would be a number that
# stops being true the next time a commit message changes length.
EXTENT_SECTORS="$(printf '%s' "$provisioned" | sed -n 's/.*, \([0-9]*\) of [0-9]* sector(s).*/\1/p')"
[ -n "$EXTENT_SECTORS" ] || fail "the provisioner did not report how much of the extent it used"
READER_REQUESTS=$((1 + EXTENT_SECTORS))
# The state-store sequence, derived from `STATE_STORE_V1`'s protocol exactly as
# `state-store.sh` derives it:
#
#   initializer   capacity, READ 0, WRITE 0                              3
#   store A       capacity and READ 0 to open; then object 1 and object 2
#                 at a payload sector and a header each                  6
#   store B       capacity and READ 0 to open, and READ 2                3
STORE_BLOCK_REQUESTS=12
# And what the store's own clients ask *it*, which reaches the journal through the
# same `endpoint_reply_word` row because a reply is a reply whoever sends it: five
# per store generation, as `state-store.sh` derives them — the writer's three shape
# refusals and two `PUT`s, and the reader's three refusals, one absent id and one
# `GET`.
STORE_CLIENT_REQUESTS=10
EXPECTED_REQUESTS=$((2 * READER_REQUESTS + STORE_BLOCK_REQUESTS + STORE_CLIENT_REQUESTS))
# **The independent checker runs over every extent this gate hands to QEMU.** A negative
# the canonical reader refuses with a class two independent readers agreed on is a
# statement about the contract; one only the reader refuses is a statement about the
# reader.
python3 "$TOOLS/verify.py" --extent "$OUT/extent.img" --capsule "$OUT/capsule.bin" \
    --git-dir "$WORK/.git" > /dev/null ||
    fail "the independent checker does not agree the ordinary extent is the capsule's"

bash "$HERE/run.sh" \
    --out "$OUT/live" \
    --capsule "$OUT/capsule.bin" \
    --nucleus "$NUCLEUS" \
    --stage4-block-device \
    --stage4-block-overlay "$REPOSITORY_FIRST_SECTOR:$OUT/extent.img" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.PCI_ROOT TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
    > /dev/null

LOG="$OUT/live/events.log"
count() { grep -c "$1" "$LOG" || true; }

# --- 0a: the journal this gate reads is the journal the machine wrote ----------
#
# `run.sh` keeps the lines of `serial.log` that *begin* with `TOS.`, so anything
# glued to the front of one is dropped whole — and a gate then asserts over a
# journal with holes in it while every count it makes still looks plausible. That
# is not hypothetical: the nucleus used to stream frames belonging to other
# processes onto the serial line, because it read a report region as one physical
# run that had been mapped frame by frame, and what it cost was a whole reader
# generation's account.
#
# So the filtered log is held against the raw one: every `TOS.` in the transport
# must be at the start of its line. A gate that could not tell a missing record
# from a record that never happened is a gate that reports the second when it
# means the first.
python3 - "$OUT/live/serial.log" <<'INTACT' || fail "the boot journal reached the log with holes in it"
import re
import sys

raw = open(sys.argv[1], "rb").read().decode("utf-8", errors="replace")
raw = re.sub(r"\x1b\[[0-9;=?]*[A-Za-z]", "", raw).replace("\r", "")
glued = [line for line in raw.split("\n") if "TOS." in line and not line.startswith("TOS.")]
if glued:
    print(f"{len(glued)} journal line(s) did not reach the start of a line; first:",
          file=sys.stderr)
    print("  " + repr(glued[0][:160]), file=sys.stderr)
    raise SystemExit(1)
print(f"{sum(1 for line in raw.split(chr(10)) if line.startswith('TOS.'))} journal "
      f"record(s), every one of them whole")
INTACT
completed=$(grep '^TOS\.RUN\.COMPLETED value=' "$LOG" | sed 's/^TOS\.RUN\.COMPLETED value=//')

# --- 0: every module reported a success account --------------------------------
for value in $completed; do
    case "${value#i64:}" in
        -*) fail "a module reported the failure $value; the boot reported:
       $(printf '%s ' $completed)" ;;
    esac
done

# --- 1: the linkage, twice, from two processes ---------------------------------
readers="$(count "^TOS\.RUN\.COMPLETED value=$EXPECTED_READER\$")"
[ "$readers" = 2 ] ||
    fail "no repository reader proved the linkage; the boot reported:
       $(printf '%s ' $completed)"

# --- 1b: §11d's restart shape, in order ----------------------------------------
#
# **A second generation is a claim about sequence, so the sequence is what is
# asserted.** Not two accounts somewhere in the log: generation A reaching the
# witness, ending, and being collected — and only then B being created, reading
# the same device for itself, reaching the same witness, exiting normally and
# being collected. Two matching accounts with no order between them would be
# satisfied by a boot that ran them side by side, which is a different claim and
# one `IPC_V1` §2 does not even permit here.
python3 - "$LOG" "$EXPECTED_READER" <<'RESTART' || fail "the second reader generation is not ADR-0102 §11d's restart shape"
import sys

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
witness = f"TOS.RUN.COMPLETED value={sys.argv[2]}"

# The steps, in the order §11d requires them, each matched against the rest of
# the journal after the one before it. A step that matched earlier text would be
# a step this boot did not take in this order.
steps = [
    ("A reaches the linkage witness", lambda line: line == witness),
    ("A exits normally", lambda line: line.startswith("TOS.RUN.PROCESS_EXIT ")
        and "self_reported_status=0" in line),
    ("A is collected", lambda line: line == "TOS.RUN.INTERFACE operation=process_wait_child status=0"),
    # **B's beginning and not B's creation**, because which of the two reaches
    # the log first is a scheduling detail: a child can run before its creator's
    # syscall returns and drains, and this check reported a broken restart shape
    # on the boots where it did. That five children were created is counted
    # below; what this step is about is that B *ran* after A was collected.
    ("B begins over the same module, after A was collected", lambda line:
        line.startswith("TOS.RUN.BEGIN path=system/repository/reader.tos ")),
    ("B reads the boot identity for itself", lambda line:
        line == "TOS.RUN.INTERFACE operation=boot_identity_read status=0"),
    ("B reads the device for itself", lambda line:
        line == "TOS.RUN.INTERFACE operation=endpoint_call_word_carrying status=0"),
    ("B reaches the same linkage witness", lambda line: line == witness),
    ("B exits normally", lambda line: line.startswith("TOS.RUN.PROCESS_EXIT ")
        and "self_reported_status=0" in line),
    ("B is collected", lambda line: line == "TOS.RUN.INTERFACE operation=process_wait_child status=0"),
]

at = 0
reached = []
for name, matches in steps:
    while at < len(events) and not matches(events[at]):
        at += 1
    if at == len(events):
        print(f"the journal does not reach: {name}", file=sys.stderr)
        print("  reached, in order: " + "; ".join(reached), file=sys.stderr)
        raise SystemExit(1)
    reached.append(f"{name} @{at}")
    at += 1
print("both reader generations reached the witness, in §11d's order")
RESTART

[ "$(count "^TOS\.RUN\.COMPLETED value=$EXPECTED_SUPERVISOR\$")" = 1 ] ||
    fail "the supervisor did not complete its three phases"
[ "$(count "^TOS\.RUN\.COMPLETED value=$EXPECTED_BLOCK\$")" = 1 ] ||
    fail "the block service did not report the Stage 4D device facts and the reads it
       served; it reported: $(printf '%s ' $completed)"

# --- 2: three modules, four processes, nothing stalled -------------------------
grep -q '^TOS\.RUN\.BEGIN .* modules=7$' "$LOG" ||
    fail "the boot did not run a set of seven modules"
[ "$(count '^TOS\.RUN\.BEGIN path=')" = 9 ] ||
    fail "nine processes did not begin: $(count '^TOS\.RUN\.BEGIN path=') did"
# **Exactly one census, and it is the one that ends the boot.** When the only
# things left are the supervisor waiting on its child relation and the block
# service waiting for a message, nothing can satisfy either and `SYSTEM_ABI_V1`
# §6's liveness rule ends both — which is how this boot finishes. A *second*
# census would be a stall somebody did not plan for, so the number is asserted
# rather than the absence.
stalls="$(count '^TOS\.RUN\.LIVENESS .*verdict=stalled')"
[ "$stalls" = 1 ] ||
    fail "$stalls liveness censuses declared a stall; exactly one may — the one that ends
       the block service's receive once every client is collected"
[ "$(count '^TOS\.RUN\.INTERFACE operation=process_create_funded status=0')" = 8 ] ||
    fail "eight children were not created"
[ "$(count '^TOS\.RUN\.INTERFACE operation=process_wait_child status=0$')" = 8 ] ||
    fail "eight endings were not collected"

# --- 2a: a real object survives the repository reads ---------------------------
#
# ADR-0102 §11c's last row, in the shape the row asks for. A 512-byte object with a
# distinctive witness is `PUT` by a writer that then ends; the store generation that
# served it ends and is collected; every repository read of two reader generations
# goes past on the same device through the same block service; and then a **new**
# store generation re-reads the header from the device and a client `GET`s that
# object and verifies every byte.
#
# **The accounts are ADR-0099's own**, because the modules are: the store's
# isolation boundary, its refusal surface and its byte witness are already accepted
# and are not re-derived here. What this boot adds is what happens between the `PUT`
# and the `GET`.
for account in "$EXPECTED_FORMATTED" "$EXPECTED_STATE_A" "$EXPECTED_WRITER" \
               "$EXPECTED_STATE_B" "$EXPECTED_STORE_READER"; do
    [ "$(count "^TOS\.RUN\.COMPLETED value=$account\$")" = 1 ] ||
        fail "the state-store sequence did not report $account exactly once; the boot
       reported: $(printf '%s ' $completed)
       ADR-0102 §11c requires a GET of an object the store holds to still succeed
       after the repository reads, in the same boot"
done
echo "repository-linkage: a 512-byte object PUT before the repository reads was" \
     "GET back and verified byte by byte after them, by a new store generation"

# --- 2b: exactly the requests the extent implies, and not one more --------------
#
# **Counted on the answers, because a reply is the service's own account of a
# request it completed** — and because the journal of this boot demonstrably loses
# lines (see §11d's note above), while a lost line can only make a count too small
# and never too large. Two generations of one reader against one extent imply
# exactly `2 * (1 CAPACITY + one READ per sector the extent uses)`, and that is the
# number, derived from what the provisioner wrote rather than written here.
answered="$(count '^TOS\.RUN\.INTERFACE operation=endpoint_reply_word status=0$')"
[ "$answered" = "$EXPECTED_REQUESTS" ] ||
    fail "the block service answered $answered request(s) and this extent implies
       $EXPECTED_REQUESTS: one CAPACITY and $EXTENT_SECTORS sector read(s) per reader
       generation — the header, the table and the sectors this extent's object chain
       occupies — plus $STORE_BLOCK_REQUESTS device request(s) and
       $STORE_CLIENT_REQUESTS client request(s) for the state-store sequence"
received="$(count '^TOS\.RUN\.INTERFACE operation=endpoint_receive_call_region status=0$')"
[ "$received" -le "$answered" ] ||
    fail "the block service answered $answered request(s) and received $received; a
       service cannot answer fewer requests than it took"
echo "repository-linkage: $answered device request(s) answered: two generation(s)" \
     "of $READER_REQUESTS, and $STORE_BLOCK_REQUESTS + $STORE_CLIENT_REQUESTS for" \
     "the state-store sequence"

# --- 2c: the endowment accounting, counted by the launcher and the nucleus ------
#
# **Not claimed by a module.** `MAX_ENDOWMENT` is four, and the reader's plan draws
# exactly four: a budget, the block service's endpoint, an inbox of its own and the
# boot identity. The supervisor is endowed by the launcher and is not a plan.
python3 - "$LOG" <<'ENDOWMENT' || fail "the endowment accounting is not the one ADR-0102 §11b requires"
import sys
from collections import Counter

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
PREFIX = "TOS.RUN.PROCESS_ENDOWED "

sizes = Counter()
for line in events:
    if not line.startswith(PREFIX):
        continue
    for field in line.split(" "):
        if field.startswith("capabilities="):
            sizes[int(field.removeprefix("capabilities="))] += 1

#   8   the supervisor: four endpoints, the bus, the boot identity, its own
#       control and the root remainder
#   4   each repository reader generation, each state-store generation and the
#       initializer that starts from the store's plan — which is MAX_ENDOWMENT
#   3   the block service, the writer and the store's reader
expected = {8: 1, 4: 5, 3: 3}
if dict(sizes) != expected:
    print(f"expected endowment sizes {expected}, saw {dict(sizes)}", file=sys.stderr)
    raise SystemExit(1)
ENDOWMENT

# --- 3: the reader read the boot identity, twice, and nobody else did ----------
[ "$(count '^TOS\.RUN\.INTERFACE operation=boot_identity_read status=0$')" = 2 ] ||
    fail "the boot identity was not read exactly twice — once per reader generation"

# --- 4: the capability topology, which is the reader's isolation boundary ------
python3 - "$LOG" <<'TOPOLOGY' || fail "the boot's capability topology is not the one this slice requires"
import sys
from collections import Counter

events = [line.rstrip("\r\n") for line in open(sys.argv[1], encoding="utf-8", errors="replace")]
PREFIX = "TOS.RUN.REQUEST binding="

seen = Counter()
for line in events:
    if not line.startswith(PREFIX):
        continue
    rest = line[len(PREFIX):].split(" ")
    seen[(rest[0], rest[1].removeprefix("interface="))] += 1

#   the supervisor's own, each once: it is the only process the launcher endows
#   budget     every child
#   serve      the block service
#   block      block.device.v1's client endpoint: the two reader generations
#   inbox      the two reader generations
#   identity   system.boot.Identity: the two reader generations, and nothing else
#   device     the platform bus: the supervisor and the block service
expected = {
    ("process", "system.process.Control"): 1,
    ("memory", "system.memory.Authority"): 1,
    ("block_serve_full", "system.ipc.Endpoint"): 1,
    ("state_serve_full", "system.ipc.Endpoint"): 1,
    ("state_inbox_full", "system.ipc.Endpoint"): 1,
    ("client_inbox_full", "system.ipc.Endpoint"): 1,
    ("boot_identity_full", "system.boot.Identity"): 1,
    # every child
    ("budget", "system.memory.Authority"): 8,
    # the block service, and each store generation
    ("serve", "system.ipc.Endpoint"): 3,
    # block.device.v1's client endpoint: the initializer, both store generations
    # and both repository readers — and nothing else in this boot
    ("block", "system.ipc.Endpoint"): 5,
    # the initializer, both store generations, both repository readers, the
    # writer and the store's reader
    ("inbox", "system.ipc.Endpoint"): 7,
    # state.store.v1's client endpoint: clients only
    ("store", "system.ipc.Endpoint"): 2,
    ("identity", "system.boot.Identity"): 2,
    ("device", "platform.pci.Bus"): 2,
}
if dict(seen) != expected:
    missing = {k: v for k, v in expected.items() if seen.get(k) != v}
    extra = {k: v for k, v in seen.items() if expected.get(k) != v}
    print(f"expected {expected}", file=sys.stderr)
    print(f"wrong: {missing}; unexpected: {extra}", file=sys.stderr)
    raise SystemExit(1)

# **No process reached any other part of the machine** (§11b.7). The dictionary
# above already refuses an unexpected pairing; these are named because they are
# the claim — a reader that mapped a window, an interrupt or a DMA region of its
# own would show here, and the bus above is requested by the supervisor and the
# block service and by nobody else.
for never in ("platform.pci.FunctionConfig", "platform.irq.Source",
              "platform.dma.Region", "platform.mmio.Region"):
    if any(f"interface={never} " in line for line in events if line.startswith(PREFIX)):
        print(f"{never} was requested by name, and only the block service reaches "
              f"the device in this boot", file=sys.stderr)
        raise SystemExit(1)
TOPOLOGY

# --- 5: every address was inside the extent, proved by the device (§6b) --------
#
# **The device is the witness, not the reader.** The block service's journal
# carries no per-request sector, so "no address was below 65 or at or above 2113"
# cannot be read out of the ordinary boot's log. What can be done instead is to
# take the bound away from the reader and give it to the machine: the same capsule
# and the same extent, on a device of **exactly** 2113 sectors — the smallest the
# layout admits. A read at or past 2113 is then refused by the device itself and
# the reader reports `REPO_BLOCK`; the boot reaching the same witness is the
# statement that it never asked for one.
#
# The lower bound is structural rather than measured: every address this reader
# forms is `REPOSITORY_FIRST_SECTOR + index` with `index` refused at or past
# `REPOSITORY_SECTORS`, so there is no expression in it that can name a sector
# below 65. That is stated here because it is a reading of the source, and a gate
# should say which of its claims came from the machine and which did not.
bash "$HERE/run.sh" \
    --out "$OUT/minimum" \
    --capsule "$OUT/capsule.bin" \
    --nucleus "$NUCLEUS" \
    --stage4-block-device \
    --stage4-block-sectors 2113 \
    --stage4-block-overlay "$REPOSITORY_FIRST_SECTOR:$OUT/extent.img" \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
    > /dev/null || fail "the minimum-device boot did not reach a clean halt"
grep -aq "^TOS\.RUN\.COMPLETED value=$EXPECTED_READER\$" "$OUT/minimum/events.log" ||
    fail "on a device of exactly 2113 sectors the reader did not reach the witness; an
       address at or past the extent's end would have been refused by the device
       (ADR-0102 §6b), and the accounts were:
       $(grep -a '^TOS\.RUN\.COMPLETED value=' "$OUT/minimum/events.log" | tr '\n' ' ')"
rm -rf "$OUT/minimum"

echo "repository-linkage: the ordinary boot holds"

# --- every refusal ADR-0102 §11c names, each its own boot ----------------------
#
# **A boot each, because a class is a claim about what the canonical reader did.**
# The host checker agrees with each of these over the same bytes (the selftest
# profile's `check-repository-extent` runs all of them), but what §11c asks for is
# the *reader's* verdict, and a reader's verdict comes from a machine that ran it.
#
# The reader's account is `class + 16 * proved`. What is asserted here is the
# class — the vocabulary §10a fixed — and that the account is **positive**, which
# is the other half of §11e: a negative would be an incomplete lower operation
# reported as one, and none of these is that.
refusals=0
refusal_boot() {
    # $1 = name, $2 = expected class, $3 = capsule, $4 = extent (or "" for none),
    # $5... = extra run.sh arguments
    name=$1; want=$2; capsule=$3; extent=$4; shift 4
    overlay=()
    [ -n "$extent" ] && overlay=(--stage4-block-overlay "$REPOSITORY_FIRST_SECTOR:$extent")
    bash "$HERE/run.sh" \
        --out "$OUT/refusal" \
        --capsule "$capsule" \
        --nucleus "$NUCLEUS" \
        --stage4-block-device \
        ${overlay+"${overlay[@]}"} \
        "$@" \
        --expect 33 \
        --require "TOS.NUCLEUS.ENTRY TOS.RUN.COMPLETED TOS.HALT" \
        --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP" \
        > /dev/null || fail "the $name boot did not reach a clean halt"
    # **The reader's verdict is the account two processes reported.** Both reader
    # generations meet the same extent and refuse identically, and nothing else in
    # the boot reports the same number twice — the block service, the supervisor
    # and each of the store's two initializer runs report one value each. Picking
    # "the first positive account" instead would pick whichever process happened
    # to finish first, which is a different thing that is usually the same.
    account="$(grep -a '^TOS\.RUN\.COMPLETED value=' "$OUT/refusal/events.log" |
        sed 's/^TOS\.RUN\.COMPLETED value=i64://' |
        awk '$1 > 0 { seen[$1]++ } END { for (v in seen) if (seen[v] == 2) print v }')"
    case "$account" in
        "" | *[!0-9]*)
            fail "the $name boot produced no single repository verdict two reader
       generations agreed on; it reported:
       $(grep -a '^TOS\.RUN\.COMPLETED value=' "$OUT/refusal/events.log" | tr '\n' ' ')" ;;
    esac
    got=$((account % 16))
    [ "$got" = "$want" ] ||
        fail "the $name boot was refused with class $got and ADR-0102 §11c names $want
       (the reader's account was $account)"
    refusals=$((refusals + 1))
    printf 'repository-linkage: %-24s refused with class %s\n' "$name" "$got"
}

# **The extent must be shown to exist before it is read** (§6a1). A device one
# sector short of the layout: the reader asks `CAPACITY`, is answered below 2113,
# and issues no repository-sector READ at all.
refusal_boot capacity 2 "$OUT/capsule.bin" "" --stage4-block-sectors 2112
python3 - "$OUT/refusal/events.log" "$STORE_BLOCK_REQUESTS" "$STORE_CLIENT_REQUESTS" <<'NOREAD' || fail "a repository sector was read on a device too small to hold the extent"
import sys

events = open(sys.argv[1], encoding="utf-8", errors="replace").read().splitlines()
store_block, store_client = int(sys.argv[2]), int(sys.argv[3])

# **Counted as an exact total, because an extra read cannot hide inside one.** Every
# request either service completed is one `endpoint_reply_word`, and on this boot the
# whole of what may be asked is the state-store sequence plus one `CAPACITY` per
# repository reader: §6a1 requires the extent's existence to be established before a
# repository sector is read, and a device of 2112 sectors cannot hold it. One
# repository-sector READ would make this number 25 rather than 24.
#
# Attributing carrying calls instead would not do: a state-store client uses one to
# take the region its `GET` is answered with, so a carrying call is no longer a
# repository read by its shape alone.
answered = sum(1 for line in events
               if line == "TOS.RUN.INTERFACE operation=endpoint_reply_word status=0")
want = store_block + store_client + 2
if answered != want:
    print(f"{answered} request(s) were answered and this boot implies {want}: the "
          f"state-store sequence and one CAPACITY per reader", file=sys.stderr)
    raise SystemExit(1)
print(f"no repository-sector read was issued: {answered} request(s), exactly the "
      f"state store's and one CAPACITY per reader")
NOREAD

# **The correct commit and the wrong boot blob** (§11c). The capsule's boot module
# is taken from a repository path that is not the landing path, so traversal
# succeeds, every object verifies against its own id, and the SHA-256 comparison is
# the one thing that fails.
build_capsule "$UNLINKED" source/tests/vectors/repository-linkage/init.tos \
    "$OUT/unlinked.bin" "$GITROOT/source/system/boot/init.tos"
python3 "$TOOLS/provision.py" --capsule "$OUT/unlinked.bin" --git-dir "$WORK/.git" \
    --out "$OUT/unlinked.img" > /dev/null
python3 "$TOOLS/verify.py" --extent "$OUT/unlinked.img" --capsule "$OUT/unlinked.bin" \
    --expect REPO_LINKAGE > /dev/null ||
    fail "the independent checker does not read REPO_LINKAGE over the unlinked extent"
refusal_boot linkage 6 "$OUT/unlinked.bin" "$OUT/unlinked.img"

# **A detached capsule names no commit at all**, so there is nothing to link to and
# the linkage is refused rather than attempted.
cp "$FIXTURE/init.tos" "$WORK/source/system/boot/init.tos"
manifest source/system/boot/init.tos
(cd "$WORK" && "$TOOL" --detached --licence "$ROOT/system/boot/NOTICES.txt" \
    --out "$OUT/detached.bin" manifest.txt) > /dev/null
refusal_boot detached 7 "$OUT/detached.bin" "$OUT/extent.img"

# The extent and entry mutations, against the ordinary capsule.
extent_boot() {
    python3 "$TOOLS/provision.py" --capsule "$OUT/capsule.bin" --git-dir "$WORK/.git" \
        --out "$OUT/mutant.img" --mutate "$1" > /dev/null ||
        fail "provisioning the $1 mutation failed"
    python3 "$TOOLS/verify.py" --extent "$OUT/mutant.img" --capsule "$OUT/capsule.bin" \
        --expect "$3" > /dev/null ||
        fail "the independent checker does not read $3 over the $1 mutation"
    refusal_boot "$1" "$2" "$OUT/capsule.bin" "$OUT/mutant.img"
}

extent_boot object-byte      5 REPO_OID
extent_boot zlib-header      1 REPO_FORMAT
extent_boot len-nlen         1 REPO_FORMAT
extent_boot adler            1 REPO_FORMAT
extent_boot trailing         1 REPO_FORMAT
extent_boot padding          1 REPO_FORMAT
extent_boot duplicate-oid    1 REPO_FORMAT
extent_boot unsorted         1 REPO_FORMAT
extent_boot object-count     2 REPO_BOUNDS
extent_boot object-length    2 REPO_BOUNDS
extent_boot missing-blob     3 REPO_MISSING
extent_boot dynamic-huffman  7 REPO_UNSUPPORTED
extent_boot oid-algorithm    7 REPO_UNSUPPORTED
extent_boot other-commit     3 REPO_MISSING

# The object mutations, each of which produces a **new root commit** — so each has
# a capsule of its own, whose boot module comes from a path off the mutated one
# because ordinary Git could not otherwise build it (§11c, and the tool's own
# header says why).
object_boot() {
    root="$(python3 "$TOOLS/provision.py" --root "$LINKED" --git-dir "$WORK/.git" \
        --out "$OUT/mutant.img" --mutate "$1" --print-root)" ||
        fail "provisioning the $1 mutation failed"
    build_capsule "$root" source/tests/vectors/repository-linkage/init.tos "$OUT/mutant.bin"
    python3 "$TOOLS/verify.py" --extent "$OUT/mutant.img" --capsule "$OUT/mutant.bin" \
        --expect "$3" > /dev/null ||
        fail "the independent checker does not read $3 over the $1 mutation"
    refusal_boot "$1" "$2" "$OUT/mutant.bin" "$OUT/mutant.img"
}

object_boot git-object-header    1 REPO_FORMAT
object_boot tree-entry-truncated 1 REPO_FORMAT
object_boot duplicate-component  1 REPO_FORMAT
object_boot wrong-kind           4 REPO_KIND
object_boot missing-component    3 REPO_MISSING

# --- ADR-0102 §11a: the identity as a capability, not as a source of facts -----
#
# Three rules that a reader holding exactly the right authority never meets, and
# each is its own boot of one process — so there is nothing else in the boot that
# a result could have come from.

# **Denial**: the same operation and the same interface under a binding nothing
# answers. The launcher endows the identity as `boot_identity_full`; this module
# asks for it as `identity`, and `docs/42` §2 has the request asked once, before
# the first instruction, with a denial that is neither an empty authority nor a
# zero handle. The run is refused at startup with the binding named.
DENIED="$(synthesise "$FIXTURE/denied.tos")"
build_capsule "$DENIED" source/system/boot/init.tos "$OUT/denied.bin" "$FIXTURE/denied.tos"
bash "$HERE/run.sh" \
    --out "$OUT/identity" \
    --capsule "$OUT/denied.bin" \
    --nucleus "$NUCLEUS" \
    --stage4-block-device \
    --expect 75 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REFUSED TOS.BOOTMODULE.FAIL" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.COMPLETED TOS.RUN.INTERFACE TOS.RUN.REQUEST" \
    > /dev/null || fail "a process denied system.boot.Identity was not refused at startup"
grep -aq 'reason=capability-denied binding=identity interface=system.boot.Identity' \
    "$OUT/identity/events.log" ||
    fail "the refusal does not name the binding and the interface it was denied for:
       $(grep -a 'TOS.RUN.REFUSED' "$OUT/identity/events.log" | tr '\n' ' ')"
echo "repository-linkage: a process not endowed system.boot.Identity cannot read it —"
echo "  refused before its first instruction, with the binding named"

# **Attenuation**: intersection and not validation, and an empty intersection is
# a refusal rather than a rightless handle.
IDENTITY="$(synthesise "$FIXTURE/identity.tos")"
build_capsule "$IDENTITY" source/system/boot/init.tos "$OUT/identity.bin" "$FIXTURE/identity.tos"
#   1 read   2 shape   4 not empty   8 widening intersects
#   16 the narrowed name reads   32 the empty intersection refuses
EXPECTED_IDENTITY="i64:63"
bash "$HERE/run.sh" \
    --out "$OUT/identity" \
    --capsule "$OUT/identity.bin" \
    --nucleus "$NUCLEUS" \
    --stage4-block-device \
    --expect 33 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.COMPLETED TOS.HALT" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.UNSTARTABLE TOS.RUN.TRAP TOS.RUN.REFUSED" \
    > /dev/null || fail "the identity attenuation boot did not reach a clean halt"
grep -aq "^TOS\.RUN\.COMPLETED value=$EXPECTED_IDENTITY\$" "$OUT/identity/events.log" ||
    fail "the identity probe did not prove all six of its rules; it reported:
       $(grep -a '^TOS.RUN.COMPLETED value=' "$OUT/identity/events.log" | tr '\n' ' ')"
[ "$(grep -ac '^TOS\.RUN\.INTERFACE operation=capability_attenuate status=-1$' \
    "$OUT/identity/events.log")" = 1 ] ||
    fail "exactly one attenuation must be refused: the one whose intersection is empty"
echo "repository-linkage: attenuation of system.boot.Identity is intersection —"
echo "  asking for read|send yields read and still reads; asking for send alone is refused"

# **Canonical text cannot put the identity in a message** (§3e). ADR-0102's
# accepted authority path is bootstrap holder -> launch-plan endowment -> reader,
# and generic capability transfer over IPC is not part of the slice. The reason it
# is unreachable is structural: every carrying row of `SYSTEM_INTERFACE_V1` §4 is
# nominally typed for `system.ipc.Endpoint` in the carried position. So the
# negative is a module that declares the row correctly, holds a real identity and a
# real endpoint, and passes the identity where an endpoint is required — and the
# checker refuses it before the boot runs an instruction.
#
# The nucleus refuses it too, and that is audited rather than asserted here:
# `resolve_transfers` is the only generic message-transfer path, it is called from
# one site, and `Object::travels_in_a_message` answers no for this kind. That
# branch is a backstop for an artifact that did not come through the checker, so no
# boot of canonical text can reach it.
CARRY="$(synthesise "$FIXTURE/carry.tos")"
build_capsule "$CARRY" source/system/boot/init.tos "$OUT/carry.bin" "$FIXTURE/carry.tos"
bash "$HERE/run.sh" \
    --out "$OUT/identity" \
    --capsule "$OUT/carry.bin" \
    --nucleus "$NUCLEUS" \
    --stage4-block-device \
    --expect 75 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.REFUSED TOS.BOOTMODULE.FAIL" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.COMPLETED TOS.RUN.INTERFACE TOS.RUN.REQUEST" \
    > /dev/null || fail "a module carrying system.boot.Identity in a message was not refused"
grep -aq 'E1215_ARGUMENT_TYPE_MISMATCH .*callee=endpoint_send_carrying .*expected=system\.ipc\.Endpoint actual=system\.boot\.Identity' \
    "$OUT/identity/events.log" ||
    fail "the refusal does not name the carried position, the type it requires and the
       type that was offered:
       $(grep -a 'TOS.RUN.DIAGNOSTIC\|TOS.RUN.REFUSED' "$OUT/identity/events.log" | tr '\n' ' ')"
echo "repository-linkage: canonical text cannot carry system.boot.Identity in a"
echo "  message — the carried position is typed for system.ipc.Endpoint and the"
echo "  checker refuses it (E1215_ARGUMENT_TYPE_MISMATCH)"

# **Object kind 14 with a non-zero launch scope is refused** (§3a1). The same
# launcher constant with one field wrong, so the boot does not start.
bash "$HERE/run.sh" \
    --out "$OUT/identity" \
    --capsule "$OUT/identity.bin" \
    --nucleus "$(nucleus_for test-repository-linkage-scope)" \
    --stage4-block-device \
    --expect 71 \
    --require "TOS.NUCLEUS.ENTRY TOS.RUN.UNSTARTABLE TOS.MEM.FAIL" \
    --forbid "TOS.EXCEPTION TOS.PANIC TOS.RUN.COMPLETED TOS.RUN.BEGIN" \
    > /dev/null || fail "an endowment of kind 14 with a non-zero scope was not refused"
echo "repository-linkage: an endowment description of kind 14 with a non-zero scope"
echo "  is refused, so the boot does not start"

# --- ADR-0102 §8: the depth guard is live --------------------------------------
#
# The same fixed traversal against the same extent, read by a reader whose
# `MAX_TREE_DEPTH` is two. It runs past the bound at the third component and
# refuses `REPO_BOUNDS` — which is the guard doing its work rather than a
# condition the product cannot reach.
# The ordinary boot's commit and the ordinary launcher; only the reader the
# capsule carries at `/system/repository/reader.tos` is the depth-limited one.
cp "$FIXTURE/init.tos" "$WORK/source/system/boot/init.tos"
cp "$OUT/reader-depth.tos" "$WORK/source/tests/vectors/repository-linkage/reader-depth.tos"
manifest source/system/boot/init.tos
sed -i 's|repository-linkage/reader\.tos$|repository-linkage/reader-depth.tos|' \
    "$WORK/manifest.txt"
(cd "$WORK" && "$TOOL" --git-commit "$LINKED" \
    --licence "$ROOT/system/boot/NOTICES.txt" --out "$OUT/depth.bin" manifest.txt) > /dev/null
refusal_boot depth 2 "$OUT/depth.bin" "$OUT/extent.img"

rm -rf "$WORK" "$OUT/refusal" "$OUT/identity" "$OUT/mutant.img" "$OUT/mutant.bin" \
    "$OUT/reader-depth.tos" "$TARGET" "$ROOT/target/test-repository-linkage-scope"
echo "repository-linkage: PASS (the linkage proved from a real device by two" \
     "generations, $refusals refusal(s) each with the ADR-0102 §10a class §11c names" \
     "for it, the identity's denial, attenuation, scope and no-message rules, and a" \
     "512-byte state-store object read back after all of it)"
