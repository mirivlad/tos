# SPDX-License-Identifier: GPL-3.0-or-later
"""The numbers ADR-0102 §6, §7 and §8 decide, and nothing else.

Two host tools read this file and the canonical reader states the same numbers
in its own text. That is three statements of one format, which is what a
persistent format costs; what this module exists to prevent is the *writer* and
the *checker* sharing an encoder, because then a checker would agree with a
writer that was wrong.
"""

SECTOR_BYTES = 512

# §6a — where the extent lives on the shared device.
REPOSITORY_FIRST_SECTOR = 65
REPOSITORY_SECTORS = 2048
REQUIRED_CAPACITY = REPOSITORY_FIRST_SECTOR + REPOSITORY_SECTORS  # 2113

# §7a — the header.
MAGIC = b"TOSGITR1"
FORMAT_VERSION = 1
OID_ALGORITHM_SHA1 = 1
OID_LENGTH_SHA1 = 20
ENTRY_SIZE = 48
TABLE_SECTORS = 3
DATA_START = 4

# §8 — the bounds.
MAX_OBJECTS = 32
MAX_OBJECT_UNCOMPRESSED_BYTES = 128 * 1024
MAX_TREE_DEPTH = 16
MAX_TREE_ENTRIES = 256
MAX_PATH_BYTES = 256

# §10a — the refusal vocabulary, as the canonical reader reports it.
REPO_OK = 0
REPO_FORMAT = 1
REPO_BOUNDS = 2
REPO_MISSING = 3
REPO_KIND = 4
REPO_OID = 5
REPO_LINKAGE = 6
REPO_UNSUPPORTED = 7
REPO_BLOCK = 8

# §5d, §9 — the one path this profile traverses.
LANDING_PATH = ("source", "system", "boot", "init.tos")

TREE_MODE = b"40000"
BLOB_MODES = (b"100644", b"100755")

assert ENTRY_SIZE * MAX_OBJECTS == TABLE_SECTORS * SECTOR_BYTES
assert DATA_START == 1 + TABLE_SECTORS
