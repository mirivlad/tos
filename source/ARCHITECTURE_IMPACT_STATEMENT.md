<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1 architecture impact statement

- Task: CODEX_START.md — Stage 1 trusted boot foundation.
- Change level (docs/21): **Level 2** — establishes new versioned contracts
  (boot ABI v1, capsule format v1) without amending invariants and without a
  Level 3 trust-boundary or persistent-format change to existing TOS components
  (none exist yet).
- Owner decision: implementation lives under `source/` in the `tos` repository,
  mirroring the docs/17 monorepo tree. This is a scoping/layout arrangement, not a
  change to any contract. `docs/17_REPOSITORY_LAYOUT.md` remains normative at the
  conceptual level; its tree is satisfied verbatim beneath `source/`.

## Invariants affected

No invariant is amended or removed. The work **exercises**:

- I-01 canonical source: `/system/boot/init.tos` is carried and validated as
  canonical text.
- I-02 minimal binary trusted base: loader + nucleus are `no_std`, dependency-free;
  no feature is promoted into the nucleus for convenience.
- I-03 / I-09 versioned boundaries: boot ABI v1 and capsule v1 are versioned from
  the first commit; identity is algorithm-qualified (SHA-256).
- I-10 deterministic identity: capsule build is reproducible; identical inputs
  yield identical bytes.
- I-18 derived-artifact provenance: capsule manifest binds source commit/detached
  source-set, content hashes, builder, ABI and digests.
- I-13 / I-21: real capsule parse/validation and QEMU boot exercise the intended
  contract; no MVP substitution.

## Canonical representation after the change

Non-nucleus boot content remains canonical text (`/system/boot/init.tos`). The
capsule is a **transport and recovery seed**, never the installed system.

## Trusted-base impact

Loader and nucleus remain `no_std` and dependency-free (SHA-256 re-implemented
from the published FIPS-180-4 specification; UEFI bindings hand-written against
the UEFI spec — no third-party crate enters the trusted base). Unsafe Rust is
isolated to the loader's UEFI FFI and given a `SAFETY:` justification.

## Source-to-runtime impact

The nucleus reports `TOS.IDENTITY` binding result, source identity and capsule
digest. `init.tos` content hash printed at boot is verified equal to the canonical
input hash by an architecture test. The host-generated
`tos-capsule-provenance-v1` sidecar independently binds that capsule digest to
its format/ABI/target, source material blobs, retained licence notice and R0
reproducibility statement; it is release evidence, not a nucleus input.

## Recovery and rollback impact

Stage 1 establishes the capsule mechanism that later recovery stages consume; it
adds no activation or rollback logic. Recoverability is preserved by keeping the
capsule deterministic and its provenance recorded.

## Declared compatibility profile

Reaching only **G0** (commit-addressed identity): boot and provenance name a
source digest, but no persistent Git repository is parsed. No Git-compatibility
claim is made.

## Stage identity gate (Stage 1)

Question: does the first boot artifact prove it carries canonical source from an
identified repository state? Evidence produced: capsule manifest binding
source-set/paths/hashes/builder/ABI/output digest; nucleus structured source
identity for init.tos; corruption and digest-mismatch tests fail closed;
documentation integrity passes at the worktree; real git repository exists with a
non-placeholder baseline commit.

## Threat-model impact (docs/34)

New parser boundaries exercised with negative tests: capsule parser
(bounded, total, no panic on arbitrary bytes) and boot ABI validation. Evidence
level E2 (automated positive + negative tests; QEMU corruption cases). No remote,
no signature-policy claim, no driver boundary touched.

## Performance contract

- Capsule validation budget (docs/35, Stage 1): a capsule of 1000 files / 16 MiB
  payload validates and resolves `/system/boot/init.tos` in ≤ 250 ms p95 on the
  QEMU CI profile. This stage reports the measurement; it does not waive it.

## Dependencies and licences

- Dependencies: none external in the trusted base (loader, nucleus, capsule,
  SHA-256 all no_std and self-contained). Host tools are dependency-free.
- Licences: boot/, nucleus/, host-tools/ and crates → GPL-3.0-or-later;
  interfaces/boot and conformance vectors → Apache-2.0; docs → CC-BY-SA-4.0.
  Every file is SPDX-tagged.
- License of public spec reimplementation: FIPS 180-4 SHA-256 reimplemented from
  specification (hardware fact), no expressive code copied.
- Patent exposure: capsule transport and content-addressed boot ABI are common
  mechanisms; no patent claim combination is deliberately reproduced. Noted for
  the Stage 5 content-addressed-activation review.

## Tests that enforce the decision

- Capsule round-trip, property (bounds/overflow) and fuzz (deterministic corpus)
  tests in `source/tests/`.
- Golden valid/invalid version vectors — 13 binaries in
  `source/tests/vectors/capsule-v1/`: 1 valid (built from the real
  `source/system/boot/init.tos` with the real NOTICES.txt licence tail) and
  12 invalid (bad magic, truncated, kind none, path traversal, duplicate path
  entry, file-reserved bytes, path-flag misuse, duplicate/unreferenced file
  index, boot-canonical cross-check mismatch, licence-tail mismatch,
  payload gap). The generator `tests/vectors/gen/gen.sh` rebuilds them all
  deterministically.
- Capsule lib unit tests (11) and integration tests (8): golden reproduction,
  tamper/truncation sweeps over every byte, deterministic rebuild, parser
  invalid-vector expectations.
- UEFI loader: compile-time layout assertions pin every EFI structure offset
  (`EFI_TABLE_HEADER` 24 B, SystemTable header_size 0x78, BootServices at
  ST+0x60, RuntimeServices at ST+0x58, EFI_MEMORY_DESCRIPTOR 40 B); all
  firmware calls check status with `efi_error`/`efi_success`; error-path
  statuses (EFI_BUFFER_TOO_SMALL probe, EFI_INVALID_PARAMETER retry) are
  handled explicitly, not by ignoring the status.
- QEMU success + corrupted-capsule tests asserting serial event IDs and result
  codes (`host-tools/qemu-test/run.sh` builds the capsule with
  `--git-commit HEAD` so the identity gate runs in the real boot path).
- Architecture test: init.tos printed hash equals canonical input hash.
- Stage 1 identity gate: `tos-capsule-tool --git-commit HEAD` resolves the
  commit oid, verifies `cat-file -e`, checks every manifest source against
  `git cat-file blob HEAD:path`, and emits a meta JSON with
  commit / sha256(commit-oid) / repo_path + content_sha256 / capsule_sha256.
  Proven negative: a tampered init.tos is refused (exit 2); deterministic
  rebuild yields the identical capsule_sha256.
- Capsule provenance sidecar: ADR-0024 and
  `interfaces/boot/CAPSULE_PROVENANCE_V1.md` define deterministic v1 metadata;
  its independent host checker rejects a capsule, Git-blob, material, notice or
  digest mismatch, and the ordinary QEMU preparation path runs it before ESP
  creation.
- SPDX, DCO, documentation-integrity `--check`, deterministic-builder tests.

## Known limitations (declared now, accepted)

- Boot ABI v1 reuses the UEFI page tables after ExitBootServices (identity
  mapping); owning page tables is a later-memory-manager decision, tracked as a
  Stage 3 risk, not hidden.
- No allocator, no scheduler, no language: explicitly out of scope for Stage 1.
