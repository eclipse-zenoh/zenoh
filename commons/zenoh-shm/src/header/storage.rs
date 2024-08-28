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
    collections::LinkedList,
    sync::{Arc, Mutex},
};

use static_init::dynamic;
use zenoh_result::{zerror, ZResult};

use super::{
    allocated_descriptor::AllocatedHeaderDescriptor,
    descriptor::{HeaderIndex, OwnedHeaderDescriptor},
    segment::HeaderSegment,
};

#[dynamic(lazy, drop)]
pub static mut GLOBAL_HEADER_STORAGE: HeaderStorage = HeaderStorage::new(32768usize).unwrap();

pub struct HeaderStorage {
    available: Arc<Mutex<LinkedList<OwnedHeaderDescriptor>>>,
}

impl HeaderStorage {
    fn new(initial_header_count: usize) -> ZResult<Self> {
        let initial_segment = Arc::new(HeaderSegment::create(initial_header_count)?);
        let mut initially_available = LinkedList::<OwnedHeaderDescriptor>::default();

        for index in 0..initial_header_count {
            let header = unsafe { initial_segment.array.elem(index as HeaderIndex) };
            let descriptor = OwnedHeaderDescriptor::new(initial_segment.clone(), header);

            // init generation (this is not really necessary, but we do)
            descriptor
                .header()
                .generation
                .store(0, std::sync::atomic::Ordering::SeqCst);

            initially_available.push_back(descriptor);
        }

        Ok(Self {
            available: Arc::new(Mutex::new(initially_available)),
        })
    }

    pub fn allocate_header(&self) -> ZResult<AllocatedHeaderDescriptor> {
        let mut guard = self.available.lock().map_err(|e| zerror!("{e}"))?;
        let popped = guard.pop_front();
        drop(guard);

        let descriptor = popped.ok_or_else(|| zerror!("no free headers available"))?;

        //initialize header fields
        let header = descriptor.header();
        header
            .refcount
            .store(1, std::sync::atomic::Ordering::SeqCst);
        header
            .watchdog_invalidated
            .store(false, std::sync::atomic::Ordering::SeqCst);

        Ok(AllocatedHeaderDescriptor { descriptor })
    }

    pub fn reclaim_header(&self, header: OwnedHeaderDescriptor) {
        // header deallocated - increment it's generation to invalidate any existing references
        header
            .header()
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut guard = self.available.lock().unwrap();
        guard.push_front(header);
    }
}
