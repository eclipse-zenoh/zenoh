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

use super::shm::Segment;

pub type SegmentID = u32;

#[derive(Clone, Eq, Hash, PartialEq, PartialOrd, Ord, Debug)]
pub struct Descriptor {
    pub id: SegmentID,
    pub index_and_bitpos: u32,
}

impl From<&OwnedDescriptor> for Descriptor {
    fn from(item: &OwnedDescriptor) -> Self {
        let index = unsafe {
            item.atomic
                .offset_from(item.segment.shmem.as_ptr() as *const AtomicU64)
        } as u32;
        let bitpos = {
            // todo: can be optimized
            let mut v = item.mask;
            let mut bitpos = 0u32;
            while v > 1 {
                bitpos += 1;
                v >>= 1;
            }
            bitpos
        };
        let index_and_bitpos = (index << 5) | bitpos;
        Descriptor {
            id: item.segment.id,
            index_and_bitpos,
        }
    }
}

#[derive(Clone)]
pub struct OwnedDescriptor {
    pub segment: Arc<Segment>,
    pub atomic: *const AtomicU64,
    pub mask: u64,
}

impl OwnedDescriptor {
    pub fn confirm(&self) {
        unsafe {
            (*self.atomic).fetch_or(self.mask, std::sync::atomic::Ordering::SeqCst);
        };
    }
}

unsafe impl Send for OwnedDescriptor {}
unsafe impl Sync for OwnedDescriptor {}

impl Ord for OwnedDescriptor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.atomic.cmp(&other.atomic) {
            core::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.mask.cmp(&other.mask)
    }
}

impl PartialOrd for OwnedDescriptor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.atomic.partial_cmp(&other.atomic) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.mask.partial_cmp(&other.mask)
    }
}

impl PartialEq for OwnedDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.atomic == other.atomic && self.mask == other.mask
    }
}
impl Eq for OwnedDescriptor {}
