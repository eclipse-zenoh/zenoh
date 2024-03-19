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
    collections::HashMap,
    sync::{Arc, RwLock},
};

use zenoh_result::{bail, ZResult};

use crate::api::{
    client::{
        shared_memory_client::SharedMemoryClient, shared_memory_segment::SharedMemorySegment,
    },
    common::types::ProtocolID,
    protocol_implementations::posix::{
        posix_shared_memory_client::PosixSharedMemoryClient, protocol_id::POSIX_PROTOCOL_ID,
    },
};

use crate::reader::{ClientStorage, GlobalDataSegmentID};

lazy_static! {
    /// A global lazily-initialized SHM client storage.
    /// When initialized, contains default client set,
    /// see SharedMemoryClientStorage::with_default_client_set
    #[zenoh_macros::unstable_doc]
    pub static ref GLOBAL_CLIENT_STORAGE: Arc<SharedMemoryClientStorage> = Arc::new(
        SharedMemoryClientStorage::builder()
            .with_default_client_set()
            .build()
    );
}

/// Builder to create new client storages
#[zenoh_macros::unstable_doc]
pub struct SharedMemoryClientSetBuilder;

impl SharedMemoryClientSetBuilder {
    /// Add client to the storage (without including the default client set)
    #[zenoh_macros::unstable_doc]
    pub fn with_client<Tclient>(
        self,
        id: ProtocolID,
        client: Box<Tclient>,
    ) -> SharedMemoryClientStorageBuilder
    where
        Tclient: SharedMemoryClient + 'static,
    {
        let clients = HashMap::from([(id, client as Box<dyn SharedMemoryClient>)]);
        SharedMemoryClientStorageBuilder::new(clients)
    }

    /// Include default clients
    #[zenoh_macros::unstable_doc]
    pub fn with_default_client_set(self) -> SharedMemoryClientStorageBuilder {
        let clients = HashMap::from([(
            POSIX_PROTOCOL_ID,
            Box::new(PosixSharedMemoryClient {}) as Box<dyn SharedMemoryClient>,
        )]);
        SharedMemoryClientStorageBuilder::new(clients)
    }
}

pub struct SharedMemoryClientStorageBuilder {
    clients: HashMap<ProtocolID, Box<dyn SharedMemoryClient>>,
}

impl SharedMemoryClientStorageBuilder {
    fn new(clients: HashMap<ProtocolID, Box<dyn SharedMemoryClient>>) -> Self {
        Self { clients }
    }

    /// Add client to the storage
    #[zenoh_macros::unstable_doc]
    pub fn with_client<Tclient>(mut self, id: ProtocolID, client: Box<Tclient>) -> ZResult<Self>
    where
        Tclient: SharedMemoryClient + 'static,
    {
        match self.clients.entry(id) {
            std::collections::hash_map::Entry::Occupied(occupied) => {
                bail!("Client already exists for id {id}: {:?}!", occupied)
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(client as Box<dyn SharedMemoryClient>);
                Ok(self)
            }
        }
    }

    /// Build the storage with parameters specified on previous step
    #[zenoh_macros::unstable_doc]
    pub fn build(self) -> SharedMemoryClientStorage {
        SharedMemoryClientStorage::new(self.clients)
    }
}

/// A storage for SHM clients.
/// Runtime or Session constructed with instance of this type gets capabilities to read
/// SHM buffers for Protocols added to this instance.
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub struct SharedMemoryClientStorage {
    pub(crate) clients: ClientStorage<Box<dyn SharedMemoryClient>>,
    pub(crate) segments: RwLock<HashMap<GlobalDataSegmentID, Arc<dyn SharedMemorySegment>>>,
}

impl Eq for SharedMemoryClientStorage {}

impl PartialEq for SharedMemoryClientStorage {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl SharedMemoryClientStorage {
    /// Get the builder to construct a new storage
    #[zenoh_macros::unstable_doc]
    pub fn builder() -> SharedMemoryClientSetBuilder {
        SharedMemoryClientSetBuilder
    }

    /// Get the list of supported SHM protocols.
    #[zenoh_macros::unstable_doc]
    pub fn supported_protocols(&self) -> Vec<ProtocolID> {
        self.clients.get_clients().keys().copied().collect()
    }

    fn new(clients: HashMap<ProtocolID, Box<dyn SharedMemoryClient>>) -> Self {
        Self {
            clients: ClientStorage::new(clients),
            segments: RwLock::default(),
        }
    }
}
