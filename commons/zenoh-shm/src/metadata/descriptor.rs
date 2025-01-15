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

use std::sync::{atomic::AtomicU64, Arc};

use super::segment::MetadataSegment;
use crate::header::chunk_header::ChunkHeaderType;

pub type MetadataSegmentID = u16;
pub type MetadataIndex = u16;

#[derive(Clone, Eq, Hash, PartialEq, PartialOrd, Ord, Debug)]
pub struct MetadataDescriptor {
    pub id: MetadataSegmentID,
    pub index: MetadataIndex,
}

impl From<&OwnedMetadataDescriptor> for MetadataDescriptor {
    fn from(item: &OwnedMetadataDescriptor) -> Self {
        let id = item.segment.data.id();
        let index = unsafe { item.segment.data.fast_index_compute(item.header) };

        Self { id, index }
    }
}

#[derive(Clone)]
pub struct OwnedMetadataDescriptor {
    pub(crate) segment: Arc<MetadataSegment>,
    header: *const ChunkHeaderType,
    watchdog_atomic: *const AtomicU64,
    watchdog_mask: u64,
}

unsafe impl Send for OwnedMetadataDescriptor {}
unsafe impl Sync for OwnedMetadataDescriptor {}

impl OwnedMetadataDescriptor {
    pub(crate) fn new(
        segment: Arc<MetadataSegment>,
        header: *const ChunkHeaderType,
        watchdog_atomic: *const AtomicU64,
        watchdog_mask: u64,
    ) -> Self {
        Self {
            segment,
            header,
            watchdog_atomic,
            watchdog_mask,
        }
    }

    #[inline(always)]
    pub fn header(&self) -> &ChunkHeaderType {
        unsafe { &(*self.header) }
    }

    pub fn confirm(&self) {
        unsafe {
            (*self.watchdog_atomic)
                .fetch_or(self.watchdog_mask, std::sync::atomic::Ordering::SeqCst);
        };
    }

    pub(crate) fn validate(&self) -> u64 {
        unsafe {
            (*self.watchdog_atomic)
                .fetch_and(!self.watchdog_mask, std::sync::atomic::Ordering::SeqCst)
                & self.watchdog_mask
        }
    }

    #[cfg(feature = "test")]
    pub fn test_validate(&self) -> u64 {
        self.validate()
    }
}

// The ordering strategy is important. See storage implementation for details
impl Ord for OwnedMetadataDescriptor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.watchdog_atomic.cmp(&other.watchdog_atomic) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.watchdog_mask.cmp(&other.watchdog_mask)
    }
}

impl PartialOrd for OwnedMetadataDescriptor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for OwnedMetadataDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.watchdog_atomic == other.watchdog_atomic && self.watchdog_mask == other.watchdog_mask
    }
}
impl Eq for OwnedMetadataDescriptor {}

impl std::fmt::Debug for OwnedMetadataDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedHeaderDescriptor")
            .field("header", &self.header)
            .finish()
    }
}
