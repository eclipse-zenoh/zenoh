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
}

impl SharedMemoryFactoryBuilder {
    // Add a new SharedMemoryProvider
    pub fn provider<Tprovider>(mut self, id: ProtocolID, backend: Box<Tprovider>) -> ZResult<Self>
    where
        Tprovider: SharedMemoryProviderBackend + 'static,
    {
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

    // Build the factory
    pub fn build(self) -> SharedMemoryFactory {
        SharedMemoryFactory::new(self.backends)
    }
}

// the shared memory factory
pub struct SharedMemoryFactory {
    providers: HashMap<ProtocolID, SharedMemoryProvider>,
}
impl SharedMemoryFactory {
    // Get the builder
    pub fn builder() -> SharedMemoryFactoryBuilder {
        SharedMemoryFactoryBuilder::default()
    }

    // Get the provider instance by id
    pub fn provider(&mut self, id: ProtocolID) -> Option<&mut SharedMemoryProvider> {
        self.providers.get_mut(&id)
    }

    fn new(mut backends: HashMap<ProtocolID, Box<dyn SharedMemoryProviderBackend>>) -> Self {
        // construct providers from provider backends
        let providers = backends
            .drain()
            .map(|(key, backend)| (key, SharedMemoryProvider::new(backend, key)))
            .collect();

        Self { providers }
    }
}
