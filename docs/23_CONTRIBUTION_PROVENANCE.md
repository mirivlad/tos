<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Contribution and provenance policy

## DCO model

TOS uses Developer Certificate of Origin 1.1 sign-off rather than a mandatory copyright-assignment CLA. A signed commit records that the contributor has the right to submit the contribution under the declared licence.

Required trailer:

```text
Signed-off-by: Real Name <email@example.com>
```

Bots may create commits, but an accountable human or legal entity must review and sign them before merge.

## AI-assisted work

AI output is not assumed to be novel or licence-clean. The submitting human must:

- inspect the complete diff;
- identify any suspicious reproduction of known code;
- avoid prompts that request copying a third-party implementation;
- preserve tool transcripts when provenance is uncertain;
- ensure the contribution can be explained and maintained;
- take responsibility through DCO sign-off.

For substantial generated modules, the pull request records tool name, model/version if known, prompting context category, human reviewer and verification performed. Private prompts or secrets are not required to be published.

## Imported code record

Every non-trivial imported or adapted work records:

- upstream project and canonical location;
- exact version, commit or release;
- original file paths;
- original licence and notices;
- modifications;
- compatibility decision;
- whether the code is runtime, build-only or test-only.

## Clean-room reimplementation

When a useful implementation has an incompatible licence, TOS may use a documented clean-room process:

1. one person or document extracts public functional requirements and hardware facts without copying expressive code;
2. the implementation is written from that neutral specification;
3. reviewers compare behavior, not source expression;
4. records identify the public specifications used;
5. no claim of legal “clean room” protection is made without counsel when stakes are material.

## Provenance gates

A contribution is blocked when:

- licence cannot be identified;
- DCO sign-off is missing;
- copied code appears incompatible;
- generated code origin is materially uncertain;
- a known patent dependency is intentionally hidden;
- the contributor lacks authority to submit employer-owned work.
