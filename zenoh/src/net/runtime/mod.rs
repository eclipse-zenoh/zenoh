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

use super::primitives::DeMux;
use super::routing;
use super::routing::router::Router;
use crate::config::{unwrap_or_default, Config, ModeDependent, Notifier};
use crate::GIT_VERSION;
pub use adminspace::AdminSpace;
use futures::stream::StreamExt;
use futures::Future;
use std::any::Any;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uhlc::{HLCBuilder, HLC};
use zenoh_link::{EndPoint, Link};
use zenoh_plugin_trait::{PluginStartArgs, StructVersion};
use zenoh_protocol::core::{Locator, WhatAmI, ZenohId};
use zenoh_protocol::network::NetworkMessage;
use zenoh_result::{bail, ZResult};
use zenoh_sync::get_mut_unchecked;
use zenoh_transport::{
    multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
    TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

struct RuntimeState {
    zid: ZenohId,
    whatami: WhatAmI,
    next_id: AtomicU32,
    metadata: serde_json::Value,
    router: Arc<Router>,
    config: Notifier<Config>,
    manager: TransportManager,
    transport_handlers: std::sync::RwLock<Vec<Arc<dyn TransportEventHandler>>>,
    locators: std::sync::RwLock<Vec<Locator>>,
    hlc: Option<Arc<HLC>>,
    token: CancellationToken,
}

#[derive(Clone)]
pub struct Runtime {
    state: Arc<RuntimeState>,
}

impl StructVersion for Runtime {
    fn struct_version() -> u64 {
        1
    }
    fn struct_features() -> &'static str {
        crate::FEATURES
    }
}

impl PluginStartArgs for Runtime {}

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

        let zid = *config.id();

        log::info!("Using PID: {}", zid);

        let whatami = unwrap_or_default!(config.mode());
        let metadata = config.metadata().clone();
        let hlc = (*unwrap_or_default!(config.timestamping().enabled().get(whatami)))
            .then(|| Arc::new(HLCBuilder::new().with_id(uhlc::ID::from(&zid)).build()));

        let router = Arc::new(Router::new(zid, whatami, hlc.clone(), &config)?);

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
                next_id: AtomicU32::new(1), // 0 is reserved for routing core
                metadata,
                router,
                config: config.clone(),
                manager: transport_manager,
                transport_handlers: std::sync::RwLock::new(vec![]),
                locators: std::sync::RwLock::new(vec![]),
                hlc,
                token: CancellationToken::new(),
            }),
        };
        *handler.runtime.write().unwrap() = Some(runtime.clone());
        get_mut_unchecked(&mut runtime.state.router.clone()).init_link_state(runtime.clone());

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
        &self.state.manager
    }

    pub fn new_handler(&self, handler: Arc<dyn TransportEventHandler>) {
        zwrite!(self.state.transport_handlers).push(handler);
    }

    #[inline]
    pub fn next_id(&self) -> u32 {
        self.state.next_id.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn close(&self) -> ZResult<()> {
        log::trace!("Runtime::close())");
        // TODO: Check this whether is able to terminate all spawned task by Runtime::spawn
        self.state.token.cancel();
        self.manager().close().await;
        Ok(())
    }

    pub fn new_timestamp(&self) -> Option<uhlc::Timestamp> {
        self.state.hlc.as_ref().map(|hlc| hlc.new_timestamp())
    }

    pub fn get_locators(&self) -> Vec<Locator> {
        self.state.locators.read().unwrap().clone()
    }

    pub(crate) fn spawn<F, T>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let token = self.state.token.clone();
        zenoh_runtime::ZRuntime::Net.spawn(async move {
            tokio::select! {
                _ = token.cancelled() => {}
                _ = future => {}
            }
        })
    }

    pub(crate) fn router(&self) -> Arc<Router> {
        self.state.router.clone()
    }

    pub fn config(&self) -> &Notifier<Config> {
        &self.state.config
    }

    pub fn hlc(&self) -> Option<&HLC> {
        self.state.hlc.as_ref().map(Arc::as_ref)
    }

    pub fn zid(&self) -> ZenohId {
        self.state.zid
    }

    pub fn whatami(&self) -> WhatAmI {
        self.state.whatami
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
                    zread!(runtime.state.transport_handlers)
                        .iter()
                        .filter_map(|handler| {
                            handler.new_unicast(peer.clone(), transport.clone()).ok()
                        })
                        .collect();
                Ok(Arc::new(RuntimeSession {
                    runtime: runtime.clone(),
                    endpoint: std::sync::RwLock::new(None),
                    main_handler: runtime
                        .state
                        .router
                        .new_transport_unicast(transport)
                        .unwrap(),
                    slave_handlers,
                }))
            }
            None => bail!("Runtime not yet ready!"),
        }
    }

    fn new_multicast(
        &self,
        transport: TransportMulticast,
    ) -> ZResult<Arc<dyn TransportMulticastEventHandler>> {
        match zread!(self.runtime).as_ref() {
            Some(runtime) => {
                let slave_handlers: Vec<Arc<dyn TransportMulticastEventHandler>> =
                    zread!(runtime.state.transport_handlers)
                        .iter()
                        .filter_map(|handler| handler.new_multicast(transport.clone()).ok())
                        .collect();
                runtime
                    .state
                    .router
                    .new_transport_multicast(transport.clone())?;
                Ok(Arc::new(RuntimeMuticastGroup {
                    runtime: runtime.clone(),
                    transport,
                    slave_handlers,
                }))
            }
            None => bail!("Runtime not yet ready!"),
        }
    }
}

pub(super) struct RuntimeSession {
    pub(super) runtime: Runtime,
    pub(super) endpoint: std::sync::RwLock<Option<EndPoint>>,
    pub(super) main_handler: Arc<DeMux>,
    pub(super) slave_handlers: Vec<Arc<dyn TransportPeerEventHandler>>,
}

impl TransportPeerEventHandler for RuntimeSession {
    fn handle_message(&self, msg: NetworkMessage) -> ZResult<()> {
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

pub(super) struct RuntimeMuticastGroup {
    pub(super) runtime: Runtime,
    pub(super) transport: TransportMulticast,
    pub(super) slave_handlers: Vec<Arc<dyn TransportMulticastEventHandler>>,
}

impl TransportMulticastEventHandler for RuntimeMuticastGroup {
    fn new_peer(&self, peer: TransportPeer) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let slave_handlers: Vec<Arc<dyn TransportPeerEventHandler>> = self
            .slave_handlers
            .iter()
            .filter_map(|handler| handler.new_peer(peer.clone()).ok())
            .collect();
        Ok(Arc::new(RuntimeMuticastSession {
            main_handler: self
                .runtime
                .state
                .router
                .new_peer_multicast(self.transport.clone(), peer)?,
            slave_handlers,
        }))
    }

    fn closing(&self) {
        for handler in &self.slave_handlers {
            handler.closed();
        }
    }

    fn closed(&self) {
        for handler in &self.slave_handlers {
            handler.closed();
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub(super) struct RuntimeMuticastSession {
    pub(super) main_handler: Arc<DeMux>,
    pub(super) slave_handlers: Vec<Arc<dyn TransportPeerEventHandler>>,
}

impl TransportPeerEventHandler for RuntimeMuticastSession {
    fn handle_message(&self, msg: NetworkMessage) -> ZResult<()> {
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
