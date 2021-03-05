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
mod adminspace;
pub mod orchestrator;

use super::plugins;
use super::protocol;
use super::routing;

use super::protocol::core::{whatami, PeerId};
use super::protocol::session::{
    SessionDispatcher, SessionManager, SessionManagerConfig, SessionManagerOptionalConfig,
};
use super::routing::router::Router;
pub use adminspace::AdminSpace;
use async_std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use orchestrator::SessionOrchestrator;
use uhlc::HLC;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::*;
use zenoh_util::{zerror, zerror2};

pub struct RuntimeState {
    pub pid: PeerId,
    pub router: Arc<Router>,
    pub orchestrator: SessionOrchestrator,
}

pub(crate) fn parse_mode(m: &str) -> Result<whatami::Type, ()> {
    match m {
        "peer" => Ok(whatami::PEER),
        "client" => Ok(whatami::CLIENT),
        "router" => Ok(whatami::ROUTER),
        _ => Err(()),
    }
}

#[derive(Clone)]
pub struct Runtime {
    state: Arc<RwLock<RuntimeState>>,
}

impl Runtime {
    pub async fn new(version: u8, config: ConfigProperties, id: Option<&str>) -> ZResult<Runtime> {
        let pid = if let Some(s) = id {
            // filter-out '-' characters (in case s has UUID format)
            let s = s.replace('-', "");
            let vec = hex::decode(&s).map_err(|e| {
                zerror2!(ZErrorKind::Other {
                    descr: format!("Invalid id: {} - {}", s, e)
                })
            })?;
            let size = vec.len();
            if size > PeerId::MAX_SIZE {
                return zerror!(ZErrorKind::Other {
                    descr: format!("Invalid id size: {} ({} bytes max)", size, PeerId::MAX_SIZE)
                });
            }
            let mut id = [0u8; PeerId::MAX_SIZE];
            id[..size].copy_from_slice(vec.as_slice());
            PeerId::new(size, id)
        } else {
            PeerId::from(uuid::Uuid::new_v4())
        };

        log::info!("Using PID: {}", pid);

        let whatami = parse_mode(config.get_or(&ZN_MODE_KEY, ZN_MODE_DEFAULT)).unwrap();
        let hlc = if config
            .get_or(&ZN_ADD_TIMESTAMP_KEY, ZN_ADD_TIMESTAMP_DEFAULT)
            .to_lowercase()
            == ZN_TRUE
        {
            Some(HLC::with_system_time(uhlc::ID::from(&pid)))
        } else {
            None
        };
        let mut router = Arc::new(Router::new(pid.clone(), whatami, hlc));
        let mut orchestrator = SessionOrchestrator::new(whatami, router.clone());

        let sm_config = SessionManagerConfig {
            version,
            whatami,
            id: pid.clone(),
            handler: SessionDispatcher::SessionOrchestrator(Arc::new(orchestrator.clone())),
        };
        let sm_opt_config = SessionManagerOptionalConfig::from_properties(&config).await?;

        let session_manager = SessionManager::new(sm_config, sm_opt_config);
        let peers_autoconnect = config
            .get_or(&ZN_PEERS_AUTOCONNECT_KEY, ZN_PEERS_AUTOCONNECT_DEFAULT)
            .to_lowercase()
            == ZN_TRUE;
        if whatami != whatami::CLIENT
            && config
                .get_or(&ZN_LINK_STATE_KEY, ZN_LINK_STATE_DEFAULT)
                .to_lowercase()
                == ZN_TRUE
        {
            unsafe {
                Arc::get_mut_unchecked(&mut router)
                    .init_link_state(orchestrator.clone(), peers_autoconnect)
                    .await;
            }
        }
        match orchestrator
            .init(session_manager, config, peers_autoconnect)
            .await
        {
            Ok(()) => Ok(Runtime {
                state: Arc::new(RwLock::new(RuntimeState {
                    pid,
                    router,
                    orchestrator,
                })),
            }),
            Err(err) => Err(err),
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

    pub async fn get_pid_str(&self) -> String {
        self.read().await.pid.to_string()
    }
}
