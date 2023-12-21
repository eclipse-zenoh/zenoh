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
        Arc, Mutex,
    },
    thread::{self},
    time::Duration,
};

use zenoh_result::{bail, zerror, ZResult};

use crate::header::descriptor::OwnedHeaderDescriptor;

use super::descriptor::OwnedDescriptor;

// todo: optimize validation by packing descriptors
pub struct WatchdogValidator {
    watched: Arc<Mutex<BTreeMap<OwnedDescriptor, OwnedHeaderDescriptor>>>,
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
        let watched = Arc::new(Mutex::new(
            BTreeMap::<OwnedDescriptor, OwnedHeaderDescriptor>::default(),
        ));
        let running = Arc::new(AtomicBool::new(true));

        let c_watched = watched.clone();
        let c_running = running.clone();
        let _ = thread::spawn(move || {
            while c_running.load(Ordering::Relaxed) {
                let mut guard = c_watched.lock().unwrap();
                guard.retain(|watchdog, header| {
                    let old_val = watchdog.validate();
                    if old_val == 0 {
                        header.header().watchdog_flag.store(false, Ordering::SeqCst);
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

    pub fn add(&self, watchdog: OwnedDescriptor, header: OwnedHeaderDescriptor) -> ZResult<()> {
        let mut guard = self.watched.lock().map_err(|e| zerror!("{e}"))?;
        if guard.insert(watchdog, header).is_none() {
            return Ok(());
        }
        bail!("Watchdog already exists!")
    }
}
