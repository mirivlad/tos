<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1 Phase 1 Capsule Identity Implementation Plan

> **For agentic workers:** Execute inline, task by task. Observe every RED
> test locally, but commit only GREEN states.

**Goal:** Reconcile the accepted alignment decision, reject non-canonical
SHA-1 OID padding, prove BootInfo identity mismatch in QEMU, then prepare and
stop at the detached identity ADR and vector-provenance proposals.

**Architecture:** ADR-0017 controls unaligned capsule content; the conflicting
interface-draft wording is Level 0. ADR-0016 controls raw Git OID padding; its
parser enforcement is Level 1. Detached identity is Level 3 and stays proposal
only until an explicitly accepted ADR. BootInfo corruption is an explicit,
non-default test-only loader feature using the ordinary ESP/OVMF/QEMU path.

**Tech Stack:** Rust `no_std` crates, Bash QEMU harness/vector generator,
existing Python generator helpers, Cargo, QEMU, OVMF and mtools.

## Global constraints

- Do not start Stage 1.5, Phase 2 or later closure queues.
- F-08 remains open: `source/interfaces/boot/*.md` has no independent tier.
  Cite only ADR-0016, ADR-0017 and subsequently accepted ADRs as authority.
- Do not make canonical unaligned `content_offset` invalid, change capsule v1
  bytes/version for alignment, or add dependencies.
- Never publish a red test. Restore local RED-only changes before preparing the
  corresponding green commit.
- Do not change detached builder/parser behavior or regenerate detached vectors
  before detached ADR approval.
- Do not assign a blanket SPDX licence to `.bin` vectors. F-22 is resolved by
  provenance analysis, not inference from file location.
- Do not add or regenerate a tracked binary vector before the owner accepts a
  vector provenance/licensing model. An existing `.bin` exclusion in an SPDX
  check is not authority to add a binary artifact.
- Do not change default production loader behavior, ABI layout, trusted-base
  boundary, recovery behavior or G0 scope.
- Use `git commit -s`; keep `PROGRESS.md` out of scope.

---

### Task 1: Record local RED evidence without a red commit

**Files:** local-only then retained green as `scripts/tests/check-capsule-format-alignment.sh`, `source/crates/capsule/src/lib.rs`, and `scripts/tests/qemu-bootinfo-identity-mismatch.sh`.

**Produces:** Reproducible proof that rule 16 contradicts ADR-0017, SHA-1 tail
bytes are accepted, and no BootInfo e2e harness mechanism exists.

- [ ] **Step 1: Write/run local alignment check**

```bash
#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
set -euo pipefail
doc=source/interfaces/boot/CAPSULE_FORMAT_V1.md
rg -F 'content_offset` are **not** required' "$doc" >/dev/null
if sed -n '/^16\. /,/^17\. /p' "$doc" | rg -q misaligned; then
  echo 'FAIL: rule 16 contradicts ADR-0017' >&2
  exit 1
fi
echo 'capsule-format-alignment: PASS'
```

Run `bash scripts/tests/check-capsule-format-alignment.sh`. Expected RED: exit
1 with the stated contradiction.

- [ ] **Step 2: Write/run local SHA-1 padding test**

Add a test-only `sha1_capsule()` helper to the existing capsule test module. It
sets `SRC_KIND_GIT`, `OID_ALG_SHA1`, `OID_LEN_SHA1`, fills only the first 20
identity bytes, adds `/system/boot/init.tos`, and builds. Mutate byte
`off::SRC_VALUE + OID_LEN_SHA1 as usize`, recompute the whole digest, then run:

```rust
assert!(parse(&bytes).is_err(), "non-zero SHA-1 OID padding was accepted");
```

Run `cd source && cargo test -p tos-capsule sha1_oid_nonzero_padding_is_rejected`.
Expected RED: assertion failure because `parse()` returns `Ok`.

- [ ] **Step 3: Record absent BootInfo e2e mechanism**

Run this command before adding a custom loader option. The supplied path is
intentionally in an isolated test target directory, not the default artifact
path:

```bash
cd source && bash host-tools/qemu-test/run.sh --out target/qemu-bootinfo-identity-mismatch --loader target/test-corrupt-bootinfo/x86_64-unknown-uefi/release/tos-uefi-loader.efi --expect 67 --require 'TOS.BOOT.ENTRY TOS.CAPSULE.OK TOS.BOOT.HANDOFF TOS.NUCLEUS.ENTRY TOS.IDENTITY.MISMATCH TOS.CAPSULE.FAIL' --forbid 'TOS.HALT TOS.PANIC'
```

Expected RED: explicit `unknown option: --loader`, proving no pre-existing e2e
test path. Restore every local red-only file afterwards and confirm clean
`git status --short`.

### Task 2: Reconcile alignment text under ADR-0017

**Files:**

- Modify: `source/interfaces/boot/CAPSULE_FORMAT_V1.md:226-253`
- Create: `scripts/tests/check-capsule-format-alignment.sh`
- Modify: `source/tests/integration/src/lib.rs`
- Modify: `WORKLOG_STAGE1_HARDENING.md`

**Produces:** Level 0 wording reconciliation and machine-checkable evidence
that the committed canonical fixture remains unaligned and accepted.

- [ ] **Step 1: Apply only the lower-text correction**

Replace rule 16 exactly with:

```markdown
16. file content out of payload bounds;
```

No parser, layout, version or vector change belongs in this step.

- [ ] **Step 2: Assert actual unaligned acceptance**

In `golden_valid_parses`, after loading `h`, add:

```rust
let second_content_offset_at = h.file_table_offset as usize + 64;
let second_content_offset = u64::from_le_bytes(
    bytes[second_content_offset_at..second_content_offset_at + 8].try_into().unwrap(),
);
assert_ne!(second_content_offset % 8, 0, "fixture must exercise unaligned content");
assert!(parse(&bytes).is_ok(), "canonical unaligned capsule must parse");
```

Run `bash scripts/tests/check-capsule-format-alignment.sh` and
`cd source && cargo test -p tos-tests-integration golden_valid_parses`.
Expected GREEN: both pass; the fixture's second `content_offset` is 430 and is
therefore intentionally unaligned.

- [ ] **Step 3: Commit and verify**

Append actual commands/results and ADR-0017 rationale to Worklog. Commit using
one `git add` of the four files and message
`docs: reconcile capsule alignment rule`. Run `sh scripts/check-spdx.sh` and
the focused integration test. Do not assert that the interface draft acquired a
new authority tier.

### Task 3: Enforce SHA-1 OID padding end to end

**Files:**

- Modify: `source/crates/capsule/src/lib.rs`
- Modify: `source/boot/uefi-loader/src/main.rs`
- Modify: `source/tests/integration/src/lib.rs`
- Create: `source/host-tools/qemu-test/sha1-oid-padding.sh`
- Modify: `source/interfaces/boot/CAPSULE_FORMAT_V1.md`
- Modify: `WORKLOG_STAGE1_HARDENING.md`

**Produces:** Level 1 enforcement of ADR-0016's zero-padding representation
with matching host and real-loader error evidence. The QEMU scenario creates
its malformed capsule beneath ignored `source/target/`; it does not add a
tracked `.bin` fixture before F-22 is resolved.

- [ ] **Step 1: Restore/observe RED then add one error**

Restore the Task 1 padding test and observe its asserted acceptance. Add
`NonZeroOidPadding` to `CapsError`, then in the existing Git identity branch,
after algorithm/length validation, add exactly:

```rust
if h.source_oid_alg == OID_ALG_SHA1
    && h.source_identity_value[OID_LEN_SHA1 as usize..].iter().any(|&b| b != 0)
{
    return Err(CapsError::NonZeroOidPadding);
}
```

Change the assertion to:

```rust
assert_eq!(parse(&bytes), Err(CapsError::NonZeroOidPadding));
```

Do not apply a tail rule to SHA-256 OIDs. Add
`CapsError::NonZeroOidPadding => b"NonZeroOidPadding"` to loader `error_tag`.

- [ ] **Step 2: Add in-memory integration and ephemeral QEMU evidence**

Add an integration test that copies `valid-001.bin` in memory, writes the Git
source identity header as SHA-1 (`kind=1`, `alg=1`, `len=20`), fills its first
20 bytes with deterministic raw-OID bytes, sets the first unused tail byte to
`0x01`, recomputes `whole_capsule_digest`, and asserts
`Err(CapsError::NonZeroOidPadding)`. It must not write a fixture.

Create `source/host-tools/qemu-test/sha1-oid-padding.sh`. The script must
derive the same malformed bytes from
`source/tests/vectors/capsule-v1/valid-001.bin` into
`source/target/qemu-negative-sha1-padding/invalid-sha1-oid-padding.bin`, then
invoke the existing `run.sh` with that explicit capsule, `--expect 67`, require
`TOS.CAPSULE.FAIL` and `capsule_err=NonZeroOidPadding`, and forbid
`TOS.NUCLEUS.ENTRY`. Its generated file must remain under ignored `target/` and
must not be listed in `vectors.tsv`.

This changes no detached identity semantics and creates no committed binary
vector. The transformation recipe is retained in the script so that a later,
owner-approved provenance record can describe the derivation completely.

- [ ] **Step 3: Prove GREEN at every layer**

Run:

```bash
cd source && cargo test -p tos-capsule sha1_oid_nonzero_padding_is_rejected
cd source && cargo test -p tos-tests-integration every_vector_matches_its_declared_outcome
cd source && cargo test -p tos-tests-integration sha1_oid_nonzero_padding_is_rejected
cd source && bash host-tools/qemu-test/sha1-oid-padding.sh
```

Expected: host parser returns `NonZeroOidPadding`; all 12 existing committed
negative fixtures retain their declared outcomes; the ephemeral QEMU capsule
prints `NonZeroOidPadding`, exits 67, and never enters the nucleus.

- [ ] **Step 4: Commit only GREEN state**

State the already accepted zero-padding rule in the interface draft without
claiming independent authority. Record observed evidence, commit all Task 3
files with `capsule: reject nonzero SHA-1 OID padding`, then run
`./scripts/preflight.sh --full`. Expected: 15/15 PASS with 12 committed
negative vectors plus one direct ephemeral SHA-1-padding QEMU scenario.

### Task 4: Implement isolated BootInfo mismatch e2e evidence

**Files:**

- Modify: `source/boot/uefi-loader/Cargo.toml`
- Modify: `source/boot/uefi-loader/src/main.rs:521-539`
- Modify: `source/host-tools/qemu-test/run.sh:18-104`
- Create: `source/host-tools/qemu-test/bootinfo-identity-mismatch.sh`
- Create: `scripts/tests/qemu-bootinfo-identity-mismatch.sh`
- Modify: `WORKLOG_STAGE1_HARDENING.md`

**Produces:** An explicitly built test artifact that changes one BootInfo mirror
byte after a verified header copy, while ordinary production boot stays intact.

- [ ] **Step 1: Restore/observe the Task 1 harness RED**

Run the absent-`--loader` command. Expected: only `unknown option: --loader`.

- [ ] **Step 2: Add a non-default feature and corruption block**

Add to the loader package manifest:

```toml
[features]
test-corrupt-bootinfo-identity = []
```

Do not add it to `default`. Immediately after the existing copy from parsed
header into `bi.capsule_source_identity`, add:

```rust
#[cfg(feature = "test-corrupt-bootinfo-identity")]
{
    bi.capsule_source_identity[0] ^= 0x01;
}
```

- [ ] **Step 3: Add custom-loader input without touching default harness path**

Parse `--loader FILE` in `run.sh`; use the existing loader path only when it is
absent. Keep capsule construction, ESP layout, OVMF discovery, q35/qemu64/256
MiB and isa-debug-exit arguments unchanged. The mismatch script must require
one loader-path argument and fail before invoking QEMU when it is absent; it
passes that argument to `run.sh --loader` and has no default-loader fallback.
Supplying explicit `--forbid` prevents the ordinary exit-67 loader-rejection
default from forbidding the required nucleus entry.

- [ ] **Step 4: Verify isolation and failure path**

The regression script runs, in order. It records the digest of the ordinary
artifact before the feature build and proves that the feature build neither
overwrites it nor becomes the implicit `run.sh` choice:

```bash
cargo build --release -p tos-uefi-loader --target x86_64-unknown-uefi
normal_loader=target/x86_64-unknown-uefi/release/tos-uefi-loader.efi
normal_sha256_before=$(sha256sum "$normal_loader" | awk '{print $1}')
bash host-tools/qemu-test/run.sh --out target/qemu-normal-control --expect 33
CARGO_TARGET_DIR=target/test-corrupt-bootinfo cargo build --release -p tos-uefi-loader --target x86_64-unknown-uefi --features test-corrupt-bootinfo-identity
test "$(sha256sum "$normal_loader" | awk '{print $1}')" = "$normal_sha256_before"
test -f target/test-corrupt-bootinfo/x86_64-unknown-uefi/release/tos-uefi-loader.efi
bash host-tools/qemu-test/bootinfo-identity-mismatch.sh target/test-corrupt-bootinfo/x86_64-unknown-uefi/release/tos-uefi-loader.efi
bash host-tools/qemu-test/run.sh --out target/qemu-normal-control-after --expect 33
```

Expected: normal artifact emits `TOS.HALT`/exit 33; explicit feature reaches
the real nucleus, emits `TOS.IDENTITY.MISMATCH`, never emits `TOS.HALT`, and
exits 67. The default artifact digest is unchanged, the test artifact exists
only below `target/test-corrupt-bootinfo/`, and the final unqualified success
path still exits 33.

- [ ] **Step 5: Commit and full verification**

Commit with `qemu: test BootInfo identity mismatch`, then run
`./scripts/preflight.sh --full` plus the direct feature scenario. Both must
pass; default preflight intentionally does not compile the corruption feature.

### Task 5: Prepare detached identity ADR proposal and stop

**Files:**

- Create: `docs/adr/0018-detached-capsule-source-identity.md`
- Create: `docs/superpowers/specs/2026-08-09-detached-identity-proposal-test.md`
- Modify: `MANIFEST.txt`, `SHA256SUMS`, `WORKLOG_STAGE1_HARDENING.md`

**Produces:** A Status `Proposed` Level 3 ADR and local proposal evidence, not
a detached builder/parser/vector change.

- [ ] **Step 1: Prepare uncommitted proposal evidence**

The local-only test computes `SHA-256(concat(content_digest_i))` in canonical
file-table order and compares it with `source_identity_value`. Current synthetic
`0x42` fixture is expected to fail. Remove that test from the worktree after
recording output; do not commit it as an active contract.

- [ ] **Step 2: Draft complete ADR-0018**

It must define exact formula/order, invalid zero-file semantics, builder-owned
identity computation, independent parser recomputation/rejection, migration of
synthetic vectors only after acceptance, deterministic/provenance consequences,
and every `docs/21` impact answer: invariants, canonical representation,
trusted base, source-to-runtime, recovery/rollback, compatibility, threat,
performance, dependency, licence, patent and test evidence.

Keep Status `Proposed` and do not add it to `docs/SPECIFICATION_SOURCES.txt`.
Regenerate only the release manifest/checksums, because it is a tracked release
file. Commit `docs: propose detached capsule identity`, run documentation,
release, SPDX and DCO gates, then stop for architect approval.

### Task 6: Prepare vector provenance/licensing proposal and stop

**Files:**

- Create: `docs/superpowers/specs/2026-08-09-capsule-vector-provenance-proposal.md`
- Modify: `MANIFEST.txt`, `SHA256SUMS`, `WORKLOG_STAGE1_HARDENING.md`

**Produces:** A non-normative F-22 proposal, not a container licence decision
or regenerated `.bin` set.

- [ ] **Step 1: Record material/authority inventory**

Document that `LICENSE.md` permits Apache only for explicitly designated
reusable vectors, requires generated-artifact provenance, and that the valid
fixture embeds GPL-class `init.tos` and a licence notice. Record the conflicting
README, `vectors.tsv` and architecture statement declarations.

- [ ] **Step 2: Specify the manifest proposal**

Use this exact per-vector shape:

```json
{
  "format": "tos-capsule-vector-provenance-v1",
  "vector": "valid-001.bin",
  "sha256": "<64 lowercase hex>",
  "generator": {"path": "source/tests/vectors/gen/gen.sh", "version": 1},
  "source_commit": "<full Git OID or detached declaration>",
  "inputs": [{"path": "system/boot/init.tos", "spdx": "GPL-3.0-or-later"}],
  "generated_artifact": true,
  "derivation": null
}
```

Separate reusable format/harness metadata from the mixed-material binary
container. The schema must also support a derived invalid vector by replacing
`derivation: null` with an object containing its `base_vector` and a precise
`transformation_recipe`; for example, the future SHA-1 padding vector would
name `valid-001.bin` and specify its SHA-1 header rewrite, non-zero unused-tail
byte, and whole-digest recomputation. Ask the architect to identify existing
authority for a single container classification; the proposal must not decide
it.

- [ ] **Step 3: Commit proposal and stop**

Regenerate release metadata, commit `docs: propose capsule vector provenance`,
run its check and checksum verification, then report the exact remaining policy
decision. Do not add or regenerate vectors, edit their SPDX/provenance files,
or begin detached implementation. After the owner accepts a provenance model,
a separately approved continuation may add the SHA-1 fixture and its complete
provenance record; this plan does not authorize that action.

## Plan self-review

- Alignment, SHA-1, BootInfo e2e, detached proposal and vector provenance each
  have distinct evidence and a stop boundary.
- Local RED evidence is never a published red state.
- No task silently promotes an unassigned interface draft or chooses a vector
  licence without existing-policy authority.
