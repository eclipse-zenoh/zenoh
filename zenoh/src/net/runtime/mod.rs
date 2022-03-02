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

use super::routing;
use super::routing::pubsub::full_reentrant_route_data;
use super::routing::router::{LinkStateInterceptor, Router};
use crate::config::{Config, Notifier};
pub use adminspace::AdminSpace;
use async_std::stream::StreamExt;
use async_std::sync::Arc;
use async_std::task::JoinHandle;
use futures::Future;
use std::any::Any;
use std::time::Duration;
use stop_token::future::FutureExt;
use stop_token::{StopSource, TimedOutError};
use uhlc::{HLCBuilder, HLC};
use zenoh_core::Result as ZResult;
use zenoh_core::{bail, zerror};
use zenoh_link::{Link, Locator};
use zenoh_protocol;
use zenoh_protocol::core::{PeerId, WhatAmI};
use zenoh_protocol::proto::{ZenohBody, ZenohMessage};
use zenoh_sync::get_mut_unchecked;
use zenoh_transport;
use zenoh_transport::{
    TransportEventHandler, TransportManager, TransportMulticast, TransportMulticastEventHandler,
    TransportPeer, TransportPeerEventHandler, TransportUnicast,
};

pub struct RuntimeState {
    pub pid: PeerId,
    pub whatami: WhatAmI,
    pub router: Arc<Router>,
    pub config: Notifier<Config>,
    pub manager: TransportManager,
    pub hlc: Option<Arc<HLC>>,
    pub(crate) stop_source: std::sync::RwLock<Option<StopSource>>,
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
    pub async fn new(version: u8, mut config: Config) -> ZResult<Runtime> {
        // Make sure to have have enough threads spawned in the async futures executor
        zasync_executor_init!();

        config
            .set_version(Some(version))
            .map_err(|e| zerror!("Unable to set version: {:?}", e))?;

        let pid = if let Some(s) = config.id() {
            s.parse()?
        } else {
            PeerId::from(uuid::Uuid::new_v4())
        };

        println!("Using PID: {}", pid);

        let whatami = config.mode().unwrap_or(crate::config::WhatAmI::Peer);
        let hlc = if config.add_timestamp().unwrap_or(false) {
            Some(Arc::new(
                HLCBuilder::new().with_id(uhlc::ID::from(&pid)).build(),
            ))
        } else {
            None
        };

        let peers_autoconnect = config.peers_autoconnect().unwrap_or(true);
        let routers_autoconnect_gossip = config
            .scouting()
            .gossip()
            .autoconnect()
            .map(|f| f.matches(whatami))
            .unwrap_or(false);
        let use_link_state = whatami == WhatAmI::Router && config.link_state().unwrap_or(true);
        let queries_default_timeout = config.queries_default_timeout().unwrap_or_else(|| {
            zenoh_cfg_properties::config::ZN_QUERIES_DEFAULT_TIMEOUT_DEFAULT
                .parse()
                .unwrap()
        });

        let router = Arc::new(Router::new(
            pid,
            whatami,
            hlc.clone(),
            Duration::from_millis(queries_default_timeout),
        ));

        let handler = Arc::new(RuntimeTransportEventHandler {
            runtime: std::sync::RwLock::new(None),
        });

        let transport_manager = TransportManager::builder()
            .from_config(&config)
            .await?
            .version(version)
            .whatami(whatami)
            .pid(pid)
            .build(handler.clone())?;

        let config = Notifier::new(config);

        let mut runtime = Runtime {
            state: Arc::new(RuntimeState {
                pid,
                whatami,
                router,
                config: config.clone(),
                manager: transport_manager,
                hlc,
                stop_source: std::sync::RwLock::new(Some(StopSource::new())),
            }),
        };
        *handler.runtime.write().unwrap() = Some(runtime.clone());
        if use_link_state {
            get_mut_unchecked(&mut runtime.router.clone()).init_link_state(
                runtime.clone(),
                peers_autoconnect,
                routers_autoconnect_gossip,
            );
        }

        let receiver = config.subscribe();
        runtime.spawn({
            let runtime2 = runtime.clone();
            async move {
                let mut stream = receiver.into_stream();
                while let Some(event) = stream.next().await {
                    if &*event == "peers" {
                        if let Err(e) = runtime2.update_peers().await {
                            log::error!("Error updating peers : {}", e);
                        }
                    }
                }
            }
        });

        match runtime.start().await {
            Ok(()) => Ok(runtime),
            Err(err) => Err(err),
        }
    }

    #[inline(always)]
    pub fn manager(&self) -> &TransportManager {
        &self.manager
    }

    pub async fn close(&self) -> ZResult<()> {
        log::trace!("Runtime::close())");
        drop(self.stop_source.write().unwrap().take());
        self.manager().close().await;
        Ok(())
    }

    pub fn get_pid_str(&self) -> String {
        self.pid.to_string()
    }

    pub fn new_timestamp(&self) -> Option<uhlc::Timestamp> {
        self.hlc.as_ref().map(|hlc| hlc.new_timestamp())
    }

    pub(crate) fn spawn<F, T>(&self, future: F) -> Option<JoinHandle<Result<T, TimedOutError>>>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.stop_source
            .read()
            .unwrap()
            .as_ref()
            .map(|source| async_std::task::spawn(future.timeout_at(source.token())))
    }
}

struct RuntimeTransportEventHandler {
    runtime: std::sync::RwLock<Option<Runtime>>,
}

impl TransportEventHandler for RuntimeTransportEventHandler {
    fn new_unicast(
        &self,
        _peer: TransportPeer,
        transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        match zread!(self.runtime).as_ref() {
            Some(runtime) => Ok(Arc::new(RuntimeSession {
                runtime: runtime.clone(),
                locator: std::sync::RwLock::new(None),
                sub_event_handler: runtime.router.new_transport_unicast(transport).unwrap(),
            })),
            None => bail!("Runtime not yet ready!"),
        }
    }

    fn new_multicast(
        &self,
        _transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        // @TODO
        unimplemented!();
    }
}

pub(super) struct RuntimeSession {
    pub(super) runtime: Runtime,
    pub(super) locator: std::sync::RwLock<Option<Locator>>,
    pub(super) sub_event_handler: Arc<LinkStateInterceptor>,
}

impl TransportPeerEventHandler for RuntimeSession {
    fn handle_message(&self, mut msg: ZenohMessage) -> ZResult<()> {
        // critical path shortcut
        if let ZenohBody::Data(data) = msg.body {
            if data.reply_context.is_none() {
                let face = &self.sub_event_handler.face.state;
                full_reentrant_route_data(
                    &self.sub_event_handler.tables,
                    face,
                    &data.key,
                    msg.channel,
                    data.congestion_control,
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
