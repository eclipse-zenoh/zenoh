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

use once_cell::unsync::Lazy;
use zenoh_result::{bail, zerror, ZResult};

use super::{
    common::types::ProtocolID,
    provider::{
        shared_memory_provider::SharedMemoryProvider,
        shared_memory_provider_backend::SharedMemoryProviderBackend,
    },
};

type BackendFunc = dyn FnOnce() -> ZResult<Box<dyn SharedMemoryProviderBackend>>;

// the builder for shared memory factory
#[derive(Default)]
pub struct SharedMemoryFactoryBuilder {
    backends: HashMap<ProtocolID, Box<BackendFunc>>,
}

impl SharedMemoryFactoryBuilder {
    // Add a new SharedMemoryProvider
    pub fn provider<Tfunc>(mut self, id: ProtocolID, backend: Tfunc) -> ZResult<Self>
    where
        Tfunc: FnOnce() -> ZResult<Box<dyn SharedMemoryProviderBackend>> + 'static,
    {
        match self.backends.entry(id) {
            std::collections::hash_map::Entry::Occupied(_) => {
                bail!("Provider backend already exists for id {id}!")
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(Box::new(backend));
                Ok(self)
            }
        }
    }

    // Build the factory
    pub fn build(self) -> SharedMemoryFactory {
        SharedMemoryFactory::new(self.backends)
    }
}

type LazyFunc = Box<dyn FnOnce() -> ZResult<SharedMemoryProvider>>;

// the shared memory factory
pub struct SharedMemoryFactory {
    providers: HashMap<ProtocolID, Lazy<ZResult<SharedMemoryProvider>, LazyFunc>>,
}
impl SharedMemoryFactory {
    // Get the builder
    pub fn builder() -> SharedMemoryFactoryBuilder {
        SharedMemoryFactoryBuilder::default()
    }

    // Get the provider instance by id
    pub fn provider(&mut self, id: ProtocolID) -> ZResult<&mut SharedMemoryProvider> {
        self.providers
            .get_mut(&id)
            .map(|val| val.as_mut().map_err(|e| zerror!("{e}")))
            .unwrap_or_else(|| bail!("No provider for protocol {id}"))
            .map_err(|e| e.into())
    }

    fn new(mut backends: HashMap<ProtocolID, Box<BackendFunc>>) -> Self {
        // construct providers from provider backends
        let providers = backends
            .drain()
            .map(|(key, backend)| {
                let v: Lazy<ZResult<SharedMemoryProvider>, LazyFunc> =
                    Lazy::new(Box::new(move || {
                        Ok(SharedMemoryProvider::new(backend()?, key))
                    }));
                (key, v)
            })
            .collect();

        Self { providers }
    }
}
