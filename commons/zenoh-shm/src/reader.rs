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
    collections::HashMap,
    sync::{Arc, RwLock},
};

use zenoh_result::{bail, zerror, ZResult};

use crate::{
    api::{
        client::{
            shared_memory_client::SharedMemoryClient, shared_memory_segment::SharedMemorySegment,
        },
        common::types::{ProtocolID, SegmentID},
        protocol_implementations::posix::{
            posix_shared_memory_client::PosixSharedMemoryClient, protocol_id::POSIX_PROTOCOL_ID,
        },
    },
    header::subscription::GLOBAL_HEADER_SUBSCRIPTION,
    watchdog::confirmator::GLOBAL_CONFIRMATOR,
    SharedMemoryBuf, SharedMemoryBufInfo,
};

#[derive(Debug, PartialEq, Eq, Hash)]
struct GlobalDataSegmentID {
    protocol: ProtocolID,
    segment: SegmentID,
}

impl GlobalDataSegmentID {
    fn new(protocol: ProtocolID, segment: SegmentID) -> Self {
        Self { protocol, segment }
    }
}

#[derive(Debug)]
pub struct SharedMemoryReader {
    clients: HashMap<ProtocolID, Box<dyn SharedMemoryClient>>,
    segments: RwLock<HashMap<GlobalDataSegmentID, Arc<dyn SharedMemorySegment>>>,
}

impl Eq for SharedMemoryReader {}

impl PartialEq for SharedMemoryReader {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Default for SharedMemoryReader {
    fn default() -> Self {
        let clients = HashMap::from([(
            POSIX_PROTOCOL_ID,
            Box::new(PosixSharedMemoryClient {}) as Box<dyn SharedMemoryClient>,
        )]);
        Self {
            clients,
            segments: Default::default(),
        }
    }
}

impl SharedMemoryReader {
    pub(crate) fn new(clients: HashMap<ProtocolID, Box<dyn SharedMemoryClient>>) -> Self {
        Self {
            clients,
            segments: RwLock::default(),
        }
    }

    pub fn read_shmbuf(&self, info: &SharedMemoryBufInfo) -> ZResult<SharedMemoryBuf> {
        // Read does not increment the reference count as it is assumed
        // that the sender of this buffer has incremented it for us.

        // attach to the watchdog before doing other stuff
        let watchdog = Arc::new(GLOBAL_CONFIRMATOR.add(&info.watchdog_descriptor)?);

        let segment = self.ensure_segment(info)?;
        let shmb = SharedMemoryBuf {
            header: GLOBAL_HEADER_SUBSCRIPTION.link(&info.header_descriptor)?,
            buf: segment.map(info.data_descriptor.chunk)?,
            info: info.clone(),
            watchdog,
        };

        // Validate buffer's generation
        match shmb.is_generation_valid() {
            true => Ok(shmb),
            false => bail!("Buffer is invalidated"),
        }
    }

    fn ensure_segment(&self, info: &SharedMemoryBufInfo) -> ZResult<Arc<dyn SharedMemorySegment>> {
        let id = GlobalDataSegmentID::new(info.shm_protocol, info.data_descriptor.segment);

        // fastest path: try to get access to already mounted SHM segment
        // read lock allows concurrent execution of multiple requests
        let r_guard = self.segments.read().unwrap();
        if let Some(val) = r_guard.get(&id) {
            return Ok(val.clone());
        }
        // fastest path failed: need to mount a new segment

        // drop read lock because we're gonna obtain write lock further
        drop(r_guard);

        // find appropriate client
        let client = self
            .clients
            .get(&id.protocol)
            .ok_or_else(|| zerror!("Unsupported SHM protocol: {}", id.protocol))?;

        // obtain write lock...
        let mut w_guard = self.segments.write().unwrap();

        // many concurrent threads may be racing for mounting this particular segment, so we must check again if the segment exists
        match w_guard.entry(id) {
            // (rare case) segment already mounted
            std::collections::hash_map::Entry::Occupied(occupied) => Ok(occupied.get().clone()),

            // (common case) mount a new segment and add it to the map
            std::collections::hash_map::Entry::Vacant(vacant) => {
                let new_segment = client.attach(info.data_descriptor.segment)?;
                Ok(vacant.insert(new_segment).clone())
            }
        }
    }
}
