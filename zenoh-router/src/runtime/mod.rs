//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use async_std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use zenoh_util::core::ZResult;
use zenoh_protocol::core::PeerId;
use zenoh_protocol::proto::WhatAmI;
use zenoh_protocol::session::{SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
use crate::routing::broker::Broker;
use crate::runtime::orchestrator::SessionOrchestrator;

pub mod orchestrator;

mod adminspace;
pub use adminspace::AdminSpace;

pub struct RuntimeMut {
    pub pid: PeerId,
    pub broker: Arc<Broker>,
    pub orchestrator: SessionOrchestrator,
}

#[derive(Clone)]
pub struct Runtime {
    muta: Arc<RwLock<RuntimeMut>>
}

impl Runtime {

    pub fn new(version: u8, whatami: WhatAmI) -> Runtime {
        let pid = PeerId{id: uuid::Uuid::new_v4().as_bytes().to_vec()};
        log::debug!("Generated PID: {}", pid);

        let broker = Arc::new(Broker::new());

        let sm_config = SessionManagerConfig {
            version,
            whatami,
            id: pid.clone(),
            handler: broker.clone()
        };

        let sm_opt_config = SessionManagerOptionalConfig {
            lease: None,
            keep_alive: None,
            sn_resolution: None,
            batch_size: None,
            timeout: None,
            retries: None,
            max_sessions: None,
            max_links: None 
        };

        let session_manager = SessionManager::new(sm_config, Some(sm_opt_config));
        let orchestrator = SessionOrchestrator::new(session_manager);

        let muta = Arc::new(RwLock::new(RuntimeMut {
            pid,
            broker,
            orchestrator,
        }));

        Runtime{muta}
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, RuntimeMut> {
        self.muta.read().await
    }

    pub async fn write(&self) -> RwLockWriteGuard<'_, RuntimeMut> {
        self.muta.write().await
    }

    pub async fn close(&self) -> ZResult<()> {
        self.write().await.orchestrator.close().await
    }
}