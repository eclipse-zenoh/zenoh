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
use super::info::*;
use super::publication::*;
use super::query::*;
use super::queryable::*;
use super::subscriber::*;
use super::*;
use crate::config::{Config, Notifier};
use crate::net::protocol::core::EMPTY_EXPR_ID;
use async_std::sync::Arc;
use async_std::task;
use flume::{bounded, Sender};
use log::{error, trace, warn};
use net::protocol::{
    core::{
        key_expr, queryable, AtomicZInt, Channel, CongestionControl, ExprId, KeyExpr,
        QueryConsolidation, QueryTarget, QueryableInfo, SubInfo, ZInt,
    },
    io::ZBuf,
    message::{DataInfo, RoutingContext},
};
use net::routing::face::Face;
use net::runtime::Runtime;
use net::transport::Primitives;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;
use std::time::Duration;
use uhlc::HLC;
use zenoh_util::core::Result as ZResult;
use zenoh_util::sync::zpinbox;

zconfigurable! {
    pub(crate) static ref API_DATA_RECEPTION_CHANNEL_SIZE: usize = 256;
    pub(crate) static ref API_QUERY_RECEPTION_CHANNEL_SIZE: usize = 256;
    pub(crate) static ref API_REPLY_EMISSION_CHANNEL_SIZE: usize = 256;
    pub(crate) static ref API_REPLY_RECEPTION_CHANNEL_SIZE: usize = 256;
    pub(crate) static ref API_OPEN_SESSION_DELAY: u64 = 500;
}

pub(crate) struct SessionState {
    pub(crate) primitives: Option<Arc<Face>>, // @TODO replace with MaybeUninit ??
    pub(crate) expr_id_counter: AtomicUsize,  // @TODO: manage rollover and uniqueness
    pub(crate) qid_counter: AtomicZInt,
    pub(crate) decl_id_counter: AtomicUsize,
    pub(crate) local_resources: HashMap<ExprId, Resource>,
    pub(crate) remote_resources: HashMap<ExprId, Resource>,
    pub(crate) publications: Vec<String>,
    pub(crate) subscribers: HashMap<Id, Arc<SubscriberState>>,
    pub(crate) local_subscribers: HashMap<Id, Arc<SubscriberState>>,
    pub(crate) queryables: HashMap<Id, Arc<QueryableState>>,
    pub(crate) queries: HashMap<ZInt, QueryState>,
    pub(crate) local_routing: bool,
    pub(crate) join_subscriptions: Vec<String>,
    pub(crate) join_publications: Vec<String>,
}

impl SessionState {
    pub(crate) fn new(
        local_routing: bool,
        join_subscriptions: Vec<String>,
        join_publications: Vec<String>,
    ) -> SessionState {
        SessionState {
            primitives: None,
            expr_id_counter: AtomicUsize::new(1), // Note: start at 1 because 0 is reserved for NO_RESOURCE
            qid_counter: AtomicZInt::new(0),
            decl_id_counter: AtomicUsize::new(0),
            local_resources: HashMap::new(),
            remote_resources: HashMap::new(),
            publications: Vec::new(),
            subscribers: HashMap::new(),
            local_subscribers: HashMap::new(),
            queryables: HashMap::new(),
            queries: HashMap::new(),
            local_routing,
            join_subscriptions,
            join_publications,
        }
    }
}

impl SessionState {
    #[inline]
    fn get_local_res(&self, id: &ExprId) -> Option<&Resource> {
        self.local_resources.get(id)
    }

    #[inline]
    fn get_remote_res(&self, id: &ExprId) -> Option<&Resource> {
        match self.remote_resources.get(id) {
            None => self.local_resources.get(id),
            res => res,
        }
    }

    #[inline]
    fn get_res(&self, id: &ExprId, local: bool) -> Option<&Resource> {
        if local {
            self.get_local_res(id)
        } else {
            self.get_remote_res(id)
        }
    }

    #[inline]
    fn localid_to_expr(&self, id: &ExprId) -> ZResult<String> {
        match self.local_resources.get(id) {
            Some(res) => Ok(res.name.clone()),
            None => bail!("{}", id),
        }
    }

    #[inline]
    fn id_to_expr(&self, id: &ExprId) -> ZResult<String> {
        match self.remote_resources.get(id) {
            Some(res) => Ok(res.name.clone()),
            None => self.localid_to_expr(id),
        }
    }

    pub fn remotekey_to_expr(&self, key_expr: &KeyExpr) -> ZResult<String> {
        if key_expr.scope == EMPTY_EXPR_ID {
            Ok(key_expr.suffix.to_string())
        } else if key_expr.suffix.is_empty() {
            self.id_to_expr(&key_expr.scope)
        } else {
            Ok(self.id_to_expr(&key_expr.scope)? + key_expr.suffix.as_ref())
        }
    }

    pub fn localkey_to_expr(&self, key_expr: &KeyExpr) -> ZResult<String> {
        if key_expr.scope == EMPTY_EXPR_ID {
            Ok(key_expr.suffix.to_string())
        } else if key_expr.suffix.is_empty() {
            self.localid_to_expr(&key_expr.scope)
        } else {
            Ok(self.localid_to_expr(&key_expr.scope)? + key_expr.suffix.as_ref())
        }
    }

    pub fn key_expr_to_expr(&self, key_expr: &KeyExpr, local: bool) -> ZResult<String> {
        if local {
            self.localkey_to_expr(key_expr)
        } else {
            self.remotekey_to_expr(key_expr)
        }
    }
}

impl fmt::Debug for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "SessionState{{ subscribers: {} }}",
            self.subscribers.len()
        )
    }
}

pub(crate) struct Resource {
    pub(crate) name: String,
    pub(crate) subscribers: Vec<Arc<SubscriberState>>,
    pub(crate) local_subscribers: Vec<Arc<SubscriberState>>,
}

impl Resource {
    pub(crate) fn new(name: String) -> Self {
        Resource {
            name,
            subscribers: vec![],
            local_subscribers: vec![],
        }
    }
}

/// A zenoh session.
///
pub struct Session {
    pub(crate) runtime: Runtime,
    pub(crate) state: Arc<RwLock<SessionState>>,
    pub(crate) alive: bool,
}

impl Session {
    pub(crate) fn clone(&self) -> Self {
        Session {
            runtime: self.runtime.clone(),
            state: self.state.clone(),
            alive: false,
        }
    }

    pub(super) fn new<C: std::convert::TryInto<Config> + Send + 'static>(
        config: C,
    ) -> impl ZFuture<Output = ZResult<Session>>
    where
        <C as std::convert::TryInto<Config>>::Error: std::fmt::Debug,
    {
        debug!("Zenoh Rust API {}", GIT_VERSION);
        zpinbox(async move {
            let config: Config = match config.try_into() {
                Ok(c) => c,
                Err(e) => {
                    bail!("invalid configuration {:?}", &e)
                }
            };
            debug!("Config: {:?}", &config);
            let local_routing = config.local_routing().unwrap_or(true);
            let join_subscriptions = config.join_on_startup().subscriptions().clone();
            let join_publications = config.join_on_startup().publications().clone();
            match Runtime::new(config).await {
                Ok(runtime) => {
                    let session = Self::init(
                        runtime,
                        local_routing,
                        join_subscriptions,
                        join_publications,
                    )
                    .await;
                    // Workaround for the declare_and_shoot problem
                    task::sleep(Duration::from_millis(*API_OPEN_SESSION_DELAY)).await;
                    Ok(session)
                }
                Err(err) => Err(err),
            }
        })
    }

    /// Consumes and leaks the given `Session`, returning a `'static` mutable
    /// reference to it. The given `Session` will live  for the remainder of
    /// the program's life. Dropping the returned reference will cause a memory
    /// leak.
    ///
    /// This is useful to move entities (like [`Subscriber`](Subscriber)) which
    /// lifetimes are bound to the session lifetime in several threads or tasks.
    ///
    /// Note: the given zenoh `Session` cannot be closed any more. At process
    /// termination the zenoh session will terminate abruptly.
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use futures::prelude::*;
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap().leak();
    /// let mut subscriber = session.subscribe("/key/expression").await.unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Some(sample) = subscriber.receiver().next().await {
    ///         println!("Received : {:?}", sample);
    ///     }
    /// }).await;
    /// # })
    /// ```
    pub fn leak(self) -> &'static mut Self {
        Box::leak(Box::new(self))
    }

    /// Returns the identifier for this session.
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn id(&self) -> impl ZFuture<Output = String> {
        zready(self.runtime.get_zid_str())
    }

    pub fn hlc(&self) -> Option<&HLC> {
        self.runtime.hlc.as_ref().map(Arc::as_ref)
    }

    /// Initialize a Session with an existing Runtime.
    /// This operation is used by the plugins to share the same Runtime than the router.
    #[doc(hidden)]
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn init(
        runtime: Runtime,
        local_routing: bool,
        join_subscriptions: Vec<String>,
        join_publications: Vec<String>,
    ) -> impl ZFuture<Output = Session> {
        let router = runtime.router.clone();
        let state = Arc::new(RwLock::new(SessionState::new(
            local_routing,
            join_subscriptions,
            join_publications,
        )));
        let session = Session {
            runtime,
            state: state.clone(),
            alive: true,
        };
        let primitives = Some(router.new_primitives(Arc::new(session.clone())));
        zwrite!(state).primitives = primitives;
        zready(session)
    }

    fn close_alive(self) -> impl ZFuture<Output = ZResult<()>> {
        zpinbox(async move {
            trace!("close()");
            self.runtime.close().await?;

            let primitives = zwrite!(self.state).primitives.as_ref().unwrap().clone();
            primitives.send_close();

            Ok(())
        })
    }

    /// Close the zenoh [`Session`](Session).
    ///
    /// Sessions are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the Session asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// session.close().await.unwrap();
    /// # })
    /// ```
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn close(mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.alive = false;
        self.close_alive()
    }

    /// Get the current configuration of the zenoh [`Session`](Session).
    ///
    /// The returned configuration [`Notifier`] can be used to read the current
    /// zenoh configuration through the [`get`](Notifier::get) function or
    /// modify the zenoh configuration through the [`insert`](Notifier::insert),
    /// [`insert_json`](Notifier::insert_json) or [`insert_json5`](Notifier::insert_json5)
    /// funtion.
    ///
    /// # Examples
    /// ### Read current zenoh configuration
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let peers = session.config().await.get("peers").unwrap();
    /// # })
    /// ```
    ///
    /// ### Modify current zenoh configuration
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let _ = session.config().await.insert_json5("peers", r#"["tcp/127.0.0.1/7447"]"#);
    /// # })
    /// ```
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn config(&self) -> impl ZFuture<Output = &Notifier<Config>> {
        zready(&self.runtime.config)
    }

    /// Get informations about the zenoh [`Session`](Session).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let info = session.info();
    /// # })
    /// ```
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn info(&self) -> impl ZFuture<Output = InfoProperties> {
        trace!("info()");
        let sessions = self.runtime.manager().get_transports();
        let peer_pids = sessions
            .iter()
            .filter(|s| {
                s.get_whatami()
                    .ok()
                    .map(|what| what == WhatAmI::Peer)
                    .unwrap_or(false)
            })
            .filter_map(|s| {
                s.get_zid()
                    .ok()
                    .map(|pid| hex::encode_upper(pid.as_slice()))
            })
            .collect::<Vec<String>>();
        let mut router_pids = vec![];
        if self.runtime.whatami == WhatAmI::Router {
            router_pids.push(hex::encode_upper(self.runtime.pid.as_slice()));
        }
        router_pids.extend(
            sessions
                .iter()
                .filter(|s| {
                    s.get_whatami()
                        .ok()
                        .map(|what| what == WhatAmI::Router)
                        .unwrap_or(false)
                })
                .filter_map(|s| {
                    s.get_zid()
                        .ok()
                        .map(|pid| hex::encode_upper(pid.as_slice()))
                })
                .collect::<Vec<String>>(),
        );

        let mut info = InfoProperties::default();
        info.insert(ZN_INFO_PEER_PID_KEY, peer_pids.join(","));
        info.insert(ZN_INFO_ROUTER_PID_KEY, router_pids.join(","));
        info.insert(
            ZN_INFO_PID_KEY,
            hex::encode_upper(self.runtime.pid.as_slice()),
        );
        zready(info)
    }

    /// Associate a numerical Id with the given key expression.
    ///
    /// This numerical Id will be used on the network to save bandwidth and
    /// ease the retrieval of the concerned key expression in the routing tables.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression to map to a numerical Id
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let expr_id = session.declare_expr("/key/expression").await.unwrap();
    /// # })
    /// ```
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn declare_expr<'a, IntoKeyExpr>(
        &self,
        key_expr: IntoKeyExpr,
    ) -> impl ZFuture<Output = ZResult<ExprId>>
    where
        IntoKeyExpr: Into<KeyExpr<'a>>,
    {
        let key_expr = key_expr.into();
        trace!("declare_expr({:?})", key_expr);
        let mut state = zwrite!(self.state);

        zready(state.localkey_to_expr(&key_expr).map(|expr| {
            match state
                .local_resources
                .iter()
                .find(|(_expr_id, res)| res.name == expr)
            {
                Some((expr_id, _res)) => *expr_id,
                None => {
                    let expr_id = state.expr_id_counter.fetch_add(1, Ordering::SeqCst) as ZInt;
                    let mut res = Resource::new(expr.clone());
                    for sub in state.subscribers.values() {
                        if key_expr::matches(&expr, &sub.key_expr_str) {
                            res.subscribers.push(sub.clone());
                        }
                    }

                    state.local_resources.insert(expr_id, res);

                    let primitives = state.primitives.as_ref().unwrap().clone();
                    drop(state);
                    primitives.decl_resource(expr_id, &key_expr);

                    expr_id
                }
            }
        }))
    }

    /// Undeclare the *numerical Id/resource key* association previously declared
    /// with [`declare_expr`](Session::declare_expr).
    ///
    /// # Arguments
    ///
    /// * `expr_id` - The numerical Id to unmap
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let expr_id = session.declare_expr("/key/expression").await.unwrap();
    /// session.undeclare_expr(expr_id).await;
    /// # })
    /// ```
    #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
    pub fn undeclare_expr(&self, expr_id: ExprId) -> impl ZFuture<Output = ZResult<()>> {
        trace!("undeclare_expr({:?})", expr_id);
        let mut state = zwrite!(self.state);
        state.local_resources.remove(&expr_id);

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.forget_resource(expr_id);

        zready(Ok(()))
    }

    /// Declare a publication for the given key expression.
    ///
    /// Puts that match the given key expression will only be sent on the network
    /// if matching subscribers exist in the system.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression to publish
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// session.declare_publication("/key/expression").await.unwrap();
    /// session.put("/key/expression", "value").await.unwrap();
    /// # })
    /// ```
    pub fn declare_publication<'a, IntoKeyExpr>(
        &self,
        key_expr: IntoKeyExpr,
    ) -> impl ZFuture<Output = ZResult<()>>
    where
        IntoKeyExpr: Into<KeyExpr<'a>>,
    {
        let key_expr = key_expr.into();
        log::trace!("declare_publication({:?})", key_expr);
        zready({
            let mut state = zwrite!(self.state);
            state.localkey_to_expr(&key_expr).map(|key_expr_str| {
                if !state.publications.iter().any(|p| *p == key_expr_str) {
                    let declared_pub = if let Some(join_pub) = state
                        .join_publications
                        .iter()
                        .find(|s| key_expr::include(s, &key_expr_str))
                    {
                        let joined_pub = state
                            .publications
                            .iter()
                            .any(|p| key_expr::include(join_pub, p));
                        (!joined_pub).then(|| join_pub.clone().into())
                    } else {
                        Some(key_expr.clone())
                    };
                    state.publications.push(key_expr_str);

                    if let Some(res) = declared_pub {
                        let primitives = state.primitives.as_ref().unwrap().clone();
                        drop(state);
                        primitives.decl_publisher(&res, None);
                    }
                }
            })
        })
    }

    /// Undeclare a publication previously declared
    /// with [`declare_publication`](Session::declare_publication).
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression of the publication to undeclarte
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// session.declare_publication("/key/expression").await.unwrap();
    /// session.put("/key/expression", "value").await.unwrap();
    /// session.undeclare_publication("/key/expression").await.unwrap();
    /// # })
    /// ```
    pub fn undeclare_publication<'a, IntoKeyExpr>(
        &self,
        key_expr: IntoKeyExpr,
    ) -> impl ZFuture<Output = ZResult<()>>
    where
        IntoKeyExpr: Into<KeyExpr<'a>>,
    {
        let key_expr = key_expr.into();
        let mut state = zwrite!(self.state);
        zready(state.localkey_to_expr(&key_expr).and_then(|key_expr_str| {
            if let Some(idx) = state.publications.iter().position(|p| *p == key_expr_str) {
                trace!("undeclare_publication({:?})", key_expr_str);
                state.publications.remove(idx);
                match state
                    .join_publications
                    .iter()
                    .find(|s| key_expr::include(s, &key_expr_str))
                {
                    Some(join_pub) => {
                        let joined_pub = state
                            .publications
                            .iter()
                            .any(|p| key_expr::include(join_pub, p));
                        if !joined_pub {
                            let primitives = state.primitives.as_ref().unwrap().clone();
                            let key_expr = join_pub.clone().into();
                            drop(state);
                            primitives.forget_publisher(&key_expr, None);
                        }
                    }
                    None => {
                        let primitives = state.primitives.as_ref().unwrap().clone();
                        drop(state);
                        primitives.forget_publisher(&key_expr, None);
                    }
                };
                Ok(())
            } else {
                bail!("Unable to find publication")
            }
        }))
    }

    pub(crate) fn declare_any_subscriber(
        &self,
        key_expr: &KeyExpr,
        invoker: SubscriberInvoker,
        info: &SubInfo,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let key_expr_str = state.localkey_to_expr(key_expr)?;
        let sub_state = Arc::new(SubscriberState {
            id,
            key_expr: key_expr.to_owned(),
            key_expr_str,
            invoker,
        });
        let declared_sub = match state
            .join_subscriptions
            .iter()
            .find(|s| key_expr::include(s, &sub_state.key_expr_str))
        {
            Some(join_sub) => {
                let joined_sub = state.subscribers.values().any(|s| {
                    key_expr::include(join_sub, &state.localkey_to_expr(&s.key_expr).unwrap())
                });
                (!joined_sub).then(|| join_sub.clone().into())
            }
            None => {
                let twin_sub = state.subscribers.values().any(|s| {
                    state.localkey_to_expr(&s.key_expr).unwrap()
                        == state.localkey_to_expr(&sub_state.key_expr).unwrap()
                });
                (!twin_sub).then(|| sub_state.key_expr.clone())
            }
        };

        state.subscribers.insert(sub_state.id, sub_state.clone());
        for res in state.local_resources.values_mut() {
            if key_expr::matches(&sub_state.key_expr_str, &res.name) {
                res.subscribers.push(sub_state.clone());
            }
        }
        for res in state.remote_resources.values_mut() {
            if key_expr::matches(&sub_state.key_expr_str, &res.name) {
                res.subscribers.push(sub_state.clone());
            }
        }

        if let Some(key_expr) = declared_sub {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);

            // If key_expr is a pure Expr, remap it to optimal Rid or RidWithSuffix
            let key_expr = if key_expr.scope == EMPTY_EXPR_ID {
                match key_expr.suffix.as_ref().find('*') {
                    Some(pos) => {
                        let scope = self.declare_expr(&key_expr.suffix.as_ref()[..pos]).wait()?;
                        KeyExpr {
                            scope,
                            suffix: key_expr.suffix.as_ref()[pos..].to_string().into(),
                        }
                    }
                    None => {
                        let scope = self.declare_expr(key_expr.suffix.as_ref()).wait()?;
                        KeyExpr {
                            scope,
                            suffix: "".into(),
                        }
                    }
                }
            } else {
                key_expr
            };

            primitives.decl_subscriber(&key_expr, info, None);
        }

        Ok(sub_state)
    }

    pub(crate) fn declare_any_local_subscriber(
        &self,
        key_expr: &KeyExpr,
        invoker: SubscriberInvoker,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let key_expr_str = state.localkey_to_expr(key_expr)?;
        let sub_state = Arc::new(SubscriberState {
            id,
            key_expr: key_expr.to_owned(),
            key_expr_str,
            invoker,
        });
        state
            .local_subscribers
            .insert(sub_state.id, sub_state.clone());
        for res in state.local_resources.values_mut() {
            if key_expr::matches(&sub_state.key_expr_str, &res.name) {
                res.local_subscribers.push(sub_state.clone());
            }
        }
        for res in state.remote_resources.values_mut() {
            if key_expr::matches(&sub_state.key_expr_str, &res.name) {
                res.local_subscribers.push(sub_state.clone());
            }
        }

        Ok(sub_state)
    }

    /// Create a [`Subscriber`](Subscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The resourkey expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use futures::prelude::*;
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let mut subscriber = session.subscribe("/key/expression").await.unwrap();
    /// while let Some(sample) = subscriber.receiver().next().await {
    ///     println!("Received : {:?}", sample);
    /// }
    /// # })
    /// ```
    pub fn subscribe<'a, 'b, IntoKeyExpr>(
        &'a self,
        key_expr: IntoKeyExpr,
    ) -> SubscriberBuilder<'a, 'b>
    where
        IntoKeyExpr: Into<KeyExpr<'b>>,
    {
        SubscriberBuilder {
            session: self,
            key_expr: key_expr.into(),
            reliability: Reliability::default(),
            mode: SubMode::default(),
            period: None,
            local: false,
        }
    }

    pub(crate) fn unsubscribe(&self, sid: usize) -> impl ZFuture<Output = ZResult<()>> {
        let mut state = zwrite!(self.state);
        zready(if let Some(sub_state) = state.subscribers.remove(&sid) {
            trace!("unsubscribe({:?})", sub_state);
            for res in state.local_resources.values_mut() {
                res.subscribers.retain(|sub| sub.id != sub_state.id);
            }
            for res in state.remote_resources.values_mut() {
                res.subscribers.retain(|sub| sub.id != sub_state.id);
            }

            // Note: there might be several Subscribers on the same KeyExpr.
            // Before calling forget_subscriber(key_expr), check if this was the last one.
            state.localkey_to_expr(&sub_state.key_expr).map(|key_expr| {
                match state
                    .join_subscriptions
                    .iter()
                    .find(|s| key_expr::include(s, &key_expr))
                {
                    Some(join_sub) => {
                        let joined_sub = state.subscribers.values().any(|s| {
                            key_expr::include(
                                join_sub,
                                &state.localkey_to_expr(&s.key_expr).unwrap(),
                            )
                        });
                        if !joined_sub {
                            let primitives = state.primitives.as_ref().unwrap().clone();
                            let key_expr = join_sub.clone().into();
                            drop(state);
                            primitives.forget_subscriber(&key_expr, None);
                        }
                    }
                    None => {
                        let twin_sub = state.subscribers.values().any(|s| {
                            state.localkey_to_expr(&s.key_expr).unwrap()
                                == state.localkey_to_expr(&sub_state.key_expr).unwrap()
                        });
                        if !twin_sub {
                            let primitives = state.primitives.as_ref().unwrap().clone();
                            drop(state);
                            primitives.forget_subscriber(&sub_state.key_expr, None);
                        }
                    }
                };
            })
        } else if let Some(sub_state) = state.local_subscribers.remove(&sid) {
            trace!("unsubscribe({:?})", sub_state);
            for res in state.local_resources.values_mut() {
                res.local_subscribers.retain(|sub| sub.id != sub_state.id);
            }
            for res in state.remote_resources.values_mut() {
                res.local_subscribers.retain(|sub| sub.id != sub_state.id);
            }
            Ok(())
        } else {
            Err(zerror!("Unable to find subscriber").into())
        })
    }

    pub(crate) fn twin_qabl(state: &SessionState, key: &KeyExpr, kind: ZInt) -> bool {
        state.queryables.values().any(|q| {
            q.kind == kind
                && state.localkey_to_expr(&q.key_expr).unwrap()
                    == state.localkey_to_expr(key).unwrap()
        })
    }

    #[cfg(not(feature = "complete_n"))]
    pub(crate) fn complete_twin_qabl(state: &SessionState, key: &KeyExpr, kind: ZInt) -> bool {
        state.queryables.values().any(|q| {
            q.complete
                && q.kind == kind
                && state.localkey_to_expr(&q.key_expr).unwrap()
                    == state.localkey_to_expr(key).unwrap()
        })
    }

    #[cfg(feature = "complete_n")]
    pub(crate) fn complete_twin_qabls(state: &SessionState, key: &KeyExpr, kind: ZInt) -> ZInt {
        state
            .queryables
            .values()
            .filter(|q| {
                q.complete
                    && q.kind == kind
                    && state.localkey_to_expr(&q.key_expr).unwrap()
                        == state.localkey_to_expr(key).unwrap()
            })
            .count() as ZInt
    }

    /// Create a [`Queryable`](Queryable) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching the queries the
    /// [`Queryable`](Queryable) will reply to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use futures::prelude::*;
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let mut queryable = session.queryable("/key/expression").await.unwrap();
    /// while let Some(query) = queryable.receiver().next().await {
    ///     query.reply_async(Sample::new(
    ///         "/key/expression".to_string(),
    ///         "value",
    ///     )).await;
    /// }
    /// # })
    /// ```
    pub fn queryable<'a, 'b, IntoKeyExpr>(
        &'a self,
        key_expr: IntoKeyExpr,
    ) -> QueryableBuilder<'a, 'b>
    where
        IntoKeyExpr: Into<KeyExpr<'b>>,
    {
        QueryableBuilder {
            session: self,
            key_expr: key_expr.into(),
            kind: EVAL,
            complete: true,
        }
    }

    pub(crate) fn close_queryable(&self, qid: usize) -> impl ZFuture<Output = ZResult<()>> {
        let mut state = zwrite!(self.state);
        zready(if let Some(qable_state) = state.queryables.remove(&qid) {
            trace!("close_queryable({:?})", qable_state);
            if Session::twin_qabl(&state, &qable_state.key_expr, qable_state.kind) {
                // There still exist Queryables on the same KeyExpr.
                if qable_state.complete {
                    #[cfg(feature = "complete_n")]
                    {
                        let complete = Session::complete_twin_qabls(
                            &state,
                            &qable_state.key_expr,
                            qable_state.kind,
                        );
                        let primitives = state.primitives.as_ref().unwrap();
                        let qabl_info = QueryableInfo {
                            complete,
                            distance: 0,
                        };
                        primitives.decl_queryable(
                            &qable_state.key_expr,
                            qable_state.kind,
                            &qabl_info,
                            None,
                        );
                    }
                    #[cfg(not(feature = "complete_n"))]
                    {
                        if !Session::complete_twin_qabl(
                            &state,
                            &qable_state.key_expr,
                            qable_state.kind,
                        ) {
                            let primitives = state.primitives.as_ref().unwrap();
                            let qabl_info = QueryableInfo {
                                complete: 0,
                                distance: 0,
                            };
                            primitives.decl_queryable(
                                &qable_state.key_expr,
                                qable_state.kind,
                                &qabl_info,
                                None,
                            );
                        }
                    }
                }
            } else {
                // There are no more Queryables on the same KeyExpr.
                let primitives = state.primitives.as_ref().unwrap();
                primitives.forget_queryable(&qable_state.key_expr, qable_state.kind, None);
            }
            Ok(())
        } else {
            Err(zerror!("Unable to find queryable").into())
        })
    }

    /// Write data.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching resources to write
    /// * `payload` - The value to write
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// session.put("/key/expression", "value")
    ///        .encoding(Encoding::TEXT_PLAIN).await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn put<'a, IntoKeyExpr, IntoValue>(
        &'a self,
        key_expr: IntoKeyExpr,
        value: IntoValue,
    ) -> Writer<'a>
    where
        IntoKeyExpr: Into<KeyExpr<'a>>,
        IntoValue: Into<Value>,
    {
        Writer {
            session: self,
            key_expr: key_expr.into(),
            value: Some(value.into()),
            kind: None,
            congestion_control: CongestionControl::default(),
            priority: Priority::default(),
        }
    }

    /// Delete data.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching resources to delete
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// session.delete("/key/expression");
    /// # })
    /// ```
    #[inline]
    pub fn delete<'a, IntoKeyExpr>(&'a self, key_expr: IntoKeyExpr) -> Writer<'a>
    where
        IntoKeyExpr: Into<KeyExpr<'a>>,
    {
        Writer {
            session: self,
            key_expr: key_expr.into(),
            value: Some(Value::empty()),
            kind: Some(data_kind::DELETE),
            congestion_control: CongestionControl::default(),
            priority: Priority::default(),
        }
    }

    #[inline]
    fn invoke_subscriber(
        invoker: &SubscriberInvoker,
        key_expr: String,
        payload: ZBuf,
        data_info: Option<DataInfo>,
    ) {
        match invoker {
            SubscriberInvoker::Handler(handler) => {
                let handler = &mut *zwrite!(handler);
                handler(Sample::with_info(key_expr.into(), payload, data_info));
            }
            SubscriberInvoker::Sender(sender) => {
                if let Err(e) = sender.send(Sample::with_info(key_expr.into(), payload, data_info))
                {
                    error!("SubscriberInvoker error: {}", e);
                }
            }
        }
    }

    pub(crate) fn handle_data(
        &self,
        local: bool,
        key_expr: &KeyExpr,
        info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        let state = zread!(self.state);
        if key_expr.suffix.is_empty() {
            match state.get_res(&key_expr.scope, local) {
                Some(res) => {
                    if !local && res.subscribers.len() == 1 {
                        let sub = res.subscribers.get(0).unwrap();
                        Session::invoke_subscriber(&sub.invoker, res.name.clone(), payload, info);
                    } else {
                        if !local || state.local_routing {
                            for sub in &res.subscribers {
                                Session::invoke_subscriber(
                                    &sub.invoker,
                                    res.name.clone(),
                                    payload.clone(),
                                    info.clone(),
                                );
                            }
                        }
                        if local {
                            for sub in &res.local_subscribers {
                                Session::invoke_subscriber(
                                    &sub.invoker,
                                    res.name.clone(),
                                    payload.clone(),
                                    info.clone(),
                                );
                            }
                        }
                    }
                }
                None => {
                    error!("Received Data for unkown expr_id: {}", key_expr.scope);
                }
            }
        } else {
            match state.key_expr_to_expr(key_expr, local) {
                Ok(key_expr) => {
                    if !local || state.local_routing {
                        for sub in state.subscribers.values() {
                            if key_expr::matches(&sub.key_expr_str, &key_expr) {
                                Session::invoke_subscriber(
                                    &sub.invoker,
                                    key_expr.clone(),
                                    payload.clone(),
                                    info.clone(),
                                );
                            }
                        }
                    }
                    if local {
                        for sub in state.local_subscribers.values() {
                            if key_expr::matches(&sub.key_expr_str, &key_expr) {
                                Session::invoke_subscriber(
                                    &sub.invoker,
                                    key_expr.clone(),
                                    payload.clone(),
                                    info.clone(),
                                );
                            }
                        }
                    }
                }
                Err(err) => {
                    error!("Received Data for unkown key_expr: {}", err);
                }
            }
        }
    }

    pub(crate) fn pull(&self, key_expr: &KeyExpr) -> impl ZFuture<Output = ZResult<()>> {
        trace!("pull({:?})", key_expr);
        let state = zread!(self.state);
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.send_pull(true, key_expr, 0, &None);
        zready(Ok(()))
    }

    /// Query data from the matching queryables in the system.
    /// The result of the query is provided as a [`ReplyReceiver`](ReplyReceiver).
    ///
    /// # Arguments
    ///
    /// * `selector` - The selection of resources to query
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use futures::prelude::*;
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(config::peer()).await.unwrap();
    /// let mut replies = session.get("/key/expression").await.unwrap();
    /// while let Some(reply) = replies.next().await {
    ///     println!(">> Received {:?}", reply.data);
    /// }
    /// # })
    /// ```
    pub fn get<'a, 'b, IntoSelector>(&'a self, selector: IntoSelector) -> Getter<'a, 'b>
    where
        IntoSelector: Into<Selector<'b>>,
    {
        let selector = selector.into();
        let consolidation = if selector.has_time_range() {
            Some(QueryConsolidation::none())
        } else {
            Some(QueryConsolidation::default())
        };
        Getter {
            session: self,
            selector,
            target: Some(QueryTarget::default()),
            consolidation,
        }
    }

    pub(crate) fn handle_query(
        &self,
        local: bool,
        key_expr: &KeyExpr,
        value_selector: &str,
        qid: ZInt,
        target: QueryTarget,
        _consolidation: QueryConsolidation,
    ) {
        let (primitives, key_expr, kinds_and_senders) = {
            let state = zread!(self.state);
            match state.key_expr_to_expr(key_expr, local) {
                Ok(key_expr) => {
                    let kinds_and_senders = state
                        .queryables
                        .values()
                        .filter(
                            |queryable| match state.localkey_to_expr(&queryable.key_expr) {
                                Ok(qablname) => {
                                    key_expr::matches(&qablname, &key_expr)
                                        && ((queryable.kind == queryable::ALL_KINDS
                                            || target.kind == queryable::ALL_KINDS)
                                            || (queryable.kind & target.kind != 0))
                                }
                                Err(err) => {
                                    error!(
                                        "{}. Internal error (queryable key_expr to key_expr failed).",
                                        err
                                    );
                                    false
                                }
                            },
                        )
                        .map(|qable| (qable.kind, qable.sender.clone()))
                        .collect::<Vec<(ZInt, Sender<Query>)>>();
                    (
                        state.primitives.as_ref().unwrap().clone(),
                        key_expr,
                        kinds_and_senders,
                    )
                }
                Err(err) => {
                    error!("Received Query for unkown key_expr: {}", err);
                    return;
                }
            }
        };

        let value_selector = value_selector.to_string();
        let (rep_sender, rep_receiver) = bounded(*API_REPLY_EMISSION_CHANNEL_SIZE);

        let pid = self.runtime.pid; // @TODO build/use prebuilt specific pid

        for (kind, req_sender) in kinds_and_senders {
            let _ = req_sender.send(Query {
                key_selector: key_expr.clone().into(),
                value_selector: value_selector.clone(),
                replies_sender: RepliesSender {
                    kind,
                    sender: rep_sender.clone(),
                },
            });
        }
        drop(rep_sender); // all senders need to be dropped for the channel to close

        // router is not re-entrant

        if local {
            let this = self.clone();
            task::spawn(async move {
                while let Some((replier_kind, sample)) = rep_receiver.stream().next().await {
                    let (key_expr, payload, data_info) = sample.split();
                    this.send_reply_data(
                        qid,
                        replier_kind,
                        pid,
                        key_expr,
                        Some(data_info),
                        payload,
                    );
                }
                this.send_reply_final(qid);
            });
        } else {
            task::spawn(async move {
                while let Some((replier_kind, sample)) = rep_receiver.stream().next().await {
                    let (key_expr, payload, data_info) = sample.split();
                    primitives.send_reply_data(
                        qid,
                        replier_kind,
                        pid,
                        key_expr,
                        Some(data_info),
                        payload,
                    );
                }
                primitives.send_reply_final(qid);
            });
        }
    }

    pub fn key_expr_to_expr(&self, key_expr: &KeyExpr) -> ZResult<String> {
        let state = zread!(self.state);
        state.remotekey_to_expr(key_expr)
    }
}

impl Primitives for Session {
    fn decl_resource(&self, expr_id: ZInt, key_expr: &KeyExpr) {
        trace!("recv Decl Resource {} {:?}", expr_id, key_expr);
        let state = &mut zwrite!(self.state);
        match state.remotekey_to_expr(key_expr) {
            Ok(key_expr) => {
                let mut res = Resource::new(key_expr.clone());
                for sub in state.subscribers.values() {
                    if key_expr::matches(&key_expr, &sub.key_expr_str) {
                        res.subscribers.push(sub.clone());
                    }
                }

                state.remote_resources.insert(expr_id, res);
            }
            Err(_) => error!("Received Resource for unkown key_expr: {}", key_expr),
        }
    }

    fn forget_resource(&self, _expr_id: ZInt) {
        trace!("recv Forget Resource {}", _expr_id);
    }

    fn decl_publisher(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Decl Publisher {:?}", _key_expr);
    }

    fn forget_publisher(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Publisher {:?}", _key_expr);
    }

    fn decl_subscriber(
        &self,
        _key_expr: &KeyExpr,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Decl Subscriber {:?} , {:?}", _key_expr, _sub_info);
    }

    fn forget_subscriber(&self, _key_expr: &KeyExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Subscriber {:?}", _key_expr);
    }

    fn decl_queryable(
        &self,
        _key_expr: &KeyExpr,
        _kind: ZInt,
        _qabl_info: &QueryableInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Decl Queryable {:?}", _key_expr);
    }

    fn forget_queryable(
        &self,
        _key_expr: &KeyExpr,
        _kind: ZInt,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Forget Queryable {:?}", _key_expr);
    }

    fn send_data(
        &self,
        key_expr: &KeyExpr,
        payload: ZBuf,
        channel: Channel,
        congestion_control: CongestionControl,
        info: Option<DataInfo>,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!(
            "recv Data {:?} {:?} {:?} {:?} {:?}",
            key_expr,
            payload,
            channel,
            congestion_control,
            info,
        );
        self.handle_data(false, key_expr, info, payload)
    }

    fn send_query(
        &self,
        key_expr: &KeyExpr,
        value_selector: &str,
        qid: ZInt,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!(
            "recv Query {:?} {:?} {:?} {:?}",
            key_expr,
            value_selector,
            target,
            consolidation
        );
        self.handle_query(false, key_expr, value_selector, qid, target, consolidation)
    }

    fn send_reply_data(
        &self,
        qid: ZInt,
        replier_kind: ZInt,
        replier_id: ZenohId,
        key_expr: KeyExpr,
        data_info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        trace!(
            "recv ReplyData {:?} {:?} {:?} {:?} {:?} {:?}",
            qid,
            replier_kind,
            replier_id,
            key_expr,
            data_info,
            payload
        );
        let state = &mut zwrite!(self.state);
        let key_expr = match state.remotekey_to_expr(&key_expr) {
            Ok(name) => name,
            Err(e) => {
                error!("Received ReplyData for unkown key_expr: {}", e);
                return;
            }
        };
        match state.queries.get_mut(&qid) {
            Some(query) => {
                let new_reply = Reply {
                    data: Sample::with_info(key_expr.into(), payload, data_info),
                    replier_kind,
                    replier_id,
                };
                match query.reception_mode {
                    ConsolidationMode::None => {
                        let _ = query.rep_sender.send(new_reply);
                    }
                    ConsolidationMode::Lazy => {
                        match query
                            .replies
                            .as_ref()
                            .unwrap()
                            .get(new_reply.data.key_expr.as_str())
                        {
                            Some(reply) => {
                                if new_reply.data.timestamp > reply.data.timestamp {
                                    query.replies.as_mut().unwrap().insert(
                                        new_reply.data.key_expr.to_string(),
                                        new_reply.clone(),
                                    );
                                    let _ = query.rep_sender.send(new_reply);
                                }
                            }
                            None => {
                                query
                                    .replies
                                    .as_mut()
                                    .unwrap()
                                    .insert(new_reply.data.key_expr.to_string(), new_reply.clone());
                                let _ = query.rep_sender.send(new_reply);
                            }
                        }
                    }
                    ConsolidationMode::Full => {
                        match query
                            .replies
                            .as_ref()
                            .unwrap()
                            .get(new_reply.data.key_expr.as_str())
                        {
                            Some(reply) => {
                                if new_reply.data.timestamp > reply.data.timestamp {
                                    query.replies.as_mut().unwrap().insert(
                                        new_reply.data.key_expr.to_string(),
                                        new_reply.clone(),
                                    );
                                }
                            }
                            None => {
                                query
                                    .replies
                                    .as_mut()
                                    .unwrap()
                                    .insert(new_reply.data.key_expr.to_string(), new_reply.clone());
                            }
                        };
                    }
                }
            }
            None => {
                warn!("Received ReplyData for unkown Query: {}", qid);
            }
        }
    }

    fn send_reply_final(&self, qid: ZInt) {
        trace!("recv ReplyFinal {:?}", qid);
        let mut state = zwrite!(self.state);
        match state.queries.get_mut(&qid) {
            Some(mut query) => {
                query.nb_final -= 1;
                if query.nb_final == 0 {
                    let query = state.queries.remove(&qid).unwrap();
                    if query.reception_mode == ConsolidationMode::Full {
                        for (_, reply) in query.replies.unwrap().into_iter() {
                            let _ = query.rep_sender.send(reply);
                        }
                    }
                    trace!("Close query {}", qid);
                }
            }
            None => {
                warn!("Received ReplyFinal for unkown Query: {}", qid);
            }
        }
    }

    fn send_pull(
        &self,
        _is_final: bool,
        _key_expr: &KeyExpr,
        _pull_id: ZInt,
        _max_samples: &Option<ZInt>,
    ) {
        trace!(
            "recv Pull {:?} {:?} {:?} {:?}",
            _is_final,
            _key_expr,
            _pull_id,
            _max_samples
        );
    }

    fn send_close(&self) {
        trace!("recv Close");
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.clone().close_alive().wait();
        }
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Session{{...}}")
    }
}
