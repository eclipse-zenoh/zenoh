//
// Copyright (c) 2025 ZettaScale Technology
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

use super::{
    allocated_descriptor::AllocatedMetadataDescriptor,
    descriptor::{MetadataIndex, OwnedMetadataDescriptor},
    segment::MetadataSegment,
};

#[dynamic(lazy, drop)]
pub static mut GLOBAL_METADATA_STORAGE: MetadataStorage = MetadataStorage::new().unwrap();

pub struct MetadataStorage {
    available: Arc<Mutex<BTreeSet<OwnedMetadataDescriptor>>>,
}

impl MetadataStorage {
    fn new() -> ZResult<Self> {
        let initial_segment = Arc::new(MetadataSegment::create()?);
        let mut initially_available = BTreeSet::<OwnedMetadataDescriptor>::default();

        for index in 0..initial_segment.data.count() {
            let (header, watchdog, mask) = unsafe {
                initial_segment
                    .data
                    .fast_elem_compute(index as MetadataIndex)
            };
            let descriptor =
                OwnedMetadataDescriptor::new(initial_segment.clone(), header, watchdog, mask);

            // init generation (this is not really necessary, but we do)
            descriptor
                .header()
                .generation
                .store(0, std::sync::atomic::Ordering::SeqCst);

            initially_available.insert(descriptor);
        }

        Ok(Self {
            available: Arc::new(Mutex::new(initially_available)),
        })
    }

    pub fn allocate(&self) -> ZResult<AllocatedMetadataDescriptor> {
        let mut guard = self.available.lock().map_err(|e| zerror!("{e}"))?;
        let popped = guard.pop_first();
        drop(guard);

        let descriptor = popped.ok_or_else(|| zerror!("no free headers available"))?;

        Ok(AllocatedMetadataDescriptor::new(descriptor))
    }

    pub fn reclaim(&self, descriptor: OwnedMetadataDescriptor) {
        // header deallocated - increment it's generation to invalidate any existing references
        descriptor
            .header()
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut guard = self.available.lock().unwrap();
        let _new_insert = guard.insert(descriptor);
        #[cfg(feature = "test")]
        assert!(_new_insert);
    }
}
