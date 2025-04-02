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
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use static_init::dynamic;
use zenoh_result::{zerror, ZResult};

use super::{
    descriptor::{MetadataDescriptor, MetadataSegmentID, OwnedMetadataDescriptor},
    segment::MetadataSegment,
};

#[dynamic(lazy, drop)]
pub static mut GLOBAL_METADATA_SUBSCRIPTION: Subscription = Subscription::new();

pub struct Subscription {
    linked_table: Mutex<BTreeMap<MetadataSegmentID, Arc<MetadataSegment>>>,
}

impl Subscription {
    fn new() -> Self {
        Self {
            linked_table: Mutex::default(),
        }
    }

    pub fn link(&self, descriptor: &MetadataDescriptor) -> ZResult<OwnedMetadataDescriptor> {
        let mut guard = self.linked_table.lock().map_err(|e| zerror!("{e}"))?;
        // ensure segment
        let segment = match guard.entry(descriptor.id) {
            std::collections::btree_map::Entry::Vacant(vacant) => {
                let segment = Arc::new(MetadataSegment::open(descriptor.id)?);
                vacant.insert(segment.clone());
                segment
            }
            std::collections::btree_map::Entry::Occupied(occupied) => occupied.get().clone(),
        };
        drop(guard);

        // construct owned descriptor
        // SAFETY: MetadataDescriptor source guarantees that descriptor.index is valid for segment
        let (header, watchdog) = unsafe { segment.data.fast_elem_compute(descriptor.index) };

        let owned_descriptor = OwnedMetadataDescriptor::new(segment, header, watchdog);
        Ok(owned_descriptor)
    }
}
