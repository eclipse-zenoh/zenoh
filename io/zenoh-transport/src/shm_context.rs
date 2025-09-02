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
    shm::{LazyShmProvider, MulticastTransportShmConfig, TransportShmConfig},
    unicast::establishment::ext::shm::AuthUnicast,
};

#[derive(Clone)]
pub(super) struct MulticastTransportShmContext {
    pub(crate) shm_reader: ShmReader,
    pub(super) shm_provider: Arc<LazyShmProvider>,
    pub(super) shm_config: MulticastTransportShmConfig,
}

impl MulticastTransportShmContext {
    pub(super) fn new(shm_reader: ShmReader, shm_provider: Arc<LazyShmProvider>) -> Self {
        Self {
            shm_reader,
            shm_provider,
            shm_config: MulticastTransportShmConfig,
        }
    }
}

#[derive(Clone)]
pub(super) struct UnicastTransportShmContext {
    pub(crate) shm_reader: ShmReader,
    pub(super) shm_provider: Arc<LazyShmProvider>,
    pub(super) shm_config: TransportShmConfig,
}

impl UnicastTransportShmContext {
    pub(super) fn new(
        shm_reader: ShmReader,
        shm_provider: Arc<LazyShmProvider>,
        shm_config: TransportShmConfig,
    ) -> Self {
        Self {
            shm_reader,
            shm_provider,
            shm_config,
        }
    }
}

pub struct ShmContext {
    pub(super) shm_provider: Arc<LazyShmProvider>,
    pub(crate) shm_reader: ShmReader,
    pub(super) auth: AuthUnicast,
}

impl ShmContext {
    pub fn new(
        cfg: zenoh_config::ShmConf,
        external_reader: Option<ShmReader>,
    ) -> ZResult<Option<Self>> {
        if !*cfg.enabled() {
            return Ok(None);
        }

        let shm_provider = Arc::new(if *cfg.transport_optimization.enabled() {
            LazyShmProvider::new(
                *cfg.transport_optimization.pool_size(),
                *cfg.transport_optimization.message_size_threshold(),
            )
        } else {
            LazyShmProvider::new_disabled()
        });

        let shm_reader = external_reader
            .unwrap_or_else(|| ShmReader::new((*GLOBAL_CLIENT_STORAGE.read()).clone()));

        let auth = AuthUnicast::new(&shm_reader.supported_protocols())?;

        Ok(Some(Self {
            shm_provider,
            shm_reader,
            auth,
        }))
    }
}
