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
use zenoh_util::core::{ZResult, ZError, ZErrorKind};
use zenoh_util::zerror;
use zenoh_protocol::core::PeerId;
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::WhatAmI;
use zenoh_protocol::session::{SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
use crate::routing::broker::Broker;
use crate::runtime::orchestrator::SessionOrchestrator;

pub mod orchestrator;

mod adminspace;
pub use adminspace::AdminSpace;

pub struct RuntimeState {
    pub pid: PeerId,
    pub broker: Arc<Broker>,
    pub orchestrator: SessionOrchestrator,
}

#[derive(Clone)]
pub struct Runtime {
    state: Arc<RwLock<RuntimeState>>
}

impl Runtime {

    pub async fn new(version: u8, whatami: WhatAmI, listeners: Vec<Locator>, peers: Vec<Locator>, 
                    iface: &str, delay: std::time::Duration) -> ZResult<Runtime> {
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
        let mut orchestrator = SessionOrchestrator::new(session_manager, whatami);
        match orchestrator.init(listeners, peers, iface, delay).await {
            Ok(()) => {
                Ok(Runtime { 
                    state: Arc::new(RwLock::new(RuntimeState {
                        pid,
                        broker,
                        orchestrator,
                    }))
                })
            },
            Err(err) => zerror!(ZErrorKind::Other{ descr: "".to_string()}, err),
        }
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, RuntimeState> {
        self.state.read().await
    }

    pub async fn write(&self) -> RwLockWriteGuard<'_, RuntimeState> {
        self.state.write().await
    }

    pub async fn close(&self) -> ZResult<()> {
        self.write().await.orchestrator.close().await
    }
}