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
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc, Mutex},
    thread::{self},
    time::Duration,
};

use zenoh_result::{bail, zerror, ZResult};

use super::descriptor::OwnedDescriptor;

// todo: optimize validation by packing descriptors

pub struct WatchdogValidator {
    watched: Arc<Mutex<BTreeSet<OwnedDescriptor>>>,
    running: Arc<AtomicBool>,
}

impl Drop for WatchdogValidator {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl WatchdogValidator {
    pub fn new<F>(interval: Duration, on_dropped: F) -> Self
    where
        F: Fn(OwnedDescriptor) + Send + 'static,
    {
        let watched = Arc::new(Mutex::new(BTreeSet::<OwnedDescriptor>::default()));
        let running = Arc::new(AtomicBool::new(true));

        let c_watched = watched.clone();
        let c_running = running.clone();
        let handle = thread::spawn(move || {
            while c_running.load(std::sync::atomic::Ordering::Relaxed) {
                let mut guard = c_watched.lock().unwrap();
                guard.retain(|watchdog| {
                    let old_val = unsafe {
                        (*watchdog.atomic)
                            .fetch_and(!watchdog.mask, std::sync::atomic::Ordering::SeqCst)
                            & watchdog.mask
                    };
                    if old_val == 0 {
                        on_dropped(watchdog.clone()); // todo: get rid of .clone()
                        return false;
                    }
                    true
                });
                drop(guard);
                std::thread::sleep(interval);
            }
        });

        Self { watched, running }
    }

    pub fn add(&self, watchdog: OwnedDescriptor) -> ZResult<()> {
        let mut guard = self.watched.lock().map_err(|e| zerror!("{e}"))?;
        if guard.insert(watchdog) {
            return Ok(());
        }
        bail!("Watchdog already exists!")
    }
}
