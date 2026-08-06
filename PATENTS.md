<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# TOS patent policy

TOS follows a defensive, disclosure-first patent policy.

## Project position

- The TOS Project does not plan to seek software patents on the core architecture.
- Significant original architecture should be published in a dated, durable and searchable form so that it can serve as prior art.
- Publication is not a freedom-to-operate opinion. Existing patents can still create implementation risk even when the project independently invents a system.
- No contributor is expected to perform a worldwide patent search for ordinary patches.
- A contributor must disclose any patent claim they actually know is intentionally required by their contribution.
- The project does not accept a contribution accompanied by a private patent licence that cannot extend to downstream recipients on compatible terms.

## Patent review triggers

A focused review is mandatory before accepting designs for:

- content-addressed update and rollback mechanisms;
- verified native or bytecode caches tied to source identity;
- user-space interrupt delivery and DMA isolation;
- remote restoration and fleet activation;
- hardware-distributed textual drivers;
- a commercial appliance or User Product;
- any implementation deliberately modelled on a patented technique.

## Public records

The preliminary landscape is maintained in `docs/research/PATENT_LANDSCAPE.md`. It is a risk register, not legal advice. Each entry records jurisdiction, family, apparent status, relevant independent-claim concepts, TOS intersection and design response.

## Assertions against TOS

Patent demands, threats or licence offers must be preserved unmodified and escalated to maintainers. Developers should not admit infringement, promise payment or publicly speculate about claim construction. The project will prefer design-around, prior-art evidence, community defence and qualified counsel.

See `docs/24_PATENT_POLICY.md` and `docs/25_DEFENSIVE_PUBLICATION_PROTOCOL.md`.
