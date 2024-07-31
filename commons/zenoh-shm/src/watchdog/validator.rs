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

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use static_init::dynamic;

use super::{descriptor::OwnedDescriptor, periodic_task::PeriodicTask};

pub(super) type InvalidateCallback = Box<dyn Fn() + Send>;

#[dynamic(lazy, drop)]
pub static mut GLOBAL_VALIDATOR: WatchdogValidator =
    WatchdogValidator::new(Duration::from_millis(100));

enum Transaction {
    Add(InvalidateCallback),
    Remove,
}

#[derive(Default)]
struct ValidatedStorage {
    transactions: lockfree::queue::Queue<(Transaction, OwnedDescriptor)>,
}

impl ValidatedStorage {
    fn add(&self, descriptor: OwnedDescriptor, on_invalidated: InvalidateCallback) {
        self.transactions
            .push((Transaction::Add(on_invalidated), descriptor));
    }

    fn remove(&self, descriptor: OwnedDescriptor) {
        self.transactions.push((Transaction::Remove, descriptor));
    }

    fn collect_transactions(&self, storage: &mut BTreeMap<OwnedDescriptor, InvalidateCallback>) {
        while let Some((transaction, descriptor)) = self.transactions.pop() {
            match transaction {
                Transaction::Add(on_invalidated) => {
                    let _old = storage.insert(descriptor, on_invalidated);
                    #[cfg(feature = "test")]
                    assert!(_old.is_none());
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
        let mut watchdogs = BTreeMap::default();
        let task = PeriodicTask::new("Watchdog Validator".to_owned(), interval, move || {
            c_storage.collect_transactions(&mut watchdogs);

            watchdogs.retain(|watchdog, on_invalidated| {
                let old_val = watchdog.validate();
                if old_val == 0 {
                    on_invalidated();
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

    pub fn add(&self, watchdog: OwnedDescriptor, on_invalidated: InvalidateCallback) {
        self.storage.add(watchdog, on_invalidated);
    }

    pub fn remove(&self, watchdog: OwnedDescriptor) {
        self.storage.remove(watchdog);
    }
}
