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
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
mod adminspace;
pub mod orchestrator;

#[cfg(feature = "plugins")]
use std::sync::{Mutex, MutexGuard};
use std::{
    any::Any,
    collections::HashSet,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Weak,
    },
};

pub use adminspace::AdminSpace;
use async_trait::async_trait;
use futures::{stream::StreamExt, Future};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uhlc::{HLCBuilder, HLC};
use zenoh_config::{
    gateway::BoundFilterConf, unwrap_or_default, Interface, ModeDependent, ZenohId,
};
use zenoh_link::{EndPoint, Link};
use zenoh_plugin_trait::{PluginStartArgs, StructVersion};
use zenoh_protocol::{
    common::ZExtBody,
    core::{Locator, Reliability, WhatAmI, ZenohIdProto},
    network::{oam, NetworkBodyMut, NetworkMessageMut, Oam},
};
use zenoh_result::{bail, ZResult};
use zenoh_runtime::ZRuntime;
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
#[cfg(feature = "internal")]
use crate::session::CloseBuilder;
use crate::{
    api::{
        builders::close::{Closeable, Closee},
        config::{Config, Notifier},
    },
    net::routing::{dispatcher::gateway::Bound, router::RouterBuilder},
    GIT_VERSION, LONG_VERSION,
};

pub(crate) struct RuntimeState {
    zid: ZenohId,
    whatami: WhatAmI,
    next_id: AtomicU32,
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
    pending_connections: tokio::sync::Mutex<HashSet<ZenohIdProto>>,
    span: tracing::Span,
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
    config: zenoh_config::Config,
    #[cfg(feature = "plugins")]
    plugins_manager: Option<PluginsManager>,
    #[cfg(feature = "shared-memory")]
    shm_clients: Option<Arc<ShmClientStorage>>,
}

impl RuntimeBuilder {
    pub fn new(config: Config) -> Self {
        Self {
            config: config.0,
            #[cfg(feature = "plugins")]
            plugins_manager: None,
            #[cfg(feature = "shared-memory")]
            shm_clients: None,
        }
    }

    #[cfg(all(feature = "plugins", feature = "internal"))]
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
        let zid = (*config.id()).unwrap_or_default().into();
        tracing::info!("Using ZID: {}", zid);

        let whatami = unwrap_or_default!(config.mode());
        let hlc = (*unwrap_or_default!(config.timestamping().enabled().get(whatami)))
            .then(|| Arc::new(HLCBuilder::new().with_id(uhlc::ID::from(&zid)).build()));

        let mut router_builder = RouterBuilder::new(&config);
        if let Some(hlc) = hlc.as_ref().cloned() {
            router_builder = router_builder.hlc(hlc.clone());
        }
        let router = Arc::new(router_builder.build()?);

        let handler = Arc::new(RuntimeTransportEventHandler {
            runtime: std::sync::RwLock::new(WeakRuntime { state: Weak::new() }),
        });

        let transport_manager_builder = TransportManager::builder()
            .from_config(&config)
            .await?
            .whatami(whatami)
            .zid(zid);

        #[cfg(feature = "shared-memory")]
        let transport_manager_builder =
            transport_manager_builder.shm_reader(shm_clients.map(ShmReader::new));

        let transport_manager = transport_manager_builder.build(handler.clone())?;

        // Plugins manager
        #[cfg(feature = "plugins")]
        let plugins_manager = plugins_manager
            .take()
            .unwrap_or_else(|| load_plugins(&config));
        // Admin space creation flag
        let start_admin_space = *config.adminspace.enabled();
        // SHM lazy init flag
        #[cfg(feature = "shared-memory")]
        let shm_init_mode = *config.transport.shared_memory.mode();

        let config = Notifier::new(crate::config::Config(config));
        let span = tracing::trace_span!("rt", zid = %zid.short());
        let runtime = Runtime {
            state: Arc::new(RuntimeState {
                zid: zid.into(),
                whatami,
                next_id: AtomicU32::new(1), // 0 is reserved for routing core
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
                pending_connections: tokio::sync::Mutex::new(HashSet::new()),
                span,
            }),
        };
        *handler.runtime.write().unwrap() = Runtime::downgrade(&runtime);
        get_mut_unchecked(&mut runtime.state.router.clone()).init_hats(runtime.clone())?;

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

        #[cfg(feature = "shared-memory")]
        match shm_init_mode {
            zenoh_config::ShmInitMode::Init => zenoh_shm::init::init(),
            zenoh_config::ShmInitMode::Lazy => {}
        };

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
    pub fn plugins_manager(&self) -> MutexGuard<'_, PluginsManager> {
        zlock!(self.state.plugins_manager)
    }

    pub(crate) fn new_handler(&self, handler: Arc<dyn TransportEventHandler>) {
        zwrite!(self.state.transport_handlers).push(handler);
    }

    #[inline]
    pub fn next_id(&self) -> u32 {
        self.state.next_id.fetch_add(1, Ordering::SeqCst)
    }

    #[cfg(feature = "internal")]
    pub fn close(&self) -> CloseBuilder<Self> {
        CloseBuilder::new(self)
    }

    pub fn is_closed(&self) -> bool {
        self.state
            .task_controller
            .get_cancellation_token()
            .is_cancelled()
    }

    pub fn new_timestamp(&self) -> Option<uhlc::Timestamp> {
        self.state.hlc.as_ref().map(|hlc| hlc.new_timestamp())
    }

    pub fn get_locators(&self) -> Vec<Locator> {
        self.state.locators.read().unwrap().clone()
    }

    /// Spawns a task within runtime.
    /// Upon close runtime will block until this task completes
    pub(crate) fn spawn<F, T>(&self, future: F) -> JoinHandle<T>
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

    pub(crate) async fn insert_pending_connection(&self, zid: ZenohIdProto) -> bool {
        self.state.pending_connections.lock().await.insert(zid)
    }

    pub(crate) async fn remove_pending_connection(&self, zid: &ZenohIdProto) -> bool {
        self.state.pending_connections.lock().await.remove(zid)
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
                let _span = runtime.state.span.enter();
                let slave_handlers: Vec<Arc<dyn TransportPeerEventHandler>> =
                    zread!(runtime.state.transport_handlers)
                        .iter()
                        .filter_map(|handler| {
                            handler.new_unicast(peer.clone(), transport.clone()).ok()
                        })
                        .collect();

                let bound = compute_bound(&peer, &runtime.config().lock())?;

                fn north_bound_transport_peer_count(
                    runtime: &Runtime,
                    new_peer: &TransportPeer,
                ) -> usize {
                    ZRuntime::Application.block_in_place(async {
                        runtime
                            .manager()
                            .get_transports_unicast()
                            .await
                            .iter()
                            .filter(|transport| {
                                let Ok(peer) = transport.get_peer() else {
                                    tracing::error!(
                                        "Could not get transport peer \
                                        while computing north-bound transport count. \
                                        Will ignore this transport"
                                    );
                                    return false;
                                };

                                if &peer == new_peer {
                                    return false;
                                }

                                // NOTE(regions): compute bound instead of querying the router as
                                // the corresponding transport face might not exist yet
                                let Ok(bound) = compute_bound(&peer, &runtime.config().lock())
                                else {
                                    tracing::error!(
                                        zid = %peer.zid.short(),
                                        wai = %peer.whatami.short(),
                                        "Could not get transport peer bound \
                                        while computing north-bound transport count. \
                                        Will ignore this transport"
                                    );
                                    return false;
                                };

                                bound.is_north()
                            })
                            .count()
                    })
                }

                if bound.is_north()
                    && runtime.whatami() == WhatAmI::Client
                    && north_bound_transport_peer_count(runtime, &peer) > 0
                {
                    bail!("Client runtimes only accept one north-bound transport");
                }

                if !bound.is_north() && peer.whatami == WhatAmI::Peer {
                    transport.schedule(NetworkMessageMut {
                        body: NetworkBodyMut::OAM(&mut Oam {
                            id: oam::id::OAM_IS_GATEWAY,
                            body: ZExtBody::Unit,
                            ext_qos: oam::ext::QoSType::DECLARE,
                            ext_tstamp: None,
                        }),
                        reliability: Reliability::Reliable,
                    })?;
                }

                Ok(Arc::new(RuntimeSession {
                    runtime: runtime.clone(),
                    endpoint: std::sync::RwLock::new(None),
                    main_handler: runtime
                        .state
                        .router
                        .new_transport_unicast(transport, bound)
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
                let _span = runtime.state.span.enter();
                let slave_handlers: Vec<Arc<dyn TransportMulticastEventHandler>> =
                    zread!(runtime.state.transport_handlers)
                        .iter()
                        .filter_map(|handler| handler.new_multicast(transport.clone()).ok())
                        .collect();

                // FIXME(regions): multicast support
                let bound = Bound::unbound();

                runtime
                    .state
                    .router
                    .new_transport_multicast(transport.clone(), bound)?;
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
    fn handle_message(&self, msg: NetworkMessageMut) -> ZResult<()> {
        self.main_handler.handle_message(msg)
    }

    fn new_link(&self, link: Link) {
        let _span = self.runtime.state.span.enter();
        self.main_handler.new_link(link.clone());
        for handler in &self.slave_handlers {
            handler.new_link(link.clone());
        }
    }

    fn del_link(&self, link: Link) {
        let _span = self.runtime.state.span.enter();
        self.main_handler.del_link(link.clone());
        for handler in &self.slave_handlers {
            handler.del_link(link.clone());
        }
    }

    fn closed(&self) {
        let _span = self.runtime.state.span.enter();
        self.main_handler.closed();
        Runtime::closed_session(self);
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

        // FIXME(regions): multicast support
        let bound = Bound::unbound();

        Ok(Arc::new(RuntimeMulticastSession {
            main_handler: self.runtime.state.router.new_peer_multicast(
                self.transport.clone(),
                peer,
                bound,
            )?,
            slave_handlers,
        }))
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
    fn handle_message(&self, msg: NetworkMessageMut) -> ZResult<()> {
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

#[async_trait]
impl Closee for Arc<RuntimeState> {
    async fn close_inner(&self) {
        tracing::trace!("Runtime::close())");
        // TODO: Plugins should be stopped
        // TODO: Check this whether is able to terminate all spawned task by Runtime::spawn
        self.task_controller.terminate_all_async().await;
        self.manager.close().await;
        // clean up to break cyclic reference of self.state to itself
        self.transport_handlers.write().unwrap().clear();
        // TODO: the call below is needed to prevent intermittent leak
        // due to not freed resource Arc, that apparently happens because
        // the task responsible for resource clean up was aborted earlier than expected.
        // This should be resolved by identfying correspodning task, and placing
        // cancellation token manually inside it.
        let mut tables = self.router.tables.tables.write().unwrap();
        tables.data.root_res.close();
        tables.data.faces.clear();
    }
}

impl Closeable for Runtime {
    type TClosee = Arc<RuntimeState>;

    fn get_closee(&self) -> Self::TClosee {
        self.state.clone()
    }
}

fn compute_bound(peer: &TransportPeer, config: &Config) -> ZResult<Bound> {
    let gateway_config = config
        .gateway
        .get(zenoh_config::unwrap_or_default!(config.mode()))
        .ok_or_else(|| zerror!("Undefined gateway configuration"))?;

    fn matches(peer: &TransportPeer, filter: &BoundFilterConf) -> bool {
        filter
            .zids
            .as_ref()
            .map(|zid| zid.contains(&peer.zid.into()))
            .unwrap_or(true)
            && filter
                .interfaces
                .as_ref()
                .map(|ifaces| {
                    peer.links
                        .iter()
                        .flat_map(|link| {
                            link.interfaces
                                .iter()
                                .map(|iface| Interface(iface.to_owned()))
                        })
                        .all(|iface| ifaces.contains(&iface))
                })
                .unwrap_or(true)
            && filter
                .modes
                .as_ref()
                .map(|mode| mode.matches(peer.whatami))
                .unwrap_or(true)
    }

    let north = gateway_config
        .north
        .filters
        .as_ref()
        .map(|filters| filters.iter().any(|filter| matches(peer, filter)))
        .unwrap_or(true);

    let south = gateway_config.south.iter().position(|south| {
        south
            .filters
            .as_ref()
            .map(|filters| filters.iter().any(|filter| matches(peer, filter)))
            .unwrap_or(true)
    });

    Ok(match (north, south) {
        (true, None) => {
            tracing::debug!(zid = %peer.zid, "Transport peer is north-bound");
            Bound::north()
        }
        (false, Some(index)) => {
            tracing::debug!(zid = %peer.zid, "Transport peer is south-bound");
            Bound::south(index)
        }
        (false, None) => {
            tracing::info!(
                zid = %peer.zid,
                "Transport peer matches neither north nor south filters. \
                Using eastwest region instead"
            );
            Bound::unbound()
        }
        (true, Some(_)) => {
            tracing::warn!(
                zid = %peer.zid,
                "Transport peer matches north and south filters. \
                Using eastwest region instead"
            );
            Bound::unbound()
        }
    })
}
