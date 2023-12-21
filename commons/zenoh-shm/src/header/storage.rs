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
    collections::LinkedList,
    sync::{Arc, Mutex},
};

use zenoh_result::{zerror, ZResult};

use super::{
    allocated_descriptor::AllocatedHeaderDescriptor, descriptor::OwnedHeaderDescriptor,
    segment::HeaderSegment,
};

lazy_static! {
    pub static ref GLOBAL_HEADER_STORAGE: Storage = Storage::new().unwrap();
}

pub struct Storage {
    available: Arc<Mutex<LinkedList<OwnedHeaderDescriptor>>>,
}

impl Storage {
    fn new() -> ZResult<Self> {
        let initial_header_count = 32768usize;
        let initial_segment = Arc::new(HeaderSegment::create(initial_header_count)?);
        let mut initially_available = LinkedList::<OwnedHeaderDescriptor>::default();
        let table = initial_segment.table_and_id().0;

        for index in 0..initial_header_count {
            let header = unsafe { table.add(index) };
            let descriptor = OwnedHeaderDescriptor::new(initial_segment.clone(), header);
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

        //initialize header
        let header = descriptor.header();
        header
            .generation
            .store(0, std::sync::atomic::Ordering::SeqCst);
        header
            .refcount
            .store(1, std::sync::atomic::Ordering::SeqCst);
        header
            .watchdog_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);

        Ok(AllocatedHeaderDescriptor { descriptor })
    }

    pub(crate) fn reclaim_header(&self, header: OwnedHeaderDescriptor) {
        let mut guard = self.available.lock().unwrap();
        guard.push_front(header);
    }
}
