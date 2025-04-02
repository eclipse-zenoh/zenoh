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

use std::{collections::BTreeSet, time::Duration};

use static_init::dynamic;

use super::periodic_task::PeriodicTask;
use crate::metadata::descriptor::OwnedMetadataDescriptor;

#[dynamic(lazy, drop)]
pub static mut GLOBAL_VALIDATOR: WatchdogValidator =
    WatchdogValidator::new(Duration::from_millis(100));

enum Transaction {
    Add(OwnedMetadataDescriptor),
    Remove(OwnedMetadataDescriptor),
}

// TODO: optimize validation by packing descriptors
pub struct WatchdogValidator {
    sender: crossbeam_channel::Sender<Transaction>,
    _task: PeriodicTask,
}

impl WatchdogValidator {
    pub fn new(interval: Duration) -> Self {
        let (sender, receiver) = crossbeam_channel::unbounded::<Transaction>();

        // See ordering implementation for OwnedMetadataDescriptor
        #[allow(clippy::mutable_key_type)]
        let mut watchdogs = BTreeSet::default();
        let task = PeriodicTask::new("Watchdog Validator".to_owned(), interval, move || {
            // See ordering implementation for OwnedMetadataDescriptor
            #[allow(clippy::mutable_key_type)]
            fn collect_transactions(receiver: &crossbeam_channel::Receiver<Transaction>, storage: &mut BTreeSet<OwnedMetadataDescriptor>) {
                while let Ok(transaction) = receiver.try_recv() {
                    match transaction {
                        Transaction::Add(descriptor) => {
                            let _old = storage.insert(descriptor);
                            #[cfg(feature = "test")]
                            assert!(_old);
                        }
                        Transaction::Remove(descriptor) => {
                            let _ = storage.remove(&descriptor);
                        }
                    }
                }
            }

            collect_transactions(&receiver, &mut watchdogs);

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
            sender,
            _task: task,
        }
    }

    pub fn add(&self, watchdog: OwnedMetadataDescriptor) {
        self.sender.try_send(Transaction::Add(watchdog)).unwrap();
    }

    pub fn remove(&self, watchdog: OwnedMetadataDescriptor) {
        self.sender.try_send(Transaction::Remove(watchdog)).unwrap();
    }
}
