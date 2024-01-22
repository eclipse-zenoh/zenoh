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

use std::collections::HashMap;

use zenoh_result::{bail, ZResult};

use super::{
    client::shared_memory_client::SharedMemoryClient,
    common::types::ProtocolID,
    provider::{
        shared_memory_provider::SharedMemoryProvider,
        shared_memory_provider_backend::SharedMemoryProviderBackend,
    },
};

// the builder for shared memory factory
#[derive(Default)]
pub struct SharedMemoryFactoryBuilder {
    backends: HashMap<ProtocolID, Box<dyn SharedMemoryProviderBackend>>,
    clients: HashMap<ProtocolID, Box<dyn SharedMemoryClient>>,
}

impl SharedMemoryFactoryBuilder {
    // Add a new SharedMemoryProvider
    pub fn provider(
        mut self,
        id: ProtocolID,
        backend: Box<dyn SharedMemoryProviderBackend>,
    ) -> ZResult<Self> {
        match self.backends.entry(id) {
            std::collections::hash_map::Entry::Occupied(_) => {
                bail!("Provider backend already exists for id {id}!")
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(backend);
                Ok(self)
            }
        }
    }

    // Add a new SharedMemoryClient with specified id
    pub fn client(mut self, id: ProtocolID, client: Box<dyn SharedMemoryClient>) -> ZResult<Self> {
        match self.clients.entry(id) {
            std::collections::hash_map::Entry::Occupied(_) => {
                bail!("Client already exists for id {id}!")
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(client);
                Ok(self)
            }
        }
    }

    // Build the factory
    pub fn build(self) -> SharedMemoryFactory {
        SharedMemoryFactory::new(self.backends, self.clients)
    }
}

// the shared memory factory
pub struct SharedMemoryFactory {
    providers: HashMap<ProtocolID, SharedMemoryProvider>,
    clients: HashMap<ProtocolID, Box<dyn SharedMemoryClient>>,
}
impl SharedMemoryFactory {
    // Get the builder
    pub fn builder() -> SharedMemoryFactoryBuilder {
        SharedMemoryFactoryBuilder::default()
    }

    // Get the provider instance by id
    pub fn provider(&self, id: ProtocolID) -> Option<&SharedMemoryProvider> {
        self.providers.get(&id)
    }

    fn new(
        mut backends: HashMap<ProtocolID, Box<dyn SharedMemoryProviderBackend>>,
        clients: HashMap<ProtocolID, Box<dyn SharedMemoryClient>>,
    ) -> Self {
        // construct providers from provider backends
        let providers = backends
            .drain()
            .map(|(key, backend)| (key, SharedMemoryProvider::new(backend, key)))
            .collect();

        Self { providers, clients }
    }
}
