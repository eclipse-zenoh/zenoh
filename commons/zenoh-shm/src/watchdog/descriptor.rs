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
    hash::Hash,
    sync::{atomic::AtomicU64, Arc},
};

use super::segment::Segment;

pub type SegmentID = u32;

#[derive(Clone, Eq, Hash, PartialEq, PartialOrd, Ord, Debug)]
pub struct Descriptor {
    pub id: SegmentID,
    pub index_and_bitpos: u32,
}

impl From<&OwnedDescriptor> for Descriptor {
    fn from(item: &OwnedDescriptor) -> Self {
        let bitpos = {
            // TODO: can be optimized
            let mut v = item.mask;
            let mut bitpos = 0u32;
            while v > 1 {
                bitpos += 1;
                v >>= 1;
            }
            bitpos
        };
        let index = unsafe { item.segment.array.index(item.atomic) };
        let index_and_bitpos = (index << 6) | bitpos;
        Descriptor {
            id: item.segment.array.id(),
            index_and_bitpos,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OwnedDescriptor {
    segment: Arc<Segment>,
    pub atomic: *const AtomicU64,
    pub mask: u64,
}

unsafe impl Send for OwnedDescriptor {}
unsafe impl Sync for OwnedDescriptor {}

impl Hash for OwnedDescriptor {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.atomic.hash(state);
        self.mask.hash(state);
    }
}

impl OwnedDescriptor {
    pub(crate) fn new(segment: Arc<Segment>, atomic: *const AtomicU64, mask: u64) -> Self {
        Self {
            segment,
            atomic,
            mask,
        }
    }

    pub fn confirm(&self) {
        unsafe {
            (*self.atomic).fetch_or(self.mask, std::sync::atomic::Ordering::SeqCst);
        };
    }

    pub(crate) fn validate(&self) -> u64 {
        unsafe {
            (*self.atomic).fetch_and(!self.mask, std::sync::atomic::Ordering::SeqCst) & self.mask
        }
    }

    #[cfg(feature = "test")]
    pub fn test_validate(&self) -> u64 {
        self.validate()
    }
}

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
        Some(self.cmp(other))
    }
}

impl PartialEq for OwnedDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.atomic == other.atomic && self.mask == other.mask
    }
}
impl Eq for OwnedDescriptor {}
