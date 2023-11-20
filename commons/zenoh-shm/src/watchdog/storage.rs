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
use lazy_static::lazy_static;
use std::{
    collections::BTreeSet,
    mem::size_of,
    sync::{
        atomic::{AtomicPtr, AtomicU64, AtomicUsize},
        Arc, Mutex,
    },
    time::Duration,
};

use zenoh_result::{zerror, ZResult};

use super::{
    confirmator::{ConfirmedDescriptor, GLOBAL_CONFIRMATOR},
    descriptor::OwnedDescriptor,
    shm::Segment,
    validator::WatchdogValidator,
};

lazy_static! {
    pub static ref GLOBAL_STORAGE: Storage = Storage::new(512, Duration::from_millis(100)).unwrap();
}

pub struct Storage {
    available: Arc<Mutex<BTreeSet<OwnedDescriptor>>>,
    validator: WatchdogValidator,
}

// todo: expand and shrink Storage when needed
// OR
// support multiple descrptor assignment (allow multiple buffers to be assigned to the same watchdog)
impl Storage {
    pub fn new(initial_watchdog_count: usize, watchdog_interval: Duration) -> ZResult<Self> {
        let segment = Arc::new(Segment::create(initial_watchdog_count)?);
        let segment_address = segment.shmem.as_ptr() as *mut u64;

        let mut initially_available = BTreeSet::default();
        let subsegments = segment.shmem.len() / size_of::<u64>();
        for subsegment in 0usize..subsegments {
            let atomic = unsafe { segment_address.add(subsegment) as *mut AtomicU64 };

            for bit in 0..64 {
                let mask = 1u64 << bit;
                let descriptor = OwnedDescriptor {
                    segment: segment.clone(),
                    atomic,
                    mask,
                };
                initially_available.insert(descriptor);
            }
        }

        let available = Arc::new(Mutex::new(initially_available));

        let c_available = available.clone();
        let validator = WatchdogValidator::new(watchdog_interval, move |descriptor| {
            if let Ok(mut guard) = c_available.lock() {
                let _ = guard.insert(descriptor);
            }
        });

        Ok(Self {
            available,
            validator,
        })
    }

    pub fn allocate_watchdog(
        &self,
        refcount: AtomicPtr<AtomicUsize>,
    ) -> ZResult<ConfirmedDescriptor> {
        let mut guard = self.available.lock().map_err(|e| zerror!("{e}"))?;
        let popped = guard.pop_first();
        drop(guard);

        let watchdog = popped.ok_or_else(|| zerror!("no free watchdogs available"))?;
        let confirmed = GLOBAL_CONFIRMATOR.add_owned(watchdog.clone())?;
        self.validator.add(watchdog, refcount)?;

        Ok(confirmed)
    }
}
