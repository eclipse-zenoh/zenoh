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
    ops::Deref,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Weak,
    },
};

pub use adminspace::AdminSpace;
use async_trait::async_trait;
use futures::Future;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uhlc::{HLCBuilder, HLC};
use zenoh_config::{unwrap_or_default, GenericConfig, IConfig, ModeDependent, ZenohId};
use zenoh_keyexpr::OwnedNonWildKeyExpr;
use zenoh_link::{EndPoint, Link};
use zenoh_plugin_trait::{PluginStartArgs, StructVersion};
use zenoh_protocol::{
    core::{Locator, WhatAmI, ZenohIdProto},
    network::NetworkMessageMut,
};
use zenoh_result::{bail, ZResult};
#[cfg(feature = "shared-memory")]
use zenoh_shm::api::{
    client_storage::ShmClientStorage,
    protocol_implementations::posix::posix_shm_provider_backend::PosixShmProviderBackend,
    provider::shm_provider::ShmProvider,
};
#[cfg(feature = "shared-memory")]
use zenoh_shm::reader::ShmReader;
use zenoh_sync::get_mut_unchecked;
use zenoh_task::TaskController;
use zenoh_transport::{
    multicast::TransportMulticast, unicast::TransportUnicast, TransportEventHandler,
    TransportManager, TransportMulticastEventHandler, TransportPeer, TransportPeerEventHandler,
};

use self::orchestrator::StartConditions;
use super::{
    primitives::{DeMux, EPrimitives, Primitives},
    routing::{
        self,
        namespace::{ENamespace, Namespace},
        router::Router,
    },
};
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
    GIT_VERSION, LONG_VERSION,
};

/// State of current lazily-initialized [`ShmProvider`](ShmProvider) associated with [`Runtime`](Runtime)
#[cfg(feature = "shared-memory")]
#[zenoh_macros::unstable]
pub enum ShmProviderState {
    Disabled,
    Initializing,
    Ready(Arc<ShmProvider<PosixShmProviderBackend>>),
    Error,
}

#[cfg(feature = "shared-memory")]
#[zenoh_macros::unstable]
impl ShmProviderState {
    pub fn into_option(self) -> Option<Arc<ShmProvider<PosixShmProviderBackend>>> {
        match self {
            ShmProviderState::Ready(provider) => Some(provider),
            _ => None,
        }
    }
}

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
    namespace: Option<OwnedNonWildKeyExpr>,
}

#[allow(private_interfaces)]
pub trait IRuntime: Send + Sync {
    fn hlc(&self) -> Option<&HLC>;
    fn zid(&self) -> ZenohId;
    fn whatami(&self) -> WhatAmI;
    fn next_id(&self) -> u32;
    fn is_closed(&self) -> bool;
    fn new_timestamp(&self) -> Option<uhlc::Timestamp>;
    fn get_locators(&self) -> Vec<Locator>;
    fn get_zids(&self, whatami: WhatAmI) -> Box<dyn Iterator<Item = ZenohId> + Send + Sync>;
    fn new_handler(&self, handler: Arc<dyn TransportEventHandler>);

    #[cfg(feature = "shared-memory")]
    #[zenoh_macros::unstable]
    fn get_shm_provider(&self) -> ShmProviderState;

    fn get_transports_unicast_peers(&self) -> Vec<TransportPeer>;
    fn get_transports_multicast_peers(&self) -> Vec<Vec<TransportPeer>>;

    fn new_primitives(
        &self,
        e_primitives: Arc<dyn EPrimitives + Send + Sync>,
    ) -> (usize, Arc<dyn Primitives>);

    fn matching_status_remote(
        &self,
        key_expr: &crate::key_expr::KeyExpr,
        destination: crate::sample::Locality,
        matching_type: crate::api::matching::MatchingStatusType,
        face_id: usize,
    ) -> crate::matching::MatchingStatus;

    fn get_config(&self) -> GenericConfig;
}

impl IConfig for Notifier<Config> {
    fn get(&self, key: &str) -> ZResult<String> {
        self.lock().get_json(key)
    }

    fn queries_default_timeout_ms(&self) -> u64 {
        let guard = self.lock();
        let config = &guard.0;
        unwrap_or_default!(config.queries_default_timeout())
    }

    fn insert_json5(&self, key: &str, value: &str) -> ZResult<()> {
        self.insert_json5(key, value)
    }

    fn to_json(&self) -> String {
        self.lock().to_string()
    }
}

impl IRuntime for RuntimeState {
    fn next_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    fn is_closed(&self) -> bool {
        self.task_controller.get_cancellation_token().is_cancelled()
    }

    fn new_timestamp(&self) -> Option<uhlc::Timestamp> {
        self.hlc.as_ref().map(|hlc| hlc.new_timestamp())
    }

    fn get_locators(&self) -> Vec<Locator> {
        self.locators.read().unwrap().clone()
    }

    fn hlc(&self) -> Option<&HLC> {
        self.hlc.as_ref().map(Arc::as_ref)
    }

    fn zid(&self) -> ZenohId {
        self.zid
    }

    fn whatami(&self) -> WhatAmI {
        self.whatami
    }

    fn get_zids(&self, whatami: WhatAmI) -> Box<dyn Iterator<Item = ZenohId> + Send + Sync> {
        Box::new(
            zenoh_runtime::ZRuntime::Application
                .block_in_place(self.manager().get_transports_unicast())
                .into_iter()
                .filter_map(move |s| {
                    s.get_whatami()
                        .ok()
                        .and_then(|what| (what == whatami).then_some(()))
                        .and_then(|_| s.get_zid().map(Into::into).ok())
                }),
        )
    }

    fn new_handler(&self, handler: Arc<dyn TransportEventHandler>) {
        zwrite!(self.transport_handlers).push(handler);
    }

    fn get_transports_unicast_peers(&self) -> Vec<TransportPeer> {
        zenoh_runtime::ZRuntime::Net
            .block_in_place(self.manager.get_transports_unicast())
            .into_iter()
            .filter_map(|t| t.get_peer().ok())
            .collect::<Vec<_>>()
    }

    fn get_transports_multicast_peers(&self) -> Vec<Vec<TransportPeer>> {
        zenoh_runtime::ZRuntime::Net
            .block_in_place(self.manager.get_transports_multicast())
            .into_iter()
            .filter_map(|t| t.get_peers().ok())
            .collect::<Vec<_>>()
    }

    fn matching_status_remote(
        &self,
        key_expr: &crate::key_expr::KeyExpr,
        destination: crate::sample::Locality,
        matching_type: crate::api::matching::MatchingStatusType,
        face_id: usize,
    ) -> crate::matching::MatchingStatus {
        use crate::matching::MatchingStatus;

        let ns_key_expr = self
            .namespace
            .as_ref()
            .map(|ns| (ns / key_expr.deref()).into());

        let router = self.router();
        let tables = zread!(router.tables.tables);

        let matches = match matching_type {
            crate::api::matching::MatchingStatusType::Subscribers => {
                crate::net::routing::dispatcher::pubsub::get_matching_subscriptions(
                    router.tables.hat_code.as_ref(),
                    &tables,
                    match &ns_key_expr {
                        Some(ns_ke) => ns_ke,
                        None => key_expr,
                    },
                )
            }
            crate::api::matching::MatchingStatusType::Queryables(complete) => {
                crate::net::routing::dispatcher::queries::get_matching_queryables(
                    router.tables.hat_code.as_ref(),
                    &tables,
                    match &ns_key_expr {
                        Some(ns_ke) => ns_ke,
                        None => key_expr,
                    },
                    complete,
                )
            }
        };

        drop(tables);
        let matching = match destination {
            crate::sample::Locality::Any => !matches.is_empty(),
            crate::sample::Locality::Remote => matches.values().any(|dir| dir.id != face_id),
            crate::sample::Locality::SessionLocal => matches.values().any(|dir| dir.id == face_id),
        };
        MatchingStatus { matching }
    }

    fn new_primitives(
        &self,
        e_primitives: Arc<dyn EPrimitives + Send + Sync>,
    ) -> (usize, Arc<dyn Primitives>) {
        match &self.namespace {
            Some(ns) => {
                let face = self
                    .router
                    .new_primitives(Arc::new(ENamespace::new(ns.clone(), e_primitives)));
                (face.state.id, Arc::new(Namespace::new(ns.clone(), face)))
            }
            None => {
                let face = self.router.new_primitives(e_primitives);
                (face.state.id, face)
            }
        }
    }

    fn get_config(&self) -> GenericConfig {
        GenericConfig::new(Arc::new(self.config.clone()))
    }

    #[cfg(feature = "shared-memory")]
    #[zenoh_macros::unstable]
    fn get_shm_provider(&self) -> ShmProviderState {
        use zenoh_transport::shm::ProviderInitState;

        match &self.manager.get_shm_context() {
            Some(ctx) => match ctx.shm_provider() {
                Some(provider) => match provider.try_get_provider() {
                    ProviderInitState::Initializing => ShmProviderState::Initializing,
                    ProviderInitState::Ready(provider) => ShmProviderState::Ready(provider),
                    ProviderInitState::Error => ShmProviderState::Error,
                },
                None => ShmProviderState::Disabled,
            },
            None => ShmProviderState::Disabled,
        }
    }
}

impl RuntimeState {
    #[inline(always)]
    fn manager(&self) -> &TransportManager {
        &self.manager
    }

    #[cfg(feature = "plugins")]
    #[inline(always)]
    fn plugins_manager(&self) -> MutexGuard<'_, PluginsManager> {
        zlock!(self.plugins_manager)
    }

    /// Spawns a task within runtime.
    /// Upon close runtime will block until this task completes
    fn spawn<F, T>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.task_controller
            .spawn_with_rt(zenoh_runtime::ZRuntime::Net, future)
    }

    /// Spawns a task within runtime.
    /// Upon runtime close the task will be automatically aborted.
    fn spawn_abortable<F, T>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.task_controller
            .spawn_abortable_with_rt(zenoh_runtime::ZRuntime::Net, future)
    }

    fn router(&self) -> Arc<Router> {
        self.router.clone()
    }

    fn config(&self) -> &Notifier<Config> {
        &self.config
    }

    fn get_cancellation_token(&self) -> CancellationToken {
        self.task_controller.get_cancellation_token()
    }

    fn start_conditions(&self) -> &Arc<StartConditions> {
        &self.start_conditions
    }

    async fn insert_pending_connection(&self, zid: ZenohIdProto) -> bool {
        self.pending_connections.lock().await.insert(zid)
    }

    async fn remove_pending_connection(&self, zid: &ZenohIdProto) -> bool {
        self.pending_connections.lock().await.remove(zid)
    }
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

        let router = Arc::new(Router::new(zid, whatami, hlc.clone(), &config)?);

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

        let namespace = config.namespace().clone();
        let config = Notifier::new(crate::config::Config(config));

        let runtime = Runtime {
            state: Arc::new(RuntimeState {
                zid: zid.into(),
                whatami,
                next_id: AtomicU32::new(1), // 0 is reserved for routing core
                router,
                config,
                manager: transport_manager,
                transport_handlers: std::sync::RwLock::new(vec![]),
                locators: std::sync::RwLock::new(vec![]),
                hlc,
                task_controller: TaskController::default(),
                #[cfg(feature = "plugins")]
                plugins_manager: Mutex::new(plugins_manager),
                start_conditions: Arc::new(StartConditions::default()),
                pending_connections: tokio::sync::Mutex::new(HashSet::new()),
                namespace,
            }),
        };
        *handler.runtime.write().unwrap() = Runtime::downgrade(&runtime);
        get_mut_unchecked(&mut runtime.state.router.clone()).init_link_state(runtime.clone())?;

        // Admin space
        if start_admin_space {
            AdminSpace::start(&runtime, LONG_VERSION.clone()).await;
        }

        // Start plugins
        #[cfg(feature = "plugins")]
        start_plugins(&runtime);

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

#[derive(Clone)]
pub struct DynamicRuntime(Arc<dyn IRuntime>);

impl Deref for DynamicRuntime {
    type Target = Arc<dyn IRuntime>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl StructVersion for DynamicRuntime {
    fn struct_version() -> &'static str {
        crate::GIT_VERSION
    }
    fn struct_features() -> &'static str {
        crate::FEATURES
    }
}

impl PluginStartArgs for DynamicRuntime {}

impl Runtime {
    #[inline(always)]
    pub(crate) fn manager(&self) -> &TransportManager {
        self.state.manager()
    }

    #[cfg(feature = "plugins")]
    #[inline(always)]
    pub fn plugins_manager(&self) -> MutexGuard<'_, PluginsManager> {
        self.state.plugins_manager()
    }

    #[inline]
    pub fn next_id(&self) -> u32 {
        self.state.next_id()
    }

    #[cfg(feature = "internal")]
    pub fn close(&self) -> CloseBuilder<Self> {
        CloseBuilder::new(self)
    }

    pub fn is_closed(&self) -> bool {
        self.state.is_closed()
    }

    #[allow(dead_code)]
    pub fn new_timestamp(&self) -> Option<uhlc::Timestamp> {
        self.state.new_timestamp()
    }

    pub fn get_locators(&self) -> Vec<Locator> {
        self.state.get_locators()
    }

    /// Spawns a task within runtime.
    /// Upon close runtime will block until this task completes
    pub(crate) fn spawn<F, T>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.state.spawn(future)
    }

    /// Spawns a task within runtime.
    /// Upon runtime close the task will be automatically aborted.
    pub(crate) fn spawn_abortable<F, T>(&self, future: F) -> JoinHandle<()>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.state.spawn_abortable(future)
    }

    pub(crate) fn router(&self) -> Arc<Router> {
        self.state.router()
    }

    pub fn config(&self) -> &Notifier<Config> {
        self.state.config()
    }

    #[allow(dead_code)]
    pub fn hlc(&self) -> Option<&HLC> {
        self.state.hlc()
    }

    pub fn zid(&self) -> ZenohId {
        self.state.zid()
    }

    pub fn whatami(&self) -> WhatAmI {
        self.state.whatami()
    }

    pub fn downgrade(this: &Runtime) -> WeakRuntime {
        WeakRuntime {
            state: Arc::downgrade(&this.state),
        }
    }

    pub fn get_cancellation_token(&self) -> CancellationToken {
        self.state.get_cancellation_token()
    }

    #[cfg(feature = "shared-memory")]
    #[zenoh_macros::unstable]
    #[allow(dead_code)]
    pub fn get_shm_provider(&self) -> ShmProviderState {
        self.state.get_shm_provider()
    }

    pub(crate) fn start_conditions(&self) -> &Arc<StartConditions> {
        self.state.start_conditions()
    }

    pub(crate) async fn insert_pending_connection(&self, zid: ZenohIdProto) -> bool {
        self.state.insert_pending_connection(zid).await
    }

    pub(crate) async fn remove_pending_connection(&self, zid: &ZenohIdProto) -> bool {
        self.state.remove_pending_connection(zid).await
    }
}

impl From<Runtime> for DynamicRuntime {
    fn from(value: Runtime) -> Self {
        DynamicRuntime(value.state)
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
                    endpoints: std::sync::RwLock::new(HashSet::new()),
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
    pub(super) endpoints: std::sync::RwLock<HashSet<EndPoint>>,
    pub(super) main_handler: Arc<DeMux>,
    pub(super) slave_handlers: Vec<Arc<dyn TransportPeerEventHandler>>,
}

impl TransportPeerEventHandler for RuntimeSession {
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
        Runtime::closed_link(self, link.dst.to_endpoint());
    }

    fn closed(&self) {
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
        Ok(Arc::new(RuntimeMulticastSession {
            main_handler: self
                .runtime
                .state
                .router
                .new_peer_multicast(self.transport.clone(), peer)?,
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
        // This should be resolved by identifying corresponding task, and placing
        // cancellation token manually inside it.
        let mut tables = self.router.tables.tables.write().unwrap();
        tables.root_res.close();
        tables.faces.clear();
    }
}

impl Closeable for Runtime {
    type TClosee = Arc<RuntimeState>;

    fn get_closee(&self) -> Self::TClosee {
        self.state.clone()
    }
}

#[derive(Clone)]
pub(crate) struct GenericRuntime {
    dynamic_runtime: DynamicRuntime,
    static_runtime: Option<Runtime>,
}

impl Deref for GenericRuntime {
    type Target = DynamicRuntime;

    fn deref(&self) -> &Self::Target {
        &self.dynamic_runtime
    }
}

impl GenericRuntime {
    pub(crate) fn static_runtime(&self) -> Option<&Runtime> {
        self.static_runtime.as_ref()
    }
}

impl From<Runtime> for GenericRuntime {
    fn from(value: Runtime) -> Self {
        let static_runtime = value.clone();
        GenericRuntime {
            dynamic_runtime: value.into(),
            static_runtime: Some(static_runtime),
        }
    }
}

impl From<DynamicRuntime> for GenericRuntime {
    fn from(value: DynamicRuntime) -> Self {
        GenericRuntime {
            dynamic_runtime: value,
            static_runtime: None,
        }
    }
}
