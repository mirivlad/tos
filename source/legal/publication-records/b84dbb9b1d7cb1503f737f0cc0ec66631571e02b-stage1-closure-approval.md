<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Project Architect approval — TOS Stage 1 closure

## Decision

**Approved.**  The Project Architect approves formal closure of TOS Stage 1
— Source-bearing boot identity — on 2026-08-09.

This immutable approval record accepts the closure evidence for source commit
`f2206036e48c57b821f69d77bc72b16bfd18ee13`, recorded in
`source/legal/release-manifests/f2206036e48c57b821f69d77bc72b16bfd18ee13-stage1-report.md`,
and the all-green archive-record commit
`b84dbb9b1d7cb1503f737f0cc0ec66631571e02b`.

## Evidence accepted

- The required Stage 1 identity evidence, provenance, negative paths, stable
  Boot ABI, resource bounds, platform handoff and exception baseline are
  recorded in the immutable Stage 1 evidence report and closure audit.
- ADR-0026 P2 evidence retains native and q35/qemu64/TCG full/crypto raw
  series, exact 101,203,198-byte / 2,007-invocation accounting and the
  enforced qemu64/TCG p95 ratio no greater than 1.30.
- The archive-record commit passed Documentation integrity, Provenance gates,
  Source CI and QEMU boot/P2 CI.  Its local `./scripts/preflight.sh --full`
  result was 30/30 PASS.

## Scope of approval

This approval closes only Stage 1.  It does not begin or approve Stage 1.5,
does not expand the declared G0 compatibility profile, and does not alter the
accepted Stage 1 limitations recorded in the evidence report.

Project Architect: Vladimir Tomashevskiy
Date: 2026-08-09
