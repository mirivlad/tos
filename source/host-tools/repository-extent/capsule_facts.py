# SPDX-License-Identifier: GPL-3.0-or-later
"""The four boot facts, read out of a capsule file.

`CAPSULE_FORMAT_V1` decides this layout and `crates/capsule` implements it; this
is a third reader of it, and it exists because the provisioner and the checker
must both start from *the capsule* rather than from anything the host happens to
know (ADR-0102 §10a). It reads the header's source identity and the SHA-256 the
file table already carries for `/system/boot/init.tos` — the same two values
`system.boot.Identity` reports inside the running system (ADR-0102 §4).

It parses no payload and validates no digest: the capsule this is pointed at has
already been through `tos-capsule-tool`, and a fourth validator would be a fourth
thing to keep in step with the format.
"""

import struct
from dataclasses import dataclass

MAGIC = b"TOSCAPSU"
HEADER_SIZE = 184
PATH_ENTRY_SIZE = 16
FILE_ENTRY_SIZE = 64
BOOT_PATH = b"/system/boot/init.tos"

SRC_KIND_GIT = 1
OID_ALG_SHA1 = 1
OID_LEN_SHA1 = 20


@dataclass(frozen=True)
class BootFacts:
    """Exactly what `system.boot.IdentityRecord` carries, and nothing else."""

    source_kind: int
    oid_algorithm: int
    oid_length: int
    oid: bytes
    boot_content_sha256: bytes

    @property
    def commit_hex(self) -> str:
        return self.oid[: self.oid_length].hex()


def read(path: str) -> BootFacts:
    with open(path, "rb") as handle:
        blob = handle.read()
    if len(blob) < HEADER_SIZE or blob[:8] != MAGIC:
        raise ValueError(f"{path} is not a TOS capsule")

    path_table_offset = struct.unpack_from("<Q", blob, 40)[0]
    path_table_count = struct.unpack_from("<I", blob, 48)[0]
    path_entry_size = struct.unpack_from("<I", blob, 52)[0]
    file_table_offset = struct.unpack_from("<Q", blob, 56)[0]
    file_count = struct.unpack_from("<I", blob, 64)[0]
    file_entry_size = struct.unpack_from("<I", blob, 68)[0]
    source_kind = blob[96]
    oid_algorithm = blob[97]
    oid_length = blob[98]
    oid = blob[100:132]

    if path_entry_size != PATH_ENTRY_SIZE or file_entry_size != FILE_ENTRY_SIZE:
        raise ValueError("capsule entry sizes are not v1's")

    name_start = path_table_offset + path_table_count * PATH_ENTRY_SIZE

    boot_digest = None
    for index in range(path_table_count):
        at = path_table_offset + index * PATH_ENTRY_SIZE
        name_offset, name_length, file_index, _flags = struct.unpack_from("<IIII", blob, at)
        name = blob[name_start + name_offset : name_start + name_offset + name_length]
        if name != BOOT_PATH:
            continue
        if file_index >= file_count:
            raise ValueError("the boot path names a file the capsule does not have")
        entry = file_table_offset + file_index * FILE_ENTRY_SIZE
        boot_digest = blob[entry + 16 : entry + 48]
        break
    if boot_digest is None:
        raise ValueError(f"the capsule has no {BOOT_PATH.decode()}")

    return BootFacts(
        source_kind=source_kind,
        oid_algorithm=oid_algorithm,
        oid_length=oid_length,
        oid=oid,
        boot_content_sha256=boot_digest,
    )
