<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Update, merge, and package model

## No opaque package installation for native components

Native TOS components are source trees and manifests in repository history. A package is therefore a commit, subtree, or dependency reference, not an opaque binary archive installed into hidden locations.

## System update workflow

1. Fetch upstream objects.
2. Verify object integrity and signatures.
3. Compare upstream with local system branch.
4. Show source, capability, schema, driver, and nucleus changes.
5. Merge or fast-forward into a new candidate commit.
6. Resolve conflicts explicitly.
7. Run static checks and tests.
8. Prepare state migrations and derived caches.
9. Install candidate boot artifacts if required.
10. Set candidate ref and reboot or hot-activate eligible services.
11. Promote after health success.

## Dependency representation

Dependencies must be deterministic. Acceptable models include:

- source subtree pinned to a commit;
- repository reference pinned by content ID;
- vendored source with provenance metadata;
- module object in the same repository.

Unpinned branch names are not valid runtime dependencies.

## Lock data

The active system commit contains a lock manifest listing exact dependency identities, frontend versions, schemas, and required runtime ABI.

## Conflicts

Conflicts are ordinary source conflicts plus TOS-specific semantic conflicts:

- two modules request incompatible capability policy;
- IPC schema versions diverge;
- state migration order conflicts;
- two drivers claim one device;
- nucleus ABI requirement changes;
- language frontend semantics change cache identity.

Merge tools must present both textual and semantic conflicts.

## Local customization

Machine-specific changes should remain on a named machine branch or layered configuration repository. Updating upstream becomes a merge rather than overwriting local files.

## Third-party applications

Applications may live:

- in the system repository;
- in separate signed repositories pinned by commit;
- in a user repository;
- in a temporary source workspace.

Installation means making the source identity and manifest available to a launcher, not copying a binary into a global directory.

## Native caches from remotes

A remote may distribute verified acceleration artifacts, but they are optional. The local system verifies that an artifact matches:

- source content;
- toolchain or engine identity;
- target architecture;
- runtime ABI;
- declared build process;
- signature policy.

Failure falls back to local execution from source.

## Removal

Removing a component means creating a new commit that no longer references its source and updating user or service configuration. Historical commits retain the component until garbage-collection policy permits deletion.
