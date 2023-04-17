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
use crate::config::{unwrap_or_default, Config, ModeDependent, Notifier};
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
use zenoh_link::{EndPoint, Link};
use zenoh_protocol::{
    core::{whatami::WhatAmIMatcher, Locator, WhatAmI, ZenohId},
    zenoh::{ZenohBody, ZenohMessage},
};
use zenoh_result::{bail, ZResult};
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
    pub transport_handlers: std::sync::RwLock<Vec<Arc<dyn TransportEventHandler>>>,
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
        let mut runtime = Runtime::init(config).await?;
        match runtime.start().await {
            Ok(()) => Ok(runtime),
            Err(err) => Err(err),
        }
    }

    pub(crate) async fn init(config: Config) -> ZResult<Runtime> {
        log::debug!("Zenoh Rust API {}", GIT_VERSION);
        // Make sure to have have enough threads spawned in the async futures executor
        zasync_executor_init!();

        let zid = *config.id();

        log::info!("Using PID: {}", zid);

        let whatami = unwrap_or_default!(config.mode());
        let hlc = (*unwrap_or_default!(config.timestamping().enabled().get(whatami)))
            .then(|| Arc::new(HLCBuilder::new().with_id(uhlc::ID::from(&zid)).build()));
        let drop_future_timestamp =
            unwrap_or_default!(config.timestamping().drop_future_timestamp());

        let gossip = unwrap_or_default!(config.scouting().gossip().enabled());
        let gossip_multihop = unwrap_or_default!(config.scouting().gossip().multihop());
        let autoconnect = if gossip {
            *unwrap_or_default!(config.scouting().gossip().autoconnect().get(whatami))
        } else {
            WhatAmIMatcher::empty()
        };

        let router_link_state = whatami == WhatAmI::Router;
        let peer_link_state = whatami != WhatAmI::Client
            && unwrap_or_default!(config.routing().peer().mode()) == *"linkstate";
        let router_peers_failover_brokering =
            unwrap_or_default!(config.routing().router().peers_failover_brokering());
        let queries_default_timeout =
            Duration::from_millis(unwrap_or_default!(config.queries_default_timeout()));

        let router = Arc::new(Router::new(
            zid,
            whatami,
            hlc.clone(),
            drop_future_timestamp,
            router_peers_failover_brokering,
            queries_default_timeout,
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

        let runtime = Runtime {
            state: Arc::new(RuntimeState {
                zid,
                whatami,
                router,
                config: config.clone(),
                manager: transport_manager,
                transport_handlers: std::sync::RwLock::new(vec![]),
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
                            log::error!("Error updating peers: {}", e);
                        }
                    }
                }
            }
        });

        Ok(runtime)
    }

    #[inline(always)]
    pub fn manager(&self) -> &TransportManager {
        &self.manager
    }

    pub fn new_handler(&self, handler: Arc<dyn TransportEventHandler>) {
        zwrite!(self.state.transport_handlers).push(handler);
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
        peer: TransportPeer,
        transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        match zread!(self.runtime).as_ref() {
            Some(runtime) => {
                let slave_handlers: Vec<Arc<dyn TransportPeerEventHandler>> =
                    zread!(runtime.transport_handlers)
                        .iter()
                        .filter_map(|handler| {
                            handler.new_unicast(peer.clone(), transport.clone()).ok()
                        })
                        .collect();
                Ok(Arc::new(RuntimeSession {
                    runtime: runtime.clone(),
                    endpoint: std::sync::RwLock::new(None),
                    main_handler: runtime.router.new_transport_unicast(transport).unwrap(),
                    slave_handlers,
                }))
            }
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
    pub(super) main_handler: Arc<LinkStateInterceptor>,
    pub(super) slave_handlers: Vec<Arc<dyn TransportPeerEventHandler>>,
}

impl TransportPeerEventHandler for RuntimeSession {
    fn handle_message(&self, mut msg: ZenohMessage) -> ZResult<()> {
        // critical path shortcut
        if let ZenohBody::Data(data) = msg.body {
            if data.reply_context.is_none() {
                let face = &self.main_handler.face.state;
                full_reentrant_route_data(
                    &self.main_handler.tables.tables,
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

        self.main_handler.handle_message(msg)
    }

    fn new_link(&self, link: Link) {
        self.main_handler.new_link(link.clone());
        for handler in &self.slave_handlers {
            handler.new_link(link.clone());
        }
    }

    fn del_link(&self, link: Link) {
        self.main_handler.del_link(link.clone());
        for handler in &self.slave_handlers {
            handler.del_link(link.clone());
        }
    }

    fn closing(&self) {
        self.main_handler.closing();
        Runtime::closing_session(self);
        for handler in &self.slave_handlers {
            handler.closing();
        }
    }

    fn closed(&self) {
        self.main_handler.closed();
        for handler in &self.slave_handlers {
            handler.closed();
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
