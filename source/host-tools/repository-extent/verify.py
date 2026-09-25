#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Read a repository extent the way ADR-0102 §9 says to, and say what happened.

**Independent of the writer** (§10b). It shares the constants of §6–§8, because
those are the format and a second copy of a number is a second number to drift;
it shares no encoder, no framing and no traversal. Where `provision.py` builds
stored blocks byte by byte, this reads them bit by bit; where that tool collects
a chain from `git cat-file`, this collects it from the extent and then asks Git
whether the answer was right.

It is also the host's statement of the refusal vocabulary (§10a): the gate runs
this over every extent it hands to QEMU, so a negative whose class the canonical
reader reports is a class two independent readers agreed on.

    python3 verify.py --extent E --capsule C --git-dir D [--expect CLASS]
"""

import argparse
import hashlib
import os
import struct
import subprocess
import sys
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import capsule_facts
from extent_format import (
    BLOB_MODES,
    DATA_START,
    ENTRY_SIZE,
    FORMAT_VERSION,
    LANDING_PATH,
    MAGIC,
    MAX_OBJECTS,
    MAX_OBJECT_UNCOMPRESSED_BYTES,
    MAX_PATH_BYTES,
    MAX_TREE_DEPTH,
    MAX_TREE_ENTRIES,
    OID_ALGORITHM_SHA1,
    OID_LENGTH_SHA1,
    REPOSITORY_SECTORS,
    REPO_BOUNDS,
    REPO_FORMAT,
    REPO_KIND,
    REPO_LINKAGE,
    REPO_MISSING,
    REPO_OID,
    REPO_OK,
    REPO_UNSUPPORTED,
    SECTOR_BYTES,
    TABLE_SECTORS,
    TREE_MODE,
)

CLASS_NAMES = {
    REPO_OK: "REPO_OK",
    REPO_FORMAT: "REPO_FORMAT",
    REPO_BOUNDS: "REPO_BOUNDS",
    REPO_MISSING: "REPO_MISSING",
    REPO_KIND: "REPO_KIND",
    REPO_OID: "REPO_OID",
    REPO_LINKAGE: "REPO_LINKAGE",
    REPO_UNSUPPORTED: "REPO_UNSUPPORTED",
}
BY_NAME = {name: value for value, name in CLASS_NAMES.items()}


class Refusal(Exception):
    def __init__(self, repo_class: int, why: str):
        super().__init__(f"{CLASS_NAMES[repo_class]}: {why}")
        self.repo_class = repo_class
        self.why = why


# --- §5c, read as bits ---------------------------------------------------------


class Bits:
    """A least-significant-bit-first reader over the stream, which is what
    DEFLATE is. Reading the framing this way rather than by matching the shape
    the writer emits is the independence §10b asks for."""

    def __init__(self, blob: bytes):
        self.blob = blob
        self.at = 0
        self.bit = 0

    def take(self, count: int) -> int:
        value = 0
        for index in range(count):
            if self.at >= len(self.blob):
                raise Refusal(REPO_FORMAT, "the stream ends inside a block header")
            got = (self.blob[self.at] >> self.bit) & 1
            value |= got << index
            self.bit += 1
            if self.bit == 8:
                self.bit = 0
                self.at += 1
        return value

    def align(self) -> None:
        if self.bit:
            self.bit = 0
            self.at += 1


def inflate_stored(stream: bytes, declared: int) -> bytes:
    """Every rule §5c states, in the order it states them, and the exact end of
    the stream it makes normative."""
    if len(stream) < 2 + 4:
        raise Refusal(REPO_FORMAT, "the stream is too short to hold a zlib header and trailer")
    cmf, flg = stream[0], stream[1]
    if cmf & 0x0F != 8:
        raise Refusal(REPO_FORMAT, f"CM is {cmf & 0x0F} and not 8")
    if (cmf >> 4) > 7:
        raise Refusal(REPO_FORMAT, "CINFO is past the largest window zlib defines")
    if (cmf * 256 + flg) % 31 != 0:
        raise Refusal(REPO_FORMAT, "the zlib header check word does not hold")
    if flg & 0x20:
        raise Refusal(REPO_FORMAT, "FDICT is set and a preset dictionary is refused")

    bits = Bits(stream)
    bits.at = 2
    out = bytearray()
    while True:
        final = bits.take(1)
        kind = bits.take(2)
        if kind != 0:
            raise Refusal(
                REPO_UNSUPPORTED if kind in (1, 2) else REPO_FORMAT,
                f"BTYPE {kind:02b} is not a stored block",
            )
        bits.align()
        if bits.at + 4 > len(stream):
            raise Refusal(REPO_FORMAT, "the stream ends inside a stored-block length")
        length, nlength = struct.unpack_from("<HH", stream, bits.at)
        if length ^ 0xFFFF != nlength:
            raise Refusal(REPO_FORMAT, "LEN and NLEN are not one's complements")
        bits.at += 4
        if bits.at + length > len(stream):
            raise Refusal(REPO_FORMAT, "a stored block runs past the stream")
        if len(out) + length > declared:
            raise Refusal(REPO_BOUNDS, "the inflated bytes pass the declared length")
        out += stream[bits.at : bits.at + length]
        bits.at += length
        if final:
            break
    if bits.at + 4 != len(stream):
        raise Refusal(
            REPO_FORMAT,
            f"the stream has {len(stream) - bits.at - 4} byte(s) after its Adler-32 trailer",
        )
    (trailer,) = struct.unpack_from(">I", stream, bits.at)
    if trailer != zlib.adler32(bytes(out)):
        raise Refusal(REPO_FORMAT, "the Adler-32 trailer disagrees with the inflated bytes")
    if len(out) != declared:
        raise Refusal(REPO_FORMAT, "the inflated length is not the declared length")
    return bytes(out)


# --- §7, read as a format ------------------------------------------------------


def ceil_sectors(length: int) -> int:
    return -(-length // SECTOR_BYTES)


def read_header(extent: bytes, identity) -> dict:
    if len(extent) < REPOSITORY_SECTORS * SECTOR_BYTES:
        raise Refusal(REPO_BOUNDS, "the extent image is smaller than the reserved extent")
    if extent[0:8] != MAGIC:
        raise Refusal(REPO_FORMAT, "the extent magic is not TOSGITR1")
    fields = struct.unpack_from("<IIIIIIII", extent, 8)
    header = dict(
        zip(
            (
                "format_version",
                "oid_algorithm",
                "oid_length",
                "object_count",
                "entry_size",
                "table_sectors",
                "data_start",
                "extent_sectors",
            ),
            fields,
        )
    )
    if any(extent[40:SECTOR_BYTES]):
        raise Refusal(REPO_FORMAT, "a reserved header byte is not zero")
    if header["format_version"] != FORMAT_VERSION:
        raise Refusal(REPO_FORMAT, f"format_version {header['format_version']} is not 1")
    if header["oid_algorithm"] != OID_ALGORITHM_SHA1 or header["oid_length"] != OID_LENGTH_SHA1:
        raise Refusal(REPO_UNSUPPORTED, "the extent declares an object-id algorithm this profile does not verify")
    if (header["oid_algorithm"], header["oid_length"]) != (
        identity.oid_algorithm,
        identity.oid_length,
    ):
        raise Refusal(REPO_UNSUPPORTED, "the extent and the boot identity disagree about the object-id algorithm")
    if not 1 <= header["object_count"] <= MAX_OBJECTS:
        raise Refusal(REPO_BOUNDS, f"object_count {header['object_count']} is outside [1, {MAX_OBJECTS}]")
    if (
        header["entry_size"] != ENTRY_SIZE
        or header["table_sectors"] != TABLE_SECTORS
        or header["data_start"] != DATA_START
        or header["extent_sectors"] != REPOSITORY_SECTORS
    ):
        raise Refusal(REPO_FORMAT, "the extent layout is not the one this profile reads")
    return header


def read_table(extent: bytes, header: dict) -> list:
    table = []
    previous = None
    for index in range(MAX_OBJECTS):
        at = SECTOR_BYTES + index * ENTRY_SIZE
        entry = extent[at : at + ENTRY_SIZE]
        if index >= header["object_count"]:
            if any(entry):
                raise Refusal(REPO_FORMAT, f"table entry {index} is past object_count and not zero")
            continue
        oid = entry[:OID_LENGTH_SHA1]
        if any(entry[OID_LENGTH_SHA1:32]):
            raise Refusal(REPO_FORMAT, f"table entry {index} pads its object id with non-zero bytes")
        if any(entry[40:48]):
            raise Refusal(REPO_FORMAT, f"table entry {index} has a non-zero reserved run")
        stored, uncompressed = struct.unpack_from("<II", entry, 32)
        if not any(oid):
            raise Refusal(REPO_FORMAT, f"table entry {index} has an all-zero object id")
        if previous is not None and oid <= previous:
            raise Refusal(REPO_FORMAT, f"table entry {index} is not strictly after entry {index - 1}")
        previous = oid
        if stored == 0:
            raise Refusal(REPO_FORMAT, f"table entry {index} has a stored length of zero")
        if uncompressed > MAX_OBJECT_UNCOMPRESSED_BYTES:
            raise Refusal(REPO_BOUNDS, f"table entry {index} declares {uncompressed} uncompressed bytes")
        table.append({"oid": oid, "stored": stored, "uncompressed": uncompressed})

    sector = header["data_start"]
    for entry in table:
        entry["first_sector"] = sector
        sector += ceil_sectors(entry["stored"])
        if sector > header["extent_sectors"]:
            raise Refusal(REPO_BOUNDS, "the derived placement runs past the extent")
    return table


def object_bytes(extent: bytes, entry: dict) -> bytes:
    at = entry["first_sector"] * SECTOR_BYTES
    span = ceil_sectors(entry["stored"]) * SECTOR_BYTES
    stream = extent[at : at + entry["stored"]]
    padding = extent[at + entry["stored"] : at + span]
    if any(padding):
        raise Refusal(REPO_FORMAT, "an object's sector padding is not zero")
    raw = inflate_stored(stream, entry["uncompressed"])
    if hashlib.sha1(raw).digest() != entry["oid"]:
        raise Refusal(REPO_OID, f"object {entry['oid'].hex()} does not hash to its own bytes")
    return raw


# --- §5d, the minimum Git parser -----------------------------------------------


def envelope(raw: bytes):
    nul = raw.find(b"\0")
    if nul < 0:
        raise Refusal(REPO_FORMAT, "the object has no envelope terminator")
    head = raw[:nul]
    space = head.find(b" ")
    if space < 0:
        raise Refusal(REPO_FORMAT, "the object envelope has no size separator")
    kind, size = head[:space], head[space + 1 :]
    if kind not in (b"commit", b"tree", b"blob"):
        raise Refusal(REPO_KIND, f"object kind {kind!r} is not one this profile reads")
    if not size or not size.isdigit() or (size[0:1] == b"0" and size != b"0"):
        raise Refusal(REPO_FORMAT, f"the object envelope size {size!r} is not a plain decimal")
    payload = raw[nul + 1 :]
    if int(size) != len(payload):
        raise Refusal(REPO_FORMAT, "the object envelope size is not the payload length")
    return kind.decode(), payload


def tree_entries(payload: bytes):
    at = 0
    seen = 0
    while at < len(payload):
        seen += 1
        if seen > MAX_TREE_ENTRIES:
            raise Refusal(REPO_BOUNDS, "the tree has more entries than this profile reads")
        space = payload.find(b" ", at)
        if space < 0:
            raise Refusal(REPO_FORMAT, "a tree entry has no mode separator")
        mode = payload[at:space]
        if not mode or any(byte not in b"01234567" for byte in mode):
            raise Refusal(REPO_FORMAT, "a tree entry mode is not an ascii octal run")
        nul = payload.find(b"\0", space)
        if nul < 0:
            raise Refusal(REPO_FORMAT, "a tree entry has no name terminator")
        name = payload[space + 1 : nul]
        if not name or b"/" in name:
            raise Refusal(REPO_FORMAT, "a tree entry name is empty or holds a separator")
        if len(name) > MAX_PATH_BYTES:
            raise Refusal(REPO_BOUNDS, "a tree entry name is longer than this profile reads")
        if nul + 1 + OID_LENGTH_SHA1 > len(payload):
            raise Refusal(REPO_FORMAT, "a tree entry is missing its object id bytes")
        yield mode, name, payload[nul + 1 : nul + 1 + OID_LENGTH_SHA1]
        at = nul + 1 + OID_LENGTH_SHA1


def traverse(table, extent, root_oid: bytes):
    by_oid = {entry["oid"]: entry for entry in table}

    def load(oid: bytes, want: str):
        entry = by_oid.get(oid)
        if entry is None:
            raise Refusal(REPO_MISSING, f"object {oid.hex()} is not in the table")
        kind, payload = envelope(object_bytes(extent, entry))
        if kind != want:
            raise Refusal(REPO_KIND, f"object {oid.hex()} is a {kind} where a {want} is required")
        return payload

    commit = load(root_oid, "commit")
    if not commit.startswith(b"tree ") or len(commit) < 46 or commit[45:46] != b"\n":
        raise Refusal(REPO_FORMAT, "the commit does not begin with a tree line")
    hexed = commit[5:45]
    try:
        oid = bytes.fromhex(hexed.decode("ascii"))
    except (ValueError, UnicodeDecodeError) as bad:
        raise Refusal(REPO_FORMAT, "the commit's tree line is not 40 lowercase hex") from bad
    if hexed != hexed.lower():
        raise Refusal(REPO_FORMAT, "the commit's tree line is not lowercase hex")

    if len(LANDING_PATH) > MAX_TREE_DEPTH:
        raise Refusal(REPO_BOUNDS, "the landing path is deeper than this profile descends")
    for depth, component in enumerate(LANDING_PATH):
        payload = load(oid, "tree")
        final = depth == len(LANDING_PATH) - 1
        matches = [
            (mode, raw) for mode, name, raw in tree_entries(payload) if name == component.encode()
        ]
        if not matches:
            raise Refusal(REPO_MISSING, f"{component} is not in the tree at depth {depth}")
        if len(matches) > 1:
            raise Refusal(REPO_FORMAT, f"{component} appears {len(matches)} times in one tree")
        mode, raw = matches[0]
        if final:
            if mode not in BLOB_MODES:
                raise Refusal(REPO_KIND, f"the final entry has mode {mode!r}")
        elif mode != TREE_MODE:
            raise Refusal(REPO_KIND, f"an intermediate entry has mode {mode!r}")
        oid = raw

    return load(oid, "blob")


# --- the tool ------------------------------------------------------------------


def oracle(git_dir: str, commit: str) -> bytes:
    """Ordinary Git, asked the same question independently (`docs/08`)."""
    done = subprocess.run(
        ["git", "--git-dir", git_dir, "cat-file", "blob", f"{commit}:{'/'.join(LANDING_PATH)}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if done.returncode != 0:
        return None
    return done.stdout


def git_can_read(extent: bytes, table, where: str) -> str:
    """§5c's producer obligation, asserted rather than asserted-about.

    Every object's stream is written into a fresh loose object store **exactly
    as the extent holds it**, and ordinary Git is asked to read it back. A
    profile that restricted the producer to stored blocks and left "and Git can
    still read it" as prose would be a profile nobody had tried."""
    os.makedirs(where, exist_ok=True)
    subprocess.run(["git", "init", "-q", "--bare", where], check=True)
    for entry in table:
        oid = entry["oid"].hex()
        folder = os.path.join(where, "objects", oid[:2])
        os.makedirs(folder, exist_ok=True)
        at = entry["first_sector"] * SECTOR_BYTES
        with open(os.path.join(folder, oid[2:]), "wb") as handle:
            handle.write(extent[at : at + entry["stored"]])
    for entry in table:
        oid = entry["oid"].hex()
        done = subprocess.run(
            ["git", "--git-dir", where, "cat-file", "-p", oid],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if done.returncode != 0:
            return f"ordinary git cannot read object {oid}: {done.stderr.decode().strip()}"
    return ""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--extent", required=True)
    parser.add_argument("--capsule", required=True)
    parser.add_argument("--git-dir")
    parser.add_argument(
        "--git-loose-check",
        metavar="DIR",
        help="write every object's stream into a fresh object store at DIR and have ordinary "
        "git read it back (ADR-0102 §5c, §10b)",
    )
    parser.add_argument("--expect", default="REPO_OK", choices=sorted(BY_NAME))
    args = parser.parse_args()

    identity = capsule_facts.read(args.capsule)
    with open(args.extent, "rb") as handle:
        extent = handle.read()

    verdict = REPO_OK
    why = "the repository holds the boot text this capsule carries"
    payload = None
    try:
        if identity.source_kind != capsule_facts.SRC_KIND_GIT:
            raise Refusal(REPO_UNSUPPORTED, "the capsule declares no git source identity")
        if identity.oid_algorithm != capsule_facts.OID_ALG_SHA1:
            raise Refusal(REPO_UNSUPPORTED, "the capsule declares an object-id algorithm this profile does not verify")
        header = read_header(extent, identity)
        table = read_table(extent, header)
        payload = traverse(table, extent, identity.oid[: identity.oid_length])
        if hashlib.sha256(payload).digest() != identity.boot_content_sha256:
            raise Refusal(
                REPO_LINKAGE,
                "the repository's boot blob is not the boot text this capsule carries",
            )
    except Refusal as refusal:
        verdict, why = refusal.repo_class, refusal.why

    if args.git_loose_check and verdict == REPO_OK:
        why_not = git_can_read(extent, read_table(extent, read_header(extent, identity)), args.git_loose_check)
        if why_not:
            print(f"verify: FAIL: {why_not}", file=sys.stderr)
            return 1

    if args.git_dir and verdict == REPO_OK:
        committed = oracle(args.git_dir, identity.commit_hex)
        if committed is None:
            print("verify: FAIL: ordinary git cannot read the landing path at this commit", file=sys.stderr)
            return 1
        if committed != payload:
            print("verify: FAIL: this reader and ordinary git disagree about the blob", file=sys.stderr)
            return 1

    name = CLASS_NAMES[verdict]
    if name != args.expect:
        print(f"verify: FAIL: expected {args.expect}, read {name} ({why})", file=sys.stderr)
        return 1
    print(f"verify: {name} — {why}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
