<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Provenance record: the one authorized history rewrite

The Project Architect authorized exactly one rewrite of published history, for
one purpose: commit `80bfcc1` had reached `origin/main` without the
`Signed-off-by` trailer that `docs/23_CONTRIBUTION_PROVENANCE.md` requires, so
`scripts/check-dco.sh` — and therefore `scripts/preflight.sh --full` — failed on
every subsequent run and could not be repaired without rewriting.

This record exists so the rewrite is auditable rather than silent. The
authorization covers this repair only; no other rewrite is permitted under it.

## What changed

The DCO trailer was added to `80bfcc1`. Nothing else. The nine affected commits
were replayed in order onto the unchanged `390c08a`, so their **tree hashes are
byte-for-byte identical to the originals** and only the commit objects differ.
That equality is the evidence that no content moved with the SHAs:

```text
old HEAD tree  64e3a16fbbc382b7237516bc9547040c449eb37c
new HEAD tree  64e3a16fbbc382b7237516bc9547040c449eb37c
```

Every replayed commit's message is unchanged except for the added trailer on the
first one. Author identity and author dates are preserved; committer dates
changed, as they must when a commit object is rewritten.

## Commit mapping

Base, unchanged: `390c08a` — `fix: harden TOS Core ownership flow, scoping and
evaluation order`.

| Old SHA | New SHA | Subject |
|---|---|---|
| `80bfcc17bd1c` | `2e11a1e14f15` | feat: close the ownership frontier under ADR-0035 |
| `29bb6f3d0f65` | `f6257fa9b2ec` | feat: check declared authority for TOS Core capabilities |
| `5a510f70859c` | `082393a69cd1` | feat: check TOS Core task scopes and atomic order legality |
| `feadb2fb9056` | `4b2e00729473` | feat: check import ambiguity and loop metering |
| `cbb9c774df38` | `a04e03649af4` | test: bind the remaining frontend diagnostics to the corpus |
| `f1339431215b` | `51866b85f118` | feat: add the tos-ir/v1 schema and a deterministic lowerer |
| `ff11a680f1a3` | `3dbd0f7df328` | feat: add the independent tos-ir/v1 verifier |
| `f7cbaf4c9cc5` | `eb1fc5f4b0df` | feat: add the bounded Bootstrap reference interpreter |
| `c75f4d544ef2` | `4a16120f8f26` | docs: record the Stage 2 candidate gate and what the boot text is today |

## Verification after the rewrite

```text
scripts/check-dco.sh          OK — 182 commits carry a sign-off naming their author
tree equality                 old HEAD tree == new HEAD tree
scripts/preflight.sh --full   31 of 31 gates pass
```

Anyone holding a clone from before the rewrite can confirm the mapping by
comparing tree hashes: `git rev-parse <old-sha>^{tree}` equals
`git rev-parse <new-sha>^{tree}` for every row above.
