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

#[cfg(feature = "plugins")]
use std::sync::{Mutex, MutexGuard};
use std::{
    any::Any,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Weak,
    },
    time::Duration,
};

pub use adminspace::AdminSpace;
use futures::{stream::StreamExt, Future};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uhlc::{HLCBuilder, HLC};
use zenoh_config::wrappers::ZenohId;
use zenoh_link::{EndPoint, Link};
use zenoh_plugin_trait::{PluginStartArgs, StructVersion};
use zenoh_protocol::{
    core::{Locator, WhatAmI},
    network::NetworkMessage,
};
use zenoh_result::{bail, ZResult};
#[cfg(feature = "shared-memory")]
use zenoh_shm::api::client_storage::ShmClientStorage;
#[cfg(feature = "shared-memory")]
use zenoh_shm::reader::ShmReader;
use zenoh_sync::get_mut_unchecked;
use zenoh_task::TaskController;
use zenoh_transport::{
    multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
    TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

use self::orchestrator::StartConditions;
use super::{primitives::DeMux, routing, routing::router::Router};
#[cfg(feature = "plugins")]
use crate::api::loader::{load_plugins, start_plugins};
#[cfg(feature = "plugins")]
use crate::api::plugins::PluginsManager;
use crate::{
    config::{unwrap_or_default, Config, ModeDependent, Notifier},
    GIT_VERSION, LONG_VERSION,
};

pub(crate) struct RuntimeState {
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
    task_controller: TaskController,
    #[cfg(feature = "plugins")]
    plugins_manager: Mutex<PluginsManager>,
    start_conditions: Arc<StartConditions>,
}

pub struct WeakRuntime {
    state: Weak<RuntimeState>,
}

impl WeakRuntime {
    pub fn upgrade(&self) -> Option<Runtime> {
        self.state.upgrade().map(|state| Runtime { state })
    }
}

pub struct RuntimeBuilder {
    config: Config,
    #[cfg(feature = "plugins")]
    plugins_manager: Option<PluginsManager>,
    #[cfg(feature = "shared-memory")]
    shm_clients: Option<Arc<ShmClientStorage>>,
}

impl RuntimeBuilder {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            #[cfg(feature = "plugins")]
            plugins_manager: None,
            #[cfg(feature = "shared-memory")]
            shm_clients: None,
        }
    }

    #[cfg(feature = "plugins")]
    pub fn plugins_manager<T: Into<Option<PluginsManager>>>(mut self, plugins_manager: T) -> Self {
        self.plugins_manager = plugins_manager.into();
        self
    }

    #[cfg(feature = "shared-memory")]
    pub fn shm_clients(mut self, shm_clients: Option<Arc<ShmClientStorage>>) -> Self {
        self.shm_clients = shm_clients;
        self
    }

    pub async fn build(self) -> ZResult<Runtime> {
        let RuntimeBuilder {
            config,
            #[cfg(feature = "plugins")]
            mut plugins_manager,
            #[cfg(feature = "shared-memory")]
            shm_clients,
        } = self;

        tracing::debug!("Zenoh Rust API {}", GIT_VERSION);
        let zid = (*config.id()).into();
        tracing::info!("Using ZID: {}", zid);

        let whatami = unwrap_or_default!(config.mode());
        let metadata = config.metadata().clone();
        let hlc = (*unwrap_or_default!(config.timestamping().enabled().get(whatami)))
            .then(|| Arc::new(HLCBuilder::new().with_id(uhlc::ID::from(&zid)).build()));

        let router = Arc::new(Router::new(zid, whatami, hlc.clone(), &config)?);

        let handler = Arc::new(RuntimeTransportEventHandler {
            runtime: std::sync::RwLock::new(WeakRuntime { state: Weak::new() }),
        });

        let transport_manager = TransportManager::builder()
            .from_config(&config)
            .await?
            .whatami(whatami)
            .zid(zid);

        #[cfg(feature = "unstable")]
        let transport_manager = zcondfeat!(
            "shared-memory",
            transport_manager.shm_reader(shm_clients.map(ShmReader::new)),
            transport_manager
        )
        .build(handler.clone())?;

        #[cfg(not(feature = "unstable"))]
        let transport_manager = transport_manager.build(handler.clone())?;

        // Plugins manager
        #[cfg(feature = "plugins")]
        let plugins_manager = plugins_manager
            .take()
            .unwrap_or_else(|| load_plugins(&config));
        // Admin space creation flag
        let start_admin_space = *config.adminspace.enabled();

        let config = Notifier::new(config);
        let runtime = Runtime {
            state: Arc::new(RuntimeState {
                zid: zid.into(),
                whatami,
                next_id: AtomicU32::new(1), // 0 is reserved for routing core
                metadata,
                router,
                config: config.clone(),
                manager: transport_manager,
                transport_handlers: std::sync::RwLock::new(vec![]),
                locators: std::sync::RwLock::new(vec![]),
                hlc,
                task_controller: TaskController::default(),
                #[cfg(feature = "plugins")]
                plugins_manager: Mutex::new(plugins_manager),
                start_conditions: Arc::new(StartConditions::default()),
            }),
        };
        *handler.runtime.write().unwrap() = Runtime::downgrade(&runtime);
        get_mut_unchecked(&mut runtime.state.router.clone()).init_link_state(runtime.clone());

        // Admin space
        if start_admin_space {
            AdminSpace::start(&runtime, LONG_VERSION.clone()).await;
        }

        // Start plugins
        #[cfg(feature = "plugins")]
        start_plugins(&runtime);

        // Start notifier task
        let receiver = config.subscribe();
        let token = runtime.get_cancellation_token();
        runtime.spawn({
            let runtime2 = runtime.clone();
            async move {
                let mut stream = receiver.into_stream();
                loop {
                    tokio::select! {
                        res = stream.next() => {
                            match res {
                                Some(event) => {
                                    if &*event == "connect/endpoints" {
                                        if let Err(e) = runtime2.update_peers().await {
                                            tracing::error!("Error updating peers: {}", e);
                                        }
                                    }
                                },
                                None => { break; }
                            }
                        }
                        _ = token.cancelled() => { break; }
                    }
                }
            }
        });

        Ok(runtime)
    }
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
    #[inline(always)]
    pub(crate) fn manager(&self) -> &TransportManager {
        &self.state.manager
    }

    #[cfg(feature = "plugins")]
    #[inline(always)]
    pub(crate) fn plugins_manager(&self) -> MutexGuard<'_, PluginsManager> {
        zlock!(self.state.plugins_manager)
    }

    pub(crate) fn new_handler(&self, handler: Arc<dyn TransportEventHandler>) {
        zwrite!(self.state.transport_handlers).push(handler);
    }

    #[inline]
    pub fn next_id(&self) -> u32 {
        self.state.next_id.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn close(&self) -> ZResult<()> {
        tracing::trace!("Runtime::close())");
        // TODO: Plugins should be stopped
        // TODO: Check this whether is able to terminate all spawned task by Runtime::spawn
        self.state
            .task_controller
            .terminate_all(Duration::from_secs(10));
        self.manager().close().await;
        // clean up to break cyclic reference of self.state to itself
        self.state.transport_handlers.write().unwrap().clear();
        // TODO: the call below is needed to prevent intermittent leak
        // due to not freed resource Arc, that apparently happens because
        // the task responsible for resource clean up was aborted earlier than expected.
        // This should be resolved by identfying correspodning task, and placing
        // cancellation token manually inside it.
        self.router()
            .tables
            .tables
            .write()
            .unwrap()
            .root_res
            .close();
        Ok(())
    }

    pub fn new_timestamp(&self) -> Option<uhlc::Timestamp> {
        self.state.hlc.as_ref().map(|hlc| hlc.new_timestamp())
    }

    pub fn get_locators(&self) -> Vec<Locator> {
        self.state.locators.read().unwrap().clone()
    }

    /// Spawns a task within runtime.
    /// Upon close runtime will block until this task completes
    pub(crate) fn spawn<F, T>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.state
            .task_controller
            .spawn_with_rt(zenoh_runtime::ZRuntime::Net, future)
    }

    /// Spawns a task within runtime.
    /// Upon runtime close the task will be automatically aborted.
    pub(crate) fn spawn_abortable<F, T>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.state
            .task_controller
            .spawn_abortable_with_rt(zenoh_runtime::ZRuntime::Net, future)
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

    pub fn downgrade(this: &Runtime) -> WeakRuntime {
        WeakRuntime {
            state: Arc::downgrade(&this.state),
        }
    }

    pub fn get_cancellation_token(&self) -> CancellationToken {
        self.state.task_controller.get_cancellation_token()
    }

    pub(crate) fn start_conditions(&self) -> &Arc<StartConditions> {
        &self.state.start_conditions
    }
}

struct RuntimeTransportEventHandler {
    runtime: std::sync::RwLock<WeakRuntime>,
}

impl TransportEventHandler for RuntimeTransportEventHandler {
    fn new_unicast(
        &self,
        peer: TransportPeer,
        transport: TransportUnicast,
    ) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        match zread!(self.runtime).upgrade().as_ref() {
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
        match zread!(self.runtime).upgrade().as_ref() {
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
                Ok(Arc::new(RuntimeMulticastGroup {
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

pub(super) struct RuntimeMulticastGroup {
    pub(super) runtime: Runtime,
    pub(super) transport: TransportMulticast,
    pub(super) slave_handlers: Vec<Arc<dyn TransportMulticastEventHandler>>,
}

impl TransportMulticastEventHandler for RuntimeMulticastGroup {
    fn new_peer(&self, peer: TransportPeer) -> ZResult<Arc<dyn TransportPeerEventHandler>> {
        let slave_handlers: Vec<Arc<dyn TransportPeerEventHandler>> = self
            .slave_handlers
            .iter()
            .filter_map(|handler| handler.new_peer(peer.clone()).ok())
            .collect();
        Ok(Arc::new(RuntimeMulticastSession {
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

pub(super) struct RuntimeMulticastSession {
    pub(super) main_handler: Arc<DeMux>,
    pub(super) slave_handlers: Vec<Arc<dyn TransportPeerEventHandler>>,
}

impl TransportPeerEventHandler for RuntimeMulticastSession {
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
