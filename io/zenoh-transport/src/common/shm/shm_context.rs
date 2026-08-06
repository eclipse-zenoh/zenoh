//
// Copyright (c) 2025 ZettaScale Technology
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

use zenoh_result::ZResult;
use zenoh_shm::{api::client_storage::GLOBAL_CLIENT_STORAGE, reader::ShmReader};

use crate::{
    common::shm::interop::{LazyShmProvider, MulticastTransportShmConfig, TransportShmConfig},
    shm::{
        LazyShmProvider, MulticastTransportShmConfig, ShmOptimizationPolicy, TransportShmConfig,
    },
    unicast::establishment::ext::shm::{auth::AuthUnicast, AuthUnicast},
};

#[derive(Clone)]
pub(crate) struct MulticastTransportShmContext {
    pub(crate) shm_reader: ShmReader,
    pub(crate) shm_provider: Option<Arc<LazyShmProvider>>,
    pub(crate) shm_config: MulticastTransportShmConfig,
    pub(crate) policy: ShmOptimizationPolicy,
}

impl MulticastTransportShmContext {
    pub(super) fn new(
        shm_reader: ShmReader,
        shm_provider: Option<Arc<LazyShmProvider>>,
        policy: ShmOptimizationPolicy,
    ) -> Self {
        Self {
            shm_reader,
            shm_provider,
            shm_config: MulticastTransportShmConfig,
            policy,
        }
    }
}

#[derive(Clone)]
pub(crate) struct UnicastTransportShmContext {
    pub(crate) shm_reader: ShmReader,
    pub(crate) shm_provider: Option<Arc<LazyShmProvider>>,
    pub(crate) shm_config: TransportShmConfig,
    pub(crate) policy: ShmOptimizationPolicy,
}

impl UnicastTransportShmContext {
    pub(crate) fn new(
        shm_reader: ShmReader,
        shm_provider: Option<Arc<LazyShmProvider>>,
        shm_config: TransportShmConfig,
        policy: ShmOptimizationPolicy,
    ) -> Self {
        Self {
            shm_reader,
            shm_provider,
            shm_config,
            policy,
        }
    }
}

pub struct ShmContext {
    pub(crate) shm_reader: ShmReader,
    pub(super) shm_provider: Option<Arc<LazyShmProvider>>,
    pub(crate) policy: ShmOptimizationPolicy,
    pub(super) auth: AuthUnicast,
}

impl std::fmt::Debug for ShmContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShmContext")
            .field("shm_reader", &self.shm_reader)
            .field("shm_provider", &self.shm_provider)
            .field("auth", &"..")
            .finish()
    }
}

impl ShmContext {
    pub fn new(
        cfg: zenoh_config::ShmConf,
        external_reader: Option<ShmReader>,
    ) -> ZResult<Option<Self>> {
        if !*cfg.enabled() {
            return Ok(None);
        }

        let shm_provider = if *cfg.transport_optimization.enabled() {
            Some(Arc::new(LazyShmProvider::new(
                *cfg.transport_optimization.pool_size(),
                *cfg.transport_optimization.message_size_threshold(),
            )))
        } else {
            None
        };

        let messages = cfg.transport_optimization.messages();
        let policy = ShmOptimizationPolicy {
            put: messages.contains(&zenoh_config::DataMessage::Put),
            query: messages.contains(&zenoh_config::DataMessage::Query),
            reply: messages.contains(&zenoh_config::DataMessage::Reply),
        };

        let shm_reader = external_reader
            .unwrap_or_else(|| ShmReader::new((*GLOBAL_CLIENT_STORAGE.read()).clone()));

        let auth = AuthUnicast::new(&shm_reader.supported_protocols())?;

        Ok(Some(Self {
            shm_provider,
            policy,
            shm_reader,
            auth,
        }))
    }

    pub fn shm_provider(&self) -> &Option<Arc<LazyShmProvider>> {
        &self.shm_provider
    }
}
