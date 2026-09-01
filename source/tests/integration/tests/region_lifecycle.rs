// SPDX-License-Identifier: GPL-3.0-or-later
//! The memory-authority and region primitive, proved on its own (ADR-0075).
//!
//! This is the nucleus's own module, compiled here so the state machine can be
//! exercised on the host: the same file the freestanding build takes, not a
//! second copy of it. Nothing below maps a page or issues a syscall — what is
//! under test is the accounting and the lifecycle, which is what the operations
//! will be written against.
//!
//! The sequence the primitive has to survive, before any bundle is written
//! through it:
//!
//! ```text
//! allocate -> map writable -> freeze -> writing denied -> transfer
//!          -> sender gone -> receiver reads -> share -> a second reader
//!          -> release -> reclaimed
//! ```

#[path = "../../../nucleus/src/region.rs"]
mod region;

use region::{Mode, Refusal, Regions, MAX_MAPPING_SPACES};

const ROOT: usize = 64 * 1024 * 1024;
/// Process slots, because a region records **which** address spaces map it
/// rather than how many. They are slots of the nucleus's fixed process table,
/// so they are inside its bound; a number outside it names no address space and
/// is one of the refusals below.
const WORKER: u32 = 0;
const SUPERVISOR: u32 = 1;
const PEER: u32 = 2;

/// Allocation, and then the handoff every real creation performs.
///
/// `allocate` leaves a region the nucleus owns and nobody holds: capabilities
/// zero, one construction reference. What operation 17 does next — and what
/// this stands in for — is lay down the backing, map it, hand the caller its
/// one capability, and only then let the construction reference go.
fn handed_over(
    regions: &mut Regions,
    authority: region::AuthorityId,
    bytes: usize,
    holder: u32,
) -> region::RegionId {
    let region = regions
        .allocate(authority, bytes, holder)
        .expect("allocated");
    assert_eq!(regions.internal(region), Ok(1), "the nucleus holds it");
    assert!(regions.can_name(region), "and nobody else does yet");
    regions
        .retain_capability(region)
        .expect("the caller is given its one capability");
    regions
        .release_internal(region)
        .expect("and construction is over");
    region
}

/// The retirement a real reclamation performs.
///
/// The physical work belongs between the two halves: operation 17's reclaim
/// returns the backing frames to the pool and the lane's page tables to the
/// reserve, and only then may the authority be credited. Here there is no
/// backing yet, so the two halves are adjacent — but they are still two, and
/// the receipt is what makes the order unskippable.
fn retire(regions: &mut Regions) {
    while let Some(region) = regions.reclaimable() {
        let ticket = regions.take_reclaim(region).expect("taken once");
        assert!(regions.reclaim_bytes(&ticket) > 0);
        regions.finish_reclaim(ticket).expect("and finished once");
    }
}

/// A root authority, and the accounting checked before anything happens.
fn endowed() -> (Regions, region::AuthorityId) {
    let mut regions = Regions::new();
    let root = regions.endow_root(ROOT).expect("the boot endows a root");
    assert!(regions.accounting_holds());
    (regions, root)
}

#[test]
fn a_root_authority_is_the_only_one_that_comes_from_nowhere() {
    let (mut regions, root) = endowed();
    assert_eq!(regions.remaining(root), Ok(ROOT));

    // Everything else is reserved out of something that exists.
    let child = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    assert_eq!(regions.remaining(root), Ok(ROOT - 8 * 1024 * 1024));
    assert_eq!(regions.remaining(child), Ok(8 * 1024 * 1024));
    assert!(regions.accounting_holds());

    // And nothing may reserve more than it holds.
    assert_eq!(
        regions.attenuate(child, 8 * 1024 * 1024 + 1),
        Err(Refusal::Budget)
    );
    assert_eq!(regions.attenuate(child, 0), Err(Refusal::Empty));
    assert!(regions.accounting_holds());
}

/// A grandchild reserves out of its own parent, and no byte is debited twice.
#[test]
fn a_reservation_moves_budget_down_and_charges_it_once() {
    let (mut regions, root) = endowed();
    let child = regions.attenuate(root, 16 * 1024 * 1024).expect("reserved");
    let grandchild = regions.attenuate(child, 4 * 1024 * 1024).expect("reserved");

    assert_eq!(regions.remaining(root), Ok(ROOT - 16 * 1024 * 1024));
    assert_eq!(regions.remaining(child), Ok(12 * 1024 * 1024));
    assert_eq!(regions.remaining(grandchild), Ok(4 * 1024 * 1024));

    // The allocation debits the grandchild and nothing above it: the ancestors
    // paid when they reserved.
    let _region = handed_over(&mut regions, grandchild, 1024 * 1024, WORKER);
    assert_eq!(regions.remaining(grandchild), Ok(3 * 1024 * 1024));
    assert_eq!(regions.remaining(child), Ok(12 * 1024 * 1024));
    assert_eq!(regions.remaining(root), Ok(ROOT - 16 * 1024 * 1024));
    assert!(regions.accounting_holds());

    // And what is allocated under the root is the one megabyte, counted once.
    assert_eq!(regions.allocated(root), Ok(1024 * 1024));
}

/// No sequence lets a subtree spend more than its root budget.
#[test]
fn a_subtree_cannot_spend_more_than_it_was_given() {
    let (mut regions, root) = endowed();
    let child = regions.attenuate(root, 2 * 1024 * 1024).expect("reserved");

    assert!(regions.allocate(child, 1024 * 1024, WORKER).is_ok());
    assert!(regions.allocate(child, 1024 * 1024, WORKER).is_ok());
    assert_eq!(regions.remaining(child), Ok(0));
    assert_eq!(regions.allocate(child, 1, WORKER), Err(Refusal::Budget));
    assert_eq!(regions.allocated(child), Ok(2 * 1024 * 1024));
    assert!(regions.accounting_holds());
}

/// The whole primitive, in the order it will be used: the three states, and
/// the two consuming transitions between them.
#[test]
fn a_region_is_allocated_frozen_transferred_shared_and_reclaimed() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let bundle = handed_over(&mut regions, authority, 2 * 1024 * 1024, WORKER);

    // Writable while it is being written, in the one address space that holds
    // it — and in no other, because a mutable region has exactly one owner.
    regions
        .map(bundle, WORKER, true)
        .expect("its holder maps it writably");
    assert_eq!(regions.writable_aliases(bundle), Ok(2));
    assert_eq!(
        regions.map(bundle, SUPERVISOR, true),
        Err(Refusal::NotTheHolder),
        "nobody else may map a region with one owner"
    );
    assert_eq!(
        regions.map(bundle, WORKER, true),
        Err(Refusal::BadArgument),
        "and one process has one window, not a count of them"
    );

    // Frozen, and the postcondition is the nucleus's.
    regions
        .freeze(bundle, WORKER)
        .expect("the holder freezes it");
    assert_eq!(regions.mode(bundle), Ok(Mode::Immutable));
    assert_eq!(
        regions.writable_aliases(bundle),
        Ok(0),
        "no writable alias survives the transition"
    );
    assert_eq!(
        regions.mapped_by(bundle, WORKER),
        Ok(Some(false)),
        "the writable window became a readable one, in place"
    );
    assert_eq!(
        regions.map(bundle, WORKER, true),
        Err(Refusal::WrongMode),
        "and none can be made afterwards"
    );
    assert_eq!(
        regions.freeze(bundle, WORKER),
        Err(Refusal::WrongMode),
        "the transition has no inverse and cannot be repeated"
    );

    // Handed on: linear, so the sender keeps nothing and the receiver gets it
    // whole. Between the two the region belongs to nobody, and only the
    // nucleus's own reference keeps it alive — which is exactly the interval a
    // queued message lives in.
    regions
        .retain_internal(bundle)
        .expect("the message becomes a holder first");
    regions
        .detach(bundle, WORKER)
        .expect("the sender gives up ownership and its window");
    assert_eq!(regions.holder(bundle), Ok(None));
    assert_eq!(regions.mapped_by(bundle, WORKER), Ok(None));
    assert_eq!(
        regions.detach(bundle, WORKER),
        Err(Refusal::NotTheHolder),
        "the sender is not the holder any more"
    );
    regions
        .release_capability(bundle)
        .expect("and its handle goes with it");
    assert_eq!(
        regions.allocated(root),
        Ok(2 * 1024 * 1024),
        "the message alone keeps the backing while nothing else can reach it"
    );

    regions
        .adopt(bundle, SUPERVISOR)
        .expect("ownership arrives");
    regions
        .map(bundle, SUPERVISOR, false)
        .expect("the receiver reads it");
    regions
        .retain_capability(bundle)
        .expect("and is given its own name for it");
    regions
        .release_internal(bundle)
        .expect("the message stops being a holder last");
    assert_eq!(regions.allocated(root), Ok(2 * 1024 * 1024));

    // Shared: the second consuming transition. What was one owner's becomes
    // nobody's, and several names for it become admissible.
    regions
        .share(bundle, SUPERVISOR)
        .expect("the holder shares it");
    assert_eq!(regions.mode(bundle), Ok(Mode::Shared));
    assert_eq!(regions.holder(bundle), Ok(None), "no exclusive holder left");
    assert_eq!(
        regions.mapped_by(bundle, SUPERVISOR),
        Ok(Some(false)),
        "and the sharer's own window did not move"
    );
    assert_eq!(
        regions.share(bundle, SUPERVISOR),
        Err(Refusal::WrongMode),
        "there is nothing left for a second share to consume"
    );

    // A second holder: another name, another address space, one backing.
    regions
        .retain_capability(bundle)
        .expect("a second capability is admissible now");
    regions
        .map(bundle, PEER, false)
        .expect("and a second address space may map it");
    assert_eq!(regions.mappings(bundle), Ok((0, 2)));
    assert_eq!(regions.capabilities(bundle), Ok(2));

    // The last of everything, and only then does the backing go.
    regions
        .release_capability(bundle)
        .expect("one holder lets go");
    regions.unmap(bundle, PEER, false).expect("its window goes");
    retire(&mut regions);
    assert_eq!(
        regions.allocated(root),
        Ok(2 * 1024 * 1024),
        "the other holder still reaches it"
    );
    regions
        .release_capability(bundle)
        .expect("the last capability goes");
    regions
        .unmap(bundle, SUPERVISOR, false)
        .expect("and the last window");
    retire(&mut regions);
    assert_eq!(
        regions.allocated(root),
        Ok(0),
        "the backing returned to the authority that funded it"
    );
    assert_eq!(regions.remaining(authority), Ok(8 * 1024 * 1024));
    assert!(regions.accounting_holds());
}

/// A region is not reclaimed while anything can still reach it.
#[test]
fn a_mapping_keeps_a_region_alive_after_its_last_capability() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 4 * 1024 * 1024).expect("reserved");
    let held = handed_over(&mut regions, authority, 1024 * 1024, WORKER);
    regions.map(held, WORKER, true).expect("mapped");

    regions
        .release_capability(held)
        .expect("the capability goes");
    retire(&mut regions);
    assert_eq!(
        regions.allocated(root),
        Ok(1024 * 1024),
        "a mapping still reaches it, so the backing stays"
    );
    regions
        .unmap(held, WORKER, true)
        .expect("and now the mapping goes");
    retire(&mut regions);
    assert_eq!(regions.allocated(root), Ok(0));
    assert!(regions.accounting_holds());
}

/// Revoking an authority returns what is unspent and leaves what is not.
#[test]
fn revocation_returns_the_remainder_and_never_the_live_backing() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let region = handed_over(&mut regions, authority, 3 * 1024 * 1024, WORKER);

    regions.revoke(authority).expect("the authority is revoked");
    assert_eq!(
        regions.remaining(authority),
        Err(Refusal::NotFound),
        "a revoked authority names nothing"
    );
    assert_eq!(
        regions.remaining(root),
        Ok(ROOT - 3 * 1024 * 1024),
        "the unspent five megabytes came back, and the live three did not"
    );
    assert_eq!(regions.allocated(root), Ok(3 * 1024 * 1024));
    assert!(regions.accounting_holds());

    // The accounting node outlived its capability. When its last allocation
    // drains, what it was still holding travels up the lineage that funded it.
    regions
        .release_capability(region)
        .expect("the region's holder lets go");
    retire(&mut regions);
    assert_eq!(regions.remaining(root), Ok(ROOT));
    assert_eq!(regions.allocated(root), Ok(0));
    assert!(regions.accounting_holds());
}

/// A process ending takes its handles and its mappings with it.
///
/// **Its own, and nobody else's.** The capability references go one entry at a
/// time, because that is what the table clearing them actually knows; the
/// mappings go by process slot. A sweep over "regions this process held" could
/// do neither once a shared region can be held by several.
#[test]
fn a_process_ending_reclaims_what_only_it_could_reach() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let region = handed_over(&mut regions, authority, 2 * 1024 * 1024, WORKER);
    regions.map(region, WORKER, true).expect("mapped writably");

    // One decrement per capability entry destroyed, which is what
    // `capability::clear` performs.
    regions
        .release_capability(region)
        .expect("its one table entry goes");
    regions.mappings_destroyed(WORKER);
    retire(&mut regions);
    assert_eq!(
        regions.allocated(root),
        Ok(0),
        "nothing could reach it once its holder was gone"
    );
    assert_eq!(regions.remaining(authority), Ok(8 * 1024 * 1024));
    assert!(regions.accounting_holds());
}

/// One reader of a shared region dying leaves the others reading.
#[test]
fn a_shared_region_survives_the_death_of_one_of_its_readers() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let region = handed_over(&mut regions, authority, 1024 * 1024, WORKER);
    regions.map(region, WORKER, true).expect("mapped writably");
    regions.freeze(region, WORKER).expect("frozen");
    regions.share(region, WORKER).expect("shared");
    regions.retain_capability(region).expect("a second name");
    regions.map(region, PEER, false).expect("a second window");

    // One process ends: its entry is cleared and its window destroyed. The
    // other holder is untouched, and the backing does not move.
    regions
        .release_capability(region)
        .expect("the dying process's entry");
    regions.mappings_destroyed(PEER);
    retire(&mut regions);
    assert_eq!(regions.mappings(region), Ok((0, 1)));
    assert_eq!(regions.capabilities(region), Ok(1));
    assert_eq!(
        regions.allocated(root),
        Ok(1024 * 1024),
        "the surviving reader still reaches it"
    );

    // And when the last one goes, it goes exactly once.
    regions.release_capability(region).expect("the last name");
    regions.mappings_destroyed(WORKER);
    retire(&mut regions);
    assert_eq!(regions.allocated(root), Ok(0));
    assert_eq!(regions.remaining(authority), Ok(8 * 1024 * 1024));
    assert!(regions.accounting_holds());
}

/// Several handles in one process are still one window.
#[test]
fn several_shared_names_in_one_process_are_one_mapping() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 4 * 1024 * 1024).expect("reserved");
    let region = handed_over(&mut regions, authority, 1024 * 1024, WORKER);
    regions.map(region, WORKER, true).expect("mapped");
    regions.freeze(region, WORKER).expect("frozen");
    regions.share(region, WORKER).expect("shared");

    regions.retain_capability(region).expect("a second name");
    regions.retain_capability(region).expect("and a third");
    assert_eq!(regions.capabilities(region), Ok(3));
    assert_eq!(
        regions.mappings(region),
        Ok((0, 1)),
        "three names, one address space, one window"
    );
    assert_eq!(
        regions.map(region, WORKER, false),
        Err(Refusal::BadArgument),
        "and a second window in the same space is refused, not counted"
    );

    regions.release_capability(region).expect("one name goes");
    regions.release_capability(region).expect("another");
    retire(&mut regions);
    assert_eq!(
        regions.allocated(root),
        Ok(1024 * 1024),
        "the window and the last name still reach it"
    );
    regions.release_capability(region).expect("the last name");
    regions
        .unmap(region, WORKER, false)
        .expect("and the one window");
    retire(&mut regions);
    assert_eq!(regions.allocated(root), Ok(0));
    assert!(regions.accounting_holds());
}

/// What the primitive refuses.
#[test]
fn the_negatives_are_refusals_and_not_surprises() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 4 * 1024 * 1024).expect("reserved");
    let region = handed_over(&mut regions, authority, 1024 * 1024, WORKER);
    regions.map(region, WORKER, true).expect("mapped writably");

    // A mutable region is not transferable at all (ADR-0037).
    assert_eq!(regions.detach(region, WORKER), Err(Refusal::WrongMode));
    // Nor shareable: `share` presupposes immutability rather than producing it.
    assert_eq!(regions.share(region, WORKER), Err(Refusal::WrongMode));
    // Only the sole holder may freeze.
    assert_eq!(
        regions.freeze(region, SUPERVISOR),
        Err(Refusal::NotTheHolder)
    );
    // A second capability naming an affine region is the alias the model exists
    // to rule out.
    assert_eq!(
        regions.retain_capability(region),
        Err(Refusal::NotTheHolder)
    );
    // An unmapping by a process that did not map it, or in the wrong mode, is
    // refused rather than absorbed.
    assert_eq!(
        regions.unmap(region, SUPERVISOR, true),
        Err(Refusal::BadArgument)
    );
    assert_eq!(
        regions.unmap(region, WORKER, false),
        Err(Refusal::BadArgument)
    );
    // A process slot outside the bound names no address space.
    assert_eq!(
        regions.map(region, MAX_MAPPING_SPACES as u32, false),
        Err(Refusal::BadArgument)
    );
    // Zero-sized authority and zero-sized region are authority over nothing.
    assert_eq!(regions.allocate(authority, 0, WORKER), Err(Refusal::Empty));
    assert_eq!(regions.endow_root(0), Err(Refusal::Empty));

    // A stale handle names nothing: the region is released and its id no longer
    // resolves, whatever the generation counter has moved on to.
    regions.release_capability(region).expect("released");
    regions.unmap(region, WORKER, true).expect("and unmapped");
    retire(&mut regions);
    assert_eq!(regions.release_capability(region), Err(Refusal::NotFound));
    retire(&mut regions);
    assert_eq!(regions.freeze(region, WORKER), Err(Refusal::NotFound));
    assert!(regions.accounting_holds());
}

/// The bootstrap chain of ADR-0076 §2, and the property it exists for.
///
/// `Frames -> fixed reserves -> root MemoryAuthority -> funded initial
/// supervisor -> the supervisor's own authority`. After the root is endowed
/// nothing spends user memory except through the tree, so what the tree says is
/// committed is what the pool has actually lost.
#[test]
fn the_bootstrap_chain_charges_every_byte_once() {
    const FRAME: usize = 4096;
    const POOL: usize = 58_839 * FRAME; // the reference platform's admitted pool
    const RESERVES: usize = 9_961_472; // page tables and per-process fixed regions
    const GRANT: usize = 54 * 1024 * 1024;

    let mut regions = Regions::new();
    let root = regions
        .endow_root_after_reserves(POOL, RESERVES)
        .expect("the pool covers its reserves");
    assert_eq!(regions.remaining(root), Ok(POOL - RESERVES));

    // The initial supervisor is funded, not helped: its grant is charged to the
    // root like any other.
    let charge = regions
        .charge_grant(root, GRANT, FRAME)
        .expect("the root funds the supervisor");
    assert_eq!(charge.charged(), GRANT, "already a whole number of frames");
    assert_eq!(regions.remaining(root), Ok(POOL - RESERVES - GRANT));
    assert_eq!(regions.committed(), GRANT);
    assert!(regions.accounting_holds());

    // It then holds an authority of its own, reserved out of the root — which is
    // the only way it comes to have one.
    let supervisor = regions
        .attenuate(root, 64 * 1024 * 1024)
        .expect("the supervisor's allowance");
    let worker = regions
        .attenuate(supervisor, 96 * 1024 * 1024 + 8 * 1024 * 1024)
        .expect_err("more than the supervisor holds is refused");
    assert_eq!(worker, Refusal::Budget);

    // Everything the pool has lost is in the tree, and only once.
    let allowance = 64 * 1024 * 1024;
    assert_eq!(
        regions.remaining(root),
        Ok(POOL - RESERVES - GRANT - allowance)
    );
    assert_eq!(regions.committed(), GRANT, "a reservation is not a spend");
    assert!(regions.accounting_holds());

    // And when the supervisor is reclaimed its grant goes back where it came
    // from, leaving the pool as it was.
    regions.refund_grant(charge).expect("the grant returns");
    assert_eq!(regions.remaining(root), Ok(POOL - RESERVES - allowance));
    assert_eq!(regions.committed(), 0);
    assert!(regions.accounting_holds());
}

/// What is charged is what the machine spends, not what was asked for.
#[test]
fn a_charge_is_rounded_to_the_granule() {
    const FRAME: usize = 4096;
    let mut regions = Regions::new();
    let root = regions.endow_root(16 * 1024 * 1024).expect("endowed");

    let charge = regions
        .charge_grant(root, FRAME + 1, FRAME)
        .expect("funded");
    assert_eq!(
        charge.charged(),
        2 * FRAME,
        "a byte over a frame costs a frame"
    );
    assert_eq!(regions.remaining(root), Ok(16 * 1024 * 1024 - 2 * FRAME));

    let region = regions
        .allocate_rounded(root, FRAME + 1, FRAME, WORKER)
        .expect("allocated");
    regions.retain_capability(region).expect("handed over");
    regions.release_internal(region).expect("construction over");
    retire(&mut regions);
    assert_eq!(
        regions.allocated(root),
        Ok(4 * FRAME),
        "the grant's two frames and the region's two, all charged"
    );
    regions.release_capability(region).expect("released");
    retire(&mut regions);
    assert_eq!(regions.allocated(root), Ok(2 * FRAME));
    assert!(regions.accounting_holds());

    // A pool that cannot cover its own reserves is refused rather than wrapped.
    let mut empty = Regions::new();
    assert_eq!(
        empty.endow_root_after_reserves(1024, 2048),
        Err(Refusal::Budget)
    );
}

/// The tables are bounded, and a full one refuses rather than growing.
#[test]
fn a_full_table_refuses() {
    let (mut regions, root) = endowed();
    let mut made = 0;
    loop {
        match regions.allocate(root, 4096, WORKER) {
            Ok(_) => made += 1,
            Err(Refusal::NoRoom) => break,
            Err(other) => panic!("unexpected refusal: {other:?}"),
        }
        assert!(made <= region::MAX_REGIONS, "the table is bounded");
    }
    assert_eq!(made, region::MAX_REGIONS);
    assert!(regions.accounting_holds());

    // **And the refusal changes nothing.** `MAX_REGIONS` is a nucleus bound
    // over statically reserved slots, so the sixty-fifth region is refused
    // rather than made — and the way that goes wrong is a slot found *after*
    // the authority has been charged, which would leak a region's worth of
    // budget per refusal. The whole tree is measured on both sides of it.
    //
    // Proved here rather than from ring 3: sixty-four simultaneous regions
    // would need a fixture built to hold them and would prove the same thing
    // about the same table. What ring 3 does prove, on the real machine, is the
    // other bound — a full *capability* table, which is what a process actually
    // runs into first.
    let free = regions.remaining(root);
    let committed = regions.committed();
    let allocated = regions.allocated(root);
    assert_eq!(
        regions.allocate(root, 4096, WORKER),
        Err(Refusal::NoRoom),
        "the sixty-fifth is refused, not made"
    );
    assert_eq!(regions.remaining(root), free, "and nothing was charged");
    assert_eq!(regions.committed(), committed);
    assert_eq!(regions.allocated(root), allocated);
    assert_eq!(regions.live_regions(), region::MAX_REGIONS);
    assert!(regions.accounting_holds());
}

/// A grant is charged once, returned once, and the receipt is what says so.
#[test]
fn a_grant_charge_is_made_and_undone_exactly_once() {
    const FRAME: usize = 4096;
    let (mut regions, root) = endowed();
    let allowance = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");

    let charge = regions
        .charge_grant(allowance, 3 * 1024 * 1024, FRAME)
        .expect("funded");
    assert_eq!(charge.charged(), 3 * 1024 * 1024);
    assert_eq!(regions.remaining(allowance), Ok(5 * 1024 * 1024));
    assert_eq!(regions.committed(), 3 * 1024 * 1024);
    assert!(regions.accounting_holds());

    regions.refund_grant(charge).expect("the grant returns");
    assert_eq!(regions.remaining(allowance), Ok(8 * 1024 * 1024));
    assert_eq!(regions.committed(), 0);
    assert!(regions.accounting_holds());
}

/// A repeated refund cannot invent budget.
///
/// Production cannot express this at all — the receipt is moved into the
/// refund and there is no second one — so the test forges a duplicate and
/// watches the accounting refuse it. Both refusals matter: while the node is
/// still live the claim exceeds what it holds, and once it has drained the
/// receipt names nothing.
#[test]
fn a_second_refund_of_one_charge_is_refused_and_changes_nothing() {
    const FRAME: usize = 4096;
    let (mut regions, root) = endowed();
    let allowance = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let charge = regions
        .charge_grant(allowance, 2 * 1024 * 1024, FRAME)
        .expect("funded");
    let forged = charge.forged_duplicate();

    regions.refund_grant(charge).expect("the grant returns");
    let after = regions.remaining(allowance);
    assert_eq!(after, Ok(8 * 1024 * 1024));

    // The charge was struck out of the ledger when it settled, so the duplicate
    // names nothing — whatever the authority happens to be holding.
    assert_eq!(
        regions.refund_grant(forged),
        Err(Refusal::NotFound),
        "a settled charge is not findable a second time"
    );
    assert_eq!(regions.remaining(allowance), after);
    assert_eq!(regions.remaining(root), Ok(ROOT - 8 * 1024 * 1024));
    assert_eq!(regions.committed(), 0);
    assert!(regions.accounting_holds());
}

/// The counterexample the ledger exists for.
///
/// One authority funds two processes of the same size. The first returns
/// normally; a duplicate of its receipt then arrives while the second is still
/// running. Judged against the authority's total the duplicate is plausible —
/// the node is holding exactly what it claims — and taking it would return the
/// *second* process's bytes while that process is still using them. The charge
/// has its own identity, so there is nothing plausible about it.
#[test]
fn a_duplicate_cannot_take_another_live_charge_of_the_same_authority() {
    const FRAME: usize = 4096;
    const GRANT: usize = 54 * 1024 * 1024;
    const POOL: usize = 256 * 1024 * 1024;
    let mut regions = Regions::new();
    let root = regions.endow_root(POOL).expect("endowed");

    let first = regions.charge_grant(root, GRANT, FRAME).expect("funded");
    let second = regions.charge_grant(root, GRANT, FRAME).expect("funded");
    assert_eq!(regions.committed(), 2 * GRANT);
    let forged = first.forged_duplicate();

    regions.refund_grant(first).expect("the first returns");
    assert_eq!(regions.committed(), GRANT, "the second is still running");
    let after = regions.remaining(root);

    assert_eq!(
        regions.refund_grant(forged),
        Err(Refusal::NotFound),
        "a settled charge is not findable, whatever else the authority holds"
    );
    assert_eq!(regions.committed(), GRANT, "and the second is untouched");
    assert_eq!(regions.remaining(root), after);
    assert!(regions.accounting_holds());

    // The second still settles normally, once, for exactly its own bytes.
    regions.refund_grant(second).expect("the second returns");
    assert_eq!(regions.committed(), 0);
    assert_eq!(regions.remaining(root), Ok(POOL));
    assert!(regions.accounting_holds());
}

/// Revoking the capability does not move the grant it funded.
///
/// The receipt names an accounting incarnation, and a revoke bumps the
/// *generation*. So the capability stops resolving, the process it paid for
/// goes on running, and when it ends its bytes travel down the lineage that
/// actually paid — past the unnamed node to its parent.
#[test]
fn a_grant_returns_through_its_funder_after_the_capability_is_revoked() {
    const FRAME: usize = 4096;
    let (mut regions, root) = endowed();
    let allowance = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let charge = regions
        .charge_grant(allowance, 4 * 1024 * 1024, FRAME)
        .expect("funded");

    regions.revoke(allowance).expect("revoked");
    assert_eq!(
        regions.remaining(allowance),
        Err(Refusal::NotFound),
        "the capability names nothing any more"
    );
    assert_eq!(
        regions.remaining(root),
        Ok(ROOT - 4 * 1024 * 1024),
        "the unspent half came back and the funded half did not"
    );
    assert_eq!(regions.committed(), 4 * 1024 * 1024);
    assert!(regions.accounting_holds());

    // The receipt still resolves: it was never about the capability.
    regions
        .refund_grant(charge)
        .expect("the grant returns down the lineage that funded it");
    assert_eq!(regions.remaining(root), Ok(ROOT));
    assert_eq!(regions.committed(), 0);
    assert!(regions.accounting_holds());
}

/// A drained slot handed to a new occupant stops honouring the old one's
/// receipts.
#[test]
fn a_stale_receipt_cannot_credit_the_new_occupant_of_a_slot() {
    const FRAME: usize = 4096;
    let (mut regions, root) = endowed();
    let first = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let charge = regions
        .charge_grant(first, 2 * 1024 * 1024, FRAME)
        .expect("funded");
    let stale = charge.forged_duplicate();

    // The first occupant drains completely: its grant returns and its
    // capability goes, so the node is retired and its slot is free.
    regions.refund_grant(charge).expect("the grant returns");
    regions.revoke(first).expect("revoked");
    assert_eq!(regions.remaining(root), Ok(ROOT));

    // A new authority takes the same slot, and the old receipt is not a gift
    // to it.
    let second = regions.attenuate(root, 1024 * 1024).expect("reserved");
    let before = regions.remaining(second);
    assert_eq!(before, Ok(1024 * 1024));
    assert_eq!(
        regions.refund_grant(stale),
        Err(Refusal::NotFound),
        "a receipt from a previous occupant of the slot names nothing"
    );
    assert_eq!(regions.remaining(second), before);
    assert_eq!(regions.remaining(root), Ok(ROOT - 1024 * 1024));
    assert_eq!(regions.committed(), 0);
    assert!(regions.accounting_holds());
}

/// The two ways a charge is refused are not the same answer (ADR-0076 §7).
///
/// A size whose rounding overflows could not be served by any budget, and a
/// caller told `E_LIMIT` would retry it forever. A size that simply exceeds
/// what is left is a different fact about a different day.
#[test]
fn an_impossible_size_and_an_unaffordable_one_are_different_refusals() {
    const FRAME: usize = 4096;
    let (mut regions, root) = endowed();
    let before = regions.remaining(root);

    assert_eq!(
        regions.charge_grant(root, usize::MAX, FRAME),
        Err(Refusal::BadArgument),
        "rounding that would wrap is a domain failure"
    );
    assert_eq!(regions.remaining(root), before, "and it changed nothing");
    assert!(regions.accounting_holds());

    assert_eq!(
        regions.charge_grant(root, ROOT + 1, FRAME),
        Err(Refusal::Budget),
        "a figure that rounds cleanly and does not fit is a budget failure"
    );
    assert_eq!(regions.remaining(root), before, "and it changed nothing");
    assert!(regions.accounting_holds());

    // The same split on the region path, which charges through the same
    // arithmetic.
    assert_eq!(
        regions.allocate_rounded(root, usize::MAX, FRAME, WORKER),
        Err(Refusal::BadArgument)
    );
    assert_eq!(
        regions.allocate_rounded(root, ROOT + 1, FRAME, WORKER),
        Err(Refusal::Budget)
    );
    assert_eq!(regions.remaining(root), before);
    assert_eq!(regions.committed(), 0);
    assert!(regions.accounting_holds());
}

/// A reservation is a guarantee, not a ceiling (ADR-0076 §2b).
///
/// The whole of the distinction in one sequence: attenuation moves the *right*
/// at once and no frame with it; spending moves frames and leaves the rest
/// guaranteed to the child; and a sibling cannot have the unspent part however
/// physically free it looks.
#[test]
fn attenuation_reserves_at_once_and_commits_nothing() {
    const POOL: usize = 200 * 1024 * 1024;
    const ALLOWANCE: usize = 100 * 1024 * 1024;
    const SPENT: usize = 18 * 1024 * 1024;
    let mut regions = Regions::new();
    let root = regions.endow_root(POOL).expect("endowed");

    let a = regions.attenuate(root, ALLOWANCE).expect("reserved");
    assert_eq!(
        regions.remaining(root),
        Ok(POOL - ALLOWANCE),
        "the right left the parent the moment the child existed"
    );
    assert_eq!(
        regions.committed(),
        0,
        "and not one frame moved because of it"
    );
    assert!(regions.accounting_holds());

    // The child spends part of what it holds.
    let region = handed_over(&mut regions, a, SPENT, WORKER);
    assert_eq!(regions.committed(), SPENT);
    assert_eq!(regions.remaining(a), Ok(ALLOWANCE - SPENT));

    // A sibling cannot have the unspent 82 MiB, physically free though they
    // are. That is the reservation, not a leak: an authority that could be
    // spent by somebody else would be a limit, which is a different mechanism.
    let b = regions
        .attenuate(root, POOL - ALLOWANCE + 1)
        .expect_err("the reserved remainder is not the parent's to hand out");
    assert_eq!(b, Refusal::Budget);
    let b = regions
        .attenuate(root, POOL - ALLOWANCE)
        .expect("what the parent still holds, it may hand out");
    assert_eq!(regions.remaining(b), Ok(POOL - ALLOWANCE));
    assert_eq!(regions.committed(), SPENT, "still only what was spent");
    assert!(regions.accounting_holds());

    // Revoked with an allocation still live: the unspent remainder goes back at
    // once, the live bytes do not, and they follow the same lineage when they
    // are finally released.
    regions.revoke(a).expect("revoked");
    assert_eq!(
        regions.remaining(root),
        Ok(ALLOWANCE - SPENT),
        "the unspent remainder came back and the live bytes did not"
    );
    assert_eq!(regions.committed(), SPENT);
    assert!(regions.accounting_holds());

    regions
        .release_capability(region)
        .expect("the last capability goes");
    retire(&mut regions);
    assert_eq!(regions.remaining(root), Ok(ALLOWANCE));
    assert_eq!(regions.committed(), 0);
    assert!(regions.accounting_holds());
}

/// A process dying takes its reservations and its own allocations, and nothing
/// that something else can still reach.
///
/// The last part is what `Shared<Region<T>>` will rest on: a holder's death
/// must not free backing another holder is still reading, so reclamation is
/// driven by what can reach the region rather than by who has died.
#[test]
fn a_death_returns_what_only_the_dead_process_could_reach() {
    const POOL: usize = 64 * 1024 * 1024;
    const ALLOWANCE: usize = 32 * 1024 * 1024;
    let mut regions = Regions::new();
    let root = regions.endow_root(POOL).expect("endowed");
    let allowance = regions.attenuate(root, ALLOWANCE).expect("reserved");

    // Two regions out of the dying process's authority. One is its own; the
    // other has been frozen and handed on, so somebody else holds it.
    let private = handed_over(&mut regions, allowance, 4 * 1024 * 1024, WORKER);
    let handed_on = handed_over(&mut regions, allowance, 2 * 1024 * 1024, WORKER);
    regions.map(handed_on, WORKER, true).expect("mapped");
    regions.freeze(handed_on, WORKER).expect("frozen");
    regions
        .retain_internal(handed_on)
        .expect("a message becomes its holder");
    regions.detach(handed_on, WORKER).expect("handed on");
    regions
        .release_capability(handed_on)
        .expect("the sender's handle goes with it");
    regions.adopt(handed_on, SUPERVISOR).expect("and arrives");
    regions
        .map(handed_on, SUPERVISOR, false)
        .expect("the receiver reads it");
    regions
        .retain_capability(handed_on)
        .expect("under its own name");
    regions
        .release_internal(handed_on)
        .expect("the message lets go last");
    regions
        .map(private, WORKER, true)
        .expect("its own is mapped");
    assert_eq!(regions.committed(), 6 * 1024 * 1024);

    // The process dies: its table entries go one at a time, and its address
    // space takes its windows.
    regions
        .release_capability(private)
        .expect("its one entry over its own region");
    regions.mappings_destroyed(WORKER);
    retire(&mut regions);
    regions.revoke(allowance).expect("its authority is revoked");

    assert_eq!(
        regions.mode(private),
        Err(Refusal::NotFound),
        "what only it could reach is gone"
    );
    assert_eq!(
        regions.committed(),
        2 * 1024 * 1024,
        "and what somebody else still reaches is not freed by a death"
    );
    assert_eq!(
        regions.remaining(root),
        Ok(POOL - 2 * 1024 * 1024),
        "the unspent reservation came back; the live backing did not"
    );
    assert_eq!(
        regions.remaining(allowance),
        Err(Refusal::NotFound),
        "the dead process's authority names nothing"
    );
    assert!(regions.accounting_holds());

    // When the surviving holder lets go, the backing follows the lineage that
    // funded it, past the node that no capability names any more.
    regions
        .unmap(handed_on, SUPERVISOR, false)
        .expect("the mapping goes");
    retire(&mut regions);
    regions
        .release_capability(handed_on)
        .expect("the capability goes");
    retire(&mut regions);
    assert_eq!(regions.remaining(root), Ok(POOL));
    assert_eq!(regions.committed(), 0);
    assert!(regions.accounting_holds());
}

/// A stopped tree refuses to grow anywhere, and still lets everything shrink.
///
/// The latch cannot live at the root's accessor. Once a process holds a child
/// authority of its own — which is what operation 16 exists to give it —
/// reserving and spending never touch the root again, so a rule enforced there
/// would be enforced nowhere. It is the tree that refuses.
#[test]
fn a_stopped_tree_refuses_to_grow_and_still_gives_back() {
    const POOL: usize = 64 * 1024 * 1024;
    const FRAME: usize = 4096;
    let mut regions = Regions::new();
    let root = regions.endow_root(POOL).expect("endowed");
    let child = regions
        .attenuate(root, 32 * 1024 * 1024)
        .expect("a child the process holds");
    let region = handed_over(&mut regions, child, 4 * 1024 * 1024, WORKER);
    let charge = regions
        .charge_grant(child, 8 * 1024 * 1024, FRAME)
        .expect("something it already funded");

    let free = regions.remaining(child);
    let committed = regions.committed();
    regions.poison();
    assert!(regions.stopped());

    // Nothing anywhere in the tree may reserve or spend, and the child is the
    // one that matters: it never asks the root anything.
    assert_eq!(
        regions.attenuate(child, 1024 * 1024),
        Err(Refusal::Stopped),
        "a reservation out of a child is still a reservation"
    );
    assert_eq!(
        regions.allocate(child, 1024 * 1024, WORKER),
        Err(Refusal::Stopped)
    );
    assert_eq!(
        regions.charge_grant(child, 1024 * 1024, FRAME),
        Err(Refusal::Stopped),
        "and a funded creation is a spend like any other"
    );
    assert_eq!(regions.attenuate(root, 1024 * 1024), Err(Refusal::Stopped));
    assert_eq!(regions.allocate(root, 1024, WORKER), Err(Refusal::Stopped));
    // **What ring 3 is told, and why it is not a lie.** `Refusal::Stopped` has
    // no word in the accepted status space of `SYSTEM_ABI_V1` §4, which is
    // closed, so operations 16 and 17 answer the nearest accepted resource
    // refusal: everything but `Empty` and `BadArgument` becomes `E_LIMIT`, and
    // this is one of them. The caller is refused fail-closed, the nucleus has
    // already said `TOS.NUCLEUS.INVARIANT … funding-stopped` on the console,
    // and the refusal is not evidence that the authority's own budget ran out.
    // A state machine that produced `Stopped` where the syscall maps it to
    // something else is the only way that could go wrong, so the mapping is
    // asserted where the state is produced.
    assert!(matches!(
        regions.allocate(root, 1024, WORKER),
        Err(Refusal::Stopped)
    ));
    assert_eq!(
        regions.remaining(child),
        free,
        "and none of it changed a byte"
    );
    assert_eq!(regions.committed(), committed);
    assert!(regions.accounting_holds());

    // What must keep working is everything that makes the machine smaller: the
    // operator has to be able to reach a state worth inspecting.
    regions
        .refund_grant(charge)
        .expect("a charge still returns");
    regions
        .release_capability(region)
        .expect("a region still releases");
    retire(&mut regions);
    assert_eq!(regions.committed(), 0);
    regions.revoke(child).expect("an authority still revokes");
    assert_eq!(
        regions.remaining(root),
        Ok(POOL),
        "and every unused reservation came back"
    );
    assert!(regions.accounting_holds());
}

/// Several names, one budget: releasing one alias returns nothing.
#[test]
fn only_the_last_name_of_an_authority_returns_its_remainder() {
    const POOL: usize = 200 * 1024 * 1024;
    const ALLOWANCE: usize = 100 * 1024 * 1024;
    let mut regions = Regions::new();
    let root = regions.endow_root(POOL).expect("endowed");
    let allowance = regions
        .attenuate(root, ALLOWANCE)
        .expect("the supervisor's own name for it");
    assert_eq!(regions.names(allowance), Ok(1));

    // A second name — an alias from generic attenuation, an endowment entry, a
    // delegation. Not a second reservation: the parent's remainder does not
    // move and nothing is committed.
    let before = regions.remaining(root);
    regions.retain(allowance).expect("the worker's name for it");
    assert_eq!(regions.names(allowance), Ok(2));
    assert_eq!(regions.remaining(root), before, "an alias reserves nothing");
    assert_eq!(regions.committed(), 0, "and commits nothing");
    assert_eq!(regions.remaining(allowance), Ok(ALLOWANCE));

    // One of the two goes. The authority is still there and still works.
    regions.release_name(allowance).expect("one name goes");
    assert_eq!(
        regions.remaining(root),
        before,
        "releasing one alias returns nothing"
    );
    assert_eq!(
        regions.remaining(allowance),
        Ok(ALLOWANCE),
        "and the other name still resolves"
    );
    let spent = handed_over(&mut regions, allowance, 8 * 1024 * 1024, WORKER);

    // The last one goes: now the unused remainder returns, and the node lives
    // on only because something it funded is still alive.
    regions.release_name(allowance).expect("the last name goes");
    assert_eq!(
        regions.remaining(allowance),
        Err(Refusal::NotFound),
        "a stale handle fails once the generation has moved"
    );
    assert_eq!(
        regions.remaining(root),
        Ok(POOL - 8 * 1024 * 1024),
        "the unused remainder came back and the live backing did not"
    );
    assert!(regions.accounting_holds());

    regions
        .release_capability(spent)
        .expect("the backing is let go");
    retire(&mut regions);
    assert_eq!(regions.remaining(root), Ok(POOL));
    assert!(regions.accounting_holds());
}

/// A delegation that has committed but not arrived still names the authority.
///
/// The interval the model has to survive: the send is done, the sender may be
/// gone, and the receiver has no table entry yet. If the count passed through
/// zero there, the reservation would return to the parent and the message would
/// deliver an authority that no longer exists.
#[test]
fn an_authority_in_transit_never_returns_to_its_parent() {
    const POOL: usize = 64 * 1024 * 1024;
    const ALLOWANCE: usize = 16 * 1024 * 1024;
    let mut regions = Regions::new();
    let root = regions.endow_root(POOL).expect("endowed");
    let allowance = regions.attenuate(root, ALLOWANCE).expect("the sender's");
    let held = regions.remaining(root);

    // The send commits: the message's reference is taken *before* the sender's
    // handle is dropped, so the count never reaches zero.
    regions.retain(allowance).expect("the message names it");
    regions
        .release_name(allowance)
        .expect("and the sender's handle goes with the send");
    assert_eq!(regions.names(allowance), Ok(1));
    assert_eq!(
        regions.remaining(root),
        held,
        "nothing came back while the message was in flight"
    );

    // The sender dies before anybody receives. Still nothing comes back.
    regions.mappings_destroyed(WORKER);
    retire(&mut regions);
    assert_eq!(regions.remaining(root), held);
    assert_eq!(
        regions.remaining(allowance),
        Ok(ALLOWANCE),
        "and the authority is intact"
    );

    // The receiver acquires its own handle before the message's reference goes.
    regions.retain(allowance).expect("the receiver's handle");
    regions
        .release_name(allowance)
        .expect("and the message is done with it");
    assert_eq!(regions.names(allowance), Ok(1));
    assert_eq!(regions.remaining(allowance), Ok(ALLOWANCE));
    assert_eq!(regions.remaining(root), held);
    assert!(regions.accounting_holds());

    // A send that never commits takes no reference at all, so the sender is
    // left holding exactly what it had.
    assert_eq!(regions.names(allowance), Ok(1));
    regions
        .release_name(allowance)
        .expect("the receiver lets go");
    assert_eq!(regions.remaining(root), Ok(POOL));
    assert!(regions.accounting_holds());
}

/// The root is the boot's anchor and no process's property.
#[test]
fn the_root_outlives_every_name_and_is_never_settled() {
    const POOL: usize = 64 * 1024 * 1024;
    let mut regions = Regions::new();
    let root = regions.endow_root(POOL).expect("endowed");

    // A supervisor is funded and given an allowance that is a child of the
    // root, never the root itself.
    let charge = regions
        .charge_grant(root, 8 * 1024 * 1024, 4096)
        .expect("the supervisor's footprint");
    let allowance = regions
        .attenuate(root, POOL - 8 * 1024 * 1024)
        .expect("everything left, as a child");
    assert_eq!(
        regions.remaining(root),
        Ok(0),
        "the root keeps nothing back"
    );

    // The supervisor ends: its allowance and its footprint both come home, and
    // the root is still there to receive them.
    regions.release_name(allowance).expect("its allowance goes");
    regions.refund_grant(charge).expect("its footprint returns");
    assert_eq!(
        regions.remaining(root),
        Ok(POOL),
        "the anchor survived the process that was funded from it"
    );
    assert_eq!(regions.names(root), Ok(1), "and is still the boot's");
    assert!(regions.accounting_holds());

    // And the root has no naming lifecycle at all. Surviving these would not be
    // enough: a caller able to drive its count to zero would be a caller able
    // to unmake the anchor, however carefully the settling avoided retiring it.
    assert_eq!(regions.retain(root), Err(Refusal::Anchored));
    assert_eq!(regions.release_name(root), Err(Refusal::Anchored));
    assert_eq!(regions.revoke(root), Err(Refusal::Anchored));
    assert_eq!(regions.names(root), Ok(1));
    assert_eq!(regions.remaining(root), Ok(POOL));

    // What it does have is the operations a funding anchor needs.
    let again = regions.charge_grant(root, 4096, 4096).expect("still funds");
    let child = regions
        .attenuate(root, 1024 * 1024)
        .expect("still reserves");
    regions.refund_grant(again).expect("still receives refunds");
    regions
        .release_name(child)
        .expect("and its children are ordinary");
    assert_eq!(regions.remaining(root), Ok(POOL));
    assert!(regions.accounting_holds());
}

/// A preflight has to be able to ask without doing.
///
/// The endowment commit is only infallible if every fallible step was asked in
/// advance, and taking names is one of them. The question is a *sum*: an
/// endowment naming one authority three times costs it three names, and asking
/// three times whether one more would fit is a different question that happens
/// to have the same answer until it does not.
#[test]
fn whether_names_can_be_taken_is_answerable_without_taking_them() {
    let (mut regions, root) = endowed();
    let child = regions.attenuate(root, 1024 * 1024).expect("reserved");

    assert!(regions.can_retain(child, 1));
    assert!(regions.can_retain(child, 3), "and several at once");
    assert_eq!(regions.names(child), Ok(1), "having taken none of them");

    // The anchor has no naming lifecycle, so nothing can be taken of it — which
    // the preflight has to know before the commit tries.
    assert!(!regions.can_retain(root, 1));

    // A handle that names nothing cannot be retained either.
    regions.release_name(child).expect("the last name goes");
    assert!(!regions.can_retain(child, 1));
    assert!(regions.accounting_holds());
}

/// A region in transit is reachable by nothing a process holds, and lives.
///
/// The interval a committed delegation opens: the sender's handle and mapping
/// were taken from it atomically (ADR-0075 §5a) and the receiver has nothing
/// yet. Capability references and mappings are both zero, and the region must
/// still be there when the message arrives — which is why ADR-0075 §6 makes
/// reclamation wait on three counts rather than one.
#[test]
fn an_internal_reference_keeps_a_region_no_process_can_reach() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let sent = handed_over(&mut regions, authority, 2 * 1024 * 1024, WORKER);
    regions
        .map(sent, WORKER, true)
        .expect("the sender writes it");
    regions.freeze(sent, WORKER).expect("and freezes it");

    // The send commits: the message takes its reference before the sender's
    // handle and mapping go, so nothing is ever unreferenced.
    regions.retain_internal(sent).expect("the message names it");
    assert_eq!(regions.internal(sent), Ok(1));
    regions
        .detach(sent, WORKER)
        .expect("the sender's window and ownership go");
    retire(&mut regions);
    regions
        .release_capability(sent)
        .expect("and the sender's handle");
    retire(&mut regions);
    assert_eq!(
        regions.allocated(root),
        Ok(2 * 1024 * 1024),
        "nothing a process holds names it, and it is still there"
    );

    // The sender dies before anybody receives. Still there.
    regions.mappings_destroyed(WORKER);
    retire(&mut regions);
    assert_eq!(regions.allocated(root), Ok(2 * 1024 * 1024));
    assert!(regions.accounting_holds());

    // The receiver acquires before the message lets go, so the region never
    // passes through unreachable.
    regions.adopt(sent, SUPERVISOR).expect("ownership arrives");
    regions
        .map(sent, SUPERVISOR, false)
        .expect("the receiver reads it");
    regions.retain_capability(sent).expect("under its own name");
    regions
        .release_internal(sent)
        .expect("and the message is done");
    retire(&mut regions);
    assert_eq!(regions.internal(sent), Ok(0));
    assert_eq!(regions.allocated(root), Ok(2 * 1024 * 1024));

    // And when the last of the three goes, the backing returns.
    regions
        .release_capability(sent)
        .expect("the receiver's handle goes");
    regions
        .unmap(sent, SUPERVISOR, false)
        .expect("the receiver's mapping goes");
    retire(&mut regions);
    assert_eq!(regions.allocated(root), Ok(0));
    assert_eq!(regions.remaining(authority), Ok(8 * 1024 * 1024));
    assert!(regions.accounting_holds());

    // Releasing one that is not held is a defect, not a no-op.
    let other = handed_over(&mut regions, authority, 1024 * 1024, WORKER);
    assert_eq!(regions.release_internal(other), Err(Refusal::BadArgument));
    retire(&mut regions);
    assert!(regions.accounting_holds());
}

/// A region has one capability or none, and the object is what enforces it.
///
/// Operation 5 refuses to alias a region, but a refusal that lives in one
/// operation is a refusal every future operation has to remember. This is the
/// one that does not have to be remembered.
#[test]
fn a_region_refuses_a_second_capability() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let region = handed_over(&mut regions, authority, 1024 * 1024, WORKER);

    assert!(!regions.can_name(region), "one holder, and it is taken");
    assert_eq!(
        regions.retain_capability(region),
        Err(Refusal::NotTheHolder),
        "a second handle is the writable alias the freeze exists to remove"
    );

    // And giving up one nobody holds is a defect rather than a repeat.
    regions
        .release_capability(region)
        .expect("the holder lets go");
    retire(&mut regions);
    assert_eq!(
        regions.allocated(root),
        Ok(0),
        "and the backing went with the last way of reaching it"
    );
    assert!(regions.accounting_holds());
}

/// An impossible unmapping is refused, not absorbed.
///
/// The mapping counts decide when backing may be reclaimed, so a count that
/// clamps an impossible operation to zero is a count that cannot be trusted to
/// make that decision.
#[test]
fn an_unmapping_of_nothing_is_a_defect() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 4 * 1024 * 1024).expect("reserved");
    let region = handed_over(&mut regions, authority, 1024 * 1024, WORKER);

    assert_eq!(regions.mappings(region), Ok((0, 0)), "nothing maps it yet");
    assert_eq!(
        regions.mapped_by(region, WORKER),
        Ok(None),
        "and this process is not among them"
    );
    assert_eq!(
        regions.unmap(region, WORKER, false),
        Err(Refusal::BadArgument)
    );
    retire(&mut regions);
    assert_eq!(
        regions.unmap(region, WORKER, true),
        Err(Refusal::BadArgument)
    );
    retire(&mut regions);
    regions.map(region, WORKER, true).expect("mapped");
    assert_eq!(
        regions.mappings(region),
        Ok((1, 0)),
        "and the counts say which kind, not only how many"
    );
    assert_eq!(
        regions.mapped_by(region, WORKER),
        Ok(Some(true)),
        "and which address space, which is what a count cannot say"
    );
    regions.unmap(region, WORKER, true).expect("and unmapped");
    retire(&mut regions);
    assert_eq!(
        regions.unmap(region, WORKER, true),
        Err(Refusal::BadArgument),
        "and not twice"
    );
    retire(&mut regions);
    assert_eq!(regions.allocated(root), Ok(1024 * 1024));
    assert!(regions.accounting_holds());
}

/// Memory is not free when the last reference goes; it is free when the frames
/// are back.
///
/// Between those two moments the backing still belongs to the pool and the
/// lane's tables to the reserve. An authority credited at the first would be
/// handing out memory `Frames` still holds — the two-counters defect wearing a
/// different hat — so the receipt stands between them and the order cannot be
/// skipped.
#[test]
fn a_region_is_credited_back_only_after_its_backing_is() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let region = handed_over(&mut regions, authority, 2 * 1024 * 1024, WORKER);

    assert_eq!(regions.reclaimable(), None, "nothing to retire yet");
    assert_eq!(regions.live_regions(), 1);
    assert_eq!(
        regions.length(region),
        Ok(2 * 1024 * 1024),
        "and its length is what was charged, not what was asked for"
    );
    regions
        .release_capability(region)
        .expect("the only holder lets go");

    // Nothing can reach it, and not a byte has come back.
    assert_eq!(
        regions.allocated(root),
        Ok(2 * 1024 * 1024),
        "the bytes are still spent until the frames are back"
    );
    assert_eq!(
        regions.mode(region),
        Err(Refusal::NotFound),
        "and it resolves to nothing while it waits"
    );

    // The nucleus finds it, takes responsibility, and only then credits.
    let pending = regions.reclaimable().expect("one region awaits retirement");
    let ticket = regions.take_reclaim(pending).expect("taken");
    assert_eq!(regions.reclaim_bytes(&ticket), 2 * 1024 * 1024);
    assert_eq!(
        regions.take_reclaim(pending),
        Err(Refusal::NotFound),
        "and nobody else can take the same one"
    );
    assert_eq!(
        regions.allocated(root),
        Ok(2 * 1024 * 1024),
        "still spent while the physical work is outstanding"
    );

    regions.finish_reclaim(ticket).expect("the frames are back");
    assert_eq!(regions.allocated(root), Ok(0));
    assert_eq!(regions.remaining(authority), Ok(8 * 1024 * 1024));
    assert_eq!(regions.reclaimable(), None);
    assert!(regions.accounting_holds());

    // And the slot is reusable only now, with a generation that makes the old
    // handle name nothing.
    assert_eq!(regions.live_regions(), 0, "and the slot is nobody's");
    let next = handed_over(&mut regions, authority, 1024 * 1024, WORKER);
    assert_eq!(regions.mode(region), Err(Refusal::NotFound));
    assert_eq!(regions.mode(next), Ok(Mode::Mutable));
    assert!(regions.accounting_holds());
}

/// The mode is what a caller can observe about a region, and it only goes one
/// way — through all three states, and never back.
#[test]
fn the_mode_is_one_way() {
    let (mut regions, root) = endowed();
    let region = handed_over(&mut regions, root, 4096, WORKER);
    assert_eq!(regions.mode(region), Ok(Mode::Mutable));
    assert!(regions.mode(region).is_ok_and(|mode| mode.is_affine()));

    // Neither transition may be reached out of order, and each needs the
    // holder's own window to be exactly what the state says it is: a freeze
    // that declared a region unwritable while a writable mapping it had not
    // accounted for stood would be declaring something it had not checked.
    assert_eq!(regions.freeze(region, WORKER), Err(Refusal::WrongMode));
    regions.map(region, WORKER, true).expect("mapped writably");
    assert_eq!(regions.share(region, WORKER), Err(Refusal::WrongMode));

    regions.freeze(region, WORKER).expect("frozen");
    assert_eq!(regions.mode(region), Ok(Mode::Immutable));
    assert!(regions.mode(region).is_ok_and(|mode| mode.is_affine()));

    regions.share(region, WORKER).expect("shared");
    assert_eq!(regions.mode(region), Ok(Mode::Shared));
    assert!(!regions.mode(region).is_ok_and(|mode| mode.is_affine()));
    assert_eq!(regions.freeze(region, WORKER), Err(Refusal::WrongMode));
    assert_eq!(regions.share(region, WORKER), Err(Refusal::WrongMode));
}
