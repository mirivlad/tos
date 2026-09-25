#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Write the deterministic repository extent ADR-0102 §7 decides.

**The capsule is the authority for which commit is provisioned** (§10a). This
tool takes the root object id out of the capsule it is pointed at and resolves
*that* commit; it never asks the host what `HEAD` is. A fixture that took its
root from `HEAD` would agree with the capsule by coincidence of timing and would
stop agreeing the moment anything was committed between the two steps — which is
the class of failure the linkage exists to detect.

It collects only the bounded object chain `source/system/boot/init.tos` needs,
encodes each object as a supported stored-block loose object (§5c), and writes
the extent whole. Ordinary Git must be able to read every stream it emits, which the independent
checker beside this file asserts against `git cat-file`.

`--mutate` builds a deliberately broken extent instead, for the negatives §11c
requires. Two kinds of break, and the difference is which layer is being tested:

  extent mutations   patch the written bytes, so the stream, the table or the
                     padding is wrong while the Git objects were fine
  object mutations   rewrite a Git object and rebuild every parent above it, so
                     every object id still recomputes and what the reader meets
                     is a *consistent* repository whose content is malformed.
                     The new root commit id is printed, and the capsule for that
                     boot must name it

    python3 provision.py --capsule C --git-dir D --out extent.img [--mutate NAME]
"""

import argparse
import hashlib
import os
import struct
import subprocess
import sys
import zlib

# The two modules beside this one are imported by path and never installed, so
# nothing should be left behind for having run it: a repository with no
# `.gitignore` is a repository where every file is either tracked or a mistake.
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import capsule_facts
from extent_format import (
    BLOB_MODES,
    MAX_OBJECT_UNCOMPRESSED_BYTES,
    DATA_START,
    ENTRY_SIZE,
    FORMAT_VERSION,
    LANDING_PATH,
    MAGIC,
    MAX_OBJECTS,
    OID_ALGORITHM_SHA1,
    OID_LENGTH_SHA1,
    REPOSITORY_SECTORS,
    SECTOR_BYTES,
    TABLE_SECTORS,
    TREE_MODE,
)

ENTRY_MUTATIONS = ("object-byte", "trailing", "missing-blob", "dynamic-huffman")
EXTENT_MUTATIONS = (
    "oid-algorithm",
    "zlib-header",
    "len-nlen",
    "adler",
    "padding",
    "duplicate-oid",
    "unsorted",
    "object-count",
    "object-length",
)
ROOT_MUTATIONS = ("other-commit",)
OBJECT_MUTATIONS = (
    "git-object-header",
    "tree-entry-truncated",
    "duplicate-component",
    "wrong-kind",
    "missing-component",
)
MUTATIONS = ENTRY_MUTATIONS + EXTENT_MUTATIONS + ROOT_MUTATIONS + OBJECT_MUTATIONS


class Refused(Exception):
    pass


# --- the host repository, read through ordinary Git ----------------------------


class Repository:
    """`git cat-file` and nothing cleverer. `docs/08` §External implementations
    permits ordinary Git as an oracle on the host, and a second object-store
    reader here would be a second thing to be wrong."""

    def __init__(self, git_dir: str):
        self.git_dir = git_dir

    def _git(self, args, stdin=None, binary=True):
        done = subprocess.run(
            ["git", "--git-dir", self.git_dir] + args,
            input=stdin,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if done.returncode != 0:
            raise Refused(
                f"git {' '.join(args)} failed: {done.stderr.decode(errors='replace').strip()}"
            )
        return done.stdout if binary else done.stdout.decode()

    def kind(self, oid: str) -> str:
        return self._git(["cat-file", "-t", oid], binary=False).strip()

    def payload(self, oid: str) -> bytes:
        return self._git(["cat-file", self.kind(oid), oid])

    def write_object(self, kind: str, payload: bytes) -> str:
        """Write arbitrary bytes as an object of `kind`, unvalidated.

        `--literally` is what makes the object mutations possible at all: a
        malformed tree is exactly what the reader must refuse, and Git will not
        write one otherwise."""
        return self._git(
            ["hash-object", "-w", "-t", kind, "--literally", "--stdin"],
            stdin=payload,
            binary=False,
        ).strip()

    def commit_tree(self, tree: str, parent: str, message: str) -> str:
        """A commit with fixed identity and time, so the artifact is
        reproducible: `tos-capsule-tool`'s standard, applied to the fixture."""
        fixed = {
            "GIT_AUTHOR_NAME": "TOS fixture",
            "GIT_AUTHOR_EMAIL": "fixture@tos.invalid",
            "GIT_AUTHOR_DATE": "1000000000 +0000",
            "GIT_COMMITTER_NAME": "TOS fixture",
            "GIT_COMMITTER_EMAIL": "fixture@tos.invalid",
            "GIT_COMMITTER_DATE": "1000000000 +0000",
        }
        done = subprocess.run(
            ["git", "--git-dir", self.git_dir, "commit-tree", tree, "-p", parent, "-m", message],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env={**os.environ, **fixed},
            check=False,
        )
        if done.returncode != 0:
            raise Refused(f"git commit-tree failed: {done.stderr.decode(errors='replace')}")
        return done.stdout.decode().strip()


# --- Git object encoding, §5c --------------------------------------------------


def envelope(kind: str, payload: bytes) -> bytes:
    """`"<kind> <size>\\0" || payload` — the bytes an object id is over."""
    return kind.encode() + b" " + str(len(payload)).encode() + b"\0" + payload


def object_id(kind: str, payload: bytes) -> str:
    return hashlib.sha1(envelope(kind, payload)).hexdigest()


def stored_stream(raw: bytes) -> bytes:
    """A zlib stream of DEFLATE **stored** blocks only, ending exactly where
    §5c says it ends: one `BFINAL` block, one Adler-32 trailer, nothing after.

    A stored block carries at most 65535 bytes, so this is a *sequence* — an
    encoder that emitted one block would produce a stream no conforming reader
    could read the moment an object grew past 64 KiB."""
    out = bytearray(b"\x78\x01")  # CM=8, CINFO=7, FLEVEL=0, FDICT=0, check ok
    assert (out[0] * 256 + out[1]) % 31 == 0
    chunks = [raw[at : at + 65535] for at in range(0, len(raw), 65535)] or [b""]
    for index, chunk in enumerate(chunks):
        final = 1 if index == len(chunks) - 1 else 0
        out.append(final)  # BTYPE = 00
        out += struct.pack("<HH", len(chunk), len(chunk) ^ 0xFFFF)
        out += chunk
    out += struct.pack(">I", zlib.adler32(raw))
    return bytes(out)


# --- the bounded chain ---------------------------------------------------------


def tree_entries(payload: bytes):
    """Ordinary SHA-1 binary tree entries. Total over arbitrary bytes: a
    malformed entry raises rather than being skipped, because §5d refuses the
    whole object."""
    at = 0
    while at < len(payload):
        space = payload.find(b" ", at)
        if space < 0:
            raise Refused("tree entry has no mode separator")
        nul = payload.find(b"\0", space)
        if nul < 0:
            raise Refused("tree entry has no name terminator")
        if nul + 1 + 20 > len(payload):
            raise Refused("tree entry is truncated")
        yield payload[at:space], payload[space + 1 : nul], payload[nul + 1 : nul + 21]
        at = nul + 21


def collect(repo: Repository, root_commit: str):
    """The commit, every tree of the landing path, and the blob — in that
    order. Nothing else is read: no parent is followed and no sibling subtree is
    entered (§9)."""
    if repo.kind(root_commit) != "commit":
        raise Refused(f"{root_commit} is not a commit")
    commit = repo.payload(root_commit)
    chain = [_object("commit", commit)]

    if not commit.startswith(b"tree "):
        raise Refused("the commit does not begin with its tree line")
    tree_oid = commit[5:45].decode()
    if commit[45:46] != b"\n":
        raise Refused("the commit's tree line is not 40 hex followed by a newline")

    for depth, component in enumerate(LANDING_PATH):
        payload = repo.payload(tree_oid)
        chain.append(_object("tree", payload))
        final = depth == len(LANDING_PATH) - 1
        matches = [e for e in tree_entries(payload) if e[1] == component.encode()]
        if not matches:
            raise Refused(f"{component} is not in tree {tree_oid}")
        if len(matches) > 1:
            raise Refused(f"{component} appears more than once in tree {tree_oid}")
        mode, _name, raw = matches[0]
        want = BLOB_MODES if final else (TREE_MODE,)
        if mode not in want:
            raise Refused(f"{component} has mode {mode!r}, which the traversal may not follow")
        tree_oid = raw.hex()

    if repo.kind(tree_oid) != "blob":
        raise Refused("the landing path does not name a blob")
    chain.append(_object("blob", repo.payload(tree_oid)))
    for item in chain:
        item["in_git"] = True
    return chain


# --- the extent ----------------------------------------------------------------


def ceil_sectors(length: int) -> int:
    return (length + SECTOR_BYTES - 1) // SECTOR_BYTES


def compose(objects) -> bytes:
    """`objects` is `[(oid_hex, stored_stream, uncompressed_length)]`. The
    extent is written whole and the placement is **derived** (§7c), so two
    objects cannot be written on top of each other."""
    if not 1 <= len(objects) <= MAX_OBJECTS:
        raise Refused(f"{len(objects)} objects does not fit the table")
    ordered = sorted(objects, key=lambda o: bytes.fromhex(o[0]))

    header = bytearray(SECTOR_BYTES)
    header[0:8] = MAGIC
    struct.pack_into(
        "<IIIIIIII",
        header,
        8,
        FORMAT_VERSION,
        OID_ALGORITHM_SHA1,
        OID_LENGTH_SHA1,
        len(ordered),
        ENTRY_SIZE,
        TABLE_SECTORS,
        DATA_START,
        REPOSITORY_SECTORS,
    )

    table = bytearray(TABLE_SECTORS * SECTOR_BYTES)
    data = bytearray()
    at = DATA_START
    for index, (oid_hex, stream, uncompressed) in enumerate(ordered):
        entry = index * ENTRY_SIZE
        table[entry : entry + OID_LENGTH_SHA1] = bytes.fromhex(oid_hex)
        struct.pack_into("<II", table, entry + 32, len(stream), uncompressed)
        data += stream
        data += bytes(ceil_sectors(len(stream)) * SECTOR_BYTES - len(stream))
        at += ceil_sectors(len(stream))
    if at > REPOSITORY_SECTORS:
        raise Refused(f"the chain needs {at} sectors and the extent holds {REPOSITORY_SECTORS}")

    extent = bytearray(REPOSITORY_SECTORS * SECTOR_BYTES)
    extent[0:SECTOR_BYTES] = header
    extent[SECTOR_BYTES : SECTOR_BYTES + len(table)] = table
    extent[DATA_START * SECTOR_BYTES : DATA_START * SECTOR_BYTES + len(data)] = data
    return bytes(extent)


def entry_at(extent: bytearray, index: int) -> int:
    return SECTOR_BYTES + index * ENTRY_SIZE


def placement(extent: bytes, index: int) -> tuple:
    """`(byte offset, stored_length)` of an object, derived exactly as §7c
    derives it — re-derived here rather than remembered, so a mutation that
    changes a `stored_length` moves everything after it the way the reader
    would see it move."""
    count = struct.unpack_from("<I", extent, 20)[0]
    if index >= count:
        raise Refused(f"object {index} is past object_count {count}")
    sector = DATA_START
    for before in range(index):
        sector += ceil_sectors(struct.unpack_from("<I", extent, entry_at(extent, before) + 32)[0])
    return sector * SECTOR_BYTES, struct.unpack_from(
        "<I", extent, entry_at(extent, index) + 32
    )[0]


def index_of(extent: bytes, oid_hex: str) -> int:
    count = struct.unpack_from("<I", extent, 20)[0]
    want = bytes.fromhex(oid_hex)
    for index in range(count):
        at = entry_at(extent, index)
        if extent[at : at + OID_LENGTH_SHA1] == want:
            return index
    raise Refused(f"{oid_hex} is not in the table")


def mutate_entries(entries, chain, name: str):
    """Breaks that change an object's stored length, applied **before** the
    extent is composed.

    §7c derives every later object's placement from the lengths before it, so a
    stream edited in place that changed its length would move every object after
    it without moving its bytes — which is a second, accidental break the
    negative was not about."""
    blob = chain[-1]["oid"]
    out = []
    for oid, stream, uncompressed in entries:
        if oid != blob:
            out.append((oid, stream, uncompressed))
            continue
        if name == "object-byte":
            # One payload byte changed **and the Adler-32 repaired**, so the
            # stream is well-formed and only the object id disagrees with the
            # bytes. A flip that left the trailer stale would be refused as a
            # malformed stream and would never reach the recomputation.
            raw = bytearray(zlib.decompress(stream))
            raw[-1] ^= 0x01
            out.append((oid, stored_stream(bytes(raw)), uncompressed))
        elif name == "trailing":
            # A complete, valid stream followed by more bytes **inside**
            # `stored_length` (§5c). Nothing about the stream itself is wrong.
            out.append((oid, stream + b"\x7f" * 4, uncompressed))
        elif name == "dynamic-huffman":
            # What ordinary `zlib.compress` produces: a stream this profile
            # refuses as unsupported rather than one it fails to parse.
            out.append((oid, zlib.compress(zlib.decompress(stream), 9), uncompressed))
        elif name == "missing-blob":
            # The object the traversal ends at is simply not there (§11c): the
            # final tree names an id the table does not hold.
            continue
        else:
            raise Refused(f"unknown entry mutation {name}")
    return out


def mutate_extent(extent: bytes, chain, name: str) -> bytes:
    """One named break in the **written** bytes, each the narrowest edit that
    makes the rule it tests the only thing wrong."""
    out = bytearray(extent)
    blob = index_of(out, chain[-1]["oid"])
    start, stored = placement(out, blob)

    if name == "oid-algorithm":
        # The extent declares SHA-256 object ids while the boot identity
        # declares SHA-1. §7a refuses that rather than reinterpreting it.
        struct.pack_into("<II", out, 12, 2, 32)
    elif name == "zlib-header":
        out[start + 1] |= 0x20  # FDICT = 1, and the check word no longer holds
    elif name == "len-nlen":
        out[start + 4] ^= 0xFF  # NLEN is no longer LEN's one's complement
    elif name == "adler":
        out[start + stored - 1] ^= 0x01
    elif name == "padding":
        if stored == ceil_sectors(stored) * SECTOR_BYTES:
            raise Refused("the object ends on a sector boundary and has no padding")
        out[start + stored] = 0x5A
    elif name == "duplicate-oid":
        first, second = entry_at(out, 0), entry_at(out, 1)
        out[second : second + OID_LENGTH_SHA1] = out[first : first + OID_LENGTH_SHA1]
    elif name == "unsorted":
        first, second = entry_at(out, 0), entry_at(out, 1)
        held = bytes(out[first : first + ENTRY_SIZE])
        out[first : first + ENTRY_SIZE] = out[second : second + ENTRY_SIZE]
        out[second : second + ENTRY_SIZE] = held
    elif name == "object-count":
        struct.pack_into("<I", out, 20, MAX_OBJECTS + 1)
    elif name == "object-length":
        struct.pack_into("<I", out, entry_at(out, blob) + 36, MAX_OBJECT_UNCOMPRESSED_BYTES + 1)
    else:
        raise Refused(f"unknown extent mutation {name}")
    return bytes(out)




# --- object mutations: a consistent repository whose content is malformed ------


def encode_tree(items) -> bytes:
    return b"".join(mode + b" " + name + b"\0" + raw for mode, name, raw in items)


def mutate_objects(repo: Repository, chain, name: str):
    """Rewrite one Git object and rebuild every parent above it.

    **Every object id still recomputes from its own bytes.** That is the only
    way to reach a syntax refusal at all: §9 verifies an object against the id it
    was located by *before* it parses it, so bytes that do not hash to the id
    they were reached through are `REPO_OID` and the parser is never asked. What
    a reader meets here is a repository that is internally consistent and whose
    content this profile refuses.

    The rebuilt root commit is a different commit, so the capsule for such a
    boot names *it* — and its boot module is sourced from a path off the mutated
    one, because otherwise ordinary Git could not build that capsule at all."""
    levels = [item["payload"] for item in chain]
    names = [None, None] + [component.encode() for component in LANDING_PATH]
    root_tree, source_tree, system_tree, boot_tree = 1, 2, 3, 4

    if name == "git-object-header":
        # The envelope declares one byte more than the payload has, and the
        # object id is over exactly those broken bytes (§5d).
        body = levels[boot_tree]
        broken = b"tree " + str(len(body) + 1).encode() + b"\0" + body
        return _rebuild(repo, chain, boot_tree, envelope_bytes=broken)
    if name == "tree-entry-truncated":
        return _rebuild(repo, chain, boot_tree, payload=levels[boot_tree][:-3])
    if name == "duplicate-component":
        items = list(tree_entries(levels[system_tree]))
        twice = [entry for entry in items if entry[1] == names[boot_tree]][0]
        return _rebuild(
            repo,
            chain,
            system_tree,
            payload=encode_tree(sorted(items + [twice], key=lambda entry: entry[1])),
        )
    if name == "wrong-kind":
        # An intermediate component carrying a blob mode. §5d requires the
        # canonical tree mode there and refuses anything else as `REPO_KIND`.
        items = [
            (b"100644", entry_name, raw) if entry_name == names[boot_tree] else (mode, entry_name, raw)
            for mode, entry_name, raw in tree_entries(levels[system_tree])
        ]
        return _rebuild(repo, chain, system_tree, payload=encode_tree(items))
    if name == "missing-component":
        items = [
            entry for entry in tree_entries(levels[system_tree]) if entry[1] != names[boot_tree]
        ]
        return _rebuild(repo, chain, system_tree, payload=encode_tree(items))
    raise Refused(f"unknown object mutation {name}")


def _rebuild(repo: Repository, chain, at: int, payload=None, envelope_bytes=None):
    """Replace `chain[at]` and rebuild every parent up to a new root commit.

    Everything rebuilt is written into the fixture object store, because the
    capsule tool resolves the commit through ordinary Git. The one exception is
    a deliberately broken *envelope*: Git writes the envelope itself and cannot
    be made to write a wrong one, so that object lives only in the extent — and
    nothing in the capsule build walks to it."""
    names = [None, None] + [component.encode() for component in LANDING_PATH]
    rebuilt = list(chain)

    if envelope_bytes is not None:
        rebuilt[at] = {
            "kind": chain[at]["kind"],
            "oid": hashlib.sha1(envelope_bytes).hexdigest(),
            "payload": None,
            "envelope": envelope_bytes,
            "in_git": False,
        }
    else:
        rebuilt[at] = _object("tree" if at else "commit", payload, repo, literal=True)

    for level in range(at - 1, 0, -1):
        items = [
            (mode, entry_name, bytes.fromhex(rebuilt[level + 1]["oid"]))
            if entry_name == names[level + 1]
            else (mode, entry_name, raw)
            for mode, entry_name, raw in tree_entries(chain[level]["payload"])
        ]
        rebuilt[level] = _object("tree", encode_tree(items), repo, literal=True)

    body = b"tree " + rebuilt[1]["oid"].encode() + chain[0]["payload"][45:]
    rebuilt[0] = _object("commit", body, repo, literal=True)
    return rebuilt, rebuilt[0]["oid"]


def _object(kind: str, payload: bytes, repo: Repository = None, literal: bool = False) -> dict:
    oid = object_id(kind, payload)
    if repo is not None:
        written = repo.write_object(kind, payload)
        if written != oid:
            raise Refused(f"git stored {kind} as {written} and this tool computed {oid}")
    return {
        "kind": kind,
        "oid": oid,
        "payload": payload,
        "envelope": envelope(kind, payload),
        "in_git": repo is not None,
    }


# --- the tool ------------------------------------------------------------------


def entries_for(chain):
    return [(item["oid"], stored_stream(item["envelope"]), len(item["envelope"])) for item in chain]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument(
        "--capsule", help="the capsule whose source object id names the commit to provision"
    )
    source.add_argument(
        "--root",
        help="an explicit root commit, permitted **only** with --mutate: an object mutation "
        "produces a new root, so the capsule for that boot is built after this tool rather "
        "than before it",
    )
    parser.add_argument("--git-dir", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--mutate", choices=MUTATIONS)
    parser.add_argument(
        "--print-root",
        action="store_true",
        help="print only the root commit id of the extent that was written",
    )
    args = parser.parse_args()

    if args.root and not args.mutate:
        parser.error("--root is for mutation construction only; use --capsule otherwise")

    repo = Repository(args.git_dir)

    if args.capsule:
        facts = capsule_facts.read(args.capsule)
        if facts.source_kind != capsule_facts.SRC_KIND_GIT:
            raise Refused("the capsule declares no git source identity")
        if (facts.oid_algorithm, facts.oid_length) != (
            capsule_facts.OID_ALG_SHA1,
            capsule_facts.OID_LEN_SHA1,
        ):
            raise Refused(
                f"the capsule declares oid algorithm {facts.oid_algorithm} length "
                f"{facts.oid_length}; this profile verifies SHA-1 (ADR-0102 §5b)"
            )
        root = facts.commit_hex
    else:
        root = repo._git(["rev-parse", f"{args.root}^{{commit}}"], binary=False).strip()

    if args.mutate == "other-commit":
        # A well-formed repository of a **different** commit than the capsule
        # names. Its object ids are simply not the ones the traversal starts
        # from, and the reader does not go looking for a substitute (§11c).
        root = repo._git(["rev-parse", f"{root}^^{{commit}}"], binary=False).strip()

    chain = collect(repo, root)
    if chain[0]["oid"] != root:
        raise Refused("the commit this tool hashed is not the commit it asked for")

    if args.mutate in OBJECT_MUTATIONS:
        chain, root = mutate_objects(repo, chain, args.mutate)

    entries = entries_for(chain)
    if args.mutate in ENTRY_MUTATIONS:
        entries = mutate_entries(entries, chain, args.mutate)
    extent = compose(entries)
    if args.mutate in EXTENT_MUTATIONS:
        extent = mutate_extent(extent, chain, args.mutate)

    with open(args.out, "wb") as handle:
        handle.write(extent)

    if args.print_root:
        print(root)
        return 0

    used = DATA_START + sum(ceil_sectors(len(stream)) for _oid, stream, _length in entries)
    print(
        f"repository-extent: {len(entries)} object(s), {used} of {REPOSITORY_SECTORS} sector(s), "
        f"root {root}" + (f", mutation {args.mutate}" if args.mutate else "")
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Refused as refusal:
        print(f"repository-extent: FAIL: {refusal}", file=sys.stderr)
        raise SystemExit(1)
