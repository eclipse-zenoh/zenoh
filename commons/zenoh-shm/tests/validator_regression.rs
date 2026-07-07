//
// Copyright (c) 2026 ZettaScale Technology
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

use std::{
    ops::Deref,
    sync::{Mutex, OnceLock},
};

use zenoh_core::Wait;
use zenoh_shm::{
    api::provider::shm_provider::ShmProviderBuilder,
    metadata::{
        descriptor::MetadataDescriptor, storage::GLOBAL_METADATA_STORAGE,
        subscription::GLOBAL_METADATA_SUBSCRIPTION,
    },
    watchdog::validator::GLOBAL_VALIDATOR,
};

pub mod common;
use common::execute_concurrent;

fn validator_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn plain_metadata_traffic_does_not_enqueue_validator_transactions() {
    let _guard = validator_test_lock().lock().unwrap();
    GLOBAL_VALIDATOR.read().test_reset_transaction_counters();

    let task = |_task_index: usize, _iteration: usize| {
        let allocated_metadata = GLOBAL_METADATA_STORAGE.read().allocate().unwrap();
        let descriptor = MetadataDescriptor::from(allocated_metadata.deref());
        let _linked_metadata = GLOBAL_METADATA_SUBSCRIPTION
            .read()
            .link(&descriptor)
            .unwrap();
    };

    execute_concurrent(100, 100, move |task_index, iteration| {
        task(task_index, iteration);
        Ok::<(), ()>(())
    });

    assert_eq!(GLOBAL_VALIDATOR.read().test_transaction_counts(), (0, 0));
}

#[test]
fn provider_tracked_buffers_still_enqueue_validator_transactions() {
    let _guard = validator_test_lock().lock().unwrap();
    GLOBAL_VALIDATOR.read().test_reset_transaction_counters();

    let provider = ShmProviderBuilder::default_backend(65536).wait().unwrap();
    let buffer = provider.alloc(1024).wait().unwrap();
    drop(buffer);

    let _ = provider.garbage_collect();

    assert_eq!(GLOBAL_VALIDATOR.read().test_transaction_counts(), (1, 1));
}
