<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Defensive publication protocol

## Goal

Make original TOS architecture discoverable as prior art and prevent later parties from plausibly claiming it as a new invention, while preserving a reliable record of what was disclosed and when.

## Publication package

A defensive publication release contains:

- a clear title and abstract;
- named authors or project attribution;
- complete enabling architectural description;
- diagrams and protocol details sufficient for implementation;
- known alternatives and combinations;
- publication date and version;
- Git commit ID;
- SHA-256 digest of the archive;
- licence notice;
- stable public URL or archival identifier.

Vague slogans are weak disclosure. The publication should explain how canonical text, derived caches, commit-addressed boot, recovery, frontend extension and capability-confined textual drivers cooperate.

## Publication channels

At least two independent public channels are preferred:

1. signed release in the official Git repository;
2. durable archive such as Zenodo, Software Heritage, an institutional repository or another timestamped public archive.

A blog post may announce the release but is not the sole archival record.

## Timing

Publication occurs after the architecture is sufficiently described to enable the concept and before private commercial disclosure whenever practical. Public disclosure can limit the project’s own ability to seek patents, especially outside jurisdictions with grace periods. That is an intentional consequence of the defensive strategy and must be acknowledged by the Project Architect for major publications.

## Immutable record

After publication, the released archive is never replaced. Corrections create a new version that references the original. Git history alone is not treated as an incontrovertible timestamp because repositories can be rewritten; external archival evidence is required for significant disclosures.

## Disclosure log

`docs/research/DEFENSIVE_PUBLICATION_LOG.md` records title, version, commit, digest, public locations and covered concepts.

## No overclaim

A defensive publication does not prove that every implementation is patent-free and does not invalidate earlier priority. It contributes prior art from its effective public date.
