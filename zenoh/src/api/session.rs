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
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    fmt,
    future::{IntoFuture, Ready},
    ops::Deref,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use tracing::{error, trace, warn};
use uhlc::HLC;
use zenoh_buffers::ZBuf;
use zenoh_collections::SingleOrVec;
use zenoh_config::{unwrap_or_default, wrappers::ZenohId, Config, Notifier};
use zenoh_core::{zconfigurable, zread, Resolvable, Resolve, ResolveClosure, ResolveFuture, Wait};
#[cfg(feature = "unstable")]
use zenoh_protocol::network::{declare::SubscriberId, ext};
use zenoh_protocol::{
    core::{
        key_expr::{keyexpr, OwnedKeyExpr},
        AtomicExprId, CongestionControl, EntityId, ExprId, Parameters, Reliability, WireExpr,
        EMPTY_EXPR_ID,
    },
    network::{
        self,
        declare::{
            self, common::ext::WireExprType, queryable::ext::QueryableInfoType,
            subscriber::ext::SubscriberInfo, Declare, DeclareBody, DeclareKeyExpr,
            DeclareQueryable, DeclareSubscriber, UndeclareQueryable, UndeclareSubscriber,
        },
        interest::{InterestMode, InterestOptions},
        request::{self, ext::TargetType, Request},
        AtomicRequestId, Interest, Mapping, Push, RequestId, Response, ResponseFinal,
    },
    zenoh::{
        query::{self, ext::QueryBodyType, Consolidation},
        reply::ReplyBody,
        Del, PushBody, Put, RequestBody, ResponseBody,
    },
};
use zenoh_result::ZResult;
#[cfg(feature = "shared-memory")]
use zenoh_shm::api::client_storage::ShmClientStorage;
use zenoh_task::TaskController;

use super::{
    admin,
    builders::publisher::{
        PublicationBuilderDelete, PublicationBuilderPut, PublisherBuilder, SessionDeleteBuilder,
        SessionPutBuilder,
    },
    bytes::ZBytes,
    encoding::Encoding,
    handlers::{Callback, DefaultHandler},
    info::SessionInfo,
    key_expr::{KeyExpr, KeyExprInner},
    publisher::{Priority, PublisherState},
    query::{
        ConsolidationMode, QueryConsolidation, QueryState, QueryTarget, Reply, SessionGetBuilder,
    },
    queryable::{Query, QueryInner, QueryableBuilder, QueryableState},
    sample::{DataInfo, DataInfoIntoSample, Locality, QoS, Sample, SampleKind},
    selector::Selector,
    subscriber::{SubscriberBuilder, SubscriberState},
    value::Value,
    Id,
};
#[cfg(feature = "unstable")]
use super::{
    liveliness::{Liveliness, LivelinessTokenState},
    publisher::Publisher,
    publisher::{MatchingListenerState, MatchingStatus},
    sample::SourceInfo,
};
#[cfg(feature = "unstable")]
use crate::api::selector::ZenohParameters;
use crate::net::{
    primitives::Primitives,
    routing::dispatcher::face::Face,
    runtime::{Runtime, RuntimeBuilder},
};

zconfigurable! {
    pub(crate) static ref API_DATA_RECEPTION_CHANNEL_SIZE: usize = 256;
    pub(crate) static ref API_QUERY_RECEPTION_CHANNEL_SIZE: usize = 256;
    pub(crate) static ref API_REPLY_EMISSION_CHANNEL_SIZE: usize = 256;
    pub(crate) static ref API_REPLY_RECEPTION_CHANNEL_SIZE: usize = 256;
}

pub(crate) struct SessionState {
    pub(crate) primitives: Option<Arc<Face>>, // @TODO replace with MaybeUninit ??
    pub(crate) expr_id_counter: AtomicExprId, // @TODO: manage rollover and uniqueness
    pub(crate) qid_counter: AtomicRequestId,
    pub(crate) local_resources: HashMap<ExprId, Resource>,
    pub(crate) remote_resources: HashMap<ExprId, Resource>,
    #[cfg(feature = "unstable")]
    pub(crate) remote_subscribers: HashMap<SubscriberId, KeyExpr<'static>>,
    pub(crate) publishers: HashMap<Id, PublisherState>,
    pub(crate) subscribers: HashMap<Id, Arc<SubscriberState>>,
    pub(crate) queryables: HashMap<Id, Arc<QueryableState>>,
    #[cfg(feature = "unstable")]
    pub(crate) tokens: HashMap<Id, Arc<LivelinessTokenState>>,
    #[cfg(feature = "unstable")]
    pub(crate) matching_listeners: HashMap<Id, Arc<MatchingListenerState>>,
    pub(crate) queries: HashMap<RequestId, QueryState>,
    pub(crate) aggregated_subscribers: Vec<OwnedKeyExpr>,
    pub(crate) aggregated_publishers: Vec<OwnedKeyExpr>,
}

impl SessionState {
    pub(crate) fn new(
        aggregated_subscribers: Vec<OwnedKeyExpr>,
        aggregated_publishers: Vec<OwnedKeyExpr>,
    ) -> SessionState {
        SessionState {
            primitives: None,
            expr_id_counter: AtomicExprId::new(1), // Note: start at 1 because 0 is reserved for NO_RESOURCE
            qid_counter: AtomicRequestId::new(0),
            local_resources: HashMap::new(),
            remote_resources: HashMap::new(),
            #[cfg(feature = "unstable")]
            remote_subscribers: HashMap::new(),
            publishers: HashMap::new(),
            subscribers: HashMap::new(),
            queryables: HashMap::new(),
            #[cfg(feature = "unstable")]
            tokens: HashMap::new(),
            #[cfg(feature = "unstable")]
            matching_listeners: HashMap::new(),
            queries: HashMap::new(),
            aggregated_subscribers,
            aggregated_publishers,
        }
    }
}

impl SessionState {
    #[inline]
    fn get_local_res(&self, id: &ExprId) -> Option<&Resource> {
        self.local_resources.get(id)
    }

    #[inline]
    fn get_remote_res(&self, id: &ExprId, mapping: Mapping) -> Option<&Resource> {
        match mapping {
            Mapping::Receiver => self.local_resources.get(id),
            Mapping::Sender => self.remote_resources.get(id),
        }
    }

    #[inline]
    fn get_res(&self, id: &ExprId, mapping: Mapping, local: bool) -> Option<&Resource> {
        if local {
            self.get_local_res(id)
        } else {
            self.get_remote_res(id, mapping)
        }
    }

    pub(crate) fn remote_key_to_expr<'a>(&'a self, key_expr: &'a WireExpr) -> ZResult<KeyExpr<'a>> {
        if key_expr.scope == EMPTY_EXPR_ID {
            Ok(unsafe { keyexpr::from_str_unchecked(key_expr.suffix.as_ref()) }.into())
        } else if key_expr.suffix.is_empty() {
            match self.get_remote_res(&key_expr.scope, key_expr.mapping) {
                Some(Resource::Node(ResourceNode { key_expr, .. })) => Ok(key_expr.into()),
                Some(Resource::Prefix { prefix }) => bail!(
                    "Received {:?}, where {} is `{}`, which isn't a valid key expression",
                    key_expr,
                    key_expr.scope,
                    prefix
                ),
                None => bail!("Remote resource {} not found", key_expr.scope),
            }
        } else {
            [
                match self.get_remote_res(&key_expr.scope, key_expr.mapping) {
                    Some(Resource::Node(ResourceNode { key_expr, .. })) => key_expr.as_str(),
                    Some(Resource::Prefix { prefix }) => prefix.as_ref(),
                    None => bail!("Remote resource {} not found", key_expr.scope),
                },
                key_expr.suffix.as_ref(),
            ]
            .concat()
            .try_into()
        }
    }

    pub(crate) fn local_wireexpr_to_expr<'a>(
        &'a self,
        key_expr: &'a WireExpr,
    ) -> ZResult<KeyExpr<'a>> {
        if key_expr.scope == EMPTY_EXPR_ID {
            key_expr.suffix.as_ref().try_into()
        } else if key_expr.suffix.is_empty() {
            match self.get_local_res(&key_expr.scope) {
                Some(Resource::Node(ResourceNode { key_expr, .. })) => Ok(key_expr.into()),
                Some(Resource::Prefix { prefix }) => bail!(
                    "Received {:?}, where {} is `{}`, which isn't a valid key expression",
                    key_expr,
                    key_expr.scope,
                    prefix
                ),
                None => bail!("Remote resource {} not found", key_expr.scope),
            }
        } else {
            [
                match self.get_local_res(&key_expr.scope) {
                    Some(Resource::Node(ResourceNode { key_expr, .. })) => key_expr.as_str(),
                    Some(Resource::Prefix { prefix }) => prefix.as_ref(),
                    None => bail!("Remote resource {} not found", key_expr.scope),
                },
                key_expr.suffix.as_ref(),
            ]
            .concat()
            .try_into()
        }
    }

    pub(crate) fn wireexpr_to_keyexpr<'a>(
        &'a self,
        key_expr: &'a WireExpr,
        local: bool,
    ) -> ZResult<KeyExpr<'a>> {
        if local {
            self.local_wireexpr_to_expr(key_expr)
        } else {
            self.remote_key_to_expr(key_expr)
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

pub(crate) struct ResourceNode {
    pub(crate) key_expr: OwnedKeyExpr,
    pub(crate) subscribers: Vec<Arc<SubscriberState>>,
}
pub(crate) enum Resource {
    Prefix { prefix: Box<str> },
    Node(ResourceNode),
}

impl Resource {
    pub(crate) fn new(name: Box<str>) -> Self {
        if keyexpr::new(name.as_ref()).is_ok() {
            Self::for_keyexpr(unsafe { OwnedKeyExpr::from_boxed_string_unchecked(name) })
        } else {
            Self::Prefix { prefix: name }
        }
    }
    pub(crate) fn for_keyexpr(key_expr: OwnedKeyExpr) -> Self {
        Self::Node(ResourceNode {
            key_expr,
            subscribers: Vec::new(),
        })
    }
    pub(crate) fn name(&self) -> &str {
        match self {
            Resource::Prefix { prefix } => prefix.as_ref(),
            Resource::Node(ResourceNode { key_expr, .. }) => key_expr.as_str(),
        }
    }
    pub(crate) fn as_node_mut(&mut self) -> Option<&mut ResourceNode> {
        match self {
            Resource::Prefix { .. } => None,
            Resource::Node(node) => Some(node),
        }
    }
}

#[derive(Clone)]
pub enum SessionRef<'a> {
    Borrow(&'a Session),
    Shared(Arc<Session>),
}

impl<'s, 'a> SessionDeclarations<'s, 'a> for SessionRef<'a> {
    fn declare_subscriber<'b, TryIntoKeyExpr>(
        &'s self,
        key_expr: TryIntoKeyExpr,
    ) -> SubscriberBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SubscriberBuilder {
            session: self.clone(),
            key_expr: TryIntoKeyExpr::try_into(key_expr).map_err(Into::into),
            reliability: Reliability::DEFAULT,
            origin: Locality::default(),
            handler: DefaultHandler::default(),
        }
    }
    fn declare_queryable<'b, TryIntoKeyExpr>(
        &'s self,
        key_expr: TryIntoKeyExpr,
    ) -> QueryableBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        QueryableBuilder {
            session: self.clone(),
            key_expr: key_expr.try_into().map_err(Into::into),
            complete: false,
            origin: Locality::default(),
            handler: DefaultHandler::default(),
        }
    }
    fn declare_publisher<'b, TryIntoKeyExpr>(
        &'s self,
        key_expr: TryIntoKeyExpr,
    ) -> PublisherBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PublisherBuilder {
            session: self.clone(),
            key_expr: key_expr.try_into().map_err(Into::into),
            congestion_control: CongestionControl::DEFAULT,
            priority: Priority::DEFAULT,
            is_express: false,
            destination: Locality::default(),
        }
    }
    #[zenoh_macros::unstable]
    fn liveliness(&'s self) -> Liveliness<'a> {
        Liveliness {
            session: self.clone(),
        }
    }
    fn info(&'s self) -> SessionInfo<'a> {
        SessionInfo {
            session: self.clone(),
        }
    }
}

impl Deref for SessionRef<'_> {
    type Target = Session;

    fn deref(&self) -> &Self::Target {
        match self {
            SessionRef::Borrow(b) => b,
            SessionRef::Shared(s) => s,
        }
    }
}

impl fmt::Debug for SessionRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SessionRef::Borrow(b) => Session::fmt(b, f),
            SessionRef::Shared(s) => Session::fmt(s, f),
        }
    }
}

/// A trait implemented by types that can be undeclared.
pub trait Undeclarable<S, O, T = ZResult<()>>
where
    O: Resolve<T> + Send,
{
    fn undeclare_inner(self, session: S) -> O;
}

impl<'a, O, T, G> Undeclarable<&'a Session, O, T> for G
where
    O: Resolve<T> + Send,
    G: Undeclarable<(), O, T>,
{
    fn undeclare_inner(self, _: &'a Session) -> O {
        self.undeclare_inner(())
    }
}

/// A zenoh session.
///
pub struct Session {
    pub(crate) runtime: Runtime,
    pub(crate) state: Arc<RwLock<SessionState>>,
    pub(crate) id: u16,
    close_on_drop: bool,
    owns_runtime: bool,
    task_controller: TaskController,
}

static SESSION_ID_COUNTER: AtomicU16 = AtomicU16::new(0);
impl Session {
    pub(crate) fn init(
        runtime: Runtime,
        aggregated_subscribers: Vec<OwnedKeyExpr>,
        aggregated_publishers: Vec<OwnedKeyExpr>,
    ) -> impl Resolve<Session> {
        ResolveClosure::new(move || {
            let router = runtime.router();
            let state = Arc::new(RwLock::new(SessionState::new(
                aggregated_subscribers,
                aggregated_publishers,
            )));
            let session = Session {
                runtime: runtime.clone(),
                state: state.clone(),
                id: SESSION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
                close_on_drop: true,
                owns_runtime: false,
                task_controller: TaskController::default(),
            };

            runtime.new_handler(Arc::new(admin::Handler::new(session.clone())));

            let primitives = Some(router.new_primitives(Arc::new(session.clone())));
            zwrite!(state).primitives = primitives;

            admin::init(&session);

            session
        })
    }

    /// Consumes the given `Session`, returning a thread-safe reference-counting
    /// pointer to it (`Arc<Session>`). This is equivalent to `Arc::new(session)`.
    ///
    /// This is useful to share ownership of the `Session` between several threads
    /// and tasks. It also alows to create [`Subscriber`](Subscriber) and
    /// [`Queryable`](Queryable) with static lifetime that can be moved to several
    /// threads and tasks
    ///
    /// Note: the given zenoh `Session` will be closed when the last reference to
    /// it is dropped.
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .await
    ///     .unwrap();
    /// tokio::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received: {:?}", sample);
    ///     }
    /// }).await;
    /// # }
    /// ```
    pub fn into_arc(self) -> Arc<Self> {
        Arc::new(self)
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
    /// termination the zenoh session will terminate abruptly. If possible prefer
    /// using [`Session::into_arc()`](Session::into_arc).
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::Session::leak(zenoh::open(zenoh::config::peer()).await.unwrap());
    /// let subscriber = session.declare_subscriber("key/expression").await.unwrap();
    /// tokio::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received: {:?}", sample);
    ///     }
    /// }).await;
    /// # }
    /// ```
    pub fn leak(s: Self) -> &'static mut Self {
        Box::leak(Box::new(s))
    }

    /// Returns the identifier of the current session. `zid()` is a convenient shortcut.
    /// See [`Session::info()`](`Session::info()`) and [`SessionInfo::zid()`](`SessionInfo::zid()`) for more details.
    pub fn zid(&self) -> ZenohId {
        self.info().zid().wait()
    }

    pub fn hlc(&self) -> Option<&HLC> {
        self.runtime.hlc()
    }

    /// Close the zenoh [`Session`](Session).
    ///
    /// Sessions are automatically closed when dropped, but you may want to use this function to handle errors or
    /// close the Session asynchronously.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// session.close().await.unwrap();
    /// # }
    /// ```
    pub fn close(mut self) -> impl Resolve<ZResult<()>> {
        ResolveFuture::new(async move {
            trace!("close()");
            // set the flag first to avoid double panic if this function panic
            self.close_on_drop = false;
            self.task_controller.terminate_all(Duration::from_secs(10));
            if self.owns_runtime {
                self.runtime.close().await?;
            }
            let mut state = zwrite!(self.state);
            // clean up to break cyclic references from self.state to itself
            let primitives = state.primitives.take();
            state.queryables.clear();
            drop(state);
            primitives.as_ref().unwrap().send_close();
            Ok(())
        })
    }

    pub fn undeclare<'a, T, O>(&'a self, decl: T) -> O
    where
        O: Resolve<ZResult<()>>,
        T: Undeclarable<&'a Self, O, ZResult<()>>,
    {
        Undeclarable::undeclare_inner(decl, self)
    }

    /// Get the current configuration of the zenoh [`Session`](Session).
    ///
    /// The returned configuration [`Notifier`](Notifier) can be used to read the current
    /// zenoh configuration through the `get` function or
    /// modify the zenoh configuration through the `insert`,
    /// or `insert_json5` funtion.
    ///
    /// # Examples
    /// ### Read current zenoh configuration
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let peers = session.config().get("connect/endpoints").unwrap();
    /// # }
    /// ```
    ///
    /// ### Modify current zenoh configuration
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let _ = session.config().insert_json5("connect/endpoints", r#"["tcp/127.0.0.1/7447"]"#);
    /// # }
    /// ```
    pub fn config(&self) -> &Notifier<Config> {
        self.runtime.config()
    }
}

impl<'a> SessionDeclarations<'a, 'a> for Session {
    fn info(&self) -> SessionInfo {
        SessionRef::Borrow(self).info()
    }
    fn declare_subscriber<'b, TryIntoKeyExpr>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> SubscriberBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SessionRef::Borrow(self).declare_subscriber(key_expr)
    }
    fn declare_queryable<'b, TryIntoKeyExpr>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> QueryableBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SessionRef::Borrow(self).declare_queryable(key_expr)
    }
    fn declare_publisher<'b, TryIntoKeyExpr>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> PublisherBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SessionRef::Borrow(self).declare_publisher(key_expr)
    }
    #[zenoh_macros::unstable]
    fn liveliness(&'a self) -> Liveliness {
        SessionRef::Borrow(self).liveliness()
    }
}

impl Session {
    /// Informs Zenoh that you intend to use `key_expr` multiple times and that it should optimize its transmission.
    ///
    /// The returned `KeyExpr`'s internal structure may differ from what you would have obtained through a simple
    /// `key_expr.try_into()`, to save time on detecting the optimizations that have been associated with it.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let key_expr = session.declare_keyexpr("key/expression").await.unwrap();
    /// # }
    /// ```
    pub fn declare_keyexpr<'a, 'b: 'a, TryIntoKeyExpr>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> impl Resolve<ZResult<KeyExpr<'b>>> + 'a
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        let key_expr: ZResult<KeyExpr> = key_expr.try_into().map_err(Into::into);
        self._declare_keyexpr(key_expr)
    }

    fn _declare_keyexpr<'a, 'b: 'a>(
        &'a self,
        key_expr: ZResult<KeyExpr<'b>>,
    ) -> impl Resolve<ZResult<KeyExpr<'b>>> + 'a {
        let sid = self.id;
        ResolveClosure::new(move || {
            let key_expr: KeyExpr = key_expr?;
            let prefix_len = key_expr.len() as u32;
            let expr_id = self.declare_prefix(key_expr.as_str()).wait();
            let key_expr = match key_expr.0 {
                KeyExprInner::Borrowed(key_expr) | KeyExprInner::BorrowedWire { key_expr, .. } => {
                    KeyExpr(KeyExprInner::BorrowedWire {
                        key_expr,
                        expr_id,
                        mapping: Mapping::Sender,
                        prefix_len,
                        session_id: sid,
                    })
                }
                KeyExprInner::Owned(key_expr) | KeyExprInner::Wire { key_expr, .. } => {
                    KeyExpr(KeyExprInner::Wire {
                        key_expr,
                        expr_id,
                        mapping: Mapping::Sender,
                        prefix_len,
                        session_id: sid,
                    })
                }
            };
            Ok(key_expr)
        })
    }

    /// Put data.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - Key expression matching the resources to put
    /// * `payload` - The payload to put
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::{encoding::Encoding, prelude::*};
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// session
    ///     .put("key/expression", "payload")
    ///     .encoding(Encoding::TEXT_PLAIN)
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn put<'a, 'b: 'a, TryIntoKeyExpr, IntoZBytes>(
        &'a self,
        key_expr: TryIntoKeyExpr,
        payload: IntoZBytes,
    ) -> SessionPutBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
        IntoZBytes: Into<ZBytes>,
    {
        SessionPutBuilder {
            publisher: self.declare_publisher(key_expr),
            kind: PublicationBuilderPut {
                payload: payload.into(),
                encoding: Encoding::default(),
            },
            timestamp: None,
            attachment: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
        }
    }

    /// Delete data.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - Key expression matching the resources to delete
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// session.delete("key/expression").await.unwrap();
    /// # }
    /// ```
    #[inline]
    pub fn delete<'a, 'b: 'a, TryIntoKeyExpr>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> SessionDeleteBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SessionDeleteBuilder {
            publisher: self.declare_publisher(key_expr),
            kind: PublicationBuilderDelete,
            timestamp: None,
            attachment: None,
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
        }
    }
    /// Query data from the matching queryables in the system.
    ///
    /// Unless explicitly requested via [`GetBuilder::accept_replies`], replies are guaranteed to have
    /// key expressions that match the requested `selector`.
    ///
    /// # Arguments
    ///
    /// * `selector` - The selection of resources to query
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let replies = session.get("key/expression").await.unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     println!(">> Received {:?}", reply.result());
    /// }
    /// # }
    /// ```
    pub fn get<'a, 'b: 'a, TryIntoSelector>(
        &'a self,
        selector: TryIntoSelector,
    ) -> SessionGetBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoSelector: TryInto<Selector<'b>>,
        <TryIntoSelector as TryInto<Selector<'b>>>::Error: Into<zenoh_result::Error>,
    {
        let selector = selector.try_into().map_err(Into::into);
        let timeout = {
            let conf = self.runtime.config().lock();
            Duration::from_millis(unwrap_or_default!(conf.queries_default_timeout()))
        };
        let qos: QoS = request::ext::QoSType::REQUEST.into();
        SessionGetBuilder {
            session: self,
            selector,
            scope: Ok(None),
            target: QueryTarget::DEFAULT,
            consolidation: QueryConsolidation::DEFAULT,
            qos: qos.into(),
            destination: Locality::default(),
            timeout,
            value: None,
            attachment: None,
            handler: DefaultHandler::default(),
            #[cfg(feature = "unstable")]
            source_info: SourceInfo::empty(),
        }
    }
}

impl Session {
    pub(crate) fn clone(&self) -> Self {
        Self {
            runtime: self.runtime.clone(),
            state: self.state.clone(),
            id: self.id,
            close_on_drop: false,
            owns_runtime: self.owns_runtime,
            task_controller: self.task_controller.clone(),
        }
    }

    #[allow(clippy::new_ret_no_self)]
    pub(super) fn new(
        config: Config,
        #[cfg(feature = "shared-memory")] shm_clients: Option<Arc<ShmClientStorage>>,
    ) -> impl Resolve<ZResult<Session>> {
        ResolveFuture::new(async move {
            tracing::debug!("Config: {:?}", &config);
            let aggregated_subscribers = config.aggregation().subscribers().clone();
            let aggregated_publishers = config.aggregation().publishers().clone();
            #[allow(unused_mut)] // Required for shared-memory
            let mut runtime = RuntimeBuilder::new(config);
            #[cfg(feature = "shared-memory")]
            {
                runtime = runtime.shm_clients(shm_clients);
            }
            let mut runtime = runtime.build().await?;

            let mut session = Self::init(
                runtime.clone(),
                aggregated_subscribers,
                aggregated_publishers,
            )
            .await;
            session.owns_runtime = true;
            runtime.start().await?;
            Ok(session)
        })
    }

    pub(crate) fn declare_prefix<'a>(&'a self, prefix: &'a str) -> impl Resolve<ExprId> + 'a {
        ResolveClosure::new(move || {
            trace!("declare_prefix({:?})", prefix);
            let mut state = zwrite!(self.state);
            match state
                .local_resources
                .iter()
                .find(|(_expr_id, res)| res.name() == prefix)
            {
                Some((expr_id, _res)) => *expr_id,
                None => {
                    let expr_id = state.expr_id_counter.fetch_add(1, Ordering::SeqCst);
                    let mut res = Resource::new(Box::from(prefix));
                    if let Resource::Node(ResourceNode {
                        key_expr,
                        subscribers,
                        ..
                    }) = &mut res
                    {
                        for sub in state.subscribers.values() {
                            if key_expr.intersects(&sub.key_expr) {
                                subscribers.push(sub.clone());
                            }
                        }
                    }
                    state.local_resources.insert(expr_id, res);
                    let primitives = state.primitives.as_ref().unwrap().clone();
                    drop(state);
                    primitives.send_declare(Declare {
                        interest_id: None,
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::DeclareKeyExpr(DeclareKeyExpr {
                            id: expr_id,
                            wire_expr: WireExpr {
                                scope: 0,
                                suffix: prefix.to_owned().into(),
                                mapping: Mapping::Sender,
                            },
                        }),
                    });
                    expr_id
                }
            }
        })
    }

    pub(crate) fn declare_publisher_inner(
        &self,
        key_expr: KeyExpr,
        destination: Locality,
    ) -> ZResult<EntityId> {
        let mut state = zwrite!(self.state);
        tracing::trace!("declare_publisher({:?})", key_expr);
        let id = self.runtime.next_id();

        let mut pub_state = PublisherState {
            id,
            remote_id: id,
            key_expr: key_expr.clone().into_owned(),
            destination,
        };

        let declared_pub = (destination != Locality::SessionLocal)
            .then(|| {
                match state
                    .aggregated_publishers
                    .iter()
                    .find(|s| s.includes(&key_expr))
                {
                    Some(join_pub) => {
                        if let Some(joined_pub) = state.publishers.values().find(|p| {
                            p.destination != Locality::SessionLocal
                                && join_pub.includes(&p.key_expr)
                        }) {
                            pub_state.remote_id = joined_pub.remote_id;
                            None
                        } else {
                            Some(join_pub.clone().into())
                        }
                    }
                    None => {
                        if let Some(twin_pub) = state.publishers.values().find(|p| {
                            p.destination != Locality::SessionLocal && p.key_expr == key_expr
                        }) {
                            pub_state.remote_id = twin_pub.remote_id;
                            None
                        } else {
                            Some(key_expr.clone())
                        }
                    }
                }
            })
            .flatten();

        state.publishers.insert(id, pub_state);

        if let Some(res) = declared_pub {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            primitives.send_interest(Interest {
                id,
                mode: InterestMode::CurrentFuture,
                options: InterestOptions::KEYEXPRS + InterestOptions::SUBSCRIBERS,
                wire_expr: Some(res.to_wire(self).to_owned()),
                ext_qos: network::ext::QoSType::DEFAULT,
                ext_tstamp: None,
                ext_nodeid: network::ext::NodeIdType::DEFAULT,
            });
        }
        Ok(id)
    }

    pub(crate) fn undeclare_publisher_inner(&self, pid: Id) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        if let Some(pub_state) = state.publishers.remove(&pid) {
            trace!("undeclare_publisher({:?})", pub_state);
            if pub_state.destination != Locality::SessionLocal {
                // Note: there might be several publishers on the same KeyExpr.
                // Before calling forget_publishers(key_expr), check if this was the last one.
                if !state.publishers.values().any(|p| {
                    p.destination != Locality::SessionLocal && p.remote_id == pub_state.remote_id
                }) {
                    let primitives = state.primitives.as_ref().unwrap().clone();
                    drop(state);
                    primitives.send_interest(Interest {
                        id: pub_state.remote_id,
                        mode: InterestMode::Final,
                        options: InterestOptions::empty(),
                        wire_expr: None,
                        ext_qos: declare::ext::QoSType::DEFAULT,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                    });
                }
            }
            Ok(())
        } else {
            Err(zerror!("Unable to find publisher").into())
        }
    }

    pub(crate) fn declare_subscriber_inner(
        &self,
        key_expr: &KeyExpr,
        scope: &Option<KeyExpr>,
        origin: Locality,
        callback: Callback<'static, Sample>,
        info: &SubscriberInfo,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        tracing::trace!("declare_subscriber({:?})", key_expr);
        let id = self.runtime.next_id();
        let key_expr = match scope {
            Some(scope) => scope / key_expr,
            None => key_expr.clone(),
        };

        let mut sub_state = SubscriberState {
            id,
            remote_id: id,
            key_expr: key_expr.clone().into_owned(),
            scope: scope.clone().map(|e| e.into_owned()),
            origin,
            callback,
        };

        #[cfg(not(feature = "unstable"))]
        let declared_sub = origin != Locality::SessionLocal;
        #[cfg(feature = "unstable")]
        let declared_sub = origin != Locality::SessionLocal
            && !key_expr
                .as_str()
                .starts_with(crate::api::liveliness::PREFIX_LIVELINESS);

        let declared_sub =
            declared_sub
                .then(|| {
                    match state
                        .aggregated_subscribers
                        .iter()
                        .find(|s| s.includes(&key_expr))
                    {
                        Some(join_sub) => {
                            if let Some(joined_sub) = state.subscribers.values().find(|s| {
                                s.origin != Locality::SessionLocal && join_sub.includes(&s.key_expr)
                            }) {
                                sub_state.remote_id = joined_sub.remote_id;
                                None
                            } else {
                                Some(join_sub.clone().into())
                            }
                        }
                        None => {
                            if let Some(twin_sub) = state.subscribers.values().find(|s| {
                                s.origin != Locality::SessionLocal && s.key_expr == key_expr
                            }) {
                                sub_state.remote_id = twin_sub.remote_id;
                                None
                            } else {
                                Some(key_expr.clone())
                            }
                        }
                    }
                })
                .flatten();

        let sub_state = Arc::new(sub_state);

        state.subscribers.insert(sub_state.id, sub_state.clone());
        for res in state
            .local_resources
            .values_mut()
            .filter_map(Resource::as_node_mut)
        {
            if key_expr.intersects(&res.key_expr) {
                res.subscribers.push(sub_state.clone());
            }
        }
        for res in state
            .remote_resources
            .values_mut()
            .filter_map(Resource::as_node_mut)
        {
            if key_expr.intersects(&res.key_expr) {
                res.subscribers.push(sub_state.clone());
            }
        }

        if let Some(key_expr) = declared_sub {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            // If key_expr is a pure Expr, remap it to optimal Rid or RidWithSuffix
            // let key_expr = if !key_expr.is_optimized(self) {
            //     match key_expr.as_str().find('*') {
            //         Some(0) => key_expr.to_wire(self),
            //         Some(pos) => {
            //             let expr_id = self.declare_prefix(&key_expr.as_str()[..pos]).wait();
            //             WireExpr {
            //                 scope: expr_id,
            //                 suffix: std::borrow::Cow::Borrowed(&key_expr.as_str()[pos..]),
            //             }
            //         }
            //         None => {
            //             let expr_id = self.declare_prefix(key_expr.as_str()).wait();
            //             WireExpr {
            //                 scope: expr_id,
            //                 suffix: std::borrow::Cow::Borrowed(""),
            //             }
            //         }
            //     }
            // } else {
            //     key_expr.to_wire(self)
            // };

            primitives.send_declare(Declare {
                interest_id: None,
                ext_qos: declare::ext::QoSType::DECLARE,
                ext_tstamp: None,
                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                    id,
                    wire_expr: key_expr.to_wire(self).to_owned(),
                    ext_info: *info,
                }),
            });

            #[cfg(feature = "unstable")]
            {
                let state = zread!(self.state);
                self.update_status_up(&state, &key_expr)
            }
        } else {
            #[cfg(feature = "unstable")]
            if key_expr
                .as_str()
                .starts_with(crate::api::liveliness::PREFIX_LIVELINESS)
            {
                let primitives = state.primitives.as_ref().unwrap().clone();
                drop(state);

                primitives.send_interest(Interest {
                    id,
                    mode: InterestMode::CurrentFuture,
                    options: InterestOptions::KEYEXPRS + InterestOptions::SUBSCRIBERS,
                    wire_expr: Some(key_expr.to_wire(self).to_owned()),
                    ext_qos: network::ext::QoSType::DEFAULT,
                    ext_tstamp: None,
                    ext_nodeid: network::ext::NodeIdType::DEFAULT,
                });
            }
        }

        Ok(sub_state)
    }

    pub(crate) fn undeclare_subscriber_inner(&self, sid: Id) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        if let Some(sub_state) = state.subscribers.remove(&sid) {
            trace!("undeclare_subscriber({:?})", sub_state);
            for res in state
                .local_resources
                .values_mut()
                .filter_map(Resource::as_node_mut)
            {
                res.subscribers.retain(|sub| sub.id != sub_state.id);
            }
            for res in state
                .remote_resources
                .values_mut()
                .filter_map(Resource::as_node_mut)
            {
                res.subscribers.retain(|sub| sub.id != sub_state.id);
            }

            #[cfg(not(feature = "unstable"))]
            let send_forget = sub_state.origin != Locality::SessionLocal;
            #[cfg(feature = "unstable")]
            let send_forget = sub_state.origin != Locality::SessionLocal
                && !sub_state
                    .key_expr
                    .as_str()
                    .starts_with(crate::api::liveliness::PREFIX_LIVELINESS);
            if send_forget {
                // Note: there might be several Subscribers on the same KeyExpr.
                // Before calling forget_subscriber(key_expr), check if this was the last one.
                if !state.subscribers.values().any(|s| {
                    s.origin != Locality::SessionLocal && s.remote_id == sub_state.remote_id
                }) {
                    let primitives = state.primitives.as_ref().unwrap().clone();
                    drop(state);
                    primitives.send_declare(Declare {
                        interest_id: None,
                        ext_qos: declare::ext::QoSType::DECLARE,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                            id: sub_state.remote_id,
                            ext_wire_expr: WireExprType {
                                wire_expr: WireExpr::empty(),
                            },
                        }),
                    });
                    #[cfg(feature = "unstable")]
                    {
                        let state = zread!(self.state);
                        self.update_status_down(&state, &sub_state.key_expr)
                    }
                }
            } else {
                #[cfg(feature = "unstable")]
                if sub_state
                    .key_expr
                    .as_str()
                    .starts_with(crate::api::liveliness::PREFIX_LIVELINESS)
                {
                    let primitives = state.primitives.as_ref().unwrap().clone();
                    drop(state);

                    primitives.send_interest(Interest {
                        id: sub_state.id,
                        mode: InterestMode::Final,
                        options: InterestOptions::empty(),
                        wire_expr: None,
                        ext_qos: declare::ext::QoSType::DEFAULT,
                        ext_tstamp: None,
                        ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                    });
                }
            }
            Ok(())
        } else {
            Err(zerror!("Unable to find subscriber").into())
        }
    }

    pub(crate) fn declare_queryable_inner(
        &self,
        key_expr: &WireExpr,
        complete: bool,
        origin: Locality,
        callback: Callback<'static, Query>,
    ) -> ZResult<Arc<QueryableState>> {
        let mut state = zwrite!(self.state);
        tracing::trace!("declare_queryable({:?})", key_expr);
        let id = self.runtime.next_id();
        let qable_state = Arc::new(QueryableState {
            id,
            key_expr: key_expr.to_owned(),
            complete,
            origin,
            callback,
        });

        state.queryables.insert(id, qable_state.clone());

        if origin != Locality::SessionLocal {
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            let qabl_info = QueryableInfoType {
                complete,
                distance: 0,
            };
            primitives.send_declare(Declare {
                interest_id: None,
                ext_qos: declare::ext::QoSType::DECLARE,
                ext_tstamp: None,
                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                    id,
                    wire_expr: key_expr.to_owned(),
                    ext_info: qabl_info,
                }),
            });
        }
        Ok(qable_state)
    }

    pub(crate) fn close_queryable(&self, qid: Id) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        if let Some(qable_state) = state.queryables.remove(&qid) {
            trace!("undeclare_queryable({:?})", qable_state);
            if qable_state.origin != Locality::SessionLocal {
                let primitives = state.primitives.as_ref().unwrap().clone();
                drop(state);
                primitives.send_declare(Declare {
                    interest_id: None,
                    ext_qos: declare::ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                        id: qable_state.id,
                        ext_wire_expr: WireExprType {
                            wire_expr: qable_state.key_expr.clone(),
                        },
                    }),
                });
            }
            Ok(())
        } else {
            Err(zerror!("Unable to find queryable").into())
        }
    }

    #[zenoh_macros::unstable]
    pub(crate) fn declare_liveliness_inner(
        &self,
        key_expr: &KeyExpr,
    ) -> ZResult<Arc<LivelinessTokenState>> {
        let mut state = zwrite!(self.state);
        tracing::trace!("declare_liveliness({:?})", key_expr);
        let id = self.runtime.next_id();
        let key_expr = KeyExpr::from(*crate::api::liveliness::KE_PREFIX_LIVELINESS / key_expr);
        let tok_state = Arc::new(LivelinessTokenState {
            id,
            key_expr: key_expr.clone().into_owned(),
        });

        state.tokens.insert(tok_state.id, tok_state.clone());
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        primitives.send_declare(Declare {
            interest_id: None,
            ext_qos: declare::ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
            body: DeclareBody::DeclareSubscriber(DeclareSubscriber {
                id,
                wire_expr: key_expr.to_wire(self).to_owned(),
                ext_info: SubscriberInfo::DEFAULT,
            }),
        });
        Ok(tok_state)
    }

    #[zenoh_macros::unstable]
    pub(crate) fn undeclare_liveliness(&self, tid: Id) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        if let Some(tok_state) = state.tokens.remove(&tid) {
            trace!("undeclare_liveliness({:?})", tok_state);
            // Note: there might be several Tokens on the same KeyExpr.
            let key_expr = &tok_state.key_expr;
            let twin_tok = state.tokens.values().any(|s| s.key_expr == *key_expr);
            if !twin_tok {
                let primitives = state.primitives.as_ref().unwrap().clone();
                drop(state);
                primitives.send_declare(Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareSubscriber(UndeclareSubscriber {
                        id: tok_state.id,
                        ext_wire_expr: WireExprType::null(),
                    }),
                });
            }
            Ok(())
        } else {
            Err(zerror!("Unable to find liveliness token").into())
        }
    }

    #[zenoh_macros::unstable]
    pub(crate) fn declare_matches_listener_inner(
        &self,
        publisher: &Publisher,
        callback: Callback<'static, MatchingStatus>,
    ) -> ZResult<Arc<MatchingListenerState>> {
        let mut state = zwrite!(self.state);
        let id = self.runtime.next_id();
        tracing::trace!("matches_listener({:?}) => {id}", publisher.key_expr);
        let listener_state = Arc::new(MatchingListenerState {
            id,
            current: std::sync::Mutex::new(false),
            destination: publisher.destination,
            key_expr: publisher.key_expr.clone().into_owned(),
            callback,
        });
        state.matching_listeners.insert(id, listener_state.clone());
        drop(state);
        match listener_state.current.lock() {
            Ok(mut current) => {
                if self
                    .matching_status(&publisher.key_expr, listener_state.destination)
                    .map(|s| s.matching_subscribers())
                    .unwrap_or(true)
                {
                    *current = true;
                    (listener_state.callback)(MatchingStatus { matching: true });
                }
            }
            Err(e) => tracing::error!("Error trying to acquire MathginListener lock: {}", e),
        }
        Ok(listener_state)
    }

    #[zenoh_macros::unstable]
    pub(crate) fn matching_status(
        &self,
        key_expr: &KeyExpr,
        destination: Locality,
    ) -> ZResult<MatchingStatus> {
        let router = self.runtime.router();
        let tables = zread!(router.tables.tables);

        let matching_subscriptions =
            crate::net::routing::dispatcher::pubsub::get_matching_subscriptions(&tables, key_expr);

        drop(tables);
        let matching = match destination {
            Locality::Any => !matching_subscriptions.is_empty(),
            Locality::Remote => {
                if let Some(face) = zread!(self.state).primitives.as_ref() {
                    matching_subscriptions
                        .values()
                        .any(|dir| !Arc::ptr_eq(dir, &face.state))
                } else {
                    !matching_subscriptions.is_empty()
                }
            }
            Locality::SessionLocal => {
                if let Some(face) = zread!(self.state).primitives.as_ref() {
                    matching_subscriptions
                        .values()
                        .any(|dir| Arc::ptr_eq(dir, &face.state))
                } else {
                    false
                }
            }
        };
        Ok(MatchingStatus { matching })
    }

    #[zenoh_macros::unstable]
    pub(crate) fn update_status_up(&self, state: &SessionState, key_expr: &KeyExpr) {
        for msub in state.matching_listeners.values() {
            if key_expr.intersects(&msub.key_expr) {
                // Cannot hold session lock when calling tables (matching_status())
                // TODO: check which ZRuntime should be used
                self.task_controller
                    .spawn_with_rt(zenoh_runtime::ZRuntime::Net, {
                        let session = self.clone();
                        let msub = msub.clone();
                        async move {
                            match msub.current.lock() {
                                Ok(mut current) => {
                                    if !*current {
                                        if let Ok(status) = session
                                            .matching_status(&msub.key_expr, msub.destination)
                                        {
                                            if status.matching_subscribers() {
                                                *current = true;
                                                let callback = msub.callback.clone();
                                                (callback)(status)
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Error trying to acquire MathginListener lock: {}",
                                        e
                                    );
                                }
                            }
                        }
                    });
            }
        }
    }

    #[zenoh_macros::unstable]
    pub(crate) fn update_status_down(&self, state: &SessionState, key_expr: &KeyExpr) {
        for msub in state.matching_listeners.values() {
            if key_expr.intersects(&msub.key_expr) {
                // Cannot hold session lock when calling tables (matching_status())
                // TODO: check which ZRuntime should be used
                self.task_controller
                    .spawn_with_rt(zenoh_runtime::ZRuntime::Net, {
                        let session = self.clone();
                        let msub = msub.clone();
                        async move {
                            match msub.current.lock() {
                                Ok(mut current) => {
                                    if *current {
                                        if let Ok(status) = session
                                            .matching_status(&msub.key_expr, msub.destination)
                                        {
                                            if !status.matching_subscribers() {
                                                *current = false;
                                                let callback = msub.callback.clone();
                                                (callback)(status)
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Error trying to acquire MathginListener lock: {}",
                                        e
                                    );
                                }
                            }
                        }
                    });
            }
        }
    }

    #[zenoh_macros::unstable]
    pub(crate) fn undeclare_matches_listener_inner(&self, sid: Id) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        if let Some(state) = state.matching_listeners.remove(&sid) {
            trace!("undeclare_matches_listener_inner({:?})", state);
            Ok(())
        } else {
            Err(zerror!("Unable to find MatchingListener").into())
        }
    }

    pub(crate) fn handle_data(
        &self,
        local: bool,
        key_expr: &WireExpr,
        info: Option<DataInfo>,
        payload: ZBuf,
        attachment: Option<ZBytes>,
    ) {
        let mut callbacks = SingleOrVec::default();
        let state = zread!(self.state);
        if key_expr.suffix.is_empty() {
            match state.get_res(&key_expr.scope, key_expr.mapping, local) {
                Some(Resource::Node(res)) => {
                    for sub in &res.subscribers {
                        if sub.origin == Locality::Any
                            || (local == (sub.origin == Locality::SessionLocal))
                        {
                            match &sub.scope {
                                Some(scope) => {
                                    if !res.key_expr.starts_with(&***scope) {
                                        tracing::warn!(
                                            "Received Data for `{}`, which didn't start with scope `{}`: don't deliver to scoped Subscriber.",
                                            res.key_expr,
                                            scope,
                                        );
                                    } else {
                                        match KeyExpr::try_from(&res.key_expr[(scope.len() + 1)..])
                                        {
                                            Ok(key_expr) => callbacks.push((
                                                sub.callback.clone(),
                                                key_expr.into_owned(),
                                            )),
                                            Err(e) => {
                                                tracing::warn!(
                                                    "Error unscoping received Data for `{}`: {}",
                                                    res.key_expr,
                                                    e,
                                                );
                                            }
                                        }
                                    }
                                }
                                None => callbacks
                                    .push((sub.callback.clone(), res.key_expr.clone().into())),
                            };
                        }
                    }
                }
                Some(Resource::Prefix { prefix }) => {
                    tracing::error!(
                        "Received Data for `{}`, which isn't a key expression",
                        prefix
                    );
                    return;
                }
                None => {
                    tracing::error!("Received Data for unknown expr_id: {}", key_expr.scope);
                    return;
                }
            }
        } else {
            match state.wireexpr_to_keyexpr(key_expr, local) {
                Ok(key_expr) => {
                    for sub in state.subscribers.values() {
                        if (sub.origin == Locality::Any
                            || (local == (sub.origin == Locality::SessionLocal)))
                            && key_expr.intersects(&sub.key_expr)
                        {
                            match &sub.scope {
                                Some(scope) => {
                                    if !key_expr.starts_with(&***scope) {
                                        tracing::warn!(
                                            "Received Data for `{}`, which didn't start with scope `{}`: don't deliver to scoped Subscriber.",
                                            key_expr,
                                            scope,
                                        );
                                    } else {
                                        match KeyExpr::try_from(&key_expr[(scope.len() + 1)..]) {
                                            Ok(key_expr) => callbacks.push((
                                                sub.callback.clone(),
                                                key_expr.into_owned(),
                                            )),
                                            Err(e) => {
                                                tracing::warn!(
                                                    "Error unscoping received Data for `{}`: {}",
                                                    key_expr,
                                                    e,
                                                );
                                            }
                                        }
                                    }
                                }
                                None => callbacks
                                    .push((sub.callback.clone(), key_expr.clone().into_owned())),
                            };
                        }
                    }
                }
                Err(err) => {
                    tracing::error!("Received Data for unkown key_expr: {}", err);
                    return;
                }
            }
        };
        drop(state);
        let zenoh_collections::single_or_vec::IntoIter { drain, last } = callbacks.into_iter();
        for (cb, key_expr) in drain {
            let sample = info
                .clone()
                .into_sample(key_expr, payload.clone(), attachment.clone());
            cb(sample);
        }
        if let Some((cb, key_expr)) = last {
            let sample = info.into_sample(key_expr, payload, attachment.clone());
            cb(sample);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn query(
        &self,
        key_expr: &KeyExpr<'_>,
        parameters: &Parameters<'_>,
        scope: &Option<KeyExpr<'_>>,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        qos: QoS,
        destination: Locality,
        timeout: Duration,
        value: Option<Value>,
        attachment: Option<ZBytes>,
        #[cfg(feature = "unstable")] source: SourceInfo,
        callback: Callback<'static, Reply>,
    ) -> ZResult<()> {
        tracing::trace!(
            "get({}, {:?}, {:?})",
            Selector::borrowed(key_expr, parameters),
            target,
            consolidation
        );
        let mut state = zwrite!(self.state);
        let consolidation = match consolidation.mode {
            #[cfg(feature = "unstable")]
            ConsolidationMode::Auto if parameters.time_range().is_some() => ConsolidationMode::None,
            ConsolidationMode::Auto => ConsolidationMode::Latest,
            mode => mode,
        };
        let qid = state.qid_counter.fetch_add(1, Ordering::SeqCst);
        let nb_final = match destination {
            Locality::Any => 2,
            _ => 1,
        };

        let token = self.task_controller.get_cancellation_token();
        self.task_controller
            .spawn_with_rt(zenoh_runtime::ZRuntime::Net, {
                let state = self.state.clone();
                let zid = self.runtime.zid();
                async move {
                    tokio::select! {
                        _ = tokio::time::sleep(timeout) => {
                            let mut state = zwrite!(state);
                            if let Some(query) = state.queries.remove(&qid) {
                                std::mem::drop(state);
                                tracing::debug!("Timeout on query {}! Send error and close.", qid);
                                if query.reception_mode == ConsolidationMode::Latest {
                                    for (_, reply) in query.replies.unwrap().into_iter() {
                                        (query.callback)(reply);
                                    }
                                }
                                (query.callback)(Reply {
                                    result: Err(Value::from("Timeout").into()),
                                    replier_id: zid.into(),
                                });
                            }
                        }
                        _ = token.cancelled() => {}
                    }
                }
            });

        let key_expr = match scope {
            Some(scope) => scope / key_expr,
            None => key_expr.clone().into_owned(),
        };

        tracing::trace!("Register query {} (nb_final = {})", qid, nb_final);
        let wexpr = key_expr.to_wire(self).to_owned();
        state.queries.insert(
            qid,
            QueryState {
                nb_final,
                key_expr,
                parameters: parameters.clone().into_owned(),
                scope: scope.clone().map(|e| e.into_owned()),
                reception_mode: consolidation,
                replies: (consolidation != ConsolidationMode::None).then(HashMap::new),
                callback,
            },
        );

        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);

        if destination != Locality::SessionLocal {
            let ext_attachment = attachment.clone().map(Into::into);
            primitives.send_request(Request {
                id: qid,
                wire_expr: wexpr.clone(),
                ext_qos: qos.into(),
                ext_tstamp: None,
                ext_nodeid: request::ext::NodeIdType::DEFAULT,
                ext_target: target,
                ext_budget: None,
                ext_timeout: Some(timeout),
                payload: RequestBody::Query(zenoh_protocol::zenoh::Query {
                    consolidation,
                    parameters: parameters.to_string(),
                    #[cfg(feature = "unstable")]
                    ext_sinfo: source.into(),
                    #[cfg(not(feature = "unstable"))]
                    ext_sinfo: None,
                    ext_body: value.as_ref().map(|v| query::ext::QueryBodyType {
                        #[cfg(feature = "shared-memory")]
                        ext_shm: None,
                        encoding: v.encoding.clone().into(),
                        payload: v.payload.clone().into(),
                    }),
                    ext_attachment,
                    ext_unknown: vec![],
                }),
            });
        }
        if destination != Locality::Remote {
            self.handle_query(
                true,
                &wexpr,
                parameters.as_str(),
                qid,
                target,
                consolidation,
                value.as_ref().map(|v| query::ext::QueryBodyType {
                    #[cfg(feature = "shared-memory")]
                    ext_shm: None,
                    encoding: v.encoding.clone().into(),
                    payload: v.payload.clone().into(),
                }),
                attachment,
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn handle_query(
        &self,
        local: bool,
        key_expr: &WireExpr,
        parameters: &str,
        qid: RequestId,
        _target: TargetType,
        _consolidation: Consolidation,
        body: Option<QueryBodyType>,
        attachment: Option<ZBytes>,
    ) {
        let (primitives, key_expr, queryables) = {
            let state = zread!(self.state);
            match state.wireexpr_to_keyexpr(key_expr, local) {
                Ok(key_expr) => {
                    let queryables = state
                        .queryables
                        .iter()
                        .filter(
                            |(_, queryable)|
                                (queryable.origin == Locality::Any
                                    || (local == (queryable.origin == Locality::SessionLocal)))
                                &&
                                match state.local_wireexpr_to_expr(&queryable.key_expr) {
                                    Ok(qablname) => {
                                        qablname.intersects(&key_expr)
                                    }
                                    Err(err) => {
                                        error!(
                                            "{}. Internal error (queryable key_expr to key_expr failed).",
                                            err
                                        );
                                        false
                                    }
                                }
                        )
                        .map(|(id, qable)| (*id, qable.callback.clone()))
                        .collect::<Vec<(u32, Arc<dyn Fn(Query) + Send + Sync>)>>();
                    (
                        state.primitives.as_ref().unwrap().clone(),
                        key_expr.into_owned(),
                        queryables,
                    )
                }
                Err(err) => {
                    error!("Received Query for unkown key_expr: {}", err);
                    return;
                }
            }
        };

        let zid = self.runtime.zid();

        let query_inner = Arc::new(QueryInner {
            key_expr,
            parameters: parameters.to_owned().into(),
            qid,
            zid: zid.into(),
            primitives: if local {
                Arc::new(self.clone())
            } else {
                primitives
            },
        });
        for (eid, callback) in queryables {
            callback(Query {
                inner: query_inner.clone(),
                eid,
                value: body.as_ref().map(|b| Value {
                    payload: b.payload.clone().into(),
                    encoding: b.encoding.clone().into(),
                }),
                attachment: attachment.clone(),
            });
        }
    }
}

impl<'s> SessionDeclarations<'s, 'static> for Arc<Session> {
    /// Create a [`Subscriber`](Subscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The resourkey expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .await
    ///     .unwrap();
    /// tokio::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received: {:?}", sample);
    ///     }
    /// }).await;
    /// # }
    /// ```
    fn declare_subscriber<'b, TryIntoKeyExpr>(
        &'s self,
        key_expr: TryIntoKeyExpr,
    ) -> SubscriberBuilder<'static, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SubscriberBuilder {
            session: SessionRef::Shared(self.clone()),
            key_expr: key_expr.try_into().map_err(Into::into),
            reliability: Reliability::DEFAULT,
            origin: Locality::default(),
            handler: DefaultHandler::default(),
        }
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
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let queryable = session.declare_queryable("key/expression")
    ///     .await
    ///     .unwrap();
    /// tokio::task::spawn(async move {
    ///     while let Ok(query) = queryable.recv_async().await {
    ///         query.reply(
    ///             "key/expression",
    ///             "value",
    ///         ).await.unwrap();
    ///     }
    /// }).await;
    /// # }
    /// ```
    fn declare_queryable<'b, TryIntoKeyExpr>(
        &'s self,
        key_expr: TryIntoKeyExpr,
    ) -> QueryableBuilder<'static, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        QueryableBuilder {
            session: SessionRef::Shared(self.clone()),
            key_expr: key_expr.try_into().map_err(Into::into),
            complete: false,
            origin: Locality::default(),
            handler: DefaultHandler::default(),
        }
    }

    /// Create a [`Publisher`](crate::publisher::Publisher) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching resources to write
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// publisher.put("value").await.unwrap();
    /// # }
    /// ```
    fn declare_publisher<'b, TryIntoKeyExpr>(
        &'s self,
        key_expr: TryIntoKeyExpr,
    ) -> PublisherBuilder<'static, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PublisherBuilder {
            session: SessionRef::Shared(self.clone()),
            key_expr: key_expr.try_into().map_err(Into::into),
            congestion_control: CongestionControl::DEFAULT,
            priority: Priority::DEFAULT,
            is_express: false,
            destination: Locality::default(),
        }
    }

    /// Obtain a [`Liveliness`] struct tied to this Zenoh [`Session`].
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    fn liveliness(&'s self) -> Liveliness<'static> {
        Liveliness {
            session: SessionRef::Shared(self.clone()),
        }
    }

    fn info(&'s self) -> SessionInfo<'static> {
        SessionInfo {
            session: SessionRef::Shared(self.clone()),
        }
    }
}

impl Primitives for Session {
    fn send_interest(&self, msg: zenoh_protocol::network::Interest) {
        trace!("recv Interest {} {:?}", msg.id, msg.wire_expr);
    }
    fn send_declare(&self, msg: zenoh_protocol::network::Declare) {
        match msg.body {
            zenoh_protocol::network::DeclareBody::DeclareKeyExpr(m) => {
                trace!("recv DeclareKeyExpr {} {:?}", m.id, m.wire_expr);
                let state = &mut zwrite!(self.state);
                match state.remote_key_to_expr(&m.wire_expr) {
                    Ok(key_expr) => {
                        let mut subs = Vec::new();
                        for sub in state.subscribers.values() {
                            if key_expr.intersects(&sub.key_expr) {
                                subs.push(sub.clone());
                            }
                        }
                        let res = Resource::Node(ResourceNode {
                            key_expr: key_expr.into(),
                            subscribers: subs,
                        });

                        state.remote_resources.insert(m.id, res);
                    }
                    Err(e) => error!(
                        "Received Resource for invalid wire_expr `{}`: {}",
                        m.wire_expr, e
                    ),
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareKeyExpr(m) => {
                trace!("recv UndeclareKeyExpr {}", m.id);
            }
            zenoh_protocol::network::DeclareBody::DeclareSubscriber(m) => {
                trace!("recv DeclareSubscriber {} {:?}", m.id, m.wire_expr);
                #[cfg(feature = "unstable")]
                {
                    let mut state = zwrite!(self.state);
                    match state
                        .wireexpr_to_keyexpr(&m.wire_expr, false)
                        .map(|e| e.into_owned())
                    {
                        Ok(expr) => {
                            state.remote_subscribers.insert(m.id, expr.clone());
                            self.update_status_up(&state, &expr);

                            if expr
                                .as_str()
                                .starts_with(crate::api::liveliness::PREFIX_LIVELINESS)
                            {
                                drop(state);
                                self.handle_data(false, &m.wire_expr, None, ZBuf::default(), None);
                            }
                        }
                        Err(err) => {
                            tracing::error!(
                                "Received DeclareSubscriber for unkown wire_expr: {}",
                                err
                            )
                        }
                    }
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareSubscriber(m) => {
                trace!("recv UndeclareSubscriber {:?}", m.id);
                #[cfg(feature = "unstable")]
                {
                    let mut state = zwrite!(self.state);
                    if let Some(expr) = state.remote_subscribers.remove(&m.id) {
                        self.update_status_down(&state, &expr);

                        if expr
                            .as_str()
                            .starts_with(crate::api::liveliness::PREFIX_LIVELINESS)
                        {
                            drop(state);
                            let data_info = DataInfo {
                                kind: SampleKind::Delete,
                                ..Default::default()
                            };
                            self.handle_data(
                                false,
                                &expr.to_wire(self),
                                Some(data_info),
                                ZBuf::default(),
                                None,
                            );
                        }
                    } else {
                        tracing::error!("Received Undeclare Subscriber for unkown id: {}", m.id);
                    }
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareQueryable(m) => {
                trace!("recv DeclareQueryable {} {:?}", m.id, m.wire_expr);
            }
            zenoh_protocol::network::DeclareBody::UndeclareQueryable(m) => {
                trace!("recv UndeclareQueryable {:?}", m.id);
            }
            DeclareBody::DeclareToken(m) => {
                trace!("recv DeclareToken {:?}", m.id);
            }
            DeclareBody::UndeclareToken(m) => {
                trace!("recv UndeclareToken {:?}", m.id);
            }
            DeclareBody::DeclareFinal(_) => {
                trace!("recv DeclareFinal {:?}", msg.interest_id);
            }
        }
    }

    fn send_push(&self, msg: Push) {
        trace!("recv Push {:?}", msg);
        match msg.payload {
            PushBody::Put(m) => {
                let info = DataInfo {
                    kind: SampleKind::Put,
                    encoding: Some(m.encoding.into()),
                    timestamp: m.timestamp,
                    qos: QoS::from(msg.ext_qos),
                    source_id: m.ext_sinfo.as_ref().map(|i| i.id),
                    source_sn: m.ext_sinfo.as_ref().map(|i| i.sn as u64),
                };
                self.handle_data(
                    false,
                    &msg.wire_expr,
                    Some(info),
                    m.payload,
                    m.ext_attachment.map(Into::into),
                )
            }
            PushBody::Del(m) => {
                let info = DataInfo {
                    kind: SampleKind::Delete,
                    encoding: None,
                    timestamp: m.timestamp,
                    qos: QoS::from(msg.ext_qos),
                    source_id: m.ext_sinfo.as_ref().map(|i| i.id),
                    source_sn: m.ext_sinfo.as_ref().map(|i| i.sn as u64),
                };
                self.handle_data(
                    false,
                    &msg.wire_expr,
                    Some(info),
                    ZBuf::empty(),
                    m.ext_attachment.map(Into::into),
                )
            }
        }
    }

    fn send_request(&self, msg: Request) {
        trace!("recv Request {:?}", msg);
        match msg.payload {
            RequestBody::Query(m) => self.handle_query(
                false,
                &msg.wire_expr,
                &m.parameters,
                msg.id,
                msg.ext_target,
                m.consolidation,
                m.ext_body,
                m.ext_attachment.map(Into::into),
            ),
        }
    }

    fn send_response(&self, msg: Response) {
        trace!("recv Response {:?}", msg);
        match msg.payload {
            ResponseBody::Err(e) => {
                let mut state = zwrite!(self.state);
                match state.queries.get_mut(&msg.rid) {
                    Some(query) => {
                        let callback = query.callback.clone();
                        std::mem::drop(state);
                        let value = Value {
                            payload: e.payload.into(),
                            encoding: e.encoding.into(),
                        };
                        let replier_id = match e.ext_sinfo {
                            Some(info) => info.id.zid,
                            None => zenoh_protocol::core::ZenohIdProto::rand(),
                        };
                        let new_reply = Reply {
                            replier_id,
                            result: Err(value.into()),
                        };
                        callback(new_reply);
                    }
                    None => {
                        tracing::warn!("Received ReplyData for unkown Query: {}", msg.rid);
                    }
                }
            }
            ResponseBody::Reply(m) => {
                let mut state = zwrite!(self.state);
                let key_expr = match state.remote_key_to_expr(&msg.wire_expr) {
                    Ok(key) => key.into_owned(),
                    Err(e) => {
                        error!("Received ReplyData for unkown key_expr: {}", e);
                        return;
                    }
                };
                match state.queries.get_mut(&msg.rid) {
                    Some(query) => {
                        let c =
                            zcondfeat!("unstable", !query.parameters.reply_key_expr_any(), true);
                        if c && !query.key_expr.intersects(&key_expr) {
                            tracing::warn!(
                                "Received Reply for `{}` from `{:?}, which didn't match query `{}`: dropping Reply.",
                                key_expr,
                                msg.ext_respid,
                                query.selector()
                            );
                            return;
                        }
                        let key_expr = match &query.scope {
                            Some(scope) => {
                                if !key_expr.starts_with(&***scope) {
                                    tracing::warn!(
                                        "Received Reply for `{}` from `{:?}, which didn't start with scope `{}`: dropping Reply.",
                                        key_expr,
                                        msg.ext_respid,
                                        scope,
                                    );
                                    return;
                                }
                                match KeyExpr::try_from(&key_expr[(scope.len() + 1)..]) {
                                    Ok(key_expr) => key_expr,
                                    Err(e) => {
                                        tracing::warn!(
                                            "Error unscoping received Reply for `{}` from `{:?}: {}",
                                            key_expr,
                                            msg.ext_respid,
                                            e,
                                        );
                                        return;
                                    }
                                }
                            }
                            None => key_expr,
                        };

                        struct Ret {
                            payload: ZBuf,
                            info: DataInfo,
                            attachment: Option<ZBytes>,
                        }
                        let Ret {
                            payload,
                            info,
                            attachment,
                        } = match m.payload {
                            ReplyBody::Put(Put {
                                timestamp,
                                encoding,
                                ext_sinfo,
                                ext_attachment: _attachment,
                                payload,
                                ..
                            }) => Ret {
                                payload,
                                info: DataInfo {
                                    kind: SampleKind::Put,
                                    encoding: Some(encoding.into()),
                                    timestamp,
                                    qos: QoS::from(msg.ext_qos),
                                    source_id: ext_sinfo.as_ref().map(|i| i.id),
                                    source_sn: ext_sinfo.as_ref().map(|i| i.sn as u64),
                                },
                                attachment: _attachment.map(Into::into),
                            },
                            ReplyBody::Del(Del {
                                timestamp,
                                ext_sinfo,
                                ext_attachment: _attachment,
                                ..
                            }) => Ret {
                                payload: ZBuf::empty(),
                                info: DataInfo {
                                    kind: SampleKind::Delete,
                                    encoding: None,
                                    timestamp,
                                    qos: QoS::from(msg.ext_qos),
                                    source_id: ext_sinfo.as_ref().map(|i| i.id),
                                    source_sn: ext_sinfo.as_ref().map(|i| i.sn as u64),
                                },
                                attachment: _attachment.map(Into::into),
                            },
                        };
                        let sample = info.into_sample(key_expr.into_owned(), payload, attachment);
                        let new_reply = Reply {
                            result: Ok(sample),
                            replier_id: zenoh_protocol::core::ZenohIdProto::rand(), // TODO
                        };
                        let callback =
                            match query.reception_mode {
                                ConsolidationMode::None => {
                                    Some((query.callback.clone(), new_reply))
                                }
                                ConsolidationMode::Monotonic => {
                                    match query.replies.as_ref().unwrap().get(
                                        new_reply.result.as_ref().unwrap().key_expr.as_keyexpr(),
                                    ) {
                                        Some(reply) => {
                                            if new_reply.result.as_ref().unwrap().timestamp
                                                > reply.result.as_ref().unwrap().timestamp
                                            {
                                                query.replies.as_mut().unwrap().insert(
                                                    new_reply
                                                        .result
                                                        .as_ref()
                                                        .unwrap()
                                                        .key_expr
                                                        .clone()
                                                        .into(),
                                                    new_reply.clone(),
                                                );
                                                Some((query.callback.clone(), new_reply))
                                            } else {
                                                None
                                            }
                                        }
                                        None => {
                                            query.replies.as_mut().unwrap().insert(
                                                new_reply
                                                    .result
                                                    .as_ref()
                                                    .unwrap()
                                                    .key_expr
                                                    .clone()
                                                    .into(),
                                                new_reply.clone(),
                                            );
                                            Some((query.callback.clone(), new_reply))
                                        }
                                    }
                                }
                                Consolidation::Auto | ConsolidationMode::Latest => {
                                    match query.replies.as_ref().unwrap().get(
                                        new_reply.result.as_ref().unwrap().key_expr.as_keyexpr(),
                                    ) {
                                        Some(reply) => {
                                            if new_reply.result.as_ref().unwrap().timestamp
                                                > reply.result.as_ref().unwrap().timestamp
                                            {
                                                query.replies.as_mut().unwrap().insert(
                                                    new_reply
                                                        .result
                                                        .as_ref()
                                                        .unwrap()
                                                        .key_expr
                                                        .clone()
                                                        .into(),
                                                    new_reply,
                                                );
                                            }
                                        }
                                        None => {
                                            query.replies.as_mut().unwrap().insert(
                                                new_reply
                                                    .result
                                                    .as_ref()
                                                    .unwrap()
                                                    .key_expr
                                                    .clone()
                                                    .into(),
                                                new_reply,
                                            );
                                        }
                                    };
                                    None
                                }
                            };
                        std::mem::drop(state);
                        if let Some((callback, new_reply)) = callback {
                            callback(new_reply);
                        }
                    }
                    None => {
                        tracing::warn!("Received ReplyData for unkown Query: {}", msg.rid);
                    }
                }
            }
        }
    }

    fn send_response_final(&self, msg: ResponseFinal) {
        trace!("recv ResponseFinal {:?}", msg);
        let mut state = zwrite!(self.state);
        match state.queries.get_mut(&msg.rid) {
            Some(query) => {
                query.nb_final -= 1;
                if query.nb_final == 0 {
                    let query = state.queries.remove(&msg.rid).unwrap();
                    std::mem::drop(state);
                    if query.reception_mode == ConsolidationMode::Latest {
                        for (_, reply) in query.replies.unwrap().into_iter() {
                            (query.callback)(reply);
                        }
                    }
                    trace!("Close query {}", msg.rid);
                }
            }
            None => {
                warn!("Received ResponseFinal for unkown Request: {}", msg.rid);
            }
        }
    }

    fn send_close(&self) {
        trace!("recv Close");
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.close_on_drop {
            let _ = self.clone().close().wait();
        }
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Session").field("id", &self.zid()).finish()
    }
}

/// Functions to create zenoh entities
///
/// This trait contains functions to create zenoh entities like
/// [`Subscriber`](crate::subscriber::Subscriber), and
/// [`Queryable`](crate::queryable::Queryable)
///
/// This trait is implemented by [`Session`](crate::session::Session) itself and
/// by wrappers [`SessionRef`](crate::session::SessionRef) and [`Arc<Session>`](crate::session::Arc<Session>)
///
/// # Examples
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
/// let subscriber = session.declare_subscriber("key/expression")
///     .await
///     .unwrap();
/// tokio::task::spawn(async move {
///     while let Ok(sample) = subscriber.recv_async().await {
///         println!("Received: {:?}", sample);
///     }
/// }).await;
/// # }
/// ```
pub trait SessionDeclarations<'s, 'a> {
    /// Create a [`Subscriber`](crate::subscriber::Subscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The resourkey expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .await
    ///     .unwrap();
    /// tokio::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received: {:?}", sample);
    ///     }
    /// }).await;
    /// # }
    /// ```
    fn declare_subscriber<'b, TryIntoKeyExpr>(
        &'s self,
        key_expr: TryIntoKeyExpr,
    ) -> SubscriberBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>;

    /// Create a [`Queryable`](crate::queryable::Queryable) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching the queries the
    /// [`Queryable`](crate::queryable::Queryable) will reply to
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let queryable = session.declare_queryable("key/expression")
    ///     .await
    ///     .unwrap();
    /// tokio::task::spawn(async move {
    ///     while let Ok(query) = queryable.recv_async().await {
    ///         query.reply(
    ///             "key/expression",
    ///             "value",
    ///         ).await.unwrap();
    ///     }
    /// }).await;
    /// # }
    /// ```
    fn declare_queryable<'b, TryIntoKeyExpr>(
        &'s self,
        key_expr: TryIntoKeyExpr,
    ) -> QueryableBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>;

    /// Create a [`Publisher`](crate::publisher::Publisher) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching resources to write
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// publisher.put("value").await.unwrap();
    /// # }
    /// ```
    fn declare_publisher<'b, TryIntoKeyExpr>(
        &'s self,
        key_expr: TryIntoKeyExpr,
    ) -> PublisherBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>;

    /// Obtain a [`Liveliness`] struct tied to this Zenoh [`Session`].
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap().into_arc();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    fn liveliness(&'s self) -> Liveliness<'a>;
    /// Get informations about the zenoh [`Session`](Session).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use zenoh::prelude::*;
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let info = session.info();
    /// # }
    /// ```
    fn info(&'s self) -> SessionInfo<'a>;
}

impl crate::net::primitives::EPrimitives for Session {
    #[inline]
    fn send_interest(&self, ctx: crate::net::routing::RoutingContext<Interest>) {
        (self as &dyn Primitives).send_interest(ctx.msg)
    }

    #[inline]
    fn send_declare(&self, ctx: crate::net::routing::RoutingContext<Declare>) {
        (self as &dyn Primitives).send_declare(ctx.msg)
    }

    #[inline]
    fn send_push(&self, msg: Push) {
        (self as &dyn Primitives).send_push(msg)
    }

    #[inline]
    fn send_request(&self, ctx: crate::net::routing::RoutingContext<Request>) {
        (self as &dyn Primitives).send_request(ctx.msg)
    }

    #[inline]
    fn send_response(&self, ctx: crate::net::routing::RoutingContext<Response>) {
        (self as &dyn Primitives).send_response(ctx.msg)
    }

    #[inline]
    fn send_response_final(&self, ctx: crate::net::routing::RoutingContext<ResponseFinal>) {
        (self as &dyn Primitives).send_response_final(ctx.msg)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Open a zenoh [`Session`].
///
/// # Arguments
///
/// * `config` - The [`Config`] for the zenoh session
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// # }
/// ```
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use std::str::FromStr;
/// use zenoh::{info::ZenohId, prelude::*};
///
/// let mut config = zenoh::config::peer();
/// config.set_id(ZenohId::from_str("221b72df20924c15b8794c6bdb471150").unwrap());
/// config.connect.endpoints.extend("tcp/10.10.10.10:7447,tcp/11.11.11.11:7447".split(',').map(|s|s.parse().unwrap()));
///
/// let session = zenoh::open(config).await.unwrap();
/// # }
/// ```
pub fn open<TryIntoConfig>(config: TryIntoConfig) -> OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    OpenBuilder {
        config,
        #[cfg(feature = "shared-memory")]
        shm_clients: None,
    }
}

/// A builder returned by [`open`] used to open a zenoh [`Session`].
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::prelude::*;
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// # }
/// ```
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub struct OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    config: TryIntoConfig,
    #[cfg(feature = "shared-memory")]
    shm_clients: Option<Arc<ShmClientStorage>>,
}

#[cfg(feature = "shared-memory")]
impl<TryIntoConfig> OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    pub fn with_shm_clients(mut self, shm_clients: Arc<ShmClientStorage>) -> Self {
        self.shm_clients = Some(shm_clients);
        self
    }
}

impl<TryIntoConfig> Resolvable for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type To = ZResult<Session>;
}

impl<TryIntoConfig> Wait for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    fn wait(self) -> <Self as Resolvable>::To {
        let config: crate::config::Config = self
            .config
            .try_into()
            .map_err(|e| zerror!("Invalid Zenoh configuration {:?}", &e))?;
        Session::new(
            config,
            #[cfg(feature = "shared-memory")]
            self.shm_clients,
        )
        .wait()
    }
}

impl<TryIntoConfig> IntoFuture for OpenBuilder<TryIntoConfig>
where
    TryIntoConfig: std::convert::TryInto<crate::config::Config> + Send + 'static,
    <TryIntoConfig as std::convert::TryInto<crate::config::Config>>::Error: std::fmt::Debug,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

/// Initialize a Session with an existing Runtime.
/// This operation is used by the plugins to share the same Runtime as the router.
#[zenoh_macros::internal]
pub fn init(runtime: Runtime) -> InitBuilder {
    InitBuilder {
        runtime,
        aggregated_subscribers: vec![],
        aggregated_publishers: vec![],
    }
}

/// A builder returned by [`init`] and used to initialize a Session with an existing Runtime.
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
#[doc(hidden)]
#[zenoh_macros::internal]
pub struct InitBuilder {
    runtime: Runtime,
    aggregated_subscribers: Vec<OwnedKeyExpr>,
    aggregated_publishers: Vec<OwnedKeyExpr>,
}

#[zenoh_macros::internal]
impl InitBuilder {
    #[inline]
    pub fn aggregated_subscribers(mut self, exprs: Vec<OwnedKeyExpr>) -> Self {
        self.aggregated_subscribers = exprs;
        self
    }

    #[inline]
    pub fn aggregated_publishers(mut self, exprs: Vec<OwnedKeyExpr>) -> Self {
        self.aggregated_publishers = exprs;
        self
    }
}

#[zenoh_macros::internal]
impl Resolvable for InitBuilder {
    type To = ZResult<Session>;
}

#[zenoh_macros::internal]
impl Wait for InitBuilder {
    fn wait(self) -> <Self as Resolvable>::To {
        Ok(Session::init(
            self.runtime,
            self.aggregated_subscribers,
            self.aggregated_publishers,
        )
        .wait())
    }
}

#[zenoh_macros::internal]
impl IntoFuture for InitBuilder {
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}
