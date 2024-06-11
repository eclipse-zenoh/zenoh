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

use lazy_static::lazy_static;
use zenoh_result::{bail, ZResult};

use crate::{
    api::{
        client::{shm_client::ShmClient, shm_segment::ShmSegment},
        common::types::ProtocolID,
        protocol_implementations::posix::{
            posix_shm_client::PosixShmClient, protocol_id::POSIX_PROTOCOL_ID,
        },
    },
    reader::{ClientStorage, GlobalDataSegmentID},
};

lazy_static! {
    /// A global lazily-initialized SHM client storage.
    /// When initialized, contains default client set,
    /// see ShmClientStorage::with_default_client_set
    #[zenoh_macros::unstable_doc]
    pub static ref GLOBAL_CLIENT_STORAGE: Arc<ShmClientStorage> = Arc::new(
        ShmClientStorage::builder()
            .with_default_client_set()
            .build()
    );
}

/// Builder to create new client storages
#[zenoh_macros::unstable_doc]
pub struct ShmClientSetBuilder;

impl ShmClientSetBuilder {
    /// Add client to the storage (without including the default client set)
    #[zenoh_macros::unstable_doc]
    pub fn with_client(
        self,
        id: ProtocolID,
        client: Arc<dyn ShmClient>,
    ) -> ShmClientStorageBuilder {
        let clients = HashMap::from([(id, client)]);
        ShmClientStorageBuilder::new(clients)
    }

    /// Add list of clients to the storage (without including the default client set)
    #[zenoh_macros::unstable_doc]
    pub fn with_clients(
        self,
        clients: &[(ProtocolID, Arc<dyn ShmClient>)],
    ) -> ShmClientStorageBuilder {
        let clients = clients.iter().cloned().collect();
        ShmClientStorageBuilder::new(clients)
    }

    /// Include default clients
    #[zenoh_macros::unstable_doc]
    pub fn with_default_client_set(self) -> ShmClientStorageBuilder {
        let clients = HashMap::from([(
            POSIX_PROTOCOL_ID,
            Arc::new(PosixShmClient {}) as Arc<dyn ShmClient>,
        )]);
        ShmClientStorageBuilder::new(clients)
    }
}

#[zenoh_macros::unstable_doc]
pub struct ShmClientStorageBuilder {
    clients: HashMap<ProtocolID, Arc<dyn ShmClient>>,
}

impl ShmClientStorageBuilder {
    fn new(clients: HashMap<ProtocolID, Arc<dyn ShmClient>>) -> Self {
        Self { clients }
    }

    /// Add client to the storage
    #[zenoh_macros::unstable_doc]
    pub fn with_client(mut self, id: ProtocolID, client: Arc<dyn ShmClient>) -> ZResult<Self> {
        match self.clients.entry(id) {
            std::collections::hash_map::Entry::Occupied(occupied) => {
                bail!("Client already exists for id {id}: {:?}!", occupied)
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(client as Arc<dyn ShmClient>);
                Ok(self)
            }
        }
    }

    /// Add list of clients to the storage
    #[zenoh_macros::unstable_doc]
    pub fn with_clients(mut self, clients: &[(ProtocolID, Arc<dyn ShmClient>)]) -> Self {
        self.clients.extend(clients.iter().cloned());
        self
    }

    /// Build the storage with parameters specified on previous step
    #[zenoh_macros::unstable_doc]
    pub fn build(self) -> ShmClientStorage {
        ShmClientStorage::new(self.clients)
    }
}

/// A storage for SHM clients.
/// Runtime or Session constructed with instance of this type gets capabilities to read
/// SHM buffers for Protocols added to this instance.
#[zenoh_macros::unstable_doc]
#[derive(Debug)]
pub struct ShmClientStorage {
    pub(crate) clients: ClientStorage<Arc<dyn ShmClient>>,
    pub(crate) segments: RwLock<HashMap<GlobalDataSegmentID, Arc<dyn ShmSegment>>>,
}

impl Eq for ShmClientStorage {}

impl PartialEq for ShmClientStorage {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl ShmClientStorage {
    /// Get the builder to construct a new storage
    #[zenoh_macros::unstable_doc]
    pub fn builder() -> ShmClientSetBuilder {
        ShmClientSetBuilder
    }

    /// Get the list of supported SHM protocols.
    #[zenoh_macros::unstable_doc]
    pub fn supported_protocols(&self) -> Vec<ProtocolID> {
        self.clients.get_clients().keys().copied().collect()
    }

    fn new(clients: HashMap<ProtocolID, Arc<dyn ShmClient>>) -> Self {
        Self {
            clients: ClientStorage::new(clients),
            segments: RwLock::default(),
        }
    }
}
