<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Repository layout

## Proposed monorepo

```text
/
├── README.md
├── ARCHITECTURE.md
├── AGENTS.md
├── CODEX_START.md
├── CONTRIBUTING.md
├── GOVERNANCE.md
├── LICENSE.md
├── PATENTS.md
├── SECURITY.md
├── TRADEMARKS.md
├── TOS_DEVELOPMENT_SPECIFICATION.md   # generated; never edit
├── DCO
├── LICENSES/
├── rust-toolchain.toml
├── Cargo.toml
├── boot/
│   ├── uefi-loader/
│   └── image-spec/
├── nucleus/
│   ├── arch/x86_64/
│   ├── memory/
│   ├── process/
│   ├── capability/
│   ├── ipc/
│   ├── repository-bootstrap/
│   └── runtime-bootstrap/
├── crates/
│   ├── boot-protocol/
│   ├── capsule/
│   ├── tos-hash/
│   ├── tos-schema/
│   ├── tos-parser/
│   ├── tos-ir/
│   ├── tos-repository/
│   └── tos-sdk/
├── interfaces/
│   ├── abi/
│   ├── ipc/
│   ├── schemas/
│   └── test-vectors/
├── host-tools/
│   ├── capsule/
│   ├── image/
│   ├── qemu-test/
│   ├── repo-inspect/
│   └── provenance/
├── system/
│   ├── boot/
│   ├── services/
│   ├── drivers/
│   ├── languages/
│   ├── shell/
│   ├── ui/
│   └── policy/
├── tests/
│   ├── vectors/
│   ├── integration/
│   ├── fuzz/
│   ├── conformance/
│   ├── performance/
│   └── architecture/
├── docs/
│   ├── SPECIFICATION_SOURCES.txt
│   ├── adr/
│   ├── research/
│   └── ...
├── legal/
│   ├── third-party/
│   ├── release-manifests/
│   └── publication-records/
├── tools/
│   └── build-specification.py
└── scripts/
    └── check-generated-spec.sh
```

## Licence defaults by area

- `boot/`, `nucleus/`, `system/` and official runtime implementation: `GPL-3.0-or-later`.
- `interfaces/` and explicitly designated SDK/test-vector libraries: `Apache-2.0`.
- `docs/` and root policy prose: `CC-BY-SA-4.0`.
- SPDX headers override directory defaults only when permitted by the licensing ADR.

## Directory rules

### `boot/`

Loader and deterministic image assembly. Generated images remain outside Git; source manifests and stable format fixtures are committed.

### `nucleus/`

Only binary trusted-base implementation. A dependency/subsystem requires an ADR, transitive inventory and minimal-TCB justification.

### `crates/`

Reusable implementation libraries. A crate is not Apache merely because it is reusable; licence is explicit. Parsers shared by host/target live here when practical.

### `interfaces/`

Independent interface definitions, schemas, bindings and conformance vectors. It must not hide core implementation.

### `host-tools/`

Developer/release tooling and external oracles. Runtime restoration cannot depend on undocumented host state.

### `system/`

Canonical textual system tree, and the canonical input for the runtime
`/system` tree of an installed machine.

No generated executable caches or binary packages are committed here.

Vendor-controlled opaque material — CPU microcode, device firmware and
comparable vendor-produced bytes — is never committed into this tree. It belongs
to the runtime `/vendor` namespace under ADR-0030, carries its own
licence and redistribution terms, and is referenced from `system/` only by
declaration: vendor, object identity, version and content hash.

### `tests/performance/`

Versioned workloads, benchmark harnesses, reference-oracle metadata and result schemas from `docs/35_PERFORMANCE_CONTRACTS.md`.

### `tests/architecture/`

Tests enforcing canonical source, provenance, owner control, trusted-base boundaries, repository identity and stage identity gates.

### `docs/adr/`

Accepted ADRs are immutable except spelling/link corrections. Superseding decisions add a new ADR.

### `docs/research/`

Non-normative research records, including language evaluation, patent landscape and name search.

### `docs/language/`

Programmer-facing language guides, learning material, canonical proposed or
implemented `.tos` examples, and conformance inputs. The numbered language
contracts remain the normative source; this tree explains and exercises them.
Every canonical example has an SPDX header and one tracked source of truth.
Before a frontend exists, examples state their proposed/not-implemented status;
afterward, documentation checks bind accepted examples to parser/checker/runtime
evidence rather than maintaining duplicate Markdown snippets.

### `tools/build-specification.py`

The only supported producer of `TOS_DEVELOPMENT_SPECIFICATION.md`. Output must be deterministic for identical sources.

### `legal/release-manifests/`

Stage identity reports, release provenance, SBOMs, licence inventories and signatures/attestations.

## Generated files

Generated artifacts go under ignored `target/`, `out/` or staging directories. `TOS_DEVELOPMENT_SPECIFICATION.md` is the deliberate exception: it is committed as a generated review artifact and verified against sources in CI.

Stable golden vectors are committed because they are specification fixtures, not runtime caches.

## Monorepo rule

The initial project remains a monorepo so formats, licences, runtime contracts and conformance tests change atomically. Repository splitting requires an ADR defining compatibility, ownership, release and legal boundaries.
