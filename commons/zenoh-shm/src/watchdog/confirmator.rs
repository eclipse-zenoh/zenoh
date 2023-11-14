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
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Arc, Mutex,
    },
    thread::{self},
    time::Duration,
};

use lazy_static::lazy_static;
use log::error;
use zenoh_result::{bail, zerror, ZResult};

use super::{
    descriptor::{Descriptor, OwnedDescriptor, SegmentID},
    shm::Segment,
};

lazy_static! {
    pub static ref GLOBAL_CONFIRMATOR: WatchdogConfirmator =
        WatchdogConfirmator::new(Duration::from_millis(50));
}

pub struct ConfirmedDescriptor {
    owned: OwnedDescriptor,
    confirmed: Arc<Mutex<BTreeSet<OwnedDescriptor>>>,
}

impl Drop for ConfirmedDescriptor {
    fn drop(&mut self) {
        match self.confirmed.lock() {
            Ok(mut guard) => {
                if !guard.remove(&self.owned) {
                    error!("Watchdog not found!")
                }
            }
            Err(e) => error!("{e}"),
        }
    }
}

impl ConfirmedDescriptor {
    fn new(owned: OwnedDescriptor, confirmed: Arc<Mutex<BTreeSet<OwnedDescriptor>>>) -> Self {
        Self { owned, confirmed }
    }
}

// todo: optimize confirmation by packing descriptors AND linked table together
// todo: think about linked table cleanup
pub struct WatchdogConfirmator {
    linked_table: Mutex<BTreeMap<SegmentID, Arc<Segment>>>,
    confirmed: Arc<Mutex<BTreeSet<OwnedDescriptor>>>,
    running: Arc<AtomicBool>,
}

impl Drop for WatchdogConfirmator {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

impl WatchdogConfirmator {
    fn new(interval: Duration) -> Self {
        let confirmed = Arc::new(Mutex::new(BTreeSet::<OwnedDescriptor>::default()));
        let running = Arc::new(AtomicBool::new(true));

        let c_confirmed = confirmed.clone();
        let c_running = running.clone();
        let _ = thread::spawn(move || {
            while c_running.load(std::sync::atomic::Ordering::Relaxed) {
                let guard = c_confirmed.lock().unwrap();
                for descriptor in guard.iter() {
                    descriptor.confirm();
                }
                drop(guard);
                std::thread::sleep(interval);
            }
        });

        Self {
            linked_table: Mutex::default(),
            confirmed,
            running,
        }
    }

    pub fn add_owned(&self, watchdog: OwnedDescriptor) -> ZResult<ConfirmedDescriptor> {
        watchdog.confirm();
        let mut guard = self.confirmed.lock().map_err(|e| zerror!("{e}"))?;
        let ok = guard.insert(watchdog.clone());
        drop(guard);
        if ok {
            return Ok(ConfirmedDescriptor::new(watchdog, self.confirmed.clone()));
        }
        bail!("Watchdog already exists!")
    }

    pub fn add(&self, descriptor: Descriptor) -> ZResult<ConfirmedDescriptor> {
        let watchdog = self.link(descriptor)?;
        self.add_owned(watchdog)
    }

    fn link(&self, descriptor: Descriptor) -> ZResult<OwnedDescriptor> {
        let mut guard = self.linked_table.lock().map_err(|e| zerror!("{e}"))?;

        let segment = match guard.entry(descriptor.id) {
            std::collections::btree_map::Entry::Vacant(vacant) => {
                let segment = Arc::new(Segment::open(descriptor.id)?);
                vacant.insert(segment.clone());
                segment
            }
            std::collections::btree_map::Entry::Occupied(occupied) => occupied.get().clone(),
        };

        let index = (descriptor.index_and_bitpos >> 5) as usize;
        let bitpos = descriptor.index_and_bitpos & 0x3f;

        let atomic =
            unsafe { (segment.shmem.as_ptr() as *const u64).add(index) as *const AtomicU64 };
        let mask = 1u64 << bitpos;

        Ok(OwnedDescriptor {
            segment,
            atomic,
            mask,
        })
    }
}
