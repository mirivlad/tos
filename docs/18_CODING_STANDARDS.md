<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Coding and specification standards

## Rust

- Stable Rust pinned by repository toolchain file.
- `no_std` for loader-shared libraries and nucleus code where applicable.
- Warnings are denied in CI for project crates after the initial scaffold is stable.
- Unsafe blocks require a `SAFETY:` comment stating why preconditions hold.
- No unchecked integer arithmetic for offsets, lengths, addresses, or object sizes.
- Parsing accepts byte slices and returns structured errors; it does not panic on malformed input.
- Dependencies are minimized in trusted code and reviewed for `no_std`, unsafe use, maintenance, and license.
- Public types and invariants are documented.

## TOS Core source

Until the formatter exists, source examples follow:

- four-space indentation;
- UTF-8;
- LF line endings;
- one module per file;
- explicit imports;
- no wildcard imports in system code;
- stable names for exported interfaces;
- capability requirements adjacent to service definitions;
- typed units for sizes and durations.

## Formats

Every on-disk or wire format specifies:

- magic;
- version;
- byte order;
- alignment;
- limits;
- checksum or digest behavior;
- unknown-field behavior;
- upgrade and compatibility rules;
- canonical encoding rules;
- security considerations;
- golden vectors.

## Error codes

Errors have stable symbolic identifiers. Numeric encodings may be assigned for wire protocols, but logs and source use symbolic names.

## Documentation

- Architecture documents state goals, invariants, mechanisms, failure modes, and rejected shortcuts.
- Examples are marked illustrative unless normative.
- No document uses "later" as a substitute for defining a boundary needed now.
- Open questions are listed explicitly rather than silently decided in code.

## Commits

A project commit should be reviewable and coherent. Commit messages explain why and identify the subsystem. Architecture-changing commits include or reference an ADR.

## Tests

- New behavior requires tests.
- Bug fixes add a regression test.
- Format changes update vectors and compatibility tests.
- Unsafe code receives focused tests.
- QEMU-visible behavior receives integration coverage.

## Logging

Trusted-base logging avoids allocation where practical and uses stable event IDs. Sensitive data is never emitted by default.

## Time and randomness

Code that needs time or randomness receives explicit providers. Tests use deterministic providers. Parsing, source lowering, object identity, and commit construction must not depend on wall-clock time except where the time value is an explicit input.

## Licensing and provenance

- Every source and documentation file uses an SPDX identifier.
- New Rust implementation defaults to the licence of its directory as defined in `LICENSE.md`.
- Copying from an external source requires an import record and compatible licence.
- Linux GPL-2.0-only implementation code is not copied into GPLv3 TOS components.
- Generated code records generator and source identity; generation does not erase licence notices.
- Commits require DCO `Signed-off-by` trailers.
- AI-assisted code is reviewed and attributed to the accountable human contributor, not the model.

## Architecture impact metadata

Pull requests that add formats, dependencies, privileges or public contracts include an architecture impact statement and change level from `docs/21_ARCHITECTURE_PRESERVATION_POLICY.md`.
