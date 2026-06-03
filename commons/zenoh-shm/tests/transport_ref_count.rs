//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
#![cfg(feature = "test")]

use std::{sync::atomic::Ordering, time::Duration};

use zenoh_shm::{
    metadata::storage::GLOBAL_METADATA_STORAGE, watchdog::validator::GLOBAL_VALIDATOR,
};

// The validator runs every 100 ms. We wait 300 ms between checks to allow for
// at least two ticks regardless of when the first one fires.
const VALIDATOR_TICK_WAIT: Duration = Duration::from_millis(300);

/// When transport_ref_count > 0 the validator must not set watchdog_invalidated,
/// even if the watchdog bit was not kicked (i.e. no ConfirmedDescriptor is active).
/// This is the regression test for eclipse-zenoh/zenoh#2628: a chunk that was sent
/// over SHM transport but not yet mounted by the RX thread must not be invalidated
/// by the sender-side watchdog validator.
#[test]
fn validator_skips_invalidation_while_transport_ref_count_nonzero() {
    let descriptor = GLOBAL_METADATA_STORAGE
        .read()
        .allocate()
        .expect("allocate metadata descriptor");

    let header = descriptor.header();

    // Claim a transport reference (simulates TX side sending the chunk).
    // No ConfirmedDescriptor is registered, so the watchdog bit won't be kicked.
    header.transport_ref_count.store(1, Ordering::SeqCst);

    // Register with the validator so it will check this descriptor on the next tick.
    GLOBAL_VALIDATOR.read().add(descriptor.clone());

    // Wait for at least one validator tick.
    std::thread::sleep(VALIDATOR_TICK_WAIT);

    // The chunk is in transit: the validator must NOT have invalidated it.
    assert!(
        !header.watchdog_invalidated.load(Ordering::SeqCst),
        "validator invalidated a chunk with transport_ref_count > 0"
    );

    // Release the transport reference (simulates RX side completing read_shmbuf).
    header.transport_ref_count.store(0, Ordering::SeqCst);

    // Wait for at least one more validator tick.
    std::thread::sleep(VALIDATOR_TICK_WAIT);

    // Now transport_ref_count is 0 and no watchdog kicks are happening.
    // The validator must have invalidated the chunk.
    assert!(
        header.watchdog_invalidated.load(Ordering::SeqCst),
        "validator did not invalidate chunk after transport_ref_count reached 0"
    );
}

/// When both transport_ref_count and refcount are 0 and no watchdog kicks happen,
/// the validator must invalidate the chunk — same behaviour as before this change.
#[test]
fn validator_invalidates_when_both_counts_are_zero() {
    let descriptor = GLOBAL_METADATA_STORAGE
        .read()
        .allocate()
        .expect("allocate metadata descriptor");

    let header = descriptor.header();

    // No transport reference, no ConfirmedDescriptor — chunk is fully orphaned.
    assert_eq!(header.transport_ref_count.load(Ordering::SeqCst), 0);

    GLOBAL_VALIDATOR.read().add(descriptor.clone());

    std::thread::sleep(VALIDATOR_TICK_WAIT);

    assert!(
        header.watchdog_invalidated.load(Ordering::SeqCst),
        "validator did not invalidate fully orphaned chunk"
    );
}
