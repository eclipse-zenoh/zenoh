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
    sync::{Arc, Mutex},
};

use static_init::dynamic;
use zenoh_result::{zerror, ZResult};

use super::{allocated_watchdog::AllocatedWatchdog, descriptor::OwnedDescriptor, segment::Segment};

#[dynamic(lazy, drop)]
pub static mut GLOBAL_STORAGE: WatchdogStorage = WatchdogStorage::new(32768usize).unwrap();

pub struct WatchdogStorage {
    available: Arc<Mutex<BTreeSet<OwnedDescriptor>>>,
}

// TODO: expand and shrink Storage when needed
// OR
// support multiple descriptor assignment (allow multiple buffers to be assigned to the same watchdog)
impl WatchdogStorage {
    pub fn new(initial_watchdog_count: usize) -> ZResult<Self> {
        let segment = Arc::new(Segment::create(initial_watchdog_count)?);

        let mut initially_available = BTreeSet::default();
        let subsegments = segment.array.elem_count();
        for subsegment in 0..subsegments {
            let atomic = unsafe { segment.array.elem(subsegment as u32) };

            for bit in 0..64 {
                let mask = 1u64 << bit;
                let descriptor = OwnedDescriptor::new(segment.clone(), atomic, mask);
                let _new_insert = initially_available.insert(descriptor);
                #[cfg(feature = "test")]
                assert!(_new_insert);
            }
        }

        Ok(Self {
            available: Arc::new(Mutex::new(initially_available)),
        })
    }

    pub fn allocate_watchdog(&self) -> ZResult<AllocatedWatchdog> {
        let mut guard = self.available.lock().map_err(|e| zerror!("{e}"))?;
        let popped = guard.pop_first();
        drop(guard);

        let allocated =
            AllocatedWatchdog::new(popped.ok_or_else(|| zerror!("no free watchdogs available"))?);

        Ok(allocated)
    }

    pub(crate) fn free_watchdog(&self, descriptor: OwnedDescriptor) {
        if let Ok(mut guard) = self.available.lock() {
            let _new_insert = guard.insert(descriptor);
            #[cfg(feature = "test")]
            assert!(_new_insert);
        }
    }
}
