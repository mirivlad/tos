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
//!          -> sender gone -> receiver reads -> release -> reclaimed
//! ```

#[path = "../../../nucleus/src/region.rs"]
mod region;

use region::{Mode, Refusal, Regions};

const ROOT: usize = 64 * 1024 * 1024;
const WORKER: u32 = 7;
const SUPERVISOR: u32 = 3;

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
    let _region = regions
        .allocate(grandchild, 1024 * 1024, WORKER)
        .expect("allocated");
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

/// The whole primitive, in the order it will be used.
#[test]
fn a_region_is_allocated_frozen_transferred_and_reclaimed() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let bundle = regions
        .allocate(authority, 2 * 1024 * 1024, WORKER)
        .expect("allocated");

    // Writable while it is being written.
    regions
        .map(bundle, true)
        .expect("a writer maps it writably");
    assert_eq!(regions.writable_aliases(bundle), Ok(2));

    // Frozen, and the postcondition is the nucleus's.
    regions
        .freeze(bundle, WORKER)
        .expect("the holder freezes it");
    assert_eq!(
        regions.writable_aliases(bundle),
        Ok(0),
        "no writable alias survives the transition"
    );
    assert_eq!(
        regions.map(bundle, true),
        Err(Refusal::WrongMode),
        "and none can be made afterwards"
    );
    assert_eq!(
        regions.freeze(bundle, WORKER),
        Err(Refusal::WrongMode),
        "the transition has no inverse and cannot be repeated"
    );

    // Handed on: linear, so the sender keeps nothing.
    regions.map(bundle, false).expect("the worker reads it");
    regions
        .transfer(bundle, WORKER, SUPERVISOR)
        .expect("ownership moves");
    assert_eq!(
        regions.transfer(bundle, WORKER, SUPERVISOR),
        Err(Refusal::NotTheHolder),
        "the sender is not the holder any more"
    );

    // The receiver reads it, and the last release reclaims the backing.
    regions.map(bundle, false).expect("the receiver reads it");
    assert_eq!(regions.allocated(root), Ok(2 * 1024 * 1024));
    regions.unmap(bundle, false).expect("the mapping goes");
    regions.release(bundle).expect("the last capability goes");
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
    let held = regions
        .allocate(authority, 1024 * 1024, WORKER)
        .expect("allocated");
    regions.map(held, false).expect("mapped");

    regions.release(held).expect("the capability goes");
    assert_eq!(
        regions.allocated(root),
        Ok(1024 * 1024),
        "a mapping still reaches it, so the backing stays"
    );
    regions
        .unmap(held, false)
        .expect("and now the mapping goes");
    assert_eq!(regions.allocated(root), Ok(0));
    assert!(regions.accounting_holds());
}

/// Revoking an authority returns what is unspent and leaves what is not.
#[test]
fn revocation_returns_the_remainder_and_never_the_live_backing() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let region = regions
        .allocate(authority, 3 * 1024 * 1024, WORKER)
        .expect("allocated");

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
        .release(region)
        .expect("the region's holder lets go");
    assert_eq!(regions.remaining(root), Ok(ROOT));
    assert_eq!(regions.allocated(root), Ok(0));
    assert!(regions.accounting_holds());
}

/// A process ending takes its handles and its mappings with it.
#[test]
fn a_process_ending_reclaims_what_only_it_could_reach() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 8 * 1024 * 1024).expect("reserved");
    let region = regions
        .allocate(authority, 2 * 1024 * 1024, WORKER)
        .expect("allocated");
    regions.map(region, true).expect("mapped writably");

    regions.process_died(WORKER);
    assert_eq!(
        regions.allocated(root),
        Ok(0),
        "nothing could reach it once its holder was gone"
    );
    assert_eq!(regions.remaining(authority), Ok(8 * 1024 * 1024));
    assert!(regions.accounting_holds());
}

/// What the primitive refuses.
#[test]
fn the_negatives_are_refusals_and_not_surprises() {
    let (mut regions, root) = endowed();
    let authority = regions.attenuate(root, 4 * 1024 * 1024).expect("reserved");
    let region = regions
        .allocate(authority, 1024 * 1024, WORKER)
        .expect("allocated");

    // A mutable region is not transferable at all (ADR-0037).
    assert_eq!(
        regions.transfer(region, WORKER, SUPERVISOR),
        Err(Refusal::WrongMode)
    );
    // Only the sole holder may freeze.
    assert_eq!(
        regions.freeze(region, SUPERVISOR),
        Err(Refusal::NotTheHolder)
    );
    // Zero-sized authority and zero-sized region are authority over nothing.
    assert_eq!(regions.allocate(authority, 0, WORKER), Err(Refusal::Empty));
    assert_eq!(regions.endow_root(0), Err(Refusal::Empty));

    // A stale handle names nothing: the region is released and its id no longer
    // resolves, whatever the generation counter has moved on to.
    regions.release(region).expect("released");
    assert_eq!(regions.release(region), Err(Refusal::NotFound));
    assert_eq!(regions.freeze(region, WORKER), Err(Refusal::NotFound));
    assert!(regions.accounting_holds());
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
}

/// The mode is what a caller can observe about a region, and it only goes one
/// way.
#[test]
fn the_mode_is_one_way() {
    let (mut regions, root) = endowed();
    let region = regions.allocate(root, 4096, WORKER).expect("allocated");
    assert_eq!(regions.mode(region), Ok(Mode::Mutable));
    regions.freeze(region, WORKER).expect("frozen");
    assert_eq!(regions.mode(region), Ok(Mode::Immutable));
}
