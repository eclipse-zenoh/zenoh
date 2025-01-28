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

use std::{collections::BTreeSet, sync::Arc, time::Duration};

use static_init::dynamic;

use super::periodic_task::PeriodicTask;
use crate::metadata::descriptor::OwnedMetadataDescriptor;

#[dynamic(lazy, drop)]
pub static mut GLOBAL_VALIDATOR: WatchdogValidator =
    WatchdogValidator::new(Duration::from_millis(100));

enum Transaction {
    Add,
    Remove,
}

#[derive(Default)]
struct ValidatedStorage {
    transactions: lockfree::queue::Queue<(Transaction, OwnedMetadataDescriptor)>,
}

impl ValidatedStorage {
    fn add(&self, descriptor: OwnedMetadataDescriptor) {
        self.transactions.push((Transaction::Add, descriptor));
    }

    fn remove(&self, descriptor: OwnedMetadataDescriptor) {
        self.transactions.push((Transaction::Remove, descriptor));
    }

    // See ordering implementation for OwnedMetadataDescriptor
    #[allow(clippy::mutable_key_type)]
    fn collect_transactions(&self, storage: &mut BTreeSet<OwnedMetadataDescriptor>) {
        while let Some((transaction, descriptor)) = self.transactions.pop() {
            match transaction {
                Transaction::Add => {
                    let _old = storage.insert(descriptor);
                    #[cfg(feature = "test")]
                    assert!(_old);
                }
                Transaction::Remove => {
                    let _ = storage.remove(&descriptor);
                }
            }
        }
    }
}

// TODO: optimize validation by packing descriptors
pub struct WatchdogValidator {
    storage: Arc<ValidatedStorage>,
    _task: PeriodicTask,
}

impl WatchdogValidator {
    pub fn new(interval: Duration) -> Self {
        let storage = Arc::new(ValidatedStorage::default());

        let c_storage = storage.clone();
        // See ordering implementation for OwnedMetadataDescriptor
        #[allow(clippy::mutable_key_type)]
        let mut watchdogs = BTreeSet::default();
        let task = PeriodicTask::new("Watchdog Validator".to_owned(), interval, move || {
            c_storage.collect_transactions(&mut watchdogs);

            watchdogs.retain(|watchdog| {
                let old_val = watchdog.validate();
                if old_val == 0 {
                    watchdog
                        .header()
                        .watchdog_invalidated
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    return false;
                }
                true
            });
        });

        Self {
            storage,
            _task: task,
        }
    }

    pub fn add(&self, watchdog: OwnedMetadataDescriptor) {
        self.storage.add(watchdog);
    }

    pub fn remove(&self, watchdog: OwnedMetadataDescriptor) {
        self.storage.remove(watchdog);
    }
}
