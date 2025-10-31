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
    collections::{hash_map::Entry, HashMap},
    convert::TryInto,
    fmt, mem,
    ops::Deref,
    sync::{atomic::Ordering, Arc, Mutex, RwLock, RwLockReadGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use once_cell::sync::OnceCell;
#[zenoh_macros::internal]
use ref_cast::ref_cast_custom;
use ref_cast::RefCastCustom;
use tracing::{error, info, trace, warn};
use uhlc::Timestamp;
#[cfg(feature = "internal")]
use uhlc::HLC;
use zenoh_collections::{IntHashMap, SingleOrVec};
use zenoh_config::{
    qos::{PublisherQoSConfList, PublisherQoSConfig},
    wrappers::ZenohId,
};
#[cfg(feature = "unstable")]
use zenoh_config::{wrappers::EntityGlobalId, GenericConfig};
use zenoh_core::{zconfigurable, zread, Resolve, ResolveClosure, ResolveFuture, Wait};
use zenoh_keyexpr::keyexpr_tree::KeBoxTree;
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
            DeclareBody, DeclareKeyExpr, DeclareQueryable, DeclareSubscriber, DeclareToken,
            SubscriberId, TokenId, UndeclareQueryable, UndeclareSubscriber, UndeclareToken,
        },
        ext,
        interest::{InterestId, InterestMode, InterestOptions},
        push, request, AtomicRequestId, DeclareFinal, Interest, Mapping, Push, Request, RequestId,
        Response, ResponseFinal,
    },
    zenoh::{
        query::{self, ext::QueryBodyType},
        Del, PushBody, Put, RequestBody, ResponseBody,
    },
};
use zenoh_result::ZResult;
#[cfg(feature = "shared-memory")]
use zenoh_shm::api::client_storage::ShmClientStorage;
use zenoh_task::TaskController;

use super::builders::close::{CloseBuilder, Closeable, Closee};
#[cfg(feature = "unstable")]
use crate::api::{query::ReplyKeyExpr, sample::SourceInfo, selector::ZenohParameters};
#[cfg(feature = "internal")]
use crate::net::runtime::Runtime;
#[cfg(all(feature = "shared-memory", feature = "unstable"))]
use crate::net::runtime::ShmProviderState;
use crate::{
    api::{
        admin,
        builders::{
            publisher::{
                PublicationBuilderDelete, PublicationBuilderPut, PublisherBuilder,
                SessionDeleteBuilder, SessionPutBuilder,
            },
            querier::QuerierBuilder,
            query::SessionGetBuilder,
            queryable::QueryableBuilder,
            session::OpenBuilder,
            subscriber::SubscriberBuilder,
        },
        bytes::ZBytes,
        encoding::Encoding,
        handlers::{Callback, DefaultHandler},
        info::SessionInfo,
        key_expr::{KeyExpr, KeyExprInner},
        liveliness::Liveliness,
        matching::{MatchingListenerState, MatchingStatus, MatchingStatusType},
        publisher::{Priority, PublisherState},
        querier::QuerierState,
        query::{
            ConsolidationMode, LivelinessQueryState, QueryConsolidation, QueryState, QueryTarget,
            Reply,
        },
        queryable::{Query, QueryInner, QueryableState},
        sample::{Locality, QoS, Sample, SampleKind},
        selector::Selector,
        subscriber::{SubscriberKind, SubscriberState},
        Id,
    },
    net::{
        primitives::Primitives,
        runtime::{GenericRuntime, RuntimeBuilder},
    },
    query::ReplyError,
    Config,
};

zconfigurable! {
    pub(crate) static ref API_DATA_RECEPTION_CHANNEL_SIZE: usize = 256;
    pub(crate) static ref API_QUERY_RECEPTION_CHANNEL_SIZE: usize = 256;
    pub(crate) static ref API_REPLY_EMISSION_CHANNEL_SIZE: usize = 256;
    pub(crate) static ref API_REPLY_RECEPTION_CHANNEL_SIZE: usize = 256;
}

pub(crate) struct SessionState {
    pub(crate) primitives: Option<Arc<dyn Primitives>>, // @TODO replace with MaybeUninit ??
    pub(crate) expr_id_counter: AtomicExprId,           // @TODO: manage rollover and uniqueness
    pub(crate) qid_counter: AtomicRequestId,
    pub(crate) local_resources: IntHashMap<ExprId, LocalResource>,
    pub(crate) remote_resources: IntHashMap<ExprId, Resource>,
    pub(crate) remote_subscribers: HashMap<SubscriberId, KeyExpr<'static>>,
    pub(crate) publishers: HashMap<Id, PublisherState>,
    pub(crate) queriers: HashMap<Id, QuerierState>,
    pub(crate) remote_tokens: HashMap<TokenId, KeyExpr<'static>>,
    //pub(crate) publications: Vec<OwnedKeyExpr>,
    pub(crate) subscribers: HashMap<Id, Arc<SubscriberState>>,
    pub(crate) liveliness_subscribers: HashMap<Id, Arc<SubscriberState>>,
    pub(crate) queryables: HashMap<Id, Arc<QueryableState>>,
    pub(crate) remote_queryables: HashMap<Id, (KeyExpr<'static>, bool)>,
    pub(crate) matching_listeners: HashMap<Id, Arc<MatchingListenerState>>,
    pub(crate) queries: HashMap<RequestId, QueryState>,
    pub(crate) liveliness_queries: HashMap<InterestId, LivelinessQueryState>,
    pub(crate) aggregated_subscribers: Vec<OwnedKeyExpr>,
    pub(crate) aggregated_publishers: Vec<OwnedKeyExpr>,
    pub(crate) publisher_qos_tree: KeBoxTree<PublisherQoSConfig>,
}

impl SessionState {
    pub(crate) fn new(
        aggregated_subscribers: Vec<OwnedKeyExpr>,
        aggregated_publishers: Vec<OwnedKeyExpr>,
        publisher_qos_tree: KeBoxTree<PublisherQoSConfig>,
    ) -> SessionState {
        SessionState {
            primitives: None,
            expr_id_counter: AtomicExprId::new(1), // Note: start at 1 because 0 is reserved for NO_RESOURCE
            qid_counter: AtomicRequestId::new(0),
            local_resources: IntHashMap::new(),
            remote_resources: IntHashMap::new(),
            remote_subscribers: HashMap::new(),
            publishers: HashMap::new(),
            queriers: HashMap::new(),
            remote_tokens: HashMap::new(),
            //publications: Vec::new(),
            subscribers: HashMap::new(),
            liveliness_subscribers: HashMap::new(),
            queryables: HashMap::new(),
            remote_queryables: HashMap::new(),
            matching_listeners: HashMap::new(),
            queries: HashMap::new(),
            liveliness_queries: HashMap::new(),
            aggregated_subscribers,
            aggregated_publishers,
            publisher_qos_tree,
        }
    }
}

impl SessionState {
    #[inline]
    pub(crate) fn primitives(&self) -> ZResult<Arc<dyn Primitives>> {
        self.primitives
            .as_ref()
            .cloned()
            .ok_or_else(|| SessionClosedError.into())
    }

    #[inline]
    fn get_local_res(&self, id: &ExprId) -> Option<&Resource> {
        Some(&self.local_resources.get(id)?.resource)
    }

    #[inline]
    fn get_remote_res(&self, id: &ExprId, mapping: Mapping) -> Option<&Resource> {
        match mapping {
            Mapping::Receiver => Some(&self.local_resources.get(id)?.resource),
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

    fn register_querier<'a>(
        &mut self,
        id: EntityId,
        key_expr: &'a KeyExpr,
        destination: Locality,
    ) -> Option<KeyExpr<'a>> {
        let mut querier_state = QuerierState {
            id,
            remote_id: id,
            key_expr: key_expr.clone().into_owned(),
            destination,
        };

        let declared_querier =
            (destination != Locality::SessionLocal)
                .then(|| {
                    if let Some(twin_querier) = self.queriers.values().find(|p| {
                        p.destination != Locality::SessionLocal && &p.key_expr == key_expr
                    }) {
                        querier_state.remote_id = twin_querier.remote_id;
                        None
                    } else {
                        Some(key_expr.clone())
                    }
                })
                .flatten();
        self.queriers.insert(id, querier_state);
        declared_querier
    }

    fn register_subscriber<'a>(
        &mut self,
        id: EntityId,
        key_expr: &'a KeyExpr,
        origin: Locality,
        callback: Callback<Sample>,
    ) -> (Arc<SubscriberState>, Option<KeyExpr<'a>>) {
        let mut sub_state = SubscriberState {
            id,
            remote_id: id,
            key_expr: key_expr.clone().into_owned(),
            origin,
            callback,
            history: false,
        };

        let declared_sub = origin != Locality::SessionLocal;

        let declared_sub = declared_sub
            .then(|| {
                match self
                    .aggregated_subscribers
                    .iter()
                    .find(|s| s.includes(key_expr))
                {
                    Some(join_sub) => {
                        if let Some(joined_sub) = self
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
                        if let Some(twin_sub) = self
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

        self.subscribers_mut(SubscriberKind::Subscriber)
            .insert(sub_state.id, sub_state.clone());
        for res in self
            .local_resources
            .values_mut()
            .filter_map(LocalResource::as_node_mut)
        {
            if key_expr.intersects(&res.key_expr) {
                res.subscribers_mut(SubscriberKind::Subscriber)
                    .push(sub_state.clone());
            }
        }
        for res in self
            .remote_resources
            .values_mut()
            .filter_map(Resource::as_node_mut)
        {
            if key_expr.intersects(&res.key_expr) {
                res.subscribers_mut(SubscriberKind::Subscriber)
                    .push(sub_state.clone());
            }
        }

        (sub_state, declared_sub)
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

pub(crate) struct LocalResource {
    resource: Resource,
    declared: bool,
}

impl LocalResource {
    pub(crate) fn as_node_mut(&mut self) -> Option<&mut ResourceNode> {
        self.resource.as_node_mut()
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
    pub(crate) runtime: GenericRuntime,
    pub(crate) state: RwLock<SessionState>,
    pub(crate) id: EntityId,
    task_controller: TaskController,
    face_id: OnceCell<usize>,
}

impl fmt::Debug for SessionInner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Session").field("id", &self.zid()).finish()
    }
}

/// The [`Session`] is the main component of Zenoh. It holds the zenoh runtime object,
/// which maintains the state of the connection of the node to the Zenoh network.
///
/// The session allows declaring other zenoh entities like publishers, subscribers, queriers, queryables, etc.
/// and keeps them functioning. Closing the session will close all associated entities.
///
/// The session is cloneable so it's easy to share it between tasks and threads. Each clone of the
/// session is an `Arc` to the internal session object, so cloning is cheap and fast.
///
/// A Zenoh session is instantiated using [`zenoh::open`](crate::open)
/// with parameters specified in the [`Config`] object.
///
/// Objects created by the session ([`Publisher`](crate::pubsub::Publisher),
/// [`Subscriber`](crate::pubsub::Subscriber), [`Querier`](crate::query::Querier), etc.),
/// have lifetimes independent of the session, but they stop functioning if all clones of the session
/// object are dropped or the session is closed with the [`close`](crate::session::Session::close) method.
///
/// ### Background entities
///
/// Sometimes it is inconvenient to keep a reference to an object (for example,
/// a [`Queryable`](crate::query::Queryable)) solely to keep it alive. There is a way to
/// avoid keeping this reference and keep the object alive until the session is closed.
/// To do this, call the [`background`](crate::query::QueryableBuilder::background) method on the
/// corresponding builder. This causes the builder to return `()` instead of the object instance and
/// keeps the instance alive while the session is alive.
///
/// ### Difference between session and runtime
/// The session object holds all declared zenoh entities (publishers, subscribers, etc.) and
/// a shared reference to the runtime object which maintains the state of the zenoh node.
/// Closing the session will close all associated entities and drop the reference to the runtime.
///
/// Typically each session has its own runtime, but in some cases
/// the session may share the runtime with other sessions. This is the case for the plugins
/// where each plugin has its own session but all plugins share the same `zenohd` runtime
/// for efficiency.
/// In this case, all these sessions will have the same network identity
/// [`Session::zid`](crate::session::Session::zid).
///
/// # Examples
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// session.put("key/expression", "value").await.unwrap();
/// # }
/// ```
#[derive(RefCastCustom)]
#[repr(transparent)]
pub struct Session(pub(crate) Arc<SessionInner>);

impl Session {
    pub(crate) fn downgrade(&self) -> WeakSession {
        WeakSession::new(&self.0)
    }

    #[cfg(feature = "internal")]
    #[ref_cast_custom]
    pub(crate) const fn ref_cast(from: &Arc<SessionInner>) -> &Self;
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
/// It provides weak-like semantics to the arc-like session, without using [`Weak`].
/// Notably, it allows establishing reference cycles inside the session for the primitive
/// implementation.
/// When all `Session` instances are dropped, [`Session::close`] is called and cleans
/// the reference cycles, allowing the underlying `Arc` to be properly reclaimed.
///
/// The pseudo-weak algorithm relies on a counter wrapped in a mutex. It was indeed the simplest
/// to implement, because atomic manipulations to achieve these semantics would not have been
/// trivial at all — what could happen if a pseudo-weak is cloned while the last session instance
/// is dropped? With a mutex, it's simple, and it works perfectly fine, as we don't care about the
/// performance penalty when it comes to session entities cloning/dropping.
///
/// (Although it was planned to be used initially, `Weak` was in fact causing errors in the session
/// closing, because the primitive implementation seemed to be used in the closing operation.)
pub(crate) struct WeakSession(Arc<SessionInner>);

impl WeakSession {
    fn new(session: &Arc<SessionInner>) -> Self {
        let mut weak = session.weak_counter.lock().unwrap();
        *weak += 1;
        Self(session.clone())
    }

    #[zenoh_macros::internal]
    pub(crate) fn session(&self) -> &Session {
        Session::ref_cast(&self.0)
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

/// Error indicating the operation cannot proceed because the session is closed.
///
/// It may be returned by operations like [`Session::get`] or [`Publisher::put`](crate::api::publisher::Publisher::put) when
/// [`Session::close`] has been called before.
#[derive(Debug)]
pub struct SessionClosedError;

impl fmt::Display for SessionClosedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "session closed")
    }
}

impl std::error::Error for SessionClosedError {}

impl Session {
    pub(crate) fn init(
        runtime: GenericRuntime,
        aggregated_subscribers: Vec<OwnedKeyExpr>,
        aggregated_publishers: Vec<OwnedKeyExpr>,
    ) -> impl Resolve<Session> {
        ResolveClosure::new(move || {
            let publisher_qos = runtime
                .get_config()
                .get_typed::<PublisherQoSConfList>("qos/publication")
                .unwrap();
            let state = RwLock::new(SessionState::new(
                aggregated_subscribers,
                aggregated_publishers,
                publisher_qos.into(),
            ));
            let session = Session(Arc::new(SessionInner {
                weak_counter: Mutex::new(0),
                runtime: runtime.clone(),
                state,
                id: runtime.next_id(),
                task_controller: TaskController::default(),
                face_id: OnceCell::new(),
            }));

            runtime.new_handler(Arc::new(admin::Handler::new(session.downgrade())));
            let (_face_id, primitives) = runtime.new_primitives(Arc::new(session.downgrade()));

            zwrite!(session.0.state).primitives = Some(primitives);
            session.0.face_id.set(_face_id).unwrap(); // this is the only attempt to set value

            admin::init(session.downgrade());

            session
        })
    }

    /// Returns the identifier of the current session. `zid()` is a convenient shortcut.
    /// See [`Session::info()`](`Session::info()`) and [`SessionInfo::zid()`](`SessionInfo::zid()`) for more details.
    pub fn zid(&self) -> ZenohId {
        self.info().zid().wait()
    }

    /// Returns the [`EntityGlobalId`] of this Session.
    #[zenoh_macros::unstable]
    pub fn id(&self) -> EntityGlobalId {
        zenoh_protocol::core::EntityGlobalIdProto {
            zid: self.zid().into(),
            eid: self.0.id,
        }
        .into()
    }

    #[zenoh_macros::internal]
    pub fn hlc(&self) -> Option<&HLC> {
        self.0.runtime.hlc()
    }

    /// Close the zenoh [`Session`](Session).
    ///
    /// Every subscriber and queryable declared will stop receiving data, and further attempts to
    /// publish or query with the session or publishers will result in an error. Undeclaring an
    /// entity after session closing is a no-op. Session state can be checked with
    /// [`Session::is_closed`].
    ///
    /// Sessions are automatically closed when all their instances are dropped, same as `Arc`.
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    /// // subscriber undeclaration has not been sent over the wire
    /// subscriber_task.await.unwrap();
    /// # }
    /// ```
    pub fn close(&self) -> CloseBuilder<Self> {
        CloseBuilder::new(self)
    }

    /// Check if the session has been closed.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// assert!(!session.is_closed());
    /// session.close().await.unwrap();
    /// assert!(session.is_closed());
    /// # }
    /// ```
    pub fn is_closed(&self) -> bool {
        zread!(self.0.state).primitives.is_none()
    }

    /// Undeclare a zenoh entity declared by the session.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let keyexpr = session.declare_keyexpr("key/expression").await.unwrap();
    /// let subscriber = session
    ///     .declare_subscriber(&keyexpr)
    ///     .await
    ///     .unwrap();
    /// session.undeclare(subscriber).await.unwrap();
    /// session.undeclare(keyexpr).await.unwrap();
    /// # }
    /// ```
    pub fn undeclare<'a, T>(&'a self, decl: T) -> impl Resolve<ZResult<()>> + 'a
    where
        T: Undeclarable<&'a Session> + 'a,
    {
        UndeclarableSealed::undeclare_inner(decl, self)
    }

    /// Get the current configuration of the zenoh [`Session`](Session).
    ///
    /// The returned configuration [`Notifier`](crate::config::Notifier) can be used to read the current
    /// zenoh configuration through the `get` function or
    /// modify the zenoh configuration through the `insert`
    /// or `insert_json5` function.
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let peers = session.config().get("connect/endpoints").unwrap();
    /// # }
    /// ```
    #[zenoh_macros::unstable]
    pub fn config(&self) -> GenericConfig {
        self.0.runtime.get_config()
    }

    /// Get a new Timestamp from a Zenoh [`Session`].
    ///
    /// The returned timestamp has the current time, with the Session's runtime [`ZenohId`].
    ///
    /// # Examples
    /// ### Get a new timestamp
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let timestamp = session.new_timestamp();
    /// # }
    /// ```
    pub fn new_timestamp(&self) -> Timestamp {
        match self.0.runtime.hlc() {
            Some(hlc) => hlc.new_timestamp(),
            None => {
                // Called when the runtime is not initialized with an HLC.
                // UNIX_EPOCH returns a Timespec::zero(); unwrap should be permissible here.
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().into();
                Timestamp::new(now, self.0.zid().into())
            }
        }
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let info = session.info();
    /// # }
    /// ```
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            runtime: self.0.runtime.deref().clone(),
        }
    }

    /// Returns the [`ShmProviderState`](ShmProviderState) associated with the current [`Session`](Session)’s [`Runtime`](Runtime).
    ///
    /// Each [`Runtime`](Runtime) may create its own provider to manage internal optimizations.  
    /// This method exposes that provider so it can also be accessed at the application level.
    ///
    /// Note that the provider may not be immediately available or may be disabled via configuration.
    /// Provider initialization is concurrent and triggered by access events (both transport-internal and through this API).
    ///
    /// To use this provider, both *shared_memory* and *transport_optimization* config sections
    /// must be enabled.
    ///
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let shm_provider = session.get_shm_provider();
    /// assert!(shm_provider.into_option().is_none());
    /// std::thread::sleep(std::time::Duration::from_millis(100));
    /// let shm_provider = session.get_shm_provider();
    /// assert!(shm_provider.into_option().is_some());
    /// # }
    /// ```
    #[cfg(feature = "shared-memory")]
    #[zenoh_macros::unstable]
    pub fn get_shm_provider(&self) -> ShmProviderState {
        self.0.runtime.get_shm_provider()
    }

    /// Create a [`Subscriber`](crate::pubsub::Subscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The resource key expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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

    /// Create a [`Querier`](crate::query::Querier) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching resources to query
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let querier = session.declare_querier("key/expression")
    ///     .await
    ///     .unwrap();
    /// let replies = querier.get().await.unwrap();
    /// # }
    /// ```
    pub fn declare_querier<'b, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> QuerierBuilder<'_, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        let timeout =
            { Duration::from_millis(self.0.runtime.get_config().queries_default_timeout_ms()) };
        let qos: QoS = request::ext::QoSType::REQUEST.into();
        QuerierBuilder {
            session: self,
            key_expr: key_expr.try_into().map_err(Into::into),
            qos: qos.into(),
            destination: Locality::default(),
            target: QueryTarget::default(),
            consolidation: QueryConsolidation::default(),
            timeout,
            #[cfg(feature = "unstable")]
            accept_replies: ReplyKeyExpr::default(),
        }
    }

    /// Obtain a [`Liveliness`] struct tied to this Zenoh [`Session`].
    ///
    /// # Examples
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    ///
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .await
    ///     .unwrap();
    /// # }
    /// ```
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
        let session_id = self.0.id;
        ResolveClosure::new(move || {
            let key_expr: KeyExpr = key_expr?;
            let prefix_len = key_expr.len() as u32;
            let expr_id = self
                .0
                .declare_prefix(key_expr.as_str(), true)
                .wait()?
                .unwrap();
            let key_expr = match key_expr.0 {
                KeyExprInner::Borrowed(key_expr) | KeyExprInner::BorrowedWire { key_expr, .. } => {
                    KeyExpr(KeyExprInner::BorrowedWire {
                        key_expr,
                        expr_id,
                        mapping: Mapping::Sender,
                        prefix_len,
                        session_id,
                    })
                }
                KeyExprInner::Owned(key_expr) | KeyExprInner::Wire { key_expr, .. } => {
                    KeyExpr(KeyExprInner::Wire {
                        key_expr,
                        expr_id,
                        mapping: Mapping::Sender,
                        prefix_len,
                        session_id,
                    })
                }
            };
            Ok(key_expr)
        })
    }

    /// Publish [`SampleKind::Put`] sample directly from the session. This is a shortcut for declaring
    /// a [`Publisher`](crate::pubsub::Publisher) and calling [`put`](crate::api::publisher::Publisher::put)
    /// on it.
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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

    /// Publish a [`SampleKind::Delete`] sample directly from the session. This is a shortcut for declaring
    /// a [`Publisher`](crate::pubsub::Publisher) and calling [`delete`](crate::api::publisher::Publisher::delete) on it.
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
    /// Query data from the matching queryables in the system. This is a shortcut for declaring
    /// a [`Querier`](crate::query::Querier) and calling [`get`](crate::api::querier::Querier::get) on it.
    ///
    /// Unless explicitly requested via [`accept_replies`](crate::session::SessionGetBuilder::accept_replies),
    /// replies are guaranteed to have
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
    /// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
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
        let timeout =
            { Duration::from_millis(self.0.runtime.get_config().queries_default_timeout_ms()) };
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
            #[cfg(feature = "unstable")]
            cancellation_token: None,
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
            let aggregated_subscribers = config.0.aggregation().subscribers().clone();
            let aggregated_publishers = config.0.aggregation().publishers().clone();
            #[allow(unused_mut)] // Required for shared-memory
            let mut runtime = RuntimeBuilder::new(config);
            #[cfg(feature = "shared-memory")]
            {
                runtime = runtime.shm_clients(shm_clients);
            }
            let mut runtime = runtime.build().await?;

            let session = Self::init(
                runtime.clone().into(),
                aggregated_subscribers,
                aggregated_publishers,
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

    pub(crate) fn declare_prefix<'a>(
        &'a self,
        prefix: &'a str,
        force: bool,
    ) -> impl Resolve<ZResult<Option<ExprId>>> + 'a {
        ResolveClosure::new(move || {
            trace!("declare_prefix({:?})", prefix);
            let mut state = zwrite!(self.state);
            let primitives = state.primitives()?;
            match state
                .local_resources
                .iter()
                .find(|(_expr_id, res)| res.resource.name() == prefix)
            {
                Some((expr_id, res)) if force || res.declared => Ok(Some(*expr_id)),
                Some(_) => Ok(None),
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
                    state.local_resources.insert(
                        expr_id,
                        LocalResource {
                            resource: res,
                            declared: false,
                        },
                    );
                    drop(state);
                    primitives.send_declare(&mut Declare {
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
                    let mut state = zwrite!(self.state);
                    if let Some(res) = state.local_resources.get_mut(&expr_id) {
                        res.declared = true;
                    }
                    Ok(Some(expr_id))
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
            primitives.send_interest(&mut Interest {
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
                    primitives.send_interest(&mut Interest {
                        id: pub_state.remote_id,
                        mode: InterestMode::Final,
                        // Note: InterestMode::Final options are undefined in the current protocol specification,
                        //       they are initialized here for internal use by local egress interceptors.
                        options: InterestOptions::SUBSCRIBERS,
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

    pub(crate) fn declare_querier_inner(
        &self,
        key_expr: KeyExpr,
        destination: Locality,
    ) -> ZResult<EntityId> {
        tracing::trace!("declare_querier({:?})", key_expr);
        let mut state = zwrite!(self.state);
        let id = self.runtime.next_id();
        let declared_querier = state.register_querier(id, &key_expr, destination);
        if let Some(res) = declared_querier {
            let primitives = state.primitives()?;
            drop(state);
            primitives.send_interest(&mut Interest {
                id,
                mode: InterestMode::CurrentFuture,
                options: InterestOptions::KEYEXPRS + InterestOptions::QUERYABLES,
                wire_expr: Some(res.to_wire(self).to_owned()),
                ext_qos: network::ext::QoSType::DEFAULT,
                ext_tstamp: None,
                ext_nodeid: network::ext::NodeIdType::DEFAULT,
            });
        }
        Ok(id)
    }

    pub(crate) fn undeclare_querier_inner(&self, pid: Id) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        let Ok(primitives) = state.primitives() else {
            return Ok(());
        };
        if let Some(querier_state) = state.queriers.remove(&pid) {
            trace!("undeclare_querier({:?})", querier_state);
            if querier_state.destination != Locality::SessionLocal {
                // Note: there might be several queriers on the same KeyExpr.
                // Before calling forget_queriers(key_expr), check if this was the last one.
                if !state.queriers.values().any(|p| {
                    p.destination != Locality::SessionLocal
                        && p.remote_id == querier_state.remote_id
                }) {
                    drop(state);
                    primitives.send_interest(&mut Interest {
                        id: querier_state.remote_id,
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
            Err(zerror!("Unable to find querier").into())
        }
    }

    pub(crate) fn optimize_nonwild_prefix(&self, key_expr: &KeyExpr) -> ZResult<WireExpr<'static>> {
        let ke = key_expr.as_keyexpr();
        if let Some(prefix) = ke.get_nonwild_prefix() {
            if let Some(expr_id) = self.declare_prefix(prefix.as_str(), false).wait()? {
                return Ok(WireExpr {
                    scope: expr_id,
                    suffix: key_expr.as_str()[prefix.len()..].to_string().into(),
                    mapping: Mapping::Sender,
                });
            }
        }
        Ok(key_expr.to_wire(self).to_owned())
    }

    pub(crate) fn declare_subscriber_inner(
        self: &Arc<Self>,
        key_expr: &KeyExpr,
        origin: Locality,
        callback: Callback<Sample>,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        tracing::trace!("declare_subscriber({:?})", key_expr);
        let id = self.runtime.next_id();
        let (sub_state, declared_sub) = state.register_subscriber(id, key_expr, origin, callback);
        if let Some(key_expr) = declared_sub {
            let primitives = state.primitives()?;
            drop(state);
            let wire_expr = self.optimize_nonwild_prefix(&key_expr)?;

            primitives.send_declare(&mut Declare {
                interest_id: None,
                ext_qos: declare::ext::QoSType::DECLARE,
                ext_tstamp: None,
                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareSubscriber(DeclareSubscriber { id, wire_expr }),
            });
            let state = zread!(self.state);
            self.update_matching_status(&state, &key_expr, MatchingStatusType::Subscribers, true)
        } else if origin == Locality::SessionLocal {
            self.update_matching_status(&state, key_expr, MatchingStatusType::Subscribers, true)
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
                .filter_map(LocalResource::as_node_mut)
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

            match kind {
                SubscriberKind::Subscriber => {
                    if sub_state.origin != Locality::SessionLocal {
                        // Note: there might be several Subscribers on the same KeyExpr.
                        // Before calling forget_subscriber(key_expr), check if this was the last one.
                        if !state.subscribers(kind).values().any(|s| {
                            s.origin != Locality::SessionLocal && s.remote_id == sub_state.remote_id
                        }) {
                            drop(state);
                            primitives.send_declare(&mut Declare {
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
                            let state = zread!(self.state);
                            self.update_matching_status(
                                &state,
                                &sub_state.key_expr,
                                MatchingStatusType::Subscribers,
                                false,
                            )
                        } else {
                            drop(state);
                        }
                    } else {
                        drop(state);
                        let state = zread!(self.state);
                        self.update_matching_status(
                            &state,
                            &sub_state.key_expr,
                            MatchingStatusType::Subscribers,
                            false,
                        )
                    }
                }
                SubscriberKind::LivelinessSubscriber => {
                    if kind == SubscriberKind::LivelinessSubscriber {
                        let primitives = state.primitives()?;
                        drop(state);

                        primitives.send_interest(&mut Interest {
                            id: sub_state.id,
                            mode: InterestMode::Final,
                            // Note: InterestMode::Final options are undefined in the current protocol specification,
                            //       they are initialized here for internal use by local egress interceptors.
                            options: InterestOptions::TOKENS,
                            wire_expr: None,
                            ext_qos: declare::ext::QoSType::DEFAULT,
                            ext_tstamp: None,
                            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                        });
                    }
                }
            }

            Ok(())
        } else {
            Err(zerror!("Unable to find subscriber").into())
        }
    }

    pub(crate) fn declare_queryable_inner(
        self: &Arc<Self>,
        key_expr: &KeyExpr,
        complete: bool,
        origin: Locality,
        callback: Callback<Query>,
    ) -> ZResult<Arc<QueryableState>> {
        let mut state = zwrite!(self.state);
        tracing::trace!("declare_queryable({:?})", key_expr);
        let id = self.runtime.next_id();
        let qable_state = Arc::new(QueryableState {
            id,
            key_expr: key_expr.clone().into_owned(),
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
            let wire_expr = self.optimize_nonwild_prefix(key_expr)?;
            primitives.send_declare(&mut Declare {
                interest_id: None,
                ext_qos: declare::ext::QoSType::DECLARE,
                ext_tstamp: None,
                ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                body: DeclareBody::DeclareQueryable(DeclareQueryable {
                    id,
                    wire_expr,
                    ext_info: qabl_info,
                }),
            });
        } else {
            drop(state);
        }

        let state = zread!(self.state);
        self.update_matching_status(
            &state,
            key_expr,
            MatchingStatusType::Queryables(complete),
            true,
        );

        Ok(qable_state)
    }

    pub(crate) fn close_queryable(self: &Arc<Self>, qid: Id) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        let Ok(primitives) = state.primitives() else {
            return Ok(());
        };
        if let Some(qable_state) = state.queryables.remove(&qid) {
            trace!("undeclare_queryable({:?})", qable_state);
            if qable_state.origin != Locality::SessionLocal {
                drop(state);
                primitives.send_declare(&mut Declare {
                    interest_id: None,
                    ext_qos: declare::ext::QoSType::DECLARE,
                    ext_tstamp: None,
                    ext_nodeid: declare::ext::NodeIdType::DEFAULT,
                    body: DeclareBody::UndeclareQueryable(UndeclareQueryable {
                        id: qable_state.id,
                        ext_wire_expr: WireExprType {
                            wire_expr: WireExpr::empty(),
                        },
                    }),
                });
            } else {
                drop(state);
            }
            let state = zread!(self.state);
            self.update_matching_status(
                &state,
                &qable_state.key_expr,
                MatchingStatusType::Queryables(qable_state.complete),
                false,
            );
            Ok(())
        } else {
            Err(zerror!("Unable to find queryable").into())
        }
    }

    pub(crate) fn declare_liveliness_inner(&self, key_expr: &KeyExpr) -> ZResult<Id> {
        tracing::trace!("declare_liveliness({:?})", key_expr);
        let id = self.runtime.next_id();
        let primitives = zread!(self.state).primitives()?;
        primitives.send_declare(&mut Declare {
            interest_id: None,
            ext_qos: declare::ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: declare::ext::NodeIdType::DEFAULT,
            body: DeclareBody::DeclareToken(DeclareToken {
                id,
                wire_expr: key_expr.to_wire(self).to_owned(),
            }),
        });
        Ok(id)
    }

    pub(crate) fn declare_liveliness_subscriber_inner(
        &self,
        key_expr: &KeyExpr,
        origin: Locality,
        history: bool,
        callback: Callback<Sample>,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        trace!("declare_liveliness_subscriber({:?})", key_expr);
        let id = self.runtime.next_id();

        let sub_state = SubscriberState {
            id,
            remote_id: id,
            key_expr: key_expr.clone().into_owned(),
            origin,
            callback: callback.clone(),
            history,
        };

        let sub_state = Arc::new(sub_state);

        state
            .subscribers_mut(SubscriberKind::LivelinessSubscriber)
            .insert(sub_state.id, sub_state.clone());

        for res in state
            .local_resources
            .values_mut()
            .filter_map(LocalResource::as_node_mut)
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

        let known_tokens = if history {
            state
                .remote_tokens
                .values()
                .filter(|token| key_expr.intersects(token))
                .cloned()
                .collect::<Vec<KeyExpr<'static>>>()
        } else {
            vec![]
        };

        let primitives = state.primitives()?;
        drop(state);

        if !known_tokens.is_empty() {
            self.task_controller
                .spawn_with_rt(zenoh_runtime::ZRuntime::Net, async move {
                    for token in known_tokens {
                        callback.call(Sample {
                            key_expr: token,
                            payload: ZBytes::new(),
                            kind: SampleKind::Put,
                            encoding: Encoding::default(),
                            timestamp: None,
                            qos: QoS::default(),
                            #[cfg(feature = "unstable")]
                            reliability: Reliability::Reliable,
                            #[cfg(feature = "unstable")]
                            source_info: SourceInfo::empty(),
                            attachment: None,
                        });
                    }
                });
        }

        primitives.send_interest(&mut Interest {
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

    pub(crate) fn undeclare_liveliness(&self, tid: Id) -> ZResult<()> {
        let Ok(primitives) = zread!(self.state).primitives() else {
            return Ok(());
        };
        trace!("undeclare_liveliness({:?})", tid);
        primitives.send_declare(&mut Declare {
            interest_id: None,
            ext_qos: ext::QoSType::DECLARE,
            ext_tstamp: None,
            ext_nodeid: ext::NodeIdType::DEFAULT,
            body: DeclareBody::UndeclareToken(UndeclareToken {
                id: tid,
                ext_wire_expr: WireExprType::null(),
            }),
        });
        Ok(())
    }

    pub(crate) fn declare_matches_listener_inner(
        &self,
        key_expr: &KeyExpr,
        destination: Locality,
        match_type: MatchingStatusType,
        callback: Callback<MatchingStatus>,
    ) -> ZResult<Arc<MatchingListenerState>> {
        let mut state = zwrite!(self.state);
        let id = self.runtime.next_id();
        tracing::trace!("matches_listener({:?}: {:?}) => {id}", match_type, key_expr);
        let listener_state = Arc::new(MatchingListenerState {
            id,
            current: std::sync::Mutex::new(false),
            destination,
            key_expr: key_expr.clone().into_owned(),
            match_type,
            callback,
        });
        state.matching_listeners.insert(id, listener_state.clone());
        drop(state);
        match listener_state.current.lock() {
            Ok(mut current) => {
                if self
                    .matching_status(key_expr, listener_state.destination, match_type)
                    .map(|s| s.matching())
                    .unwrap_or(true)
                {
                    *current = true;
                    listener_state
                        .callback
                        .call(MatchingStatus { matching: true });
                }
            }
            Err(e) => tracing::error!("Error trying to acquire MatchingListener lock: {}", e),
        }
        Ok(listener_state)
    }

    fn matching_status_local(
        &self,
        key_expr: &KeyExpr,
        matching_type: MatchingStatusType,
    ) -> MatchingStatus {
        let state = zread!(self.state);
        let matching = match matching_type {
            MatchingStatusType::Subscribers => state
                .subscribers(SubscriberKind::Subscriber)
                .values()
                .any(|s| s.key_expr.intersects(key_expr)),
            MatchingStatusType::Queryables(false) => state
                .queryables
                .values()
                .any(|q| q.key_expr.intersects(key_expr)),
            MatchingStatusType::Queryables(true) => state
                .queryables
                .values()
                .any(|q| q.complete && q.key_expr.includes(key_expr)),
        };
        MatchingStatus { matching }
    }

    fn matching_status_remote(
        &self,
        key_expr: &KeyExpr,
        destination: Locality,
        matching_type: MatchingStatusType,
    ) -> ZResult<MatchingStatus> {
        Ok(self.runtime.matching_status_remote(
            key_expr,
            destination,
            matching_type,
            *self.face_id.get().unwrap(),
        ))
    }

    pub(crate) fn matching_status(
        &self,
        key_expr: &KeyExpr,
        destination: Locality,
        matching_type: MatchingStatusType,
    ) -> ZResult<MatchingStatus> {
        match destination {
            Locality::SessionLocal => Ok(self.matching_status_local(key_expr, matching_type)),
            Locality::Remote => self.matching_status_remote(key_expr, destination, matching_type),
            Locality::Any => {
                let local_match = self.matching_status_local(key_expr, matching_type);
                if local_match.matching() {
                    Ok(local_match)
                } else {
                    self.matching_status_remote(key_expr, destination, matching_type)
                }
            }
        }
    }

    pub(crate) fn update_matching_status(
        self: &Arc<Self>,
        state: &SessionState,
        key_expr: &KeyExpr,
        match_type: MatchingStatusType,
        status_value: bool,
    ) {
        for msub in state.matching_listeners.values() {
            if msub.is_matching(key_expr, match_type) {
                // Cannot hold session lock when calling tables (matching_status())
                // TODO: check which ZRuntime should be used
                self.task_controller
                    .spawn_with_rt(zenoh_runtime::ZRuntime::Net, {
                        let session = WeakSession::new(self);
                        let msub = msub.clone();
                        async move {
                            match msub.current.lock() {
                                Ok(mut current) => {
                                    if *current != status_value {
                                        if let Ok(status) = session.matching_status(
                                            &msub.key_expr,
                                            msub.destination,
                                            msub.match_type,
                                        ) {
                                            if status.matching() == status_value {
                                                *current = status_value;
                                                let callback = msub.callback.clone();
                                                callback.call(status)
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Error trying to acquire MatchingListener lock: {}",
                                        e
                                    );
                                }
                            }
                        }
                    });
            }
        }
    }

    pub(crate) fn undeclare_matches_listener_inner(&self, sid: Id) -> ZResult<()> {
        let state = {
            let mut state = zwrite!(self.state);
            if state.primitives.is_none() {
                return Ok(());
            }

            state.matching_listeners.remove(&sid)
        };

        if let Some(state) = state {
            trace!("undeclare_matches_listener_inner({:?})", state);
            Ok(())
        } else {
            Err(zerror!("Unable to find MatchingListener").into())
        }
    }

    #[allow(clippy::too_many_arguments)] // TODO fixme
    pub(crate) fn execute_subscriber_callbacks<'a>(
        &self,
        local: bool,
        kind: SubscriberKind,
        wire_expr: &WireExpr,
        qos: push::ext::QoSType,
        msg: impl FnOnce() -> &'a mut PushBody,
        historical: bool,
        #[cfg(feature = "unstable")] reliability: Reliability,
    ) {
        let mut callbacks = SingleOrVec::default();
        let state = zread!(self.state);
        if state.primitives.is_none() {
            return; // Session closing or closed
        }
        if wire_expr.suffix.is_empty() {
            match state.get_res(&wire_expr.scope, wire_expr.mapping, local) {
                Some(Resource::Node(res)) => {
                    for sub in res.subscribers(kind).iter() {
                        if (sub.origin == Locality::Any
                            || (local == (sub.origin == Locality::SessionLocal)))
                            && (sub.history || (!historical))
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
                    tracing::error!("Received Data for unknown expr_id: {}", wire_expr.scope);
                    return;
                }
            }
        } else {
            match state.wireexpr_to_keyexpr(wire_expr, local) {
                Ok(key_expr) => {
                    for sub in state.subscribers(kind).values() {
                        if (sub.origin == Locality::Any
                            || (local == (sub.origin == Locality::SessionLocal)))
                            && (sub.history || (!historical))
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
        if callbacks.is_empty() {
            return;
        }
        let msg = msg();
        let zenoh_collections::single_or_vec::IntoIter { drain, last } = callbacks.into_iter();
        for (cb, key_expr) in drain {
            #[cfg(feature = "unstable")]
            cb.call_with_message((key_expr, qos, &mut msg.clone(), reliability));
            #[cfg(not(feature = "unstable"))]
            cb.call_with_message((key_expr, qos, &mut msg.clone()));
        }
        if let Some((cb, key_expr)) = last {
            #[cfg(feature = "unstable")]
            cb.call_with_message((key_expr, qos, msg, reliability));
            #[cfg(not(feature = "unstable"))]
            cb.call_with_message((key_expr, qos, msg));
        }
    }

    #[allow(clippy::too_many_arguments)] // TODO fixme
    pub(crate) fn resolve_put(
        &self,
        key_expr: &KeyExpr,
        payload: ZBytes,
        kind: SampleKind,
        encoding: Encoding,
        congestion_control: CongestionControl,
        priority: Priority,
        is_express: bool,
        destination: Locality,
        #[cfg(feature = "unstable")] reliability: Reliability,
        timestamp: Option<uhlc::Timestamp>,
        #[cfg(feature = "unstable")] source_info: SourceInfo,
        attachment: Option<ZBytes>,
    ) -> ZResult<()> {
        trace!("write({:?}, [...])", key_expr);
        let primitives = zread!(self.state).primitives()?;
        let timestamp = timestamp.or_else(|| self.runtime.new_timestamp());
        let wire_expr = key_expr.to_wire(self);
        let ext_qos = push::ext::QoSType::new(priority.into(), congestion_control, is_express);
        let make_body = || match kind {
            SampleKind::Put => PushBody::Put(Put {
                timestamp,
                encoding: encoding.clone().into(),
                #[cfg(feature = "unstable")]
                ext_sinfo: source_info.clone().into(),
                #[cfg(not(feature = "unstable"))]
                ext_sinfo: None,
                #[cfg(feature = "shared-memory")]
                ext_shm: None,
                ext_attachment: attachment.clone().map(Into::into),
                ext_unknown: vec![],
                payload: payload.clone().into(),
            }),
            SampleKind::Delete => PushBody::Del(Del {
                timestamp,
                #[cfg(feature = "unstable")]
                ext_sinfo: source_info.clone().into(),
                #[cfg(not(feature = "unstable"))]
                ext_sinfo: None,
                ext_attachment: attachment.clone().map(Into::into),
                ext_unknown: vec![],
            }),
        };
        let mut push = Push {
            wire_expr: wire_expr.to_owned(),
            ext_qos,
            ..Push::from(make_body())
        };
        if destination != Locality::SessionLocal {
            primitives.send_push(
                &mut push,
                #[cfg(feature = "unstable")]
                reliability,
                #[cfg(not(feature = "unstable"))]
                Reliability::DEFAULT,
            );
        }
        if destination != Locality::Remote {
            self.execute_subscriber_callbacks(
                true,
                SubscriberKind::Subscriber,
                &wire_expr,
                ext_qos,
                || {
                    // check if the message has been consumed, and rebuild the payload in this case
                    if push.wire_expr == WireExpr::empty() {
                        push.payload = make_body();
                    }
                    &mut push.payload
                },
                false,
                #[cfg(feature = "unstable")]
                reliability,
            );
        }
        Ok(())
    }

    #[cfg(feature = "internal")]
    #[allow(dead_code)]
    pub(crate) fn static_runtime(&self) -> Option<&Runtime> {
        self.runtime.static_runtime()
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
        value: Option<(ZBytes, Encoding)>,
        attachment: Option<ZBytes>,
        #[cfg(feature = "unstable")] source: SourceInfo,
        callback: Callback<Reply>,
    ) -> ZResult<Id> {
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
                async move {
                    tokio::select! {
                        _ = tokio::time::sleep(timeout) => {
                            let mut state = zwrite!(session.state);
                            if let Some(query) = state.queries.remove(&qid) {
                                std::mem::drop(state);
                                tracing::debug!("Timeout on query {}! Send error and close.", qid);
                                if query.reception_mode == ConsolidationMode::Latest {
                                    for (_, reply) in query.replies.unwrap().into_iter() {
                                        query.callback.call(reply);
                                    }
                                }
                                query.callback.call(Reply {
                                    result: Err(ReplyError::new("Timeout", Encoding::ZENOH_STRING)),
                                    #[cfg(feature = "unstable")]
                                    replier_id: None
                                });
                            }
                        }
                        _ = token.cancelled() => {}
                    }
                }
            });

        tracing::trace!("Register query {} (nb_final = {})", qid, nb_final);
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
            let wexpr = key_expr.to_wire(self).to_owned();
            let ext_attachment = attachment.clone().map(Into::into);
            primitives.send_request(&mut Request {
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
                        encoding: v.1.clone().into(),
                        payload: v.0.clone().into(),
                    }),
                    ext_attachment,
                    ext_unknown: vec![],
                }),
            });
        }
        if destination != Locality::Remote {
            self.handle_query(
                zread!(self.state),
                true,
                key_expr,
                parameters.as_str(),
                qid,
                target,
                consolidation,
                value.as_ref().map(|v| query::ext::QueryBodyType {
                    #[cfg(feature = "shared-memory")]
                    ext_shm: None,
                    encoding: v.1.clone().into(),
                    payload: v.0.clone().into(),
                }),
                attachment,
            );
        }
        Ok(qid)
    }

    #[cfg(feature = "unstable")]
    pub(crate) fn cancel_query(self: &Arc<Self>, qid: Id) -> ZResult<()> {
        tracing::debug!("Cancelling query: {qid}");
        let mut state = zwrite!(self.state);
        match state.queries.remove(&qid) {
            Some(_) => bail!("Unable to find query {qid}"),
            None => Ok(()),
        }
    }

    pub(crate) fn liveliness_query(
        self: &Arc<Self>,
        key_expr: &KeyExpr<'_>,
        timeout: Duration,
        callback: Callback<Reply>,
    ) -> ZResult<Id> {
        tracing::trace!("liveliness.get({}, {:?})", key_expr, timeout);
        let mut state = zwrite!(self.state);
        // Queries must use the same id generator as liveliness subscribers.
        // This is because both query's id and subscriber's id are used as interest id,
        // so both must not overlap.
        let id = self.runtime.next_id();
        let token = self.task_controller.get_cancellation_token();
        self.task_controller
            .spawn_with_rt(zenoh_runtime::ZRuntime::Net, {
                let session = WeakSession::new(self);
                async move {
                    tokio::select! {
                        _ = tokio::time::sleep(timeout) => {
                            let mut state = zwrite!(session.state);
                            if let Some(query) = state.liveliness_queries.remove(&id) {
                                std::mem::drop(state);
                                tracing::debug!("Timeout on liveliness query {}! Send error and close.", id);
                                query.callback.call(Reply {
                                    result: Err(ReplyError::new("Timeout", Encoding::ZENOH_STRING)),
                                    #[cfg(feature = "unstable")]
                                    replier_id: None
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

        primitives.send_interest(&mut Interest {
            id,
            mode: InterestMode::Current,
            options: InterestOptions::KEYEXPRS + InterestOptions::TOKENS,
            wire_expr: Some(wexpr.clone()),
            ext_qos: request::ext::QoSType::DEFAULT,
            ext_tstamp: None,
            ext_nodeid: request::ext::NodeIdType::DEFAULT,
        });

        Ok(id)
    }

    #[cfg(feature = "unstable")]
    pub(crate) fn cancel_liveliness_query(self: &Arc<Self>, qid: Id) -> ZResult<()> {
        tracing::debug!("Cancelling liveliness query: {qid}");
        let mut state = zwrite!(self.state);
        match state.liveliness_queries.remove(&qid) {
            Some(_) => bail!("Unable to find liveliness query {qid}"),
            None => Ok(()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn handle_query(
        self: &Arc<Self>,
        state: RwLockReadGuard<'_, SessionState>,
        local: bool,
        key_expr: &KeyExpr<'_>,
        parameters: &str,
        qid: RequestId,
        target: QueryTarget,
        _consolidation: ConsolidationMode,
        body: Option<QueryBodyType>,
        attachment: Option<ZBytes>,
    ) {
        let Ok(primitives) = state.primitives() else {
            return;
        };
        let queryables = state
            .queryables
            .iter()
            .filter(|(_, queryable)| {
                (queryable.origin == Locality::Any
                    || (local == (queryable.origin == Locality::SessionLocal)))
                    && (queryable.complete || target != QueryTarget::AllComplete)
                    && queryable.key_expr.intersects(key_expr)
            })
            .map(|(id, qable)| (*id, qable.callback.clone()))
            .collect::<Vec<(u32, Callback<Query>)>>();

        drop(state);

        let zid = self.zid();

        let query_inner = Arc::new(QueryInner {
            key_expr: key_expr.clone().into_owned(),
            parameters: parameters.to_owned().into(),
            qid,
            zid: zid.into(),
            primitives: if local {
                Arc::new(WeakSession::new(self))
            } else {
                primitives
            },
        });
        if !queryables.is_empty() {
            let mut query = Query {
                inner: query_inner,
                eid: 0,
                value: body.map(|b| (b.payload.into(), b.encoding.into())),
                attachment,
            };
            for (eid, cb) in queryables {
                query.eid = eid;
                cb.call(query.clone());
            }
        }
    }
}

impl Primitives for WeakSession {
    fn send_interest(&self, msg: &mut zenoh_protocol::network::Interest) {
        trace!("recv Interest {} {:?}", msg.id, msg.wire_expr);
    }
    fn send_declare(&self, msg: &mut zenoh_protocol::network::Declare) {
        match &mut msg.body {
            zenoh_protocol::network::DeclareBody::DeclareKeyExpr(m) => {
                trace!("recv DeclareKeyExpr {} {:?}", m.id, m.wire_expr);
                let state = &mut zwrite!(self.state);
                if state.primitives.is_none() {
                    return; // Session closing or closed
                }
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
                {
                    let mut state = zwrite!(self.state);
                    if state.primitives.is_none() {
                        return; // Session closing or closed
                    }
                    match state
                        .wireexpr_to_keyexpr(&m.wire_expr, false)
                        .map(|e| e.into_owned())
                    {
                        Ok(expr) => {
                            state.remote_subscribers.insert(m.id, expr.clone());
                            self.update_matching_status(
                                &state,
                                &expr,
                                MatchingStatusType::Subscribers,
                                true,
                            );
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
                let mut state = zwrite!(self.state);
                if state.primitives.is_none() {
                    return; // Session closing or closed
                }
                if let Some(expr) = state.remote_subscribers.remove(&m.id) {
                    self.update_matching_status(
                        &state,
                        &expr,
                        MatchingStatusType::Subscribers,
                        false,
                    );
                } else {
                    tracing::error!("Received Undeclare Subscriber for unknown id: {}", m.id);
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareQueryable(m) => {
                trace!("recv DeclareQueryable {} {:?}", m.id, m.wire_expr);
                {
                    let mut state = zwrite!(self.state);
                    if state.primitives.is_none() {
                        return; // Session closing or closed
                    }
                    match state
                        .wireexpr_to_keyexpr(&m.wire_expr, false)
                        .map(|e| e.into_owned())
                    {
                        Ok(expr) => {
                            let prev = state
                                .remote_queryables
                                .insert(m.id, (expr.clone(), m.ext_info.complete));
                            if let Some((prev_expr, prev_complete)) = prev {
                                self.update_matching_status(
                                    &state,
                                    &prev_expr,
                                    MatchingStatusType::Queryables(prev_complete),
                                    false,
                                );
                            }
                            self.update_matching_status(
                                &state,
                                &expr,
                                MatchingStatusType::Queryables(m.ext_info.complete),
                                true,
                            );
                        }
                        Err(err) => {
                            tracing::error!(
                                "Received DeclareQueryable for unknown wire_expr: {}",
                                err
                            )
                        }
                    }
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareQueryable(m) => {
                trace!("recv UndeclareQueryable {:?}", m.id);
                let mut state = zwrite!(self.state);
                if state.primitives.is_none() {
                    return; // Session closing or closed
                }
                if let Some((expr, complete)) = state.remote_queryables.remove(&m.id) {
                    self.update_matching_status(
                        &state,
                        &expr,
                        MatchingStatusType::Queryables(complete),
                        false,
                    );
                } else {
                    tracing::error!("Received Undeclare Queryable for unknown id: {}", m.id);
                }
            }
            zenoh_protocol::network::DeclareBody::DeclareToken(m) => {
                let mut state = zwrite!(self.state);
                if state.primitives.is_none() {
                    return; // Session closing or closed
                }
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
                                        payload: ZBytes::new(),
                                        kind: SampleKind::Put,
                                        encoding: Encoding::default(),
                                        timestamp: None,
                                        qos: QoS::default(),
                                        #[cfg(feature = "unstable")]
                                        reliability: Reliability::Reliable,
                                        #[cfg(feature = "unstable")]
                                        source_info: SourceInfo::empty(),
                                        attachment: None,
                                    }),
                                    #[cfg(feature = "unstable")]
                                    replier_id: None,
                                };

                                query.callback.call(reply);
                                return;
                            }
                        }
                        if let Entry::Vacant(e) = state.remote_tokens.entry(m.id) {
                            e.insert(key_expr.clone());
                            drop(state);

                            let mut body = None;
                            self.execute_subscriber_callbacks(
                                false,
                                SubscriberKind::LivelinessSubscriber,
                                &m.wire_expr,
                                Default::default(),
                                || body.insert(Put::default().into()),
                                // interest_id is set if the Token is an Interest::Current.
                                // This is used to decide if subs with history=false should be called or not
                                msg.interest_id.is_some(),
                                #[cfg(feature = "unstable")]
                                Reliability::Reliable,
                            );
                        }
                    }
                    Err(err) => {
                        tracing::error!("Received DeclareToken for unknown wire_expr: {}", err)
                    }
                }
            }
            zenoh_protocol::network::DeclareBody::UndeclareToken(m) => {
                trace!("recv UndeclareToken {:?}", m.id);
                {
                    let mut state = zwrite!(self.state);
                    if state.primitives.is_none() {
                        return; // Session closing or closed
                    }
                    // interest_id is set if the Token is an Interest::Current.
                    // This is used to decide if liveliness subs with history=false should be called or not
                    // NOTE: an UndeclareToken is most likely not an Interest::Current
                    let interest_current = msg.interest_id.is_some();
                    if let Some(key_expr) = state.remote_tokens.remove(&m.id) {
                        drop(state);
                        let mut body = None;
                        self.execute_subscriber_callbacks(
                            false,
                            SubscriberKind::LivelinessSubscriber,
                            &key_expr.to_wire(self),
                            Default::default(),
                            || body.insert(Del::default().into()),
                            interest_current,
                            #[cfg(feature = "unstable")]
                            Reliability::Reliable,
                        );
                    } else if m.ext_wire_expr.wire_expr != WireExpr::empty() {
                        match state
                            .wireexpr_to_keyexpr(&m.ext_wire_expr.wire_expr, false)
                            .map(|e| e.into_owned())
                        {
                            Ok(key_expr) => {
                                drop(state);
                                let mut body = None;
                                self.execute_subscriber_callbacks(
                                    false,
                                    SubscriberKind::LivelinessSubscriber,
                                    &key_expr.to_wire(self),
                                    Default::default(),
                                    || body.insert(Del::default().into()),
                                    interest_current,
                                    #[cfg(feature = "unstable")]
                                    Reliability::Reliable,
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
                if let Some(interest_id) = msg.interest_id {
                    let mut state = zwrite!(self.state);
                    let _ = state.liveliness_queries.remove(&interest_id);
                }
            }
        }
    }

    fn send_push(&self, msg: &mut Push, _reliability: Reliability) {
        trace!("recv Push {:?}", msg);
        self.execute_subscriber_callbacks(
            false,
            SubscriberKind::Subscriber,
            &msg.wire_expr,
            msg.ext_qos,
            || &mut msg.payload,
            false,
            #[cfg(feature = "unstable")]
            _reliability,
        );
    }

    fn send_request(&self, msg: &mut Request) {
        trace!("recv Request {:?}", msg);
        match &mut msg.payload {
            RequestBody::Query(m) => {
                let state = zread!(self.state);
                match state
                    .wireexpr_to_keyexpr(&msg.wire_expr, false)
                    .map(|k| k.into_owned())
                {
                    Ok(key_expr) => {
                        self.handle_query(
                            state,
                            false,
                            &key_expr,
                            &m.parameters,
                            msg.id,
                            msg.ext_target,
                            m.consolidation,
                            mem::take(&mut m.ext_body),
                            mem::take(&mut m.ext_attachment).map(Into::into),
                        );
                    }
                    Err(err) => {
                        error!("Received Query for unknown key_expr: {}", err);
                    }
                }
            }
        }
    }

    fn send_response(&self, msg: &mut Response) {
        trace!("recv Response {:?}", msg);
        match &mut msg.payload {
            ResponseBody::Err(e) => {
                let mut state = zwrite!(self.state);
                if state.primitives.is_none() {
                    return; // Session closing or closed
                }
                match state.queries.get_mut(&msg.rid) {
                    Some(query) => {
                        let callback = query.callback.clone();
                        std::mem::drop(state);
                        let new_reply = Reply {
                            result: Err(ReplyError {
                                payload: mem::take(&mut e.payload).into(),
                                encoding: mem::take(&mut e.encoding).into(),
                            }),
                            #[cfg(feature = "unstable")]
                            replier_id: mem::take(&mut msg.ext_respid).map(|rid| {
                                zenoh_protocol::core::EntityGlobalIdProto {
                                    zid: rid.zid,
                                    eid: rid.eid,
                                }
                            }),
                        };
                        callback.call(new_reply);
                    }
                    None => {
                        tracing::warn!("Received ReplyData for unknown Query: {}", msg.rid);
                    }
                }
            }
            ResponseBody::Reply(m) => {
                let mut state = zwrite!(self.state);
                if state.primitives.is_none() {
                    return; // Session closing or closed
                }
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
                                "Received Reply for `{}` from `{:?}`, which didn't match query `{}`: dropping Reply.",
                                key_expr,
                                msg.ext_respid,
                                query.selector()
                            );
                            return;
                        }
                        let new_reply = Reply {
                            result: Ok(Sample::from_push(
                                key_expr.into_owned(),
                                msg.ext_qos,
                                &mut m.payload,
                                #[cfg(feature = "unstable")]
                                Reliability::Reliable,
                            )),
                            #[cfg(feature = "unstable")]
                            replier_id: mem::take(&mut msg.ext_respid).map(|rid| {
                                zenoh_protocol::core::EntityGlobalIdProto {
                                    zid: rid.zid,
                                    eid: rid.eid,
                                }
                            }),
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
                                                >= reply.result.as_ref().unwrap().timestamp
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
                                ConsolidationMode::Auto | ConsolidationMode::Latest => {
                                    match query.replies.as_ref().unwrap().get(
                                        new_reply.result.as_ref().unwrap().key_expr.as_keyexpr(),
                                    ) {
                                        Some(reply) => {
                                            if new_reply.result.as_ref().unwrap().timestamp
                                                >= reply.result.as_ref().unwrap().timestamp
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
                            callback.call(new_reply);
                        }
                    }
                    None => {
                        tracing::warn!("Received ReplyData for unknown Query: {}", msg.rid);
                    }
                }
            }
        }
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        trace!("recv ResponseFinal {:?}", msg);
        let mut state = zwrite!(self.state);
        if state.primitives.is_none() {
            return; // Session closing or closed
        }
        match state.queries.get_mut(&msg.rid) {
            Some(query) => {
                query.nb_final -= 1;
                if query.nb_final == 0 {
                    let query = state.queries.remove(&msg.rid).unwrap();
                    std::mem::drop(state);
                    if query.reception_mode == ConsolidationMode::Latest {
                        for (_, reply) in query.replies.unwrap().into_iter() {
                            query.callback.call(reply);
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl crate::net::primitives::EPrimitives for WeakSession {
    #[inline]
    fn send_interest(&self, ctx: crate::net::routing::RoutingContext<&mut Interest>) {
        (self as &dyn Primitives).send_interest(ctx.msg)
    }

    #[inline]
    fn send_declare(&self, ctx: crate::net::routing::RoutingContext<&mut Declare>) {
        (self as &dyn Primitives).send_declare(ctx.msg)
    }

    #[inline]
    fn send_push(&self, msg: &mut Push, reliability: Reliability) {
        (self as &dyn Primitives).send_push(msg, reliability)
    }

    #[inline]
    fn send_request(&self, msg: &mut Request) {
        (self as &dyn Primitives).send_request(msg)
    }

    #[inline]
    fn send_response(&self, msg: &mut Response) {
        (self as &dyn Primitives).send_response(msg)
    }

    #[inline]
    fn send_response_final(&self, msg: &mut ResponseFinal) {
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
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// # }
/// ```
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use std::str::FromStr;
/// use zenoh::session::ZenohId;
///
/// let mut config = zenoh::Config::default();
/// config.set_id(Some(ZenohId::from_str("221b72df20924c15b8794c6bdb471150").unwrap()));
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
    OpenBuilder::new(config)
}

#[async_trait]
impl Closee for Arc<SessionInner> {
    async fn close_inner(&self) {
        let Some(primitives) = zwrite!(self.state).primitives.take() else {
            return;
        };

        if let Some(r) = self.runtime.static_runtime() {
            // session created by plugins never have a copy of static_runtime, so the code below will run only inside zenohd
            info!(zid = %self.zid(), "close session");
            self.task_controller.terminate_all_async().await;
            let closee = r.get_closee();
            closee.close_inner().await;
        } else {
            self.task_controller.terminate_all_async().await;
            primitives.send_close();
        }

        // defer the cleanup of internal data structures by taking them out of the locked state
        // this is needed because callbacks may contain entities which need to acquire the
        // lock to be dropped, so callback must be dropped without the lock held
        let mut state = zwrite!(self.state);
        let _queryables = std::mem::take(&mut state.queryables);
        let _subscribers = std::mem::take(&mut state.subscribers);
        let _liveliness_subscribers = std::mem::take(&mut state.liveliness_subscribers);
        let _local_resources = std::mem::take(&mut state.local_resources);
        let _remote_resources = std::mem::take(&mut state.remote_resources);
        let _queries = std::mem::take(&mut state.queries);
        let _matching_listeners = std::mem::take(&mut state.matching_listeners);
        drop(state);
    }
}

impl Closeable for Session {
    type TClosee = Arc<SessionInner>;

    fn get_closee(&self) -> Self::TClosee {
        self.0.clone()
    }
}
