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

use crate::config::Config;
use crate::config::Notifier;
use crate::info::*;
use crate::key_expr::keyexpr;
use crate::key_expr::KeyExprInner;
use crate::key_expr::OwnedKeyExpr;
use crate::net::routing::face::Face;
use crate::net::runtime::Runtime;
use crate::net::transport::Primitives;
use crate::prelude::{Callback, EntityFactory, KeyExpr};
use crate::publication::*;
use crate::query::*;
use crate::queryable::*;
use crate::subscriber::*;
use crate::utils::ClosureResolve;
use crate::utils::FutureResolve;
use crate::Id;
use crate::Priority;
use crate::Sample;
use crate::SampleKind;
use crate::Selector;
use crate::Value;
use async_std::task;
use flume::bounded;
use futures::StreamExt;
use log::{error, trace, warn};
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
use uhlc::HLC;
use zenoh_core::AsyncResolve;
use zenoh_core::Resolvable;
use zenoh_core::Resolve;
use zenoh_core::SyncResolve;
use zenoh_core::{zconfigurable, zread, Result as ZResult};
use zenoh_protocol::{
    core::{
        queryable, wire_expr, AtomicZInt, Channel, CongestionControl, ConsolidationStrategy,
        ExprId, QueryTarget, QueryableInfo, SubInfo, WireExpr, ZInt,
    },
    io::ZBuf,
    proto::{DataInfo, RoutingContext},
};
use zenoh_protocol_core::WhatAmI;
use zenoh_protocol_core::ZenohId;
use zenoh_protocol_core::EMPTY_EXPR_ID;

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
    pub(crate) publications: Vec<OwnedKeyExpr>,
    pub(crate) subscribers: HashMap<Id, Arc<SubscriberState>>,
    pub(crate) local_subscribers: HashMap<Id, Arc<SubscriberState>>,
    pub(crate) queryables: HashMap<Id, Arc<QueryableState>>,
    pub(crate) queries: HashMap<ZInt, QueryState>,
    pub(crate) local_routing: bool,
    pub(crate) join_subscriptions: Vec<OwnedKeyExpr>,
    pub(crate) join_publications: Vec<OwnedKeyExpr>,
}

impl SessionState {
    pub(crate) fn new(
        local_routing: bool,
        join_subscriptions: Vec<OwnedKeyExpr>,
        join_publications: Vec<OwnedKeyExpr>,
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
    fn localid_to_expr<'a>(&'a self, id: &ExprId) -> ZResult<KeyExpr<'a>> {
        match self.local_resources.get(id) {
            Some(res) => Ok(KeyExpr::from(&*res.name)),
            None => bail!("{}", id),
        }
    }

    #[inline]
    fn id_to_expr<'a>(&'a self, id: &ExprId) -> ZResult<KeyExpr<'a>> {
        match self.remote_resources.get(id) {
            Some(res) => Ok(KeyExpr::from(&*res.name)),
            None => self.localid_to_expr(id),
        }
    }

    pub(crate) fn remotekey_to_expr<'a, 'b>(
        &'a self,
        key_expr: &'b WireExpr,
    ) -> ZResult<KeyExpr<'b>> {
        if key_expr.scope == EMPTY_EXPR_ID {
            Ok(unsafe { keyexpr::from_str_unchecked(key_expr.suffix.as_ref()) }.into())
        } else if key_expr.suffix.is_empty() {
            self.id_to_expr(&key_expr.scope).map(KeyExpr::into_owned)
        } else {
            Ok(unsafe {
                KeyExpr::from_string_unchecked(
                    [
                        self.id_to_expr(&key_expr.scope)?.as_ref(),
                        key_expr.suffix.as_ref(),
                    ]
                    .concat(),
                )
            })
        }
    }

    pub(crate) fn localkey_to_expr<'a, 'b>(
        &'a self,
        key_expr: &'b WireExpr,
    ) -> ZResult<KeyExpr<'b>> {
        if key_expr.scope == EMPTY_EXPR_ID {
            Ok(unsafe { keyexpr::from_str_unchecked(key_expr.suffix.as_ref()) }.into())
        } else if key_expr.suffix.is_empty() {
            self.localid_to_expr(&key_expr.scope)
                .map(KeyExpr::into_owned)
        } else {
            Ok(unsafe {
                KeyExpr::from_string_unchecked(
                    [
                        self.localid_to_expr(&key_expr.scope)?.as_ref(),
                        key_expr.suffix.as_ref(),
                    ]
                    .concat(),
                )
            })
        }
    }

    pub(crate) fn key_expr_to_expr<'a, 'b>(
        &'a self,
        key_expr: &'b WireExpr,
        local: bool,
    ) -> ZResult<KeyExpr<'b>> {
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
    pub(crate) name: OwnedKeyExpr,
    pub(crate) subscribers: Vec<Arc<SubscriberState>>,
    pub(crate) local_subscribers: Vec<Arc<SubscriberState>>,
}

impl Resource {
    pub(crate) fn new(name: OwnedKeyExpr) -> Self {
        Resource {
            name,
            subscribers: vec![],
            local_subscribers: vec![],
        }
    }
}

#[derive(Clone)]
pub(crate) enum SessionRef<'a> {
    Borrow(&'a Session),
    Shared(Arc<Session>),
}

impl Deref for SessionRef<'_> {
    type Target = Session;

    fn deref(&self) -> &Self::Target {
        match self {
            SessionRef::Borrow(b) => b,
            SessionRef::Shared(s) => &*s,
        }
    }
}

impl fmt::Debug for SessionRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionRef::Borrow(b) => Session::fmt(b, f),
            SessionRef::Shared(s) => Session::fmt(&*s, f),
        }
    }
}

pub trait Undeclarable<T> {
    type Output;
    type Undeclaration: Resolve<Self::Output>;
    fn undeclare(self, _: T) -> Self::Undeclaration;
}
impl<'a, T: Undeclarable<()>> Undeclarable<&'a Session> for T {
    type Output = <T as Undeclarable<()>>::Output;
    type Undeclaration = <T as Undeclarable<()>>::Undeclaration;
    fn undeclare(self, _: &'a Session) -> Self::Undeclaration {
        self.undeclare(())
    }
}

impl<'a> Undeclarable<&'a Session> for KeyExpr<'a> {
    type Output = ZResult<()>;
    type Undeclaration = KeyExprUndeclaration<'a>;
    fn undeclare(self, session: &'a Session) -> Self::Undeclaration {
        KeyExprUndeclaration {
            session,
            expr: self,
        }
    }
}
pub struct KeyExprUndeclaration<'a> {
    session: &'a Session,
    expr: KeyExpr<'a>,
}
impl Resolvable for KeyExprUndeclaration<'_> {
    type Output = ZResult<()>;
}
impl AsyncResolve for KeyExprUndeclaration<'_> {
    type Future = futures::future::Ready<Self::Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.res_sync())
    }
}
impl SyncResolve for KeyExprUndeclaration<'_> {
    fn res_sync(self) -> Self::Output {
        let KeyExprUndeclaration { session, expr } = self;
        let expr_id = match &expr.0 {
            KeyExprInner::Wire {
                key_expr,
                expr_id,
                prefix_len,
            } if *prefix_len as usize == key_expr.len() => *expr_id as u64,
            _ => return Err(zerror!("Failed to undeclare {}, make sure you use the result of `Session::declare_expr` to call `Session::undeclare`", expr).into()),
        };
        trace!("undeclare_expr({:?})", expr_id);
        let mut state = zwrite!(session.state);
        state.local_resources.remove(&expr_id);

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.forget_resource(expr_id);

        Ok(())
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
    /// Initialize a Session with an existing Runtime.
    /// This operation is used by the plugins to share the same Runtime than the router.
    #[doc(hidden)]
    pub fn init(
        runtime: Runtime,
        local_routing: bool,
        join_subscriptions: Vec<OwnedKeyExpr>,
        join_publications: Vec<OwnedKeyExpr>,
    ) -> impl Resolve<Session> {
        ClosureResolve(move || {
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
            session
        })
    }

    /// Consumes the given `Session`, returning a thread-safe reference-counting
    /// pointer to it (`Arc<Session>`). This is equivalent to `Arc::new(session)`.
    ///
    /// This is useful to share ownership of the `Session` between several threads
    /// and tasks. It also alows to create [`Subscriber`](HandlerSubscriber) and
    /// [`Queryable`](HandlerQueryable) with static lifetime that can be moved to several
    /// threads and tasks
    ///
    /// Note: the given zenoh `Session` will be closed when the last reference to
    /// it is dropped.
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received : {:?}", sample);
    ///     }
    /// }).await;
    /// # })
    /// ```
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// Consumes and leaks the given `Session`, returning a `'static` mutable
    /// reference to it. The given `Session` will live  for the remainder of
    /// the program's life. Dropping the returned reference will cause a memory
    /// leak.
    ///
    /// This is useful to move entities (like [`Subscriber`](HandlerSubscriber)) which
    /// lifetimes are bound to the session lifetime in several threads or tasks.
    ///
    /// Note: the given zenoh `Session` cannot be closed any more. At process
    /// termination the zenoh session will terminate abruptly. If possible prefer
    /// using [`Session::into_arc()`](Session::into_arc).
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    /// use zenoh::Session;
    ///
    /// let session = Session::leak(zenoh::open(config::peer()).res().await.unwrap());
    /// let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received : {:?}", sample);
    ///     }
    /// }).await;
    /// # })
    /// ```
    pub fn leak(s: Self) -> &'static mut Self {
        Box::leak(Box::new(s))
    }

    /// Returns the identifier for this session.
    pub fn id(&self) -> String {
        self.runtime.get_pid_str()
    }

    pub fn hlc(&self) -> Option<&HLC> {
        self.runtime.hlc.as_ref().map(Arc::as_ref)
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
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// session.close().res().await.unwrap();
    /// # })
    /// ```
    pub fn close(self) -> impl Resolve<ZResult<()>> {
        FutureResolve(async move {
            trace!("close()");
            self.runtime.close().await?;

            let primitives = zwrite!(self.state).primitives.as_ref().unwrap().clone();
            primitives.send_close();

            Ok(())
        })
    }

    pub fn undeclare<'a, T: Undeclarable<&'a Self>>(&'a self, decl: T) -> T::Undeclaration {
        decl.undeclare(self)
    }

    /// Get the current configuration of the zenoh [`Session`](Session).
    ///
    /// The returned configuration [`Notifier`] can be used to read the current
    /// zenoh configuration through the `get` function or
    /// modify the zenoh configuration through the `insert`,
    /// or `insert_json5` funtion.
    ///
    /// # Examples
    /// ### Read current zenoh configuration
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let peers = session.config().get("connect/endpoints").unwrap();
    /// # })
    /// ```
    ///
    /// ### Modify current zenoh configuration
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let _ = session.config().insert_json5("connect/endpoints", r#"["tcp/127.0.0.1/7447"]"#);
    /// # })
    /// ```
    #[allow(clippy::mut_from_ref)]
    pub fn config(&self) -> &mut Notifier<Config> {
        self.runtime.config.mutable()
    }

    /// Get informations about the zenoh [`Session`](Session).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let info = session.info();
    /// # })
    /// ```
    pub fn info(&self) -> impl Resolve<InfoProperties> + '_ {
        ClosureResolve(move || {
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
                    s.get_pid()
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
                        s.get_pid()
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
            info
        })
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
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let expr = session.declare_expr("key/expression").res().await.unwrap();
    /// session.undeclare(expr).res().await.unwrap(); // don't forget to undeclare `expr` when done with it
    /// # })
    /// ```
    pub fn declare_expr<'a, IntoKeyExpr>(
        &'a self,
        key_expr: IntoKeyExpr,
    ) -> impl Resolve<ZResult<KeyExpr<'a>>> + 'a
    where
        IntoKeyExpr: TryInto<KeyExpr<'a>>,
        <IntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_core::Error>,
    {
        let key_expr = key_expr.try_into().map_err(Into::into);
        ClosureResolve(move || {
            let key_expr: KeyExpr = match key_expr {
                Ok(k) => k,
                Err(e) => return Err(e),
            };
            if let KeyExprInner::Wire { expr_id, .. } = &key_expr.0 {
                if *expr_id != 0 {
                    return Ok(key_expr);
                }
            }
            trace!("declare_expr({:?})", key_expr);
            let mut state = zwrite!(self.state);

            let owned_expr = OwnedKeyExpr::from(key_expr);
            let expr = &*owned_expr;
            match state
                .local_resources
                .iter()
                .find(|(_expr_id, res)| &*res.name == expr)
            {
                Some((expr_id, _res)) => Ok(KeyExpr(KeyExprInner::Wire {
                    expr_id: *expr_id as u32,
                    prefix_len: expr.len() as u32,
                    key_expr: owned_expr,
                })),
                None => {
                    let expr_id = state.expr_id_counter.fetch_add(1, Ordering::SeqCst) as ZInt;
                    let mut res = Resource::new(owned_expr.clone());
                    for sub in state.subscribers.values() {
                        if expr.intersect(&*sub.key_expr) {
                            res.subscribers.push(sub.clone());
                        }
                    }
                    state.local_resources.insert(expr_id, res);
                    let primitives = state.primitives.as_ref().unwrap().clone();
                    drop(state);
                    let key_expr = KeyExpr(KeyExprInner::Wire {
                        expr_id: expr_id as u32,
                        prefix_len: expr.len() as u32,
                        key_expr: owned_expr,
                    });
                    primitives.decl_resource(
                        expr_id,
                        &WireExpr {
                            scope: 0,
                            suffix: std::borrow::Cow::Borrowed(key_expr.as_str()),
                        },
                    );
                    Ok(key_expr)
                }
            }
        })
    }

    /// Create a [`Subscriber`](HandlerSubscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The resourkey expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received : {:?}", sample);
    /// }
    /// # })
    /// ```
    pub fn declare_subscriber<'a, 'b, IntoKeyExpr>(
        &'a self,
        key_expr: IntoKeyExpr,
    ) -> SubscriberBuilder<'a, 'b>
    where
        IntoKeyExpr: TryInto<KeyExpr<'b>>,
        <IntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>,
    {
        SubscriberBuilder {
            session: SessionRef::Borrow(self),
            key_expr: key_expr.try_into().map_err(Into::into),
            reliability: Reliability::default(),
            mode: SubMode::default(),
            period: None,
            local: false,
        }
    }

    /// Create a [`Queryable`](HandlerQueryable) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching the queries the
    /// [`Queryable`](HandlerQueryable) will reply to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let queryable = session.declare_queryable("key/expression").res().await.unwrap();
    /// while let Ok(query) = queryable.recv_async().await {
    ///     query.reply(Ok(Sample::try_from(
    ///         "key/expression",
    ///         "value",
    ///     ).unwrap())).res().await.unwrap();
    /// }
    /// # })
    /// ```
    pub fn declare_queryable<'a, 'b, IntoKeyExpr>(
        &'a self,
        key_expr: IntoKeyExpr,
    ) -> QueryableBuilder<'a, 'b>
    where
        IntoKeyExpr: TryInto<KeyExpr<'b>>,
        <IntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>,
    {
        QueryableBuilder {
            session: SessionRef::Borrow(self),
            key_expr: key_expr.try_into().map_err(Into::into),
            kind: zenoh_protocol_core::queryable::ALL_KINDS,
            complete: true,
        }
    }

    /// Create a [`Publisher`](crate::publication::Publisher) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching resources to write
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// publisher.put("value").res().await.unwrap();
    /// # })
    /// ```
    pub fn declare_publisher<'a, IntoKeyExpr>(&'a self, key_expr: IntoKeyExpr) -> PublishBuilder<'a>
    where
        IntoKeyExpr: TryInto<KeyExpr<'a>>,
        <IntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_core::Error>,
    {
        PublishBuilder {
            session: SessionRef::Borrow(self),
            key_expr: key_expr.try_into().map_err(Into::into),
            congestion_control: CongestionControl::default(),
            priority: Priority::default(),
            local_routing: None,
        }
    }

    /// Put data.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching resources to put
    /// * `value` - The value to put
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// session
    ///     .put("key/expression", "value")
    ///     .encoding(KnownEncoding::TextPlain)
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn put<'a, IntoKeyExpr, IntoValue>(
        &'a self,
        key_expr: IntoKeyExpr,
        value: IntoValue,
    ) -> PutBuilder<'a>
    where
        IntoKeyExpr: TryInto<KeyExpr<'a>>,
        <IntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_core::Error>,
        IntoValue: Into<Value>,
    {
        PutBuilder {
            publisher: self.declare_publisher(key_expr),
            value: value.into(),
            kind: SampleKind::Put,
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
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// session.delete("key/expression").res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn delete<'a, IntoKeyExpr>(&'a self, key_expr: IntoKeyExpr) -> DeleteBuilder<'a>
    where
        IntoKeyExpr: TryInto<KeyExpr<'a>>,
        <IntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_core::Error>,
    {
        PutBuilder {
            publisher: self.declare_publisher(key_expr),
            value: Value::empty(),
            kind: SampleKind::Delete,
        }
    }
    /// Query data from the matching queryables in the system.
    ///
    /// # Arguments
    ///
    /// * `selector` - The selection of resources to query
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let replies = session.get("key/expression").res().await.unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     println!(">> Received {:?}", reply.sample);
    /// }
    /// # })
    /// ```
    pub fn get<'a, 'b, IntoSelector>(&'a self, selector: IntoSelector) -> GetBuilder<'a, 'b>
    where
        IntoSelector: TryInto<Selector<'b>>,
        <IntoSelector as TryInto<Selector<'b>>>::Error: Into<zenoh_core::Error>,
    {
        let selector = selector.try_into().map_err(Into::into);
        GetBuilder {
            session: self,
            selector,
            target: QueryTarget::default(),
            consolidation: QueryConsolidation::default(),
            local_routing: None,
        }
    }
}

impl Session {
    pub(crate) fn clone(&self) -> Self {
        Session {
            runtime: self.runtime.clone(),
            state: self.state.clone(),
            alive: false,
        }
    }

    #[allow(clippy::new_ret_no_self)]
    pub(super) fn new(config: Config) -> impl Resolve<ZResult<Session>> {
        FutureResolve(async {
            log::debug!("Config: {:?}", &config);
            let local_routing = config.local_routing().unwrap_or(true);
            let join_subscriptions = config.startup().subscribe().clone();
            let join_publications = config.startup().declare_publications().clone();
            match Runtime::new(config).await {
                Ok(runtime) => {
                    let session = Self::init(
                        runtime,
                        local_routing,
                        join_subscriptions,
                        join_publications,
                    )
                    .res_async()
                    .await;
                    // Workaround for the declare_and_shoot problem
                    task::sleep(Duration::from_millis(*API_OPEN_SESSION_DELAY)).await;
                    Ok(session)
                }
                Err(err) => Err(err),
            }
        })
    }

    /// Declare a publication for the given key expression.
    ///
    /// Puts that match the given key expression will only be sent on the network
    /// if matching subscribers exist in the system.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression to publish
    pub(crate) fn declare_publication_intent<'a, IntoKeyExpr>(
        &'a self,
        key_expr: IntoKeyExpr,
    ) -> impl Resolve<ZResult<()>> + 'a
    where
        IntoKeyExpr: TryInto<KeyExpr<'a>>,
        <IntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_core::Error>,
    {
        let key_expr = key_expr.try_into().map_err(Into::into);
        ClosureResolve(move || {
            let key_expr: KeyExpr = key_expr?;
            log::trace!("declare_publication({:?})", key_expr);
            let mut state = zwrite!(self.state);
            if !state.publications.iter().any(|p| **p == *key_expr) {
                let declared_pub = if let Some(join_pub) = state
                    .join_publications
                    .iter()
                    .find(|s| wire_expr::include(s, &key_expr))
                {
                    let joined_pub = state
                        .publications
                        .iter()
                        .any(|p| wire_expr::include(join_pub, p));
                    (!joined_pub).then(|| join_pub.clone().into())
                } else {
                    Some(key_expr.clone())
                };
                state.publications.push(OwnedKeyExpr::from(key_expr));

                if let Some(res) = declared_pub {
                    let primitives = state.primitives.as_ref().unwrap().clone();
                    drop(state);
                    primitives.decl_publisher(&(&res).into(), None);
                }
            }
            Ok(())
        })
    }

    /// Undeclare a publication previously declared
    /// with [`declare_publication`](Session::declare_publication).
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression of the publication to undeclarte
    pub(crate) fn undeclare_publication_intent<'a, IntoKeyExpr>(
        &'a self,
        key_expr: IntoKeyExpr,
    ) -> impl Resolve<ZResult<()>> + 'a
    where
        IntoKeyExpr: TryInto<KeyExpr<'a>>,
        <IntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_core::Error>,
    {
        let key_expr = key_expr.try_into().map_err(Into::into);
        ClosureResolve(move || {
            let key_expr: KeyExpr = key_expr?;
            let mut state = zwrite!(self.state);
            if let Some(idx) = state.publications.iter().position(|p| **p == *key_expr) {
                trace!("undeclare_publication({:?})", key_expr);
                state.publications.remove(idx);
                match state
                    .join_publications
                    .iter()
                    .find(|s| wire_expr::include(s, &key_expr))
                {
                    Some(join_pub) => {
                        let joined_pub = state
                            .publications
                            .iter()
                            .any(|p| wire_expr::include(join_pub, p));
                        if !joined_pub {
                            let primitives = state.primitives.as_ref().unwrap().clone();
                            let key_expr = WireExpr::from(join_pub).to_owned();
                            drop(state);
                            primitives.forget_publisher(&key_expr, None);
                        }
                    }
                    None => {
                        let primitives = state.primitives.as_ref().unwrap().clone();
                        drop(state);
                        primitives.forget_publisher(&(&key_expr).into(), None);
                    }
                };
            } else {
                bail!("Unable to find publication")
            }
            Ok(())
        })
    }

    pub(crate) fn declare_subscriber_inner(
        &self,
        key_expr: &KeyExpr,
        callback: Callback<Sample>,
        info: &SubInfo,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        log::trace!("subscribe({:?})", key_expr);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let sub_state = Arc::new(SubscriberState {
            id,
            key_expr: key_expr.clone().into_owned(),
            callback,
        });
        let declared_sub = match state
            .join_subscriptions // TODO: can this be an OwnedKeyExpr?
            .iter()
            .find(|s| wire_expr::include(s, key_expr))
        {
            Some(join_sub) => {
                let joined_sub = state
                    .subscribers
                    .values()
                    .any(|s| wire_expr::include(join_sub, &s.key_expr));
                (!joined_sub).then(|| join_sub.clone().into())
            }
            None => {
                let twin_sub = state.subscribers.values().any(|s| s.key_expr == *key_expr);
                (!twin_sub).then(|| key_expr.borrowing_clone())
            }
        };

        state.subscribers.insert(sub_state.id, sub_state.clone());
        for res in state.local_resources.values_mut() {
            if key_expr.intersect(&res.name) {
                res.subscribers.push(sub_state.clone());
            }
        }
        for res in state.remote_resources.values_mut() {
            if key_expr.intersect(&res.name) {
                res.subscribers.push(sub_state.clone());
            }
        }

        if let Some(key_expr) = declared_sub {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);

            // If key_expr is a pure Expr, remap it to optimal Rid or RidWithSuffix
            let key_expr = if !key_expr.is_optimized() {
                match key_expr.as_ref().find('*') {
                    Some(0) => key_expr,
                    Some(pos) => self.declare_expr(&key_expr.as_ref()[..pos]).res_sync()?,
                    None => self.declare_expr(key_expr).res_sync()?,
                }
            } else {
                key_expr
            };

            primitives.decl_subscriber(&(&key_expr).into(), info, None);
        }

        Ok(sub_state)
    }

    pub(crate) fn declare_local_subscriber(
        &self,
        key_expr: &KeyExpr,
        callback: Callback<Sample>,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        log::trace!("subscribe({:?})", key_expr);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let sub_state = Arc::new(SubscriberState {
            id,
            key_expr: key_expr.clone().into_owned(),
            callback,
        });
        state
            .local_subscribers
            .insert(sub_state.id, sub_state.clone());
        for res in state.local_resources.values_mut() {
            if key_expr.intersect(&*res.name) {
                res.local_subscribers.push(sub_state.clone());
            }
        }
        for res in state.remote_resources.values_mut() {
            if key_expr.intersect(&*res.name) {
                res.local_subscribers.push(sub_state.clone());
            }
        }

        Ok(sub_state)
    }
    pub(crate) fn unsubscribe(&self, sid: usize) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        if let Some(sub_state) = state.subscribers.remove(&sid) {
            trace!("unsubscribe({:?})", sub_state);
            for res in state.local_resources.values_mut() {
                res.subscribers.retain(|sub| sub.id != sub_state.id);
            }
            for res in state.remote_resources.values_mut() {
                res.subscribers.retain(|sub| sub.id != sub_state.id);
            }

            // Note: there might be several Subscribers on the same KeyExpr.
            // Before calling forget_subscriber(key_expr), check if this was the last one.
            let key_expr = &sub_state.key_expr;
            match state
                .join_subscriptions
                .iter()
                .find(|s| wire_expr::include(s, key_expr))
            {
                Some(join_sub) => {
                    let joined_sub = state
                        .subscribers
                        .values()
                        .any(|s| wire_expr::include(join_sub, &s.key_expr));
                    if !joined_sub {
                        let primitives = state.primitives.as_ref().unwrap().clone();
                        let key_expr = WireExpr::from(join_sub).to_owned();
                        drop(state);
                        primitives.forget_subscriber(&key_expr, None);
                    }
                }
                None => {
                    let twin_sub = state.subscribers.values().any(|s| s.key_expr == *key_expr);
                    if !twin_sub {
                        let primitives = state.primitives.as_ref().unwrap().clone();
                        drop(state);
                        primitives.forget_subscriber(&key_expr.into(), None);
                    }
                }
            };
            Ok(())
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
        }
    }
    pub(crate) fn declare_queryable_inner(
        &self,
        key_expr: &WireExpr,
        kind: ZInt,
        complete: bool,
        callback: Callback<Query>,
    ) -> ZResult<Arc<QueryableState>> {
        let mut state = zwrite!(self.state);
        log::trace!("queryable({:?})", key_expr);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let qable_state = Arc::new(QueryableState {
            id,
            key_expr: key_expr.to_owned(),
            kind,
            complete,
            callback: callback.into(),
        });
        #[cfg(feature = "complete_n")]
        {
            state.queryables.insert(id, qable_state.clone());

            if complete {
                let primitives = state.primitives.as_ref().unwrap().clone();
                let complete = Session::complete_twin_qabls(&state, key_expr, kind);
                drop(state);
                let qabl_info = QueryableInfo {
                    complete,
                    distance: 0,
                };
                primitives.decl_queryable(key_expr, kind, &qabl_info, None);
            }
        }
        #[cfg(not(feature = "complete_n"))]
        {
            let twin_qabl = Session::twin_qabl(&state, key_expr, kind);
            let complete_twin_qabl =
                twin_qabl && Session::complete_twin_qabl(&state, key_expr, kind);

            state.queryables.insert(id, qable_state.clone());

            if !twin_qabl || (!complete_twin_qabl && complete) {
                let primitives = state.primitives.as_ref().unwrap().clone();
                let complete = if !complete_twin_qabl && complete {
                    1
                } else {
                    0
                };
                drop(state);
                let qabl_info = QueryableInfo {
                    complete,
                    distance: 0,
                };
                primitives.decl_queryable(key_expr, kind, &qabl_info, None);
            }
        }
        Ok(qable_state)
    }

    pub(crate) fn twin_qabl(state: &SessionState, key: &WireExpr, kind: ZInt) -> bool {
        state.queryables.values().any(|q| {
            q.kind == kind
                && state.localkey_to_expr(&q.key_expr).unwrap()
                    == state.localkey_to_expr(key).unwrap()
        })
    }

    #[cfg(not(feature = "complete_n"))]
    pub(crate) fn complete_twin_qabl(state: &SessionState, key: &WireExpr, kind: ZInt) -> bool {
        state.queryables.values().any(|q| {
            q.complete
                && q.kind == kind
                && state.localkey_to_expr(&q.key_expr).unwrap()
                    == state.localkey_to_expr(key).unwrap()
        })
    }

    #[cfg(feature = "complete_n")]
    pub(crate) fn complete_twin_qabls(state: &SessionState, key: &WireExpr, kind: ZInt) -> ZInt {
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
    pub(crate) fn close_queryable(&self, qid: usize) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        if let Some(qable_state) = state.queryables.remove(&qid) {
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
        }
    }
    pub(crate) fn handle_data(
        &self,
        local: bool,
        key_expr: &WireExpr,
        info: Option<DataInfo>,
        payload: ZBuf,
        local_routing: Option<bool>,
    ) {
        let state = zread!(self.state);
        let local_routing = local_routing.unwrap_or(state.local_routing);
        if key_expr.suffix.is_empty() {
            match state.get_res(&key_expr.scope, local) {
                Some(res) => {
                    if !local && res.subscribers.len() == 1 {
                        let sub = res.subscribers.get(0).unwrap();
                        (sub.callback)(Sample::with_info(res.name.clone().into(), payload, info));
                    } else {
                        if !local || local_routing {
                            for sub in &res.subscribers {
                                (sub.callback)(Sample::with_info(
                                    res.name.clone().into(),
                                    payload.clone(),
                                    info.clone(),
                                ));
                            }
                        }
                        if local {
                            for sub in &res.local_subscribers {
                                (sub.callback)(Sample::with_info(
                                    res.name.clone().into(),
                                    payload.clone(),
                                    info.clone(),
                                ));
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
                    if !local || local_routing {
                        for sub in state.subscribers.values() {
                            if key_expr.intersect(&*sub.key_expr) {
                                (sub.callback)(Sample::with_info(
                                    key_expr.clone().into_owned(),
                                    payload.clone(),
                                    info.clone(),
                                ));
                            }
                        }
                    }
                    if local {
                        for sub in state.local_subscribers.values() {
                            if key_expr.intersect(&*sub.key_expr) {
                                (sub.callback)(Sample::with_info(
                                    key_expr.clone().into_owned(),
                                    payload.clone(),
                                    info.clone(),
                                ));
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

    pub(crate) fn pull<'a>(&'a self, key_expr: &'a KeyExpr) -> impl Resolve<ZResult<()>> + 'a {
        ClosureResolve(move || {
            trace!("pull({:?})", key_expr);
            let state = zread!(self.state);
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            primitives.send_pull(true, &key_expr.into(), 0, &None);
            Ok(())
        })
    }

    pub(crate) fn query(
        &self,
        selector: &Selector<'_>,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        local_routing: Option<bool>,
        callback: Callback<Reply>,
    ) -> ZResult<()> {
        log::trace!("get({}, {:?}, {:?})", selector, target, consolidation);
        let mut state = zwrite!(self.state);
        let consolidation = match consolidation {
            QueryConsolidation::Auto => {
                if selector.has_time_range() {
                    ConsolidationStrategy::none()
                } else {
                    ConsolidationStrategy::default()
                }
            }
            QueryConsolidation::Manual(strategy) => strategy,
        };
        let local_routing = local_routing.unwrap_or(state.local_routing);
        let qid = state.qid_counter.fetch_add(1, Ordering::SeqCst);
        let nb_final = if local_routing { 2 } else { 1 };
        log::trace!("Register query {} (nb_final = {})", qid, nb_final);
        state.queries.insert(
            qid,
            QueryState {
                nb_final,
                reception_mode: consolidation.reception,
                replies: if consolidation.reception != ConsolidationMode::None {
                    Some(HashMap::new())
                } else {
                    None
                },
                callback,
            },
        );

        let primitives = state.primitives.as_ref().unwrap().clone();

        drop(state);
        primitives.send_query(
            &(&selector.key_selector).into(),
            selector.value_selector.as_ref(),
            qid,
            zenoh_protocol_core::QueryTAK {
                kind: zenoh_protocol_core::queryable::ALL_KINDS,
                target,
            },
            consolidation,
            None,
        );
        if local_routing {
            self.handle_query(
                true,
                &(&selector.key_selector).into(),
                selector.value_selector.as_ref(),
                qid,
                zenoh_protocol_core::QueryTAK {
                    kind: zenoh_protocol_core::queryable::ALL_KINDS,
                    target,
                },
                consolidation,
            );
        }
        Ok(())
    }

    pub(crate) fn handle_query(
        &self,
        local: bool,
        key_expr: &WireExpr,
        value_selector: &str,
        qid: ZInt,
        target: zenoh_protocol_core::QueryTAK,
        _consolidation: ConsolidationStrategy,
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
                                    wire_expr::matches(&qablname, &key_expr)
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
                        .map(|qable| (qable.kind, qable.callback.clone()))
                        .collect::<Vec<(ZInt, Arc<dyn Fn(Query) + Send + Sync>)>>();
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
            req_sender(Query {
                key_selector: key_expr.clone().into_owned(),
                value_selector: value_selector.clone(),
                kind,
                replies_sender: rep_sender.clone(),
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
                        (&key_expr).into(),
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
                        (&key_expr).into(),
                        Some(data_info),
                        payload,
                    );
                }
                primitives.send_reply_final(qid);
            });
        }
    }
}

impl EntityFactory for Arc<Session> {
    /// Create a [`Subscriber`](HandlerSubscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The resourkey expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received : {:?}", sample);
    ///     }
    /// }).await;
    /// # })
    /// ```
    fn declare_subscriber<'b, IntoKeyExpr>(
        &self,
        key_expr: IntoKeyExpr,
    ) -> SubscriberBuilder<'static, 'b>
    where
        IntoKeyExpr: TryInto<KeyExpr<'b>>,
        <IntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>,
    {
        SubscriberBuilder {
            session: SessionRef::Shared(self.clone()),
            key_expr: key_expr.try_into().map_err(Into::into),
            reliability: Reliability::default(),
            mode: SubMode::default(),
            period: None,
            local: false,
        }
    }

    /// Create a [`Queryable`](HandlerQueryable) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching the queries the
    /// [`Queryable`](HandlerQueryable) will reply to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let queryable = session.declare_queryable("key/expression").res().await.unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(query) = queryable.recv_async().await {
    ///         query.reply(Ok(Sample::try_from(
    ///             "key/expression",
    ///             "value",
    ///         ).unwrap())).res().await.unwrap();
    ///     }
    /// }).await;
    /// # })
    /// ```
    fn declare_queryable<'b, IntoKeyExpr>(
        &self,
        key_expr: IntoKeyExpr,
    ) -> QueryableBuilder<'static, 'b>
    where
        IntoKeyExpr: TryInto<KeyExpr<'b>>,
        <IntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_core::Error>,
    {
        QueryableBuilder {
            session: SessionRef::Shared(self.clone()),
            key_expr: key_expr.try_into().map_err(Into::into),
            kind: zenoh_protocol_core::queryable::EVAL,
            complete: true,
        }
    }

    /// Create a [`Publisher`](crate::publication::Publisher) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching resources to write
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::*;
    /// use r#async::AsyncResolve;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression").res().await.unwrap();
    /// publisher.put("value").res().await.unwrap();
    /// # })
    /// ```
    fn declare_publisher<'a, IntoKeyExpr>(&self, key_expr: IntoKeyExpr) -> PublishBuilder<'a>
    where
        IntoKeyExpr: TryInto<KeyExpr<'a>>,
        <IntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_core::Error>,
    {
        PublishBuilder {
            session: SessionRef::Shared(self.clone()),
            key_expr: key_expr.try_into().map_err(Into::into),
            congestion_control: CongestionControl::default(),
            priority: Priority::default(),
            local_routing: None,
        }
    }
}

impl Primitives for Session {
    fn decl_resource(&self, expr_id: ZInt, key_expr: &WireExpr) {
        trace!("recv Decl Resource {} {:?}", expr_id, key_expr);
        let state = &mut zwrite!(self.state);
        match state.remotekey_to_expr(key_expr) {
            Ok(key_expr) => {
                let mut res = Resource::new(key_expr.clone().into());
                for sub in state.subscribers.values() {
                    if key_expr.intersect(&*sub.key_expr) {
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

    fn decl_publisher(&self, _key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Decl Publisher {:?}", _key_expr);
    }

    fn forget_publisher(&self, _key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Publisher {:?}", _key_expr);
    }

    fn decl_subscriber(
        &self,
        _key_expr: &WireExpr,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Decl Subscriber {:?} , {:?}", _key_expr, _sub_info);
    }

    fn forget_subscriber(&self, _key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Subscriber {:?}", _key_expr);
    }

    fn decl_queryable(
        &self,
        _key_expr: &WireExpr,
        _kind: ZInt,
        _qabl_info: &QueryableInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Decl Queryable {:?}", _key_expr);
    }

    fn forget_queryable(
        &self,
        _key_expr: &WireExpr,
        _kind: ZInt,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Forget Queryable {:?}", _key_expr);
    }

    fn send_data(
        &self,
        key_expr: &WireExpr,
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
        self.handle_data(false, key_expr, info, payload, None)
    }

    fn send_query(
        &self,
        key_expr: &WireExpr,
        value_selector: &str,
        qid: ZInt,
        target: zenoh_protocol_core::QueryTAK,
        consolidation: ConsolidationStrategy,
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
        key_expr: WireExpr,
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
                    sample: Ok(Sample::with_info(key_expr.into_owned(), payload, data_info)),
                    replier_id,
                };
                match query.reception_mode {
                    ConsolidationMode::None => {
                        let _ = (query.callback)(new_reply);
                    }
                    ConsolidationMode::Lazy => {
                        match query
                            .replies
                            .as_ref()
                            .unwrap()
                            .get(new_reply.sample.as_ref().unwrap().key_expr.as_str())
                        {
                            Some(reply) => {
                                if new_reply.sample.as_ref().unwrap().timestamp
                                    > reply.sample.as_ref().unwrap().timestamp
                                {
                                    query.replies.as_mut().unwrap().insert(
                                        new_reply.sample.as_ref().unwrap().key_expr.to_string(),
                                        new_reply.clone(),
                                    );
                                    let _ = (query.callback)(new_reply);
                                }
                            }
                            None => {
                                query.replies.as_mut().unwrap().insert(
                                    new_reply.sample.as_ref().unwrap().key_expr.to_string(),
                                    new_reply.clone(),
                                );
                                let _ = (query.callback)(new_reply);
                            }
                        }
                    }
                    ConsolidationMode::Full => {
                        match query
                            .replies
                            .as_ref()
                            .unwrap()
                            .get(new_reply.sample.as_ref().unwrap().key_expr.as_str())
                        {
                            Some(reply) => {
                                if new_reply.sample.as_ref().unwrap().timestamp
                                    > reply.sample.as_ref().unwrap().timestamp
                                {
                                    query.replies.as_mut().unwrap().insert(
                                        new_reply.sample.as_ref().unwrap().key_expr.to_string(),
                                        new_reply.clone(),
                                    );
                                }
                            }
                            None => {
                                query.replies.as_mut().unwrap().insert(
                                    new_reply.sample.as_ref().unwrap().key_expr.to_string(),
                                    new_reply.clone(),
                                );
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
                            let _ = (query.callback)(reply);
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
        _key_expr: &WireExpr,
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
            let _ = self.clone().close().res_sync();
        }
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Session").field("id", &self.id()).finish()
    }
}
