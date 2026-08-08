<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Stage 1 Phase 0 closure design

Status: owner-approved scope from the 2026-08-08 Stage 1 closure directive.

## Goal

Restore every current-main repository gate that can be repaired without
rewriting published history or changing DCO policy, and make the existing Stage
1 boot path easy to check and run locally without creating a second path.

## Change classification

This work is Level 0/1 under
`docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`:

- asset licence declarations apply the accepted ADR-0007 matrix; they do not
  select a new licence or move a licence boundary;
- generated files are refreshed by their authoritative generators;
- preflight only orchestrates existing gates;
- human QEMU mode reuses the existing capsule, ESP, OVMF machine profile and
  boot harness; it adds no boot implementation, ABI or canonical artifact;
- README/CONTRIBUTING changes are operational documentation.

No invariant changes. Canonical source, trusted base, source-to-runtime chain,
recovery/rollback behavior, compatibility profile G0, dependencies, patent
exposure and runtime licence boundaries are unchanged. No ADR is required for
Phase 0. Any discovery that contradicts this classification stops the affected
change.

## Asset licensing and provenance

Mascot files are documentation/branding artwork, not executable OS material.
ADR-0007 and `LICENSE.md` already assign specifications, tutorials and diagrams
to `CC-BY-SA-4.0`; Phase 0 records the mascot under that existing class.

Text artwork carries an SPDX header directly. PNG files cannot carry a normal
source header without rewriting their binary encoding, so
`assets/mascot/README.md` is the authoritative file list, licence and provenance
record. The SPDX gate exempts only listed binary artwork with that record; it
does not exempt `.png` globally. A newly added unrecorded PNG therefore fails.

The record states only facts available from Git: paths, introduction commit and
commit author. It does not invent a third-party source or creation tool. The
three missing DCO trailers remain a separate blocker.

## Local preflight

`scripts/preflight.sh` calls existing tools in fail-fast order and prints a
single final PASS/FAIL result. Default mode runs documentation/release, SPDX,
DCO, formatting, tests and all mandatory clippy invocations. `--full` adds the
existing deterministic fuzz target and existing QEMU success/negative suite.

The script contains no alternate implementations of those checks. DCO remains
red until the owner authorizes remediation of the historical commits.

## Human QEMU path

Root `run-tos.sh` checks required Rust tools/targets, builds the three existing
release artifacts, then delegates to
`source/host-tools/qemu-test/run.sh`.

- default: invokes the harness's explicit interactive mode;
- `--check`: invokes its existing self-judging headless behavior.

The harness keeps one preparation and QEMU argument path. Interactive mode
changes only display/serial presentation: it exposes the normal QEMU window and
tees serial to the terminal and the same log path. `isa-debug-exit`, capsule
construction, ESP construction, firmware discovery, q35/qemu64/256 MiB profile
and result semantics remain identical. The VM exits when Stage 1 writes its
normal halt result; it is not kept alive by a fake desktop loop.

## Error handling and tests

- Missing commands, firmware, Rust targets or graphical session produce a
  specific message and never install packages.
- Shell entrypoints support `--help` and reject unknown arguments.
- A focused operational test proves an unrecorded PNG fails the SPDX gate,
  verifies entrypoint help/error behavior and confirms both wrappers delegate
  to the existing scripts.
- Real verification runs default preflight components and `run-tos.sh --check`.

## Documentation

README receives a concise Quick start before the documentation map.
CONTRIBUTING records `git commit -s`, asset rules and preflight-before-push.
Generated specification, release manifest and checksums are regenerated last.

Worklog entries contain only commands actually run and their results. No Stage
1 completion language is permitted.
