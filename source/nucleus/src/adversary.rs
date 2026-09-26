// SPDX-License-Identifier: GPL-3.0-or-later
//! A bus master that writes what it likes into a driver's DMA memory.
//!
//! **Test builds only, and it stands for exactly one adversary**: `docs/34` T7's
//! DMA-capable hardware, which on the no-IOMMU reference profile can write any
//! memory it is given an address for — including the device-visible memory a
//! driver will read the device's answer out of. The reference endpoint never does
//! that, so a driver's checks of what a device reports are unreachable from any
//! honest boot. This module makes them reachable, and does nothing else.
//!
//! **What it knows is bytes, not devices.** A rule is a routed delivery number, a
//! byte offset into the DMA region of the assignment that delivery came from, a
//! byte and a count — decided by the harness, compiled into this build through
//! `TOS_HOSTILE_DEVICE`, and applied at the delivery **before** the waiter is
//! woken, which is the moment a real device would have finished writing. What a
//! ring, a descriptor or a length is lives in the harness and in the driver's
//! canonical text, never here.
//!
//! Every write it makes is put on the record, so a boot whose driver refused
//! something shows what it was shown.

use crate::dma;

/// The rules, as the harness wrote them: `DELIVERY:OFFSET:BYTE[:COUNT]`, comma
/// separated, all decimal.
const RULES: Option<&str> = option_env!("TOS_HOSTILE_DEVICE");

/// Applies every rule for this delivery of a source descending from
/// `assignment`, and says what it wrote.
pub fn on_delivery(assignment: u32, generation: u32, delivery: u32) {
    let Some(rules) = RULES else {
        return;
    };
    for rule in rules.split(',') {
        let mut fields = rule.split(':').map(|field| field.parse::<u64>().ok());
        let (Some(Some(at)), Some(Some(offset)), Some(Some(byte))) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let count = match fields.next() {
            Some(Some(count)) => count,
            _ => 1,
        };
        if at != u64::from(delivery) || byte > 0xFF || count == 0 {
            continue;
        }
        let written = dma::overwrite(assignment, generation, offset, byte as u8, count);
        tos_serial::puts(b"TOS.RUN.HOSTILE_DEVICE delivery=");
        tos_serial::put_u32_decimal(delivery);
        tos_serial::puts(b" offset=");
        tos_serial::put_u32_decimal(offset as u32);
        tos_serial::puts(b" byte=");
        tos_serial::put_u32_decimal(byte as u32);
        tos_serial::puts(b" count=");
        tos_serial::put_u32_decimal(count as u32);
        tos_serial::puts(b" written=");
        tos_serial::put_u32_decimal(u32::from(written));
        tos_serial::puts(b" asserted_by=test-adversary\r\n");
    }
}
