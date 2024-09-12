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
    convert::TryInto,
    fmt,
    future::{IntoFuture, Ready},
    ops::Deref,
    sync::{
        atomic::{AtomicU16, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use tracing::{error, info, trace, warn};
use uhlc::{Timestamp, HLC};
use zenoh_buffers::ZBuf;
use zenoh_collections::SingleOrVec;
use zenoh_config::{unwrap_or_default, wrappers::ZenohId, Config, Notifier};
use zenoh_core::{zconfigurable, zread, Resolvable, Resolve, ResolveClosure, ResolveFuture, Wait};
#[cfg(feature = "unstable")]
use zenoh_protocol::network::{
    declare::{DeclareToken, SubscriberId, TokenId, UndeclareToken},
    ext,
    interest::InterestId,
};
use zenoh_protocol::{
    core::{
        key_expr::{keyexpr, OwnedKeyExpr},
        AtomicExprId, CongestionControl, EntityId, ExprId, Parameters, Reliability, WireExpr,
        EMPTY_EXPR_ID,
    },
    network::{
        self,
        declare::{
            self, common::ext::WireExprType, queryable::ext::QueryableInfoType, Declare,
            DeclareBody, DeclareKeyExpr, DeclareQueryable, DeclareSubscriber, UndeclareQueryable,
            UndeclareSubscriber,
        },
        interest::{InterestMode, InterestOptions},
        request::{self, ext::TargetType},
        AtomicRequestId, DeclareFinal, Interest, Mapping, Push, Request, RequestId, Response,
        ResponseFinal,
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
    subscriber::{SubscriberBuilder, SubscriberKind, SubscriberState},
    value::Value,
    Id,
};
#[cfg(feature = "unstable")]
use super::{
    liveliness::{Liveliness, LivelinessTokenState},
    publisher::Publisher,
    publisher::{MatchingListenerState, MatchingStatus},
    query::LivelinessQueryState,
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
    #[cfg(feature = "unstable")]
    pub(crate) liveliness_qid_counter: AtomicRequestId,
    pub(crate) local_resources: HashMap<ExprId, Resource>,
    pub(crate) remote_resources: HashMap<ExprId, Resource>,
    #[cfg(feature = "unstable")]
    pub(crate) remote_subscribers: HashMap<SubscriberId, KeyExpr<'static>>,
    pub(crate) publishers: HashMap<Id, PublisherState>,
    #[cfg(feature = "unstable")]
    pub(crate) remote_tokens: HashMap<TokenId, KeyExpr<'static>>,
    //pub(crate) publications: Vec<OwnedKeyExpr>,
    pub(crate) subscribers: HashMap<Id, Arc<SubscriberState>>,
    pub(crate) liveliness_subscribers: HashMap<Id, Arc<SubscriberState>>,
    pub(crate) queryables: HashMap<Id, Arc<QueryableState>>,
    #[cfg(feature = "unstable")]
    pub(crate) tokens: HashMap<Id, Arc<LivelinessTokenState>>,
    #[cfg(feature = "unstable")]
    pub(crate) matching_listeners: HashMap<Id, Arc<MatchingListenerState>>,
    pub(crate) queries: HashMap<RequestId, QueryState>,
    #[cfg(feature = "unstable")]
    pub(crate) liveliness_queries: HashMap<InterestId, LivelinessQueryState>,
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
            #[cfg(feature = "unstable")]
            liveliness_qid_counter: AtomicRequestId::new(0),
            local_resources: HashMap::new(),
            remote_resources: HashMap::new(),
            #[cfg(feature = "unstable")]
            remote_subscribers: HashMap::new(),
            publishers: HashMap::new(),
            #[cfg(feature = "unstable")]
            remote_tokens: HashMap::new(),
            //publications: Vec::new(),
            subscribers: HashMap::new(),
            liveliness_subscribers: HashMap::new(),
            queryables: HashMap::new(),
            #[cfg(feature = "unstable")]
            tokens: HashMap::new(),
            #[cfg(feature = "unstable")]
            matching_listeners: HashMap::new(),
            queries: HashMap::new(),
            #[cfg(feature = "unstable")]
            liveliness_queries: HashMap::new(),
            aggregated_subscribers,
            aggregated_publishers,
        }
    }
}

impl SessionState {
    #[inline]
    pub(crate) fn primitives(&self) -> ZResult<Arc<Face>> {
        self.primitives
            .as_ref()
            .cloned()
            .ok_or_else(|| zerror!("session closed").into())
    }

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

    pub(crate) fn subscribers(&self, kind: SubscriberKind) -> &HashMap<Id, Arc<SubscriberState>> {
        match kind {
            SubscriberKind::Subscriber => &self.subscribers,
            SubscriberKind::LivelinessSubscriber => &self.liveliness_subscribers,
        }
    }

    pub(crate) fn subscribers_mut(
        &mut self,
        kind: SubscriberKind,
    ) -> &mut HashMap<Id, Arc<SubscriberState>> {
        match kind {
            SubscriberKind::Subscriber => &mut self.subscribers,
            SubscriberKind::LivelinessSubscriber => &mut self.liveliness_subscribers,
        }
    }
}

impl fmt::Debug for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "SessionState{{ subscribers: {}, liveliness_subscribers: {} }}",
            self.subscribers.len(),
            self.liveliness_subscribers.len()
        )
    }
}

pub(crate) struct ResourceNode {
    pub(crate) key_expr: OwnedKeyExpr,
    pub(crate) subscribers: Vec<Arc<SubscriberState>>,
    pub(crate) liveliness_subscribers: Vec<Arc<SubscriberState>>,
}

impl ResourceNode {
    pub(crate) fn new(key_expr: OwnedKeyExpr) -> Self {
        Self {
            key_expr,
            subscribers: Vec::new(),
            liveliness_subscribers: Vec::new(),
        }
    }

    pub(crate) fn subscribers(&self, kind: SubscriberKind) -> &Vec<Arc<SubscriberState>> {
        match kind {
            SubscriberKind::Subscriber => &self.subscribers,
            SubscriberKind::LivelinessSubscriber => &self.liveliness_subscribers,
        }
    }

    pub(crate) fn subscribers_mut(
        &mut self,
        kind: SubscriberKind,
    ) -> &mut Vec<Arc<SubscriberState>> {
        match kind {
            SubscriberKind::Subscriber => &mut self.subscribers,
            SubscriberKind::LivelinessSubscriber => &mut self.liveliness_subscribers,
        }
    }
}

pub(crate) enum Resource {
    Prefix { prefix: Box<str> },
    Node(ResourceNode),
}

impl Resource {
    pub(crate) fn new(name: Box<str>) -> Self {
        if keyexpr::new(name.as_ref()).is_ok() {
            Self::for_keyexpr(unsafe { OwnedKeyExpr::from_boxed_str_unchecked(name) })
        } else {
            Self::Prefix { prefix: name }
        }
    }
    pub(crate) fn for_keyexpr(key_expr: OwnedKeyExpr) -> Self {
        Self::Node(ResourceNode::new(key_expr))
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

/// A trait implemented by types that can be undeclared.
pub trait UndeclarableSealed<S> {
    type Undeclaration: Resolve<ZResult<()>> + Send;
    fn undeclare_inner(self, session: S) -> Self::Undeclaration;
}

impl<'a, T> UndeclarableSealed<&'a Session> for T
where
    T: UndeclarableSealed<()>,
{
    type Undeclaration = <T as UndeclarableSealed<()>>::Undeclaration;

    fn undeclare_inner(self, _session: &'a Session) -> Self::Undeclaration {
        self.undeclare_inner(())
    }
}

// NOTE: `UndeclarableInner` is only pub(crate) to hide the `undeclare_inner` method. So we don't
// care about the `private_bounds` lint in this particular case.
#[allow(private_bounds)]
/// A trait implemented by types that can be undeclared.
pub trait Undeclarable<S = ()>: UndeclarableSealed<S> {}

impl<T, S> Undeclarable<S> for T where T: UndeclarableSealed<S> {}

pub(crate) struct SessionInner {
    /// See [`WeakSession`] doc
    weak_counter: Mutex<usize>,
    pub(crate) runtime: Runtime,
    pub(crate) state: RwLock<SessionState>,
    pub(crate) id: u16,
    owns_runtime: bool,
    task_controller: TaskController,
}

impl fmt::Debug for SessionInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Session").field("id", &self.zid()).finish()
    }
}

/// The entrypoint of the zenoh API.
///
/// Zenoh session is instantiated using [`zenoh::open`](crate::open) and it can be used to declare various
/// entities like publishers, subscribers, or querybables, as well as issuing queries.
///
/// Session is an `Arc`-like type, it can be cloned, and it is closed when the last instance
/// is dropped (see [`Session::close`]).
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// session.put("key/expression", "value").await.unwrap();
/// # }
pub struct Session(pub(crate) Arc<SessionInner>);

impl Session {
    pub(crate) fn downgrade(&self) -> WeakSession {
        WeakSession::new(&self.0)
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Clone for Session {
    fn clone(&self) -> Self {
        let _weak = self.0.weak_counter.lock().unwrap();
        Self(self.0.clone())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let weak = self.0.weak_counter.lock().unwrap();
        if Arc::strong_count(&self.0) == *weak + /* the `Arc` currently dropped */ 1 {
            drop(weak);
            if let Err(error) = self.close().wait() {
                tracing::error!(error)
            }
        }
    }
}

/// `WeakSession` provides a weak-like semantic to the arc-like session, without using [`Weak`].
/// It allows notably to establish reference cycles inside the session, for the primitive
/// implementation.
/// When all `Session` instance are dropped, [`Session::close`] is be called and cleans
/// the reference cycles, allowing the underlying `Arc` to be properly reclaimed.
///
/// The pseudo-weak algorithm relies on a counter wrapped in a mutex. It was indeed the simplest
/// to implement it, because atomic manipulations to achieve this semantic would not have been
/// trivial at all — what could happen if a pseudo-weak is cloned while the last session instance
/// is dropped? With a mutex, it's simple, and it works perfectly fine, as we don't care about the
/// performance penalty when it comes to session entities cloning/dropping.
///
/// (Although it was planed to be used initially, `Weak` was in fact causing errors in the session
/// closing, because the primitive implementation seemed to be used in the closing operation.)
pub(crate) struct WeakSession(Arc<SessionInner>);

impl WeakSession {
    fn new(session: &Arc<SessionInner>) -> Self {
        let mut weak = session.weak_counter.lock().unwrap();
        *weak += 1;
        Self(session.clone())
    }
}

impl Clone for WeakSession {
    fn clone(&self) -> Self {
        let mut weak = self.0.weak_counter.lock().unwrap();
        *weak += 1;
        Self(self.0.clone())
    }
}

impl fmt::Debug for WeakSession {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for WeakSession {
    type Target = Arc<SessionInner>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for WeakSession {
    fn drop(&mut self) {
        let mut weak = self.0.weak_counter.lock().unwrap();
        *weak -= 1;
    }
}

static SESSION_ID_COUNTER: AtomicU16 = AtomicU16::new(0);
impl Session {
    pub(crate) fn init(
        runtime: Runtime,
        aggregated_subscribers: Vec<OwnedKeyExpr>,
        aggregated_publishers: Vec<OwnedKeyExpr>,
        owns_runtime: bool,
    ) -> impl Resolve<Session> {
        ResolveClosure::new(move || {
            let router = runtime.router();
            let state = RwLock::new(SessionState::new(
                aggregated_subscribers,
                aggregated_publishers,
            ));
            let session = Session(Arc::new(SessionInner {
                weak_counter: Mutex::new(0),
                runtime: runtime.clone(),
                state,
                id: SESSION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
                owns_runtime,
                task_controller: TaskController::default(),
            }));

            runtime.new_handler(Arc::new(admin::Handler::new(session.downgrade())));

            let primitives = Some(router.new_primitives(Arc::new(session.downgrade())));
            zwrite!(session.0.state).primitives = primitives;

            admin::init(session.downgrade());

            session
        })
    }

    /// Returns the identifier of the current session. `zid()` is a convenient shortcut.
    /// See [`Session::info()`](`Session::info()`) and [`SessionInfo::zid()`](`SessionInfo::zid()`) for more details.
    pub fn zid(&self) -> ZenohId {
        self.info().zid().wait()
    }

    pub fn hlc(&self) -> Option<&HLC> {
        self.0.runtime.hlc()
    }

    /// Close the zenoh [`Session`](Session).
    ///
    /// Every subscriber and queryable declared will stop receiving data, and further attempt to
    /// publish or query with the session or publishers will result in an error. Undeclaring an
    /// entity after session closing is a no-op. Session state can be checked with
    /// [`Session::is_closed`].
    ///
    /// Session are automatically closed when all its instances are dropped, same as `Arc`.
    /// You may still want to use this function to handle errors or close the session
    /// asynchronously.
    /// <br>
    /// Closing the session can also save bandwidth, as it avoids propagating the undeclaration
    /// of the remaining entities.
    ///
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber("key/expression")
    ///     .await
    ///     .unwrap();
    /// let subscriber_task = tokio::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received: {} {:?}", sample.key_expr(), sample.payload());
    ///     }
    /// });
    /// session.close().await.unwrap();
    /// // subscriber task will end as `subscriber.recv_async()` will return `Err`
    /// // subscriber undeclaration has not been sent on the wire
    /// subscriber_task.await.unwrap();
    /// # }
    /// ```
    pub fn close(&self) -> impl Resolve<ZResult<()>> + '_ {
        self.0.close()
    }

    /// Check if the session has been closed.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// assert!(!session.is_closed());
    /// session.close().await.unwrap();
    /// assert!(session.is_closed());
    /// # }
    pub fn is_closed(&self) -> bool {
        zread!(self.0.state).primitives.is_none()
    }

    pub fn undeclare<'a, T>(&'a self, decl: T) -> impl Resolve<ZResult<()>> + 'a
    where
        T: Undeclarable<&'a Session> + 'a,
    {
        UndeclarableSealed::undeclare_inner(decl, self)
    }

    /// Get the current configuration of the zenoh [`Session`](Session).
    ///
    /// The returned configuration [`Notifier`](Notifier) can be used to read the current
    /// zenoh configuration through the `get` function or
    /// modify the zenoh configuration through the `insert`,
    /// or `insert_json5` function.
    ///
    /// # Examples
    /// ### Read current zenoh configuration
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
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
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let _ = session.config().insert_json5("connect/endpoints", r#"["tcp/127.0.0.1/7447"]"#);
    /// # }
    /// ```
    pub fn config(&self) -> &Notifier<Config> {
        self.0.runtime.config()
    }

    /// Get a new Timestamp from a Zenoh session [`Session`](Session).
    ///
    /// The returned timestamp has the current time, with the Session's runtime ZenohID
    ///
    /// # Examples
    /// ### Read current zenoh configuration
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let timestamp = session.new_timestamp();
    /// # }
    /// ```
    pub fn new_timestamp(&self) -> Timestamp {
        match self.hlc() {
            Some(hlc) => hlc.new_timestamp(),
            None => {
                // Called in the case that the runtime is not initialized with an hlc
                // UNIX_EPOCH is Returns a Timespec::zero(), Unwrap Should be permissable here
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().into();
                Timestamp::new(now, self.0.zid().into())
            }
        }
    }

    /// Wrap the session into an `Arc`.
    #[deprecated(since = "1.0.0", note = "use `Session` directly instead")]
    pub fn into_arc(self) -> Arc<Session> {
        Arc::new(self)
    }
}

impl Session {
    /// Get information about the zenoh [`Session`](Session).
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let info = session.info();
    /// # }
    /// ```
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            runtime: self.0.runtime.clone(),
        }
    }

    /// Create a [`Subscriber`](crate::pubsub::Subscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The resourkey expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
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
    pub fn declare_subscriber<'b, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> SubscriberBuilder<'_, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SubscriberBuilder {
            session: self,
            key_expr: TryIntoKeyExpr::try_into(key_expr).map_err(Into::into),
            origin: Locality::default(),
            handler: DefaultHandler::default(),
            undeclare_on_drop: true,
        }
    }

    /// Create a [`Queryable`](crate::query::Queryable) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching the queries the
    ///   [`Queryable`](crate::query::Queryable) will reply to
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
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
    pub fn declare_queryable<'b, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> QueryableBuilder<'_, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        QueryableBuilder {
            session: self,
            key_expr: key_expr.try_into().map_err(Into::into),
            complete: false,
            origin: Locality::default(),
            handler: DefaultHandler::default(),
            undeclare_on_drop: true,
        }
    }

    /// Create a [`Publisher`](crate::pubsub::Publisher) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching resources to write
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .await
    ///     .unwrap();
    /// publisher.put("value").await.unwrap();
    /// # }
    /// ```
    pub fn declare_publisher<'b, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> PublisherBuilder<'_, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PublisherBuilder {
            session: self,
            key_expr: key_expr.try_into().map_err(Into::into),
            encoding: Encoding::default(),
            congestion_control: CongestionControl::DEFAULT,
            priority: Priority::DEFAULT,
            is_express: false,
            #[cfg(feature = "unstable")]
            reliability: Reliability::DEFAULT,
            destination: Locality::default(),
        }
    }

    /// Obtain a [`Liveliness`] struct tied to this Zenoh [`Session`].
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn liveliness(&self) -> Liveliness<'_> {
        Liveliness { session: self }
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
        let sid = self.0.id;
        ResolveClosure::new(move || {
            let key_expr: KeyExpr = key_expr?;
            let prefix_len = key_expr.len() as u32;
            let expr_id = self.0.declare_prefix(key_expr.as_str()).wait()?;
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
    /// use zenoh::bytes::Encoding;
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
    /// Unless explicitly requested via [`accept_replies`](crate::session::SessionGetBuilder::accept_replies), replies are guaranteed to have
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
            let conf = self.0.runtime.config().lock();
            Duration::from_millis(unwrap_or_default!(conf.queries_default_timeout()))
        };
        let qos: QoS = request::ext::QoSType::REQUEST.into();
        SessionGetBuilder {
            session: self,
            selector,
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

            let session = Self::init(
                runtime.clone(),
                aggregated_subscribers,
                aggregated_publishers,
                true,
            )
            .await;
            runtime.start().await?;
            Ok(session)
        })
    }
}
impl SessionInner {
    pub fn zid(&self) -> ZenohId {
        self.runtime.zid()
    }

    fn close(&self) -> impl Resolve<ZResult<()>> + '_ {
        ResolveFuture::new(async move {
            let Some(primitives) = zwrite!(self.state).primitives.take() else {
                return Ok(());
            };
            if self.owns_runtime {
                info!(zid = %self.zid(), "close session");
            }
            self.task_controller.terminate_all(Duration::from_secs(10));
            if self.owns_runtime {
                self.runtime.close().await?;
            } else {
                primitives.send_close();
            }
            let mut state = zwrite!(self.state);
            state.queryables.clear();
            state.subscribers.clear();
            state.liveliness_subscribers.clear();
            state.local_resources.clear();
            state.remote_resources.clear();
            #[cfg(feature = "unstable")]
            {
                state.tokens.clear();
                state.matching_listeners.clear();
            }
            Ok(())
        })
    }

    pub(crate) fn declare_prefix<'a>(
        &'a self,
        prefix: &'a str,
    ) -> impl Resolve<ZResult<ExprId>> + 'a {
        ResolveClosure::new(move || {
            trace!("declare_prefix({:?})", prefix);
            let mut state = zwrite!(self.state);
            let primitives = state.primitives()?;
            match state
                .local_resources
                .iter()
                .find(|(_expr_id, res)| res.name() == prefix)
            {
                Some((expr_id, _res)) => Ok(*expr_id),
                None => {
                    let expr_id = state.expr_id_counter.fetch_add(1, Ordering::SeqCst);
                    let mut res = Resource::new(Box::from(prefix));
                    if let Resource::Node(res_node) = &mut res {
                        for kind in [
                            SubscriberKind::Subscriber,
                            SubscriberKind::LivelinessSubscriber,
                        ] {
                            for sub in state.subscribers(kind).values() {
                                if res_node.key_expr.intersects(&sub.key_expr) {
                                    res_node.subscribers_mut(kind).push(sub.clone());
                                }
                            }
                        }
                    }
                    state.local_resources.insert(expr_id, res);
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
                    Ok(expr_id)
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
            let primitives = state.primitives()?;
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
        let Ok(primitives) = state.primitives() else {
            return Ok(());
        };
        if let Some(pub_state) = state.publishers.remove(&pid) {
            trace!("undeclare_publisher({:?})", pub_state);
            if pub_state.destination != Locality::SessionLocal {
                // Note: there might be several publishers on the same KeyExpr.
                // Before calling forget_publishers(key_expr), check if this was the last one.
                if !state.publishers.values().any(|p| {
                    p.destination != Locality::SessionLocal && p.remote_id == pub_state.remote_id
                }) {
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
        self: &Arc<Self>,
        key_expr: &KeyExpr,
        origin: Locality,
        callback: Callback<'static, Sample>,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        tracing::trace!("declare_subscriber({:?})", key_expr);
        let id = self.runtime.next_id();

        let mut sub_state = SubscriberState {
            id,
            remote_id: id,
            key_expr: key_expr.clone().into_owned(),
            origin,
            callback,
        };

        let declared_sub = origin != Locality::SessionLocal;

        let declared_sub = declared_sub
            .then(|| {
                match state
                    .aggregated_subscribers
                    .iter()
                    .find(|s| s.includes(key_expr))
                {
                    Some(join_sub) => {
                        if let Some(joined_sub) = state
                            .subscribers(SubscriberKind::Subscriber)
                            .values()
                            .find(|s| {
                                s.origin != Locality::SessionLocal && join_sub.includes(&s.key_expr)
                            })
                        {
                            sub_state.remote_id = joined_sub.remote_id;
                            None
                        } else {
                            Some(join_sub.clone().into())
                        }
                    }
                    None => {
                        if let Some(twin_sub) = state
                            .subscribers(SubscriberKind::Subscriber)
                            .values()
                            .find(|s| s.origin != Locality::SessionLocal && s.key_expr == *key_expr)
                        {
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

        state
            .subscribers_mut(SubscriberKind::Subscriber)
            .insert(sub_state.id, sub_state.clone());
        for res in state
            .local_resources
            .values_mut()
            .filter_map(Resource::as_node_mut)
        {
            if key_expr.intersects(&res.key_expr) {
                res.subscribers_mut(SubscriberKind::Subscriber)
                    .push(sub_state.clone());
            }
        }
        for res in state
            .remote_resources
            .values_mut()
            .filter_map(Resource::as_node_mut)
        {
            if key_expr.intersects(&res.key_expr) {
                res.subscribers_mut(SubscriberKind::Subscriber)
                    .push(sub_state.clone());
            }
        }

        if let Some(key_expr) = declared_sub {
            let primitives = state.primitives()?;
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
                }),
            });

            #[cfg(feature = "unstable")]
            {
                let state = zread!(self.state);
                self.update_status_up(&state, &key_expr)
            }
        }

        Ok(sub_state)
    }

    pub(crate) fn undeclare_subscriber_inner(
        self: &Arc<Self>,
        sid: Id,
        kind: SubscriberKind,
    ) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        let Ok(primitives) = state.primitives() else {
            return Ok(());
        };
        if let Some(sub_state) = state.subscribers_mut(kind).remove(&sid) {
            trace!("undeclare_subscriber({:?})", sub_state);
            for res in state
                .local_resources
                .values_mut()
                .filter_map(Resource::as_node_mut)
            {
                res.subscribers_mut(kind)
                    .retain(|sub| sub.id != sub_state.id);
            }
            for res in state
                .remote_resources
                .values_mut()
                .filter_map(Resource::as_node_mut)
            {
                res.subscribers_mut(kind)
                    .retain(|sub| sub.id != sub_state.id);
            }

            if sub_state.origin != Locality::SessionLocal && kind == SubscriberKind::Subscriber {
                // Note: there might be several Subscribers on the same KeyExpr.
                // Before calling forget_subscriber(key_expr), check if this was the last one.
                if !state.subscribers(kind).values().any(|s| {
                    s.origin != Locality::SessionLocal && s.remote_id == sub_state.remote_id
                }) {
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
                if kind == SubscriberKind::LivelinessSubscriber {
                    let primitives = state.primitives()?;
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
            let primitives = state.primitives()?;
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
        let Ok(primitives) = state.primitives() else {
            return Ok(());
        };
        if let Some(qable_state) = state.queryables.remove(&qid) {
            trace!("undeclare_queryable({:?})", qable_state);
            if qable_state.origin != Locality::SessionLocal {
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
        let tok_state = Arc::new(LivelinessTokenState {
            id,
            key_expr: key_expr.clone().into_owned(),
        });

        state.tokens.insert(tok_state.id, tok_state.clone());
        let primitives = state.primitives()?;
        drop(state);
        primitives.send_declare(Declare {
            interest_id: None,
            ext_qos: declare::ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
            body: DeclareBody::DeclareToken(DeclareToken {
                id,
                wire_expr: key_expr.to_wire(self).to_owned(),
            }),
        });
        Ok(tok_state)
    }

    #[cfg(feature = "unstable")]
    pub(crate) fn declare_liveliness_subscriber_inner(
        &self,
        key_expr: &KeyExpr,
        origin: Locality,
        history: bool,
        callback: Callback<'static, Sample>,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        trace!("declare_liveliness_subscriber({:?})", key_expr);
        let id = self.runtime.next_id();

        let sub_state = SubscriberState {
            id,
            remote_id: id,
            key_expr: key_expr.clone().into_owned(),
            origin,
            callback,
        };

        let sub_state = Arc::new(sub_state);

        state
            .subscribers_mut(SubscriberKind::LivelinessSubscriber)
            .insert(sub_state.id, sub_state.clone());

        for res in state
            .local_resources
            .values_mut()
            .filter_map(Resource::as_node_mut)
        {
            if key_expr.intersects(&res.key_expr) {
                res.subscribers_mut(SubscriberKind::LivelinessSubscriber)
                    .push(sub_state.clone());
            }
        }

        for res in state
            .remote_resources
            .values_mut()
            .filter_map(Resource::as_node_mut)
        {
            if key_expr.intersects(&res.key_expr) {
                res.subscribers_mut(SubscriberKind::LivelinessSubscriber)
                    .push(sub_state.clone());
            }
        }

        let primitives = state.primitives()?;
        drop(state);

        primitives.send_interest(Interest {
            id,
            mode: if history {
                InterestMode::CurrentFuture
            } else {
                InterestMode::Future
            },
            options: InterestOptions::KEYEXPRS + InterestOptions::TOKENS,
            wire_expr: Some(key_expr.to_wire(self).to_owned()),
            ext_qos: declare::ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
        });

        Ok(sub_state)
    }

    #[zenoh_macros::unstable]
    pub(crate) fn undeclare_liveliness(&self, tid: Id) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        let Ok(primitives) = state.primitives() else {
            return Ok(());
        };
        if let Some(tok_state) = state.tokens.remove(&tid) {
            trace!("undeclare_liveliness({:?})", tok_state);
            // Note: there might be several Tokens on the same KeyExpr.
            let key_expr = &tok_state.key_expr;
            let twin_tok = state.tokens.values().any(|s| s.key_expr == *key_expr);
            if !twin_tok {
                drop(state);
                primitives.send_declare(Declare {
                    interest_id: None,
                    ext_qos: ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareToken(UndeclareToken {
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
    pub(crate) fn update_status_up(self: &Arc<Self>, state: &SessionState, key_expr: &KeyExpr) {
        for msub in state.matching_listeners.values() {
            if key_expr.intersects(&msub.key_expr) {
                // Cannot hold session lock when calling tables (matching_status())
                // TODO: check which ZRuntime should be used
                self.task_controller
                    .spawn_with_rt(zenoh_runtime::ZRuntime::Net, {
                        let session = WeakSession::new(self);
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
    pub(crate) fn update_status_down(self: &Arc<Self>, state: &SessionState, key_expr: &KeyExpr) {
        for msub in state.matching_listeners.values() {
            if key_expr.intersects(&msub.key_expr) {
                // Cannot hold session lock when calling tables (matching_status())
                // TODO: check which ZRuntime should be used
                self.task_controller
                    .spawn_with_rt(zenoh_runtime::ZRuntime::Net, {
                        let session = WeakSession::new(self);
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
        if state.primitives.is_none() {
            return Ok(());
        }
        if let Some(state) = state.matching_listeners.remove(&sid) {
            trace!("undeclare_matches_listener_inner({:?})", state);
            Ok(())
        } else {
            Err(zerror!("Unable to find MatchingListener").into())
        }
    }

    #[allow(clippy::too_many_arguments)] // TODO fixme
    pub(crate) fn execute_subscriber_callbacks(
        &self,
        local: bool,
        key_expr: &WireExpr,
        info: Option<DataInfo>,
        payload: ZBuf,
        kind: SubscriberKind,
        #[cfg(feature = "unstable")] reliability: Reliability,
        attachment: Option<ZBytes>,
    ) {
        let mut callbacks = SingleOrVec::default();
        let state = zread!(self.state);
        if key_expr.suffix.is_empty() {
            match state.get_res(&key_expr.scope, key_expr.mapping, local) {
                Some(Resource::Node(res)) => {
                    for sub in res.subscribers(kind) {
                        if sub.origin == Locality::Any
                            || (local == (sub.origin == Locality::SessionLocal))
                        {
                            callbacks.push((sub.callback.clone(), res.key_expr.clone().into()));
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
                    for sub in state.subscribers(kind).values() {
                        if (sub.origin == Locality::Any
                            || (local == (sub.origin == Locality::SessionLocal)))
                            && key_expr.intersects(&sub.key_expr)
                        {
                            callbacks.push((sub.callback.clone(), key_expr.clone().into_owned()));
                        }
                    }
                }
                Err(err) => {
                    tracing::error!("Received Data for unknown key_expr: {}", err);
                    return;
                }
            }
        };
        drop(state);
        let zenoh_collections::single_or_vec::IntoIter { drain, last } = callbacks.into_iter();
        for (cb, key_expr) in drain {
            let sample = info.clone().into_sample(
                key_expr,
                payload.clone(),
                #[cfg(feature = "unstable")]
                reliability,
                attachment.clone(),
            );
            cb(sample);
        }
        if let Some((cb, key_expr)) = last {
            let sample = info.into_sample(
                key_expr,
                payload,
                #[cfg(feature = "unstable")]
                reliability,
                attachment.clone(),
            );
            cb(sample);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn query(
        self: &Arc<Self>,
        key_expr: &KeyExpr<'_>,
        parameters: &Parameters<'_>,
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
                let session = WeakSession::new(self);
                #[cfg(feature = "unstable")]
                let zid = self.zid();
                async move {
                    tokio::select! {
                        _ = tokio::time::sleep(timeout) => {
                            let mut state = zwrite!(session.state);
                            if let Some(query) = state.queries.remove(&qid) {
                                std::mem::drop(state);
                                tracing::debug!("Timeout on query {}! Send error and close.", qid);
                                if query.reception_mode == ConsolidationMode::Latest {
                                    for (_, reply) in query.replies.unwrap().into_iter() {
                                        (query.callback)(reply);
                                    }
                                }
                                (query.callback)(Reply {
                                    result: Err(Value::new("Timeout", Encoding::ZENOH_STRING).into()),
                                    #[cfg(feature = "unstable")]
                                    replier_id: Some(zid.into()),
                                });
                            }
                        }
                        _ = token.cancelled() => {}
                    }
                }
            });

        tracing::trace!("Register query {} (nb_final = {})", qid, nb_final);
        let wexpr = key_expr.to_wire(self).to_owned();
        state.queries.insert(
            qid,
            QueryState {
                nb_final,
                key_expr: key_expr.clone().into_owned(),
                parameters: parameters.clone().into_owned(),
                reception_mode: consolidation,
                replies: (consolidation != ConsolidationMode::None).then(HashMap::new),
                callback,
            },
        );

        let primitives = state.primitives()?;
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

    #[cfg(feature = "unstable")]
    pub(crate) fn liveliness_query(
        self: &Arc<Self>,
        key_expr: &KeyExpr<'_>,
        timeout: Duration,
        callback: Callback<'static, Reply>,
    ) -> ZResult<()> {
        tracing::trace!("liveliness.get({}, {:?})", key_expr, timeout);
        let mut state = zwrite!(self.state);
        let id = state.liveliness_qid_counter.fetch_add(1, Ordering::SeqCst);
        let token = self.task_controller.get_cancellation_token();
        self.task_controller
            .spawn_with_rt(zenoh_runtime::ZRuntime::Net, {
                let session = WeakSession::new(self);
                let zid = self.zid();
                async move {
                    tokio::select! {
                        _ = tokio::time::sleep(timeout) => {
                            let mut state = zwrite!(session.state);
                            if let Some(query) = state.liveliness_queries.remove(&id) {
                                std::mem::drop(state);
                                tracing::debug!("Timeout on liveliness query {}! Send error and close.", id);
                                (query.callback)(Reply {
                                    result: Err(Value::new("Timeout", Encoding::ZENOH_STRING).into()),
                                    #[cfg(feature = "unstable")]
                                    replier_id: Some(zid.into()),
                                });
                            }
                        }
                        _ = token.cancelled() => {}
                    }
                }
            });

        tracing::trace!("Register liveliness query {}", id);
        let wexpr = key_expr.to_wire(self).to_owned();
        state
            .liveliness_queries
            .insert(id, LivelinessQueryState { callback });

        let primitives = state.primitives()?;
        drop(state);

        primitives.send_interest(Interest {
            id,
            mode: InterestMode::Current,
            options: InterestOptions::KEYEXPRS + InterestOptions::TOKENS,
            wire_expr: Some(wexpr.clone()),
            ext_qos: request::ext::QoSType::DEFAULT,
            ext_tstamp: None,
            ext_nodeid: request::ext::NodeIdType::DEFAULT,
        });

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn handle_query(
        self: &Arc<Self>,
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
            let Ok(primitives) = state.primitives() else {
                return;
            };
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
                    (primitives, key_expr.into_owned(), queryables)
                }
                Err(err) => {
                    error!("Received Query for unknown key_expr: {}", err);
                    return;
                }
            }
        };

        let zid = self.zid();

        let query_inner = Arc::new(QueryInner {
            key_expr,
            parameters: parameters.to_owned().into(),
            qid,
            zid: zid.into(),
            primitives: if local {
                Arc::new(WeakSession::new(self))
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

impl Primitives for WeakSession {
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
                        let mut res_node = ResourceNode::new(key_expr.clone().into());
                        for kind in [
                            SubscriberKind::Subscriber,
                            SubscriberKind::LivelinessSubscriber,
                        ] {
                            for sub in state.subscribers(kind).values() {
                                if key_expr.intersects(&sub.key_expr) {
                                    res_node.subscribers_mut(kind).push(sub.clone());
                                }
                            }
                        }

                        state
                            .remote_resources
                            .insert(m.id, Resource::Node(res_node));
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
                        }
                        Err(err) => {
                            tracing::error!(
                                "Received DeclareSubscriber for unknown wire_expr: {}",
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
                    } else {
                        tracing::error!("Received Undeclare Subscriber for unknown id: {}", m.id);
                    }
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareQueryable(m) => {
                trace!("recv DeclareQueryable {} {:?}", m.id, m.wire_expr);
            }
            zenoh_protocol::network::DeclareBody::UndeclareQueryable(m) => {
                trace!("recv UndeclareQueryable {:?}", m.id);
            }
            zenoh_protocol::network::DeclareBody::DeclareToken(m) => {
                trace!("recv DeclareToken {:?}", m.id);
                #[cfg(feature = "unstable")]
                {
                    let mut state = zwrite!(self.state);
                    match state
                        .wireexpr_to_keyexpr(&m.wire_expr, false)
                        .map(|e| e.into_owned())
                    {
                        Ok(key_expr) => {
                            if let Some(interest_id) = msg.interest_id {
                                if let Some(query) = state.liveliness_queries.get(&interest_id) {
                                    let reply = Reply {
                                        result: Ok(Sample {
                                            key_expr,
                                            payload: ZBytes::empty(),
                                            kind: SampleKind::Put,
                                            encoding: Encoding::default(),
                                            timestamp: None,
                                            qos: QoS::default(),
                                            #[cfg(feature = "unstable")]
                                            reliability: Reliability::Reliable,
                                            #[cfg(feature = "unstable")]
                                            source_info: SourceInfo::empty(),
                                            #[cfg(feature = "unstable")]
                                            attachment: None,
                                        }),
                                        #[cfg(feature = "unstable")]
                                        replier_id: None,
                                    };

                                    (query.callback)(reply);
                                }
                            } else {
                                state.remote_tokens.insert(m.id, key_expr.clone());

                                drop(state);

                                self.execute_subscriber_callbacks(
                                    false,
                                    &m.wire_expr,
                                    None,
                                    ZBuf::default(),
                                    SubscriberKind::LivelinessSubscriber,
                                    #[cfg(feature = "unstable")]
                                    Reliability::Reliable,
                                    #[cfg(feature = "unstable")]
                                    None,
                                );
                            }
                        }
                        Err(err) => {
                            tracing::error!("Received DeclareToken for unknown wire_expr: {}", err)
                        }
                    }
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareToken(m) => {
                trace!("recv UndeclareToken {:?}", m.id);
                #[cfg(feature = "unstable")]
                {
                    let mut state = zwrite!(self.state);
                    if let Some(key_expr) = state.remote_tokens.remove(&m.id) {
                        drop(state);

                        let data_info = DataInfo {
                            kind: SampleKind::Delete,
                            ..Default::default()
                        };

                        self.execute_subscriber_callbacks(
                            false,
                            &key_expr.to_wire(self),
                            Some(data_info),
                            ZBuf::default(),
                            SubscriberKind::LivelinessSubscriber,
                            #[cfg(feature = "unstable")]
                            Reliability::Reliable,
                            #[cfg(feature = "unstable")]
                            None,
                        );
                    } else if m.ext_wire_expr.wire_expr != WireExpr::empty() {
                        match state
                            .wireexpr_to_keyexpr(&m.ext_wire_expr.wire_expr, false)
                            .map(|e| e.into_owned())
                        {
                            Ok(key_expr) => {
                                drop(state);

                                let data_info = DataInfo {
                                    kind: SampleKind::Delete,
                                    ..Default::default()
                                };

                                self.execute_subscriber_callbacks(
                                    false,
                                    &key_expr.to_wire(self),
                                    Some(data_info),
                                    ZBuf::default(),
                                    SubscriberKind::LivelinessSubscriber,
                                    #[cfg(feature = "unstable")]
                                    Reliability::Reliable,
                                    #[cfg(feature = "unstable")]
                                    None,
                                );
                            }
                            Err(err) => {
                                tracing::error!(
                                    "Received UndeclareToken for unknown wire_expr: {}",
                                    err
                                )
                            }
                        }
                    }
                }
            }
            DeclareBody::DeclareFinal(DeclareFinal) => {
                trace!("recv DeclareFinal {:?}", msg.interest_id);

                #[cfg(feature = "unstable")]
                if let Some(interest_id) = msg.interest_id {
                    let mut state = zwrite!(self.state);
                    let _ = state.liveliness_queries.remove(&interest_id);
                }
            }
        }
    }

    fn send_push(&self, msg: Push, _reliability: Reliability) {
        trace!("recv Push {:?}", msg);
        match msg.payload {
            PushBody::Put(m) => {
                let info = DataInfo {
                    kind: SampleKind::Put,
                    encoding: Some(m.encoding.into()),
                    timestamp: m.timestamp,
                    qos: QoS::from(msg.ext_qos),
                    source_id: m.ext_sinfo.as_ref().map(|i| i.id.into()),
                    source_sn: m.ext_sinfo.as_ref().map(|i| i.sn as u64),
                };
                self.execute_subscriber_callbacks(
                    false,
                    &msg.wire_expr,
                    Some(info),
                    m.payload,
                    SubscriberKind::Subscriber,
                    #[cfg(feature = "unstable")]
                    _reliability,
                    m.ext_attachment.map(Into::into),
                )
            }
            PushBody::Del(m) => {
                let info = DataInfo {
                    kind: SampleKind::Delete,
                    encoding: None,
                    timestamp: m.timestamp,
                    qos: QoS::from(msg.ext_qos),
                    source_id: m.ext_sinfo.as_ref().map(|i| i.id.into()),
                    source_sn: m.ext_sinfo.as_ref().map(|i| i.sn as u64),
                };
                self.execute_subscriber_callbacks(
                    false,
                    &msg.wire_expr,
                    Some(info),
                    ZBuf::empty(),
                    SubscriberKind::Subscriber,
                    #[cfg(feature = "unstable")]
                    _reliability,
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
                        let new_reply = Reply {
                            result: Err(value.into()),
                            #[cfg(feature = "unstable")]
                            replier_id: e.ext_sinfo.map(|info| info.id.zid),
                        };
                        callback(new_reply);
                    }
                    None => {
                        tracing::warn!("Received ReplyData for unknown Query: {}", msg.rid);
                    }
                }
            }
            ResponseBody::Reply(m) => {
                let mut state = zwrite!(self.state);
                let key_expr = match state.remote_key_to_expr(&msg.wire_expr) {
                    Ok(key) => key.into_owned(),
                    Err(e) => {
                        error!("Received ReplyData for unknown key_expr: {}", e);
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
                                    source_id: ext_sinfo.as_ref().map(|i| i.id.into()),
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
                                    source_id: ext_sinfo.as_ref().map(|i| i.id.into()),
                                    source_sn: ext_sinfo.as_ref().map(|i| i.sn as u64),
                                },
                                attachment: _attachment.map(Into::into),
                            },
                        };
                        let sample = info.into_sample(
                            key_expr.into_owned(),
                            payload,
                            #[cfg(feature = "unstable")]
                            Reliability::Reliable,
                            attachment,
                        );
                        let new_reply = Reply {
                            result: Ok(sample),
                            #[cfg(feature = "unstable")]
                            replier_id: None,
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
                        tracing::warn!("Received ReplyData for unknown Query: {}", msg.rid);
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
                warn!("Received ResponseFinal for unknown Request: {}", msg.rid);
            }
        }
    }

    fn send_close(&self) {
        trace!("recv Close");
    }
}

impl crate::net::primitives::EPrimitives for WeakSession {
    #[inline]
    fn send_interest(&self, ctx: crate::net::routing::RoutingContext<Interest>) {
        (self as &dyn Primitives).send_interest(ctx.msg)
    }

    #[inline]
    fn send_declare(&self, ctx: crate::net::routing::RoutingContext<Declare>) {
        (self as &dyn Primitives).send_declare(ctx.msg)
    }

    #[inline]
    fn send_push(&self, msg: Push, reliability: Reliability) {
        (self as &dyn Primitives).send_push(msg, reliability)
    }

    #[inline]
    fn send_request(&self, msg: Request) {
        (self as &dyn Primitives).send_request(msg)
    }

    #[inline]
    fn send_response(&self, msg: Response) {
        (self as &dyn Primitives).send_response(msg)
    }

    #[inline]
    fn send_response_final(&self, msg: ResponseFinal) {
        (self as &dyn Primitives).send_response_final(msg)
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
///
/// let session = zenoh::open(zenoh::config::peer()).await.unwrap();
/// # }
/// ```
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use std::str::FromStr;
/// use zenoh::session::ZenohId;
///
/// let mut config = zenoh::config::peer();
/// config.set_id(ZenohId::from_str("221b72df20924c15b8794c6bdb471150").unwrap());
/// config.connect.endpoints.set(
///     ["tcp/10.10.10.10:7447", "tcp/11.11.11.11:7447"].iter().map(|s|s.parse().unwrap()).collect());
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
            false,
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
