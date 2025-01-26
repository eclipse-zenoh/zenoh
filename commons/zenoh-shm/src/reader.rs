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

use std::{collections::HashMap, ops::Deref, sync::Arc};

use zenoh_core::{bail, zerror};
use zenoh_result::ZResult;

use crate::{
    api::{
        client::shm_segment::ShmSegment,
        client_storage::ShmClientStorage,
        common::types::{ProtocolID, SegmentID},
        provider::chunk::ChunkDescriptor,
    },
    metadata::subscription::GLOBAL_METADATA_SUBSCRIPTION,
    watchdog::confirmator::GLOBAL_CONFIRMATOR,
    ShmBufInfo, ShmBufInner,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ShmReader {
    client_storage: Arc<ShmClientStorage>,
}

impl Deref for ShmReader {
    type Target = ShmClientStorage;

    fn deref(&self) -> &Self::Target {
        &self.client_storage
    }
}

impl ShmReader {
    pub fn new(client_storage: Arc<ShmClientStorage>) -> Self {
        Self { client_storage }
    }

    pub fn read_shmbuf(&self, info: ShmBufInfo) -> ZResult<ShmBufInner> {
        // Read does not increment the reference count as it is assumed
        // that the sender of this buffer has incremented it for us.

        let metadata = GLOBAL_METADATA_SUBSCRIPTION.read().link(&info.metadata)?;
        // attach to the watchdog before doing other things
        let confirmed_metadata = Arc::new(GLOBAL_CONFIRMATOR.read().add(metadata));

        // TODO: make normal signalling, do not block async thread
        // retrieve data descriptor from metadata
        let data_descriptor = loop {
            if let Some(val) = confirmed_metadata.owned.header().data_descriptor() {
                break val;
            };
            std::thread::yield_now();
        };

        let segment = self.ensure_data_segment(
            confirmed_metadata
                .owned
                .header()
                .protocol
                .load(std::sync::atomic::Ordering::Relaxed),
            &data_descriptor,
        )?;
        let buf = segment.map(data_descriptor.chunk)?;
        let shmb = ShmBufInner {
            metadata: confirmed_metadata,
            buf,
            info: info.clone(),
        };

        // Validate buffer
        match shmb.is_valid() {
            true => Ok(shmb),
            false => bail!("Buffer is invalidated"),
        }
    }

    fn ensure_data_segment(
        &self,
        protocol_id: ProtocolID,
        descriptor: &ChunkDescriptor,
    ) -> ZResult<Arc<dyn ShmSegment>> {
        let id = GlobalDataSegmentID::new(protocol_id, descriptor.segment);

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
            .get_clients()
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
                let new_segment = client.attach(descriptor.segment)?;
                Ok(vacant.insert(new_segment).clone())
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct ClientStorage<Inner>
where
    Inner: Sized,
{
    clients: HashMap<ProtocolID, Inner>,
}

impl<Inner: Sized> ClientStorage<Inner> {
    pub(crate) fn new(clients: HashMap<ProtocolID, Inner>) -> Self {
        Self { clients }
    }

    pub(crate) fn get_clients(&self) -> &HashMap<ProtocolID, Inner> {
        &self.clients
    }
}

/// # Safety
/// Only immutable access to internal container is allowed,
/// so we are Send if the contained type is Send
unsafe impl<Inner: Send> Send for ClientStorage<Inner> {}

/// # Safety
/// Only immutable access to internal container is allowed,
/// so we are Sync if the contained type is Sync
unsafe impl<Inner: Sync> Sync for ClientStorage<Inner> {}

#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct GlobalDataSegmentID {
    protocol: ProtocolID,
    segment: SegmentID,
}

impl GlobalDataSegmentID {
    fn new(protocol: ProtocolID, segment: SegmentID) -> Self {
        Self { protocol, segment }
    }
}
