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

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc
    },
    time::{Duration, SystemTime},
};

use lazy_static::lazy_static;

use log::{error, warn};
use thread_priority::{ThreadBuilderExt, ThreadPriority};

use super::descriptor::OwnedDescriptor;

pub(super) type InvalidateCallback = Box<dyn Fn() + Send>;

lazy_static! {
    pub static ref GLOBAL_VALIDATOR: WatchdogValidator =
        WatchdogValidator::new(Duration::from_millis(100));
}

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
        self.transactions.push( (Transaction::Add(on_invalidated), descriptor));
    }

    fn remove(&self, descriptor: OwnedDescriptor) {
        self.transactions.push( (Transaction::Remove, descriptor));
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

// todo: optimize validation by packing descriptors
pub struct WatchdogValidator {
    storage: Arc<ValidatedStorage>,
    running: Arc<AtomicBool>,
}

impl Drop for WatchdogValidator {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl WatchdogValidator {
    pub fn new(interval: Duration) -> Self {
        let storage = Arc::new(ValidatedStorage::default());
        let running = Arc::new(AtomicBool::new(true));

        let c_storage = storage.clone();
        let c_running = running.clone();
        let _ = std::thread::Builder::new()
            .name("Watchdog Validator thread".to_owned())
            .spawn_with_priority(ThreadPriority::Min, move |result| {
                if let Err(e) = result {
                    error!("Watchdog Validator: error setting thread priority: {:?}, will continue operating with default priority...", e);
                    panic!("");
                }

                let mut watchdogs = BTreeMap::default();
                while c_running.load(Ordering::Relaxed) {
                    let cycle_start = std::time::Instant::now();

                    c_storage.collect_transactions(&mut watchdogs);

                    // sleep for next iteration
                    let elapsed = cycle_start.elapsed();
                    if elapsed < interval {
                        let sleep_interval = interval - elapsed;
                        std::thread::sleep(sleep_interval);
                    } else {
                        warn!("Watchdog validation timer overrun!");
                        #[cfg(feature = "test")]
                        panic!("Watchdog validation timer overrun!");
                    }

                    watchdogs
                        .retain(|watchdog, on_invalidated| {
                            let old_val = watchdog.validate();
                            if old_val == 0 {
                                on_invalidated();
                                return false;
                            }
                            true
                        });
                }
            });

        Self { storage, running }
    }

    pub fn add(
        &self,
        watchdog: OwnedDescriptor,
        on_invalidated: InvalidateCallback,
    ) {
        self.storage.add(watchdog, on_invalidated);
    }

    pub fn remove(&self, watchdog: OwnedDescriptor) {
        self.storage.remove(watchdog);
    }
}
