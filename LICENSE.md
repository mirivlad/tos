<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS licensing map

TOS uses established licenses rather than a project-specific license. This repository is intentionally multi-licensed by component class.

## License matrix

| Material | Default license | SPDX identifier |
|---|---|---|
| Nucleus, boot code, reference runtime, official system services, official drivers, activation and recovery implementation | GNU General Public License version 3 or later | `GPL-3.0-or-later` |
| Public SDKs, ABI definitions, IPC schemas, conformance harness libraries, independent integration libraries and reusable test vectors explicitly marked as such | Apache License 2.0 | `Apache-2.0` |
| Architecture documents, specifications, tutorials, diagrams, governance and policy documents | Creative Commons Attribution-ShareAlike 4.0 International | `CC-BY-SA-4.0` |
| Code fragments embedded in documentation, unless a fragment says otherwise | dual licensed | `GPL-3.0-or-later OR Apache-2.0` |
| Network services intentionally designated in their own directory | GNU Affero General Public License version 3 or later | `AGPL-3.0-or-later` |

No directory becomes AGPL-licensed merely because it communicates over a network. An AGPL component requires an explicit ADR and SPDX declaration.

## Why GPLv3-or-later for the operating system

TOS is designed so that the owner can inspect, modify and boot the actual source identity of the installed system. GPLv3 is selected because reciprocal source obligations alone are not enough for this project: when its conditions apply to a User Product, GPLv3 also addresses the information necessary to install and execute modified versions. That aligns with TOS invariant I-17: official TOS must not expose source while technically locking the owner out of loading it.

The project uses the `or-later` form to permit migration to a future GNU GPL version if the project governance later accepts it through an ADR. A distributor may always use GPLv3 under the current grant; no future migration may retroactively remove rights already granted.

## Why Apache-2.0 for interfaces and SDK material

TOS should permit independent applications, tools, language frontends and compatible implementations. Stable public interfaces therefore use a permissive license with an express patent grant. Apache-licensed interface material may be combined into the GPLv3 TOS implementation, while independent projects can use it without becoming GPL-covered merely by copying an interface library or schema.

This boundary must not be abused to move operating-system implementation into an Apache directory. The architecture-preservation policy decides whether material is an interface or part of TOS itself.

## Documentation license

Documentation is licensed under CC BY-SA 4.0. Implementing a documented protocol or idea does not automatically copy the wording of its specification. Modified copies of TOS documentation must retain attribution and ShareAlike obligations.

## File-level declarations

Every source file added by the project must carry an SPDX identifier in the conventional comment syntax for its format. Generated artifacts must carry provenance metadata identifying the licenses of their canonical sources; generated artifacts are not a way to remove license notices.

Examples:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later
```

```text
# SPDX-License-Identifier: Apache-2.0
```

```markdown
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->
```

## Full texts

The corresponding license texts are stored in `LICENSES/`:

- `LICENSES/GPL-3.0-or-later.txt`
- `LICENSES/Apache-2.0.txt`
- `LICENSES/CC-BY-SA-4.0.txt`

If an AGPL component is accepted, the repository must add the official AGPLv3 text before that component is merged.

## Copyright and contributions

Copyright remains with contributors. TOS does not require assignment of copyright to the project architect or a foundation. Contributions are accepted under the Developer Certificate of Origin 1.1 in `DCO`; every commit must contain a valid `Signed-off-by` trailer.

## No extra field-of-use restriction

TOS does not add clauses forbidding particular industries, military use, commercial use, AI use, or other fields of endeavor. Such clauses would no longer be conventional open-source licensing and would create incompatible custom terms. Ethical positions may be stated as non-binding project values, but they are not license restrictions.

## No legal warranty

This file documents project policy and is not legal advice. Before a commercial hardware distribution, jurisdiction-specific counsel should review the release, third-party notices, installation-information obligations and patent exposure.
