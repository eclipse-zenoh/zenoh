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

use std::sync::Arc;

use super::{chunk_header::ChunkHeaderType, segment::HeaderSegment};

pub type HeaderSegmentID = u16;
pub type HeaderIndex = u16;

#[derive(Clone, Eq, Hash, PartialEq, PartialOrd, Ord, Debug)]
pub struct HeaderDescriptor {
    pub id: HeaderSegmentID,
    pub index: HeaderIndex,
}

impl From<&OwnedHeaderDescriptor> for HeaderDescriptor {
    fn from(item: &OwnedHeaderDescriptor) -> Self {
        let id = item.segment.array.id();
        let index = unsafe { item.segment.array.index(item.header) };

        Self { id, index }
    }
}

#[derive(Clone)]
pub struct OwnedHeaderDescriptor {
    segment: Arc<HeaderSegment>,
    header: *const ChunkHeaderType,
}

unsafe impl Send for OwnedHeaderDescriptor {}
unsafe impl Sync for OwnedHeaderDescriptor {}

impl OwnedHeaderDescriptor {
    pub(crate) fn new(segment: Arc<HeaderSegment>, header: *const ChunkHeaderType) -> Self {
        Self { segment, header }
    }

    #[inline(always)]
    pub fn header(&self) -> &ChunkHeaderType {
        unsafe { &(*self.header) }
    }
}

impl std::fmt::Debug for OwnedHeaderDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedHeaderDescriptor")
            .field("header", &self.header)
            .finish()
    }
}
