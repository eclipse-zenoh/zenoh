//
// Copyright (c) 2022 ZettaScale Technology
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

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
mod adminspace;
pub mod orchestrator;

use super::routing;
use super::routing::pubsub::full_reentrant_route_data;
use super::routing::router::{LinkStateInterceptor, Router};
use crate::config::{Config, ModeDependent, Notifier};
use crate::GIT_VERSION;
pub use adminspace::AdminSpace;
use async_std::task::JoinHandle;
use futures::stream::StreamExt;
use futures::Future;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;
use stop_token::future::FutureExt;
use stop_token::{StopSource, TimedOutError};
use uhlc::{HLCBuilder, HLC};
use zenoh_core::{bail, Result as ZResult};
use zenoh_link::{EndPoint, Link};
use zenoh_protocol::{
    core::{whatami::WhatAmIMatcher, Locator, WhatAmI, ZenohId},
    zenoh::{ZenohBody, ZenohMessage},
};
use zenoh_sync::get_mut_unchecked;
use zenoh_transport::{
    TransportEventHandler, TransportManager, TransportMulticast, TransportMulticastEventHandler,
    TransportPeer, TransportPeerEventHandler, TransportUnicast,
};

pub struct RuntimeState {
    pub zid: ZenohId,
    pub whatami: WhatAmI,
    pub router: Arc<Router>,
    pub config: Notifier<Config>,
    pub manager: TransportManager,
    pub(crate) locators: std::sync::RwLock<Vec<Locator>>,
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
    pub async fn new(config: Config) -> ZResult<Runtime> {
        log::debug!("Zenoh Rust API {}", GIT_VERSION);
        // Make sure to have have enough threads spawned in the async futures executor
        zasync_executor_init!();

        let zid = *config.id();

        log::info!("Using PID: {}", zid);

        let whatami = config.mode().unwrap_or(crate::config::WhatAmI::Peer);
        let hlc = match whatami {
            WhatAmI::Router => config
                .timestamping()
                .enabled()
                .router()
                .cloned()
                .unwrap_or(true),
            WhatAmI::Peer => config
                .timestamping()
                .enabled()
                .peer()
                .cloned()
                .unwrap_or(false),
            WhatAmI::Client => config
                .timestamping()
                .enabled()
                .client()
                .cloned()
                .unwrap_or(false),
        }
        .then(|| Arc::new(HLCBuilder::new().with_id(uhlc::ID::from(&zid)).build()));
        let drop_future_timestamp = config
            .timestamping()
            .drop_future_timestamp()
            .unwrap_or(false);

        let gossip = config.scouting().gossip().enabled().unwrap_or(true);
        let gossip_multihop = config.scouting().gossip().multihop().unwrap_or(false);
        let autoconnect = match whatami {
            WhatAmI::Router => {
                if config.scouting().gossip().enabled().unwrap_or(true) {
                    config
                        .scouting()
                        .gossip()
                        .autoconnect()
                        .router()
                        .cloned()
                        .unwrap_or_else(|| WhatAmIMatcher::try_from(128).unwrap())
                } else {
                    WhatAmIMatcher::try_from(128).unwrap()
                }
            }
            WhatAmI::Peer => {
                if config.scouting().gossip().enabled().unwrap_or(true) {
                    config
                        .scouting()
                        .gossip()
                        .autoconnect()
                        .peer()
                        .cloned()
                        .unwrap_or_else(|| WhatAmIMatcher::try_from(131).unwrap())
                } else {
                    WhatAmIMatcher::try_from(128).unwrap()
                }
            }
            _ => WhatAmIMatcher::try_from(128).unwrap(),
        };

        let router_link_state = whatami == WhatAmI::Router;
        let peer_link_state = whatami != WhatAmI::Client
            && *config.routing().peer().mode() == Some("linkstate".to_string());

        let router_peers_failover_brokering = config
            .routing()
            .router()
            .peers_failover_brokering()
            .unwrap_or(true);

        let queries_default_timeout = config.queries_default_timeout().unwrap_or_else(|| {
            zenoh_cfg_properties::config::ZN_QUERIES_DEFAULT_TIMEOUT_DEFAULT
                .parse()
                .unwrap()
        });

        let router = Arc::new(Router::new(
            zid,
            whatami,
            hlc.clone(),
            drop_future_timestamp,
            router_peers_failover_brokering,
            Duration::from_millis(queries_default_timeout),
        ));

        let handler = Arc::new(RuntimeTransportEventHandler {
            runtime: std::sync::RwLock::new(None),
        });

        let transport_manager = TransportManager::builder()
            .from_config(&config)
            .await?
            .whatami(whatami)
            .zid(zid)
            .build(handler.clone())?;

        let config = Notifier::new(config);

        let mut runtime = Runtime {
            state: Arc::new(RuntimeState {
                zid,
                whatami,
                router,
                config: config.clone(),
                manager: transport_manager,
                locators: std::sync::RwLock::new(vec![]),
                hlc,
                stop_source: std::sync::RwLock::new(Some(StopSource::new())),
            }),
        };
        *handler.runtime.write().unwrap() = Some(runtime.clone());
        get_mut_unchecked(&mut runtime.router.clone()).init_link_state(
            runtime.clone(),
            router_link_state,
            peer_link_state,
            router_peers_failover_brokering,
            gossip,
            gossip_multihop,
            autoconnect,
        );

        let receiver = config.subscribe();
        runtime.spawn({
            let runtime2 = runtime.clone();
            async move {
                let mut stream = receiver.into_stream();
                while let Some(event) = stream.next().await {
                    if &*event == "connect/endpoints" {
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

    pub fn new_timestamp(&self) -> Option<uhlc::Timestamp> {
        self.hlc.as_ref().map(|hlc| hlc.new_timestamp())
    }

    pub fn get_locators(&self) -> Vec<Locator> {
        self.locators.read().unwrap().clone()
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
                endpoint: std::sync::RwLock::new(None),
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
    pub(super) endpoint: std::sync::RwLock<Option<EndPoint>>,
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
