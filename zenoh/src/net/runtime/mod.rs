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
use super::protocol::core::{whatami, PeerId, WhatAmI};
use super::protocol::link::{Link, Locator};
use super::protocol::proto::{ZenohBody, ZenohMessage};
use super::protocol::session::{
    Session, SessionEventHandler, SessionHandler, SessionManager, SessionManagerConfig,
};
use super::routing;
use super::routing::pubsub::full_reentrant_route_data;
use super::routing::router::{LinkStateInterceptor, Router};
pub use adminspace::AdminSpace;
use async_std::sync::Arc;
use std::any::Any;
use uhlc::HLC;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::properties::config::*;
use zenoh_util::sync::get_mut_unchecked;
use zenoh_util::{zerror, zerror2};

pub struct RuntimeState {
    pub pid: PeerId,
    pub whatami: WhatAmI,
    pub router: Arc<Router>,
    pub config: ConfigProperties,
    pub manager: SessionManager,
    pub hlc: Option<Arc<HLC>>,
}

#[derive(Clone)]
pub struct Runtime {
    state: Arc<RuntimeState>,
}

impl std::ops::Deref for Runtime {
    type Target = RuntimeState;

    fn deref(&self) -> &RuntimeState {
        self.state.deref()
    }
}

impl Runtime {
    pub async fn new(version: u8, config: ConfigProperties, id: Option<&str>) -> ZResult<Runtime> {
        // Make sure to have have enough threads spawned in the async futures executor
        zasync_executor_init!();

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

        let whatami = whatami::parse(config.get_or(&ZN_MODE_KEY, ZN_MODE_DEFAULT)).unwrap();
        let hlc = if config
            .get_or(&ZN_ADD_TIMESTAMP_KEY, ZN_ADD_TIMESTAMP_DEFAULT)
            .to_lowercase()
            == ZN_TRUE
        {
            Some(Arc::new(HLC::with_system_time(uhlc::ID::from(&pid))))
        } else {
            None
        };

        let router = Arc::new(Router::new(pid.clone(), whatami, hlc.clone()));

        let handler = Arc::new(RuntimeSessionHandler {
            runtime: std::sync::RwLock::new(None),
        });
        let sm_config = SessionManagerConfig::builder()
            .from_properties(&config)
            .await?
            .version(version)
            .whatami(whatami)
            .pid(pid.clone())
            .build(handler.clone());

        let session_manager = SessionManager::new(sm_config);
        let mut runtime = Runtime {
            state: Arc::new(RuntimeState {
                pid,
                whatami,
                router,
                config: config.clone(),
                manager: session_manager,
                hlc,
            }),
        };
        *handler.runtime.write().unwrap() = Some(runtime.clone());

        let peers_autoconnect = config
            .get_or(&ZN_PEERS_AUTOCONNECT_KEY, ZN_PEERS_AUTOCONNECT_DEFAULT)
            .to_lowercase()
            == ZN_TRUE;
        let routers_autoconnect_gossip = config
            .get_or(
                &ZN_ROUTERS_AUTOCONNECT_GOSSIP_KEY,
                ZN_ROUTERS_AUTOCONNECT_GOSSIP_DEFAULT,
            )
            .to_lowercase()
            == ZN_TRUE;
        if whatami != whatami::CLIENT
            && config
                .get_or(&ZN_LINK_STATE_KEY, ZN_LINK_STATE_DEFAULT)
                .to_lowercase()
                == ZN_TRUE
        {
            get_mut_unchecked(&mut runtime.router.clone()).init_link_state(
                runtime.clone(),
                peers_autoconnect,
                routers_autoconnect_gossip,
            );
        }
        match runtime.start().await {
            Ok(()) => Ok(runtime),
            Err(err) => Err(err),
        }
    }

    #[inline(always)]
    pub fn manager(&self) -> &SessionManager {
        &self.manager
    }

    pub async fn close(&self) -> ZResult<()> {
        log::trace!("Runtime::close())");
        for session in &mut self.manager().get_sessions() {
            session.close().await?;
        }
        Ok(())
    }

    pub fn get_pid_str(&self) -> String {
        self.pid.to_string()
    }

    pub fn new_timestamp(&self) -> Option<uhlc::Timestamp> {
        self.hlc.as_ref().map(|hlc| hlc.new_timestamp())
    }
}

struct RuntimeSessionHandler {
    runtime: std::sync::RwLock<Option<Runtime>>,
}

impl SessionHandler for RuntimeSessionHandler {
    fn new_session(&self, session: Session) -> ZResult<Arc<dyn SessionEventHandler>> {
        match &*self.runtime.read().unwrap() {
            Some(runtime) => Ok(Arc::new(RuntimeSession {
                runtime: runtime.clone(),
                locator: std::sync::RwLock::new(None),
                sub_event_handler: runtime.router.new_session(session).unwrap(),
            })),
            None => zerror!(ZErrorKind::Other {
                descr: "Runtime not yet ready!".to_string()
            }),
        }
    }
}

pub(super) struct RuntimeSession {
    pub(super) runtime: Runtime,
    pub(super) locator: std::sync::RwLock<Option<Locator>>,
    pub(super) sub_event_handler: Arc<LinkStateInterceptor>,
}

impl SessionEventHandler for RuntimeSession {
    fn handle_message(&self, mut msg: ZenohMessage) -> ZResult<()> {
        // critical path shortcut
        if let ZenohBody::Data(data) = msg.body {
            if data.reply_context.is_none() {
                let (rid, suffix) = (&data.key).into();
                let face = &self.sub_event_handler.face.state;
                full_reentrant_route_data(
                    &self.sub_event_handler.tables,
                    face,
                    rid,
                    suffix,
                    msg.channel,
                    data.data_info,
                    data.payload,
                    msg.routing_context,
                );
                return Ok(());
            } else {
                msg.body = ZenohBody::Data(data);
            }
        }

        self.sub_event_handler.handle_message(msg)
    }

    fn new_link(&self, link: Link) {
        self.sub_event_handler.new_link(link)
    }

    fn del_link(&self, link: Link) {
        self.sub_event_handler.del_link(link)
    }

    fn closing(&self) {
        self.sub_event_handler.closing();
        Runtime::closing_session(self);
    }

    fn closed(&self) {
        self.sub_event_handler.closed()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
