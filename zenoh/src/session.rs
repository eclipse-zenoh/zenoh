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

use crate::admin;
use crate::config::Config;
use crate::config::Notifier;
use crate::handlers::{Callback, DefaultHandler};
use crate::info::*;
use crate::key_expr::KeyExprInner;
#[zenoh_macros::unstable]
use crate::liveliness::{Liveliness, LivelinessTokenState};
use crate::net::routing::face::Face;
use crate::net::runtime::Runtime;
use crate::net::transport::Primitives;
use crate::prelude::Locality;
use crate::prelude::{KeyExpr, Parameters};
use crate::publication::*;
use crate::query::*;
use crate::queryable::*;
use crate::selector::TIME_RANGE_KEY;
use crate::subscriber::*;
use crate::Id;
use crate::Priority;
use crate::Sample;
use crate::SampleKind;
use crate::Selector;
use crate::Value;
use async_std::task;
use log::{error, trace, warn};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt;
use std::ops::Deref;
use std::sync::atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
use uhlc::HLC;
use zenoh_buffers::ZBuf;
use zenoh_collections::SingleOrVec;
use zenoh_config::unwrap_or_default;
use zenoh_core::{zconfigurable, zread, Resolve, ResolveClosure, ResolveFuture, SyncResolve};
use zenoh_protocol::{
    core::{
        key_expr::{keyexpr, OwnedKeyExpr},
        Channel, CongestionControl, ExprId, QueryTarget, QueryableInfo, SubInfo, WireExpr, ZInt,
        ZenohId, EMPTY_EXPR_ID,
    },
    zenoh::{DataInfo, QueryBody, RoutingContext},
};
use zenoh_result::ZResult;
use zenoh_util::core::AsyncResolve;

pub type AtomicZInt = AtomicU64;

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
    pub(crate) queryables: HashMap<Id, Arc<QueryableState>>,
    #[cfg(feature = "unstable")]
    pub(crate) tokens: HashMap<Id, Arc<LivelinessTokenState>>,
    pub(crate) queries: HashMap<ZInt, QueryState>,
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
            expr_id_counter: AtomicUsize::new(1), // Note: start at 1 because 0 is reserved for NO_RESOURCE
            qid_counter: AtomicZInt::new(0),
            decl_id_counter: AtomicUsize::new(0),
            local_resources: HashMap::new(),
            remote_resources: HashMap::new(),
            publications: Vec::new(),
            subscribers: HashMap::new(),
            queryables: HashMap::new(),
            #[cfg(feature = "unstable")]
            tokens: HashMap::new(),
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

    pub(crate) fn remote_key_to_expr<'a>(&'a self, key_expr: &'a WireExpr) -> ZResult<KeyExpr<'a>> {
        if key_expr.scope == EMPTY_EXPR_ID {
            Ok(unsafe { keyexpr::from_str_unchecked(key_expr.suffix.as_ref()) }.into())
        } else if key_expr.suffix.is_empty() {
            match self.get_remote_res(&key_expr.scope) {
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
                match self.get_remote_res(&key_expr.scope) {
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
    pub(crate) alive: bool,
}

static SESSION_ID_COUNTER: AtomicU16 = AtomicU16::new(0);
impl Session {
    pub(crate) fn init(
        runtime: Runtime,
        aggregated_subscribers: Vec<OwnedKeyExpr>,
        aggregated_publishers: Vec<OwnedKeyExpr>,
    ) -> impl Resolve<Session> {
        ResolveClosure::new(move || {
            let router = runtime.router.clone();
            let state = Arc::new(RwLock::new(SessionState::new(
                aggregated_subscribers,
                aggregated_publishers,
            )));
            let session = Session {
                runtime: runtime.clone(),
                state: state.clone(),
                id: SESSION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
                alive: true,
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
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received: {:?}", sample);
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
    /// This is useful to move entities (like [`Subscriber`](Subscriber)) which
    /// lifetimes are bound to the session lifetime in several threads or tasks.
    ///
    /// Note: the given zenoh `Session` cannot be closed any more. At process
    /// termination the zenoh session will terminate abruptly. If possible prefer
    /// using [`Session::into_arc()`](Session::into_arc).
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    /// use zenoh::Session;
    ///
    /// let session = Session::leak(zenoh::open(config::peer()).res().await.unwrap());
    /// let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received: {:?}", sample);
    ///     }
    /// }).await;
    /// # })
    /// ```
    pub fn leak(s: Self) -> &'static mut Self {
        Box::leak(Box::new(s))
    }

    /// Returns the identifier of the current session. `zid()` is a convenient shortcut.
    /// See [`Session::info()`](`Session::info()`) and [`SessionInfo::zid()`](`SessionInfo::zid()`) for more details.
    pub fn zid(&self) -> ZenohId {
        self.info().zid().res_sync()
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
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// session.close().res().await.unwrap();
    /// # })
    /// ```
    pub fn close(self) -> impl Resolve<ZResult<()>> {
        ResolveFuture::new(async move {
            trace!("close()");
            self.runtime.close().await?;

            let primitives = zwrite!(self.state).primitives.as_ref().unwrap().clone();
            primitives.send_close();

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
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let peers = session.config().get("connect/endpoints").unwrap();
    /// # })
    /// ```
    ///
    /// ### Modify current zenoh configuration
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let _ = session.config().insert_json5("connect/endpoints", r#"["tcp/127.0.0.1/7447"]"#);
    /// # })
    /// ```
    pub fn config(&self) -> &Notifier<Config> {
        &self.runtime.config
    }

    /// Get informations about the zenoh [`Session`](Session).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let info = session.info();
    /// # })
    /// ```
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            session: SessionRef::Borrow(self),
        }
    }

    /// Create a [`Subscriber`](Subscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let subscriber = session.declare_subscriber("key/expression").res().await.unwrap();
    /// while let Ok(sample) = subscriber.recv_async().await {
    ///     println!("Received: {:?}", sample);
    /// }
    /// # })
    /// ```
    pub fn declare_subscriber<'a, 'b, TryIntoKeyExpr>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> SubscriberBuilder<'a, 'b, PushMode, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SubscriberBuilder {
            session: SessionRef::Borrow(self),
            key_expr: TryIntoKeyExpr::try_into(key_expr).map_err(Into::into),
            reliability: Reliability::default(),
            mode: PushMode,
            origin: Locality::default(),
            handler: DefaultHandler,
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
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
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
    pub fn declare_queryable<'a, 'b, TryIntoKeyExpr>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> QueryableBuilder<'a, 'b, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        QueryableBuilder {
            session: SessionRef::Borrow(self),
            key_expr: key_expr.try_into().map_err(Into::into),
            complete: false,
            origin: Locality::default(),
            handler: DefaultHandler,
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
    /// let publisher = session.declare_publisher("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// publisher.put("value").res().await.unwrap();
    /// # })
    /// ```
    pub fn declare_publisher<'a, 'b, TryIntoKeyExpr>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> PublisherBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PublisherBuilder {
            session: SessionRef::Borrow(self),
            key_expr: key_expr.try_into().map_err(Into::into),
            congestion_control: CongestionControl::default(),
            priority: Priority::default(),
            destination: Locality::default(),
        }
    }

    /// Informs Zenoh that you intend to use `key_expr` multiple times and that it should optimize its transmission.
    ///
    /// The returned `KeyExpr`'s internal structure may differ from what you would have obtained through a simple
    /// `key_expr.try_into()`, to save time on detecting the optimizations that have been associated with it.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let key_expr = session.declare_keyexpr("key/expression").res().await.unwrap();
    /// # })
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
            let expr_id = self.declare_prefix(key_expr.as_str()).res_sync();
            let key_expr = match key_expr.0 {
                KeyExprInner::Borrowed(key_expr) | KeyExprInner::BorrowedWire { key_expr, .. } => {
                    KeyExpr(KeyExprInner::BorrowedWire {
                        key_expr,
                        expr_id,
                        prefix_len,
                        session_id: sid,
                    })
                }
                KeyExprInner::Owned(key_expr) | KeyExprInner::Wire { key_expr, .. } => {
                    KeyExpr(KeyExprInner::Wire {
                        key_expr,
                        expr_id,
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
    pub fn put<'a, 'b: 'a, TryIntoKeyExpr, IntoValue>(
        &'a self,
        key_expr: TryIntoKeyExpr,
        value: IntoValue,
    ) -> PutBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
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
    /// * `key_expr` - Key expression matching the resources to delete
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// session.delete("key/expression").res().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn delete<'a, 'b: 'a, TryIntoKeyExpr>(
        &'a self,
        key_expr: TryIntoKeyExpr,
    ) -> DeleteBuilder<'a, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PutBuilder {
            publisher: self.declare_publisher(key_expr),
            value: Value::empty(),
            kind: SampleKind::Delete,
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
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let replies = session.get("key/expression").res().await.unwrap();
    /// while let Ok(reply) = replies.recv_async().await {
    ///     println!(">> Received {:?}", reply.sample);
    /// }
    /// # })
    /// ```
    pub fn get<'a, 'b: 'a, IntoSelector>(
        &'a self,
        selector: IntoSelector,
    ) -> GetBuilder<'a, 'b, DefaultHandler>
    where
        IntoSelector: TryInto<Selector<'b>>,
        <IntoSelector as TryInto<Selector<'b>>>::Error: Into<zenoh_result::Error>,
    {
        let selector = selector.try_into().map_err(Into::into);
        let conf = self.runtime.config.lock();
        GetBuilder {
            session: self,
            selector,
            scope: Ok(None),
            target: QueryTarget::default(),
            consolidation: QueryConsolidation::default(),
            destination: Locality::default(),
            timeout: Duration::from_millis(unwrap_or_default!(conf.queries_default_timeout())),
            value: None,
            handler: DefaultHandler,
        }
    }

    /// Obtain a [`Liveliness`] struct tied to this Zenoh [`Session`].
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[zenoh_macros::unstable]
    pub fn liveliness(&self) -> Liveliness {
        Liveliness {
            session: SessionRef::Borrow(self),
        }
    }
}

impl Session {
    pub(crate) fn clone(&self) -> Self {
        Session {
            runtime: self.runtime.clone(),
            state: self.state.clone(),
            id: self.id,
            alive: false,
        }
    }

    #[allow(clippy::new_ret_no_self)]
    pub(super) fn new(config: Config) -> impl Resolve<ZResult<Session>> + Send {
        ResolveFuture::new(async move {
            log::debug!("Config: {:?}", &config);
            let aggregated_subscribers = config.aggregation().subscribers().clone();
            let aggregated_publishers = config.aggregation().publishers().clone();
            match Runtime::init(config).await {
                Ok(mut runtime) => {
                    let session = Self::init(
                        runtime.clone(),
                        aggregated_subscribers,
                        aggregated_publishers,
                    )
                    .res_async()
                    .await;
                    match runtime.start().await {
                        Ok(()) => {
                            // Workaround for the declare_and_shoot problem
                            task::sleep(Duration::from_millis(*API_OPEN_SESSION_DELAY)).await;
                            Ok(session)
                        }
                        Err(err) => Err(err),
                    }
                }
                Err(err) => Err(err),
            }
        })
    }

    pub(crate) fn declare_prefix<'a>(&'a self, prefix: &'a str) -> impl Resolve<u64> + Send + 'a {
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
                    let expr_id = state.expr_id_counter.fetch_add(1, Ordering::SeqCst) as ZInt;
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
                    primitives.decl_resource(
                        expr_id,
                        &WireExpr {
                            scope: 0,
                            suffix: std::borrow::Cow::Borrowed(prefix),
                        },
                    );
                    expr_id
                }
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
    pub(crate) fn declare_publication_intent<'a>(
        &'a self,
        key_expr: KeyExpr<'a>,
    ) -> impl Resolve<Result<(), std::convert::Infallible>> + Send + 'a {
        ResolveClosure::new(move || {
            log::trace!("declare_publication({:?})", key_expr);
            let mut state = zwrite!(self.state);
            if !state.publications.iter().any(|p| **p == **key_expr) {
                let declared_pub = if let Some(join_pub) = state
                    .aggregated_publishers
                    .iter()
                    .find(|s| s.includes(&key_expr))
                {
                    let joined_pub = state.publications.iter().any(|p| join_pub.includes(p));
                    (!joined_pub).then(|| join_pub.clone().into())
                } else {
                    Some(key_expr.clone())
                };
                state.publications.push(key_expr.into());

                if let Some(res) = declared_pub {
                    let primitives = state.primitives.as_ref().unwrap().clone();
                    drop(state);
                    primitives.decl_publisher(&res.to_wire(self), None);
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
    pub(crate) fn undeclare_publication_intent<'a>(
        &'a self,
        key_expr: KeyExpr<'a>,
    ) -> impl Resolve<ZResult<()>> + 'a {
        ResolveClosure::new(move || {
            let mut state = zwrite!(self.state);
            if let Some(idx) = state.publications.iter().position(|p| **p == *key_expr) {
                trace!("undeclare_publication({:?})", key_expr);
                state.publications.remove(idx);
                match state
                    .aggregated_publishers
                    .iter()
                    .find(|s| s.includes(&key_expr))
                {
                    Some(join_pub) => {
                        let joined_pub = state.publications.iter().any(|p| join_pub.includes(p));
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
                        primitives.forget_publisher(&key_expr.to_wire(self), None);
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
        scope: &Option<KeyExpr>,
        origin: Locality,
        callback: Callback<'static, Sample>,
        info: &SubInfo,
    ) -> ZResult<Arc<SubscriberState>> {
        let mut state = zwrite!(self.state);
        log::trace!("subscribe({:?})", key_expr);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let key_expr = match scope {
            Some(scope) => scope / key_expr,
            None => key_expr.clone(),
        };

        let sub_state = Arc::new(SubscriberState {
            id,
            key_expr: key_expr.clone().into_owned(),
            scope: scope.clone().map(|e| e.into_owned()),
            origin,
            callback,
        });

        #[cfg(not(feature = "unstable"))]
        let declared_sub = origin != Locality::SessionLocal;
        #[cfg(feature = "unstable")]
        let declared_sub = origin != Locality::SessionLocal
            && !key_expr
                .as_str()
                .starts_with(crate::liveliness::PREFIX_LIVELINESS);

        let declared_sub = declared_sub
            .then(|| {
                match state
                .aggregated_subscribers // TODO: can this be an OwnedKeyExpr?
                .iter()
                .find(|s| s.includes( &key_expr))
                {
                    Some(join_sub) => {
                        let joined_sub = state.subscribers.values().any(|s| {
                            s.origin != Locality::SessionLocal && join_sub.includes(&s.key_expr)
                        });
                        (!joined_sub).then(|| join_sub.clone().into())
                    }
                    None => {
                        let twin_sub = state
                            .subscribers
                            .values()
                            .any(|s| s.origin != Locality::SessionLocal && s.key_expr == key_expr);
                        (!twin_sub).then(|| key_expr.clone())
                    }
                }
            })
            .flatten();

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
            //             let expr_id = self.declare_prefix(&key_expr.as_str()[..pos]).res_sync();
            //             WireExpr {
            //                 scope: expr_id,
            //                 suffix: std::borrow::Cow::Borrowed(&key_expr.as_str()[pos..]),
            //             }
            //         }
            //         None => {
            //             let expr_id = self.declare_prefix(key_expr.as_str()).res_sync();
            //             WireExpr {
            //                 scope: expr_id,
            //                 suffix: std::borrow::Cow::Borrowed(""),
            //             }
            //         }
            //     }
            // } else {
            //     key_expr.to_wire(self)
            // };

            primitives.decl_subscriber(&key_expr.to_wire(self), info, None);
        }

        Ok(sub_state)
    }

    pub(crate) fn unsubscribe(&self, sid: usize) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        if let Some(sub_state) = state.subscribers.remove(&sid) {
            trace!("unsubscribe({:?})", sub_state);
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
                    .starts_with(crate::liveliness::PREFIX_LIVELINESS);
            if send_forget {
                // Note: there might be several Subscribers on the same KeyExpr.
                // Before calling forget_subscriber(key_expr), check if this was the last one.
                let key_expr = &sub_state.key_expr;
                match state
                    .aggregated_subscribers
                    .iter()
                    .find(|s| s.includes(key_expr))
                {
                    Some(join_sub) => {
                        let joined_sub = state.subscribers.values().any(|s| {
                            s.origin != Locality::SessionLocal && join_sub.includes(&s.key_expr)
                        });
                        if !joined_sub {
                            let primitives = state.primitives.as_ref().unwrap().clone();
                            let key_expr = WireExpr::from(join_sub).to_owned();
                            drop(state);
                            primitives.forget_subscriber(&key_expr, None);
                        }
                    }
                    None => {
                        let twin_sub = state
                            .subscribers
                            .values()
                            .any(|s| s.origin != Locality::SessionLocal && s.key_expr == *key_expr);
                        if !twin_sub {
                            let primitives = state.primitives.as_ref().unwrap().clone();
                            drop(state);
                            primitives.forget_subscriber(&key_expr.to_wire(self), None);
                        }
                    }
                };
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
        log::trace!("queryable({:?})", key_expr);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let qable_state = Arc::new(QueryableState {
            id,
            key_expr: key_expr.to_owned(),
            complete,
            origin,
            callback,
        });
        #[cfg(feature = "complete_n")]
        {
            state.queryables.insert(id, qable_state.clone());

            if origin != Locality::SessionLocal && complete {
                let primitives = state.primitives.as_ref().unwrap().clone();
                let complete = Session::complete_twin_qabls(&state, key_expr);
                drop(state);
                let qabl_info = QueryableInfo {
                    complete,
                    distance: 0,
                };
                primitives.decl_queryable(key_expr, &qabl_info, None);
            }
        }
        #[cfg(not(feature = "complete_n"))]
        {
            let twin_qabl = Session::twin_qabl(&state, key_expr);
            let complete_twin_qabl = twin_qabl && Session::complete_twin_qabl(&state, key_expr);

            state.queryables.insert(id, qable_state.clone());

            if origin != Locality::SessionLocal && (!twin_qabl || (!complete_twin_qabl && complete))
            {
                let primitives = state.primitives.as_ref().unwrap().clone();
                let complete = ZInt::from(!complete_twin_qabl && complete);
                drop(state);
                let qabl_info = QueryableInfo {
                    complete,
                    distance: 0,
                };
                primitives.decl_queryable(key_expr, &qabl_info, None);
            }
        }
        Ok(qable_state)
    }

    pub(crate) fn twin_qabl(state: &SessionState, key: &WireExpr) -> bool {
        state.queryables.values().any(|q| {
            q.origin != Locality::SessionLocal
                && state.local_wireexpr_to_expr(&q.key_expr).unwrap()
                    == state.local_wireexpr_to_expr(key).unwrap()
        })
    }

    #[cfg(not(feature = "complete_n"))]
    pub(crate) fn complete_twin_qabl(state: &SessionState, key: &WireExpr) -> bool {
        state.queryables.values().any(|q| {
            q.origin != Locality::SessionLocal
                && q.complete
                && state.local_wireexpr_to_expr(&q.key_expr).unwrap()
                    == state.local_wireexpr_to_expr(key).unwrap()
        })
    }

    #[cfg(feature = "complete_n")]
    pub(crate) fn complete_twin_qabls(state: &SessionState, key: &WireExpr) -> ZInt {
        state
            .queryables
            .values()
            .filter(|q| {
                q.origin != Locality::SessionLocal
                    && q.complete
                    && state.local_wireexpr_to_expr(&q.key_expr).unwrap()
                        == state.local_wireexpr_to_expr(key).unwrap()
            })
            .count() as ZInt
    }

    pub(crate) fn close_queryable(&self, qid: usize) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        if let Some(qable_state) = state.queryables.remove(&qid) {
            trace!("close_queryable({:?})", qable_state);
            if qable_state.origin != Locality::SessionLocal {
                let primitives = state.primitives.as_ref().unwrap().clone();
                if Session::twin_qabl(&state, &qable_state.key_expr) {
                    // There still exist Queryables on the same KeyExpr.
                    if qable_state.complete {
                        #[cfg(feature = "complete_n")]
                        {
                            let complete =
                                Session::complete_twin_qabls(&state, &qable_state.key_expr);
                            drop(state);
                            let qabl_info = QueryableInfo {
                                complete,
                                distance: 0,
                            };
                            primitives.decl_queryable(&qable_state.key_expr, &qabl_info, None);
                        }
                        #[cfg(not(feature = "complete_n"))]
                        {
                            if !Session::complete_twin_qabl(&state, &qable_state.key_expr) {
                                drop(state);
                                let qabl_info = QueryableInfo {
                                    complete: 0,
                                    distance: 0,
                                };
                                primitives.decl_queryable(&qable_state.key_expr, &qabl_info, None);
                            }
                        }
                    }
                } else {
                    // There are no more Queryables on the same KeyExpr.
                    drop(state);
                    primitives.forget_queryable(&qable_state.key_expr, None);
                }
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
        log::trace!("declare_liveliness({:?})", key_expr);
        let id = state.decl_id_counter.fetch_add(1, Ordering::SeqCst);
        let key_expr = KeyExpr::from(*crate::liveliness::KE_PREFIX_LIVELINESS / key_expr);
        let tok_state = Arc::new(LivelinessTokenState {
            id,
            key_expr: key_expr.clone().into_owned(),
        });

        state.tokens.insert(tok_state.id, tok_state.clone());
        let primitives = state.primitives.as_ref().unwrap().clone();
        drop(state);
        let sub_info = SubInfo::default();
        primitives.decl_subscriber(&key_expr.to_wire(self), &sub_info, None);
        Ok(tok_state)
    }

    #[zenoh_macros::unstable]
    pub(crate) fn undeclare_liveliness(&self, tid: usize) -> ZResult<()> {
        let mut state = zwrite!(self.state);
        if let Some(tok_state) = state.tokens.remove(&tid) {
            trace!("undeclare_liveliness({:?})", tok_state);
            // Note: there might be several Tokens on the same KeyExpr.
            let key_expr = &tok_state.key_expr;
            let twin_tok = state.tokens.values().any(|s| s.key_expr == *key_expr);
            if !twin_tok {
                let primitives = state.primitives.as_ref().unwrap().clone();
                drop(state);
                primitives.forget_subscriber(&key_expr.to_wire(self), None);
            }
            Ok(())
        } else {
            Err(zerror!("Unable to find liveliness token").into())
        }
    }

    pub(crate) fn handle_data(
        &self,
        local: bool,
        key_expr: &WireExpr,
        info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        let mut callbacks = SingleOrVec::default();
        let state = zread!(self.state);
        if key_expr.suffix.is_empty() {
            match state.get_res(&key_expr.scope, local) {
                Some(Resource::Node(res)) => {
                    for sub in &res.subscribers {
                        if sub.origin == Locality::Any
                            || (local == (sub.origin == Locality::SessionLocal))
                        {
                            match &sub.scope {
                                Some(scope) => {
                                    if !res.key_expr.starts_with(&***scope) {
                                        log::warn!(
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
                                                log::warn!(
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
                    log::error!(
                        "Received Data for `{}`, which isn't a key expression",
                        prefix
                    );
                    return;
                }
                None => {
                    log::error!("Received Data for unknown expr_id: {}", key_expr.scope);
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
                                        log::warn!(
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
                                                log::warn!(
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
                    log::error!("Received Data for unkown key_expr: {}", err);
                    return;
                }
            }
        };
        drop(state);
        let zenoh_collections::single_or_vec::IntoIter { drain, last } = callbacks.into_iter();
        for (cb, key_expr) in drain {
            cb(Sample::with_info(key_expr, payload.clone(), info.clone()));
        }
        if let Some((cb, key_expr)) = last {
            cb(Sample::with_info(key_expr, payload, info));
        }
    }

    pub(crate) fn pull<'a>(&'a self, key_expr: &'a KeyExpr) -> impl Resolve<ZResult<()>> + 'a {
        ResolveClosure::new(move || {
            trace!("pull({:?})", key_expr);
            let state = zread!(self.state);
            let primitives = state.primitives.as_ref().unwrap().clone();
            drop(state);
            primitives.send_pull(true, &key_expr.to_wire(self), 0, &None);
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn query(
        &self,
        selector: &Selector<'_>,
        scope: &Option<KeyExpr<'_>>,
        target: QueryTarget,
        consolidation: QueryConsolidation,
        destination: Locality,
        timeout: Duration,
        value: Option<Value>,
        callback: Callback<'static, Reply>,
    ) -> ZResult<()> {
        log::trace!("get({}, {:?}, {:?})", selector, target, consolidation);
        let mut state = zwrite!(self.state);
        let consolidation = match consolidation.mode {
            Mode::Auto => {
                if selector.decode().any(|(k, _)| k.as_ref() == TIME_RANGE_KEY) {
                    ConsolidationMode::None
                } else {
                    ConsolidationMode::Latest
                }
            }
            Mode::Manual(mode) => mode,
        };
        let qid = state.qid_counter.fetch_add(1, Ordering::SeqCst);
        let nb_final = match destination {
            Locality::Any => 2,
            _ => 1,
        };
        task::spawn({
            let state = self.state.clone();
            let zid = self.runtime.zid;
            async move {
                task::sleep(timeout).await;
                let mut state = zwrite!(state);
                if let Some(query) = state.queries.remove(&qid) {
                    std::mem::drop(state);
                    log::debug!("Timout on query {}! Send error and close.", qid);
                    if query.reception_mode == ConsolidationMode::Latest {
                        for (_, reply) in query.replies.unwrap().into_iter() {
                            (query.callback)(reply);
                        }
                    }
                    (query.callback)(Reply {
                        sample: Err("Timeout".into()),
                        replier_id: zid,
                    });
                }
            }
        });

        let selector = match scope {
            Some(scope) => Selector {
                key_expr: scope / &*selector.key_expr,
                parameters: selector.parameters.clone(),
            },
            None => selector.clone(),
        };

        log::trace!("Register query {} (nb_final = {})", qid, nb_final);
        let wexpr = selector.key_expr.to_wire(self);
        state.queries.insert(
            qid,
            QueryState {
                nb_final,
                selector: selector.clone().into_owned(),
                scope: scope.clone().map(|e| e.into_owned()),
                reception_mode: consolidation,
                replies: (consolidation != ConsolidationMode::None).then(HashMap::new),
                callback,
            },
        );

        let primitives = state.primitives.as_ref().unwrap().clone();

        drop(state);
        if destination != Locality::SessionLocal {
            primitives.send_query(
                &wexpr,
                selector.parameters(),
                qid,
                target,
                consolidation,
                value.as_ref().map(|v| {
                    let data_info = DataInfo {
                        encoding: Some(v.encoding.clone()),
                        ..Default::default()
                    };
                    QueryBody {
                        data_info,
                        payload: v.payload.clone(),
                    }
                }),
                None,
            );
        }
        if destination != Locality::Remote {
            self.handle_query(
                true,
                &wexpr,
                selector.parameters(),
                qid,
                target,
                consolidation,
                value.map(|v| {
                    let data_info = DataInfo {
                        encoding: Some(v.encoding),
                        ..Default::default()
                    };
                    QueryBody {
                        data_info,
                        payload: v.payload,
                    }
                }),
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
        qid: ZInt,
        _target: QueryTarget,
        _consolidation: ConsolidationMode,
        body: Option<QueryBody>,
    ) {
        let (primitives, key_expr, callbacks) = {
            let state = zread!(self.state);
            match state.wireexpr_to_keyexpr(key_expr, local) {
                Ok(key_expr) => {
                    let callbacks = state
                        .queryables
                        .values()
                        .filter(
                            |queryable|
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
                        .map(|qable| qable.callback.clone())
                        .collect::<Vec<Arc<dyn Fn(Query) + Send + Sync>>>();
                    (
                        state.primitives.as_ref().unwrap().clone(),
                        key_expr.into_owned(),
                        callbacks,
                    )
                }
                Err(err) => {
                    error!("Received Query for unkown key_expr: {}", err);
                    return;
                }
            }
        };

        let parameters = parameters.to_owned();

        let zid = self.runtime.zid; // @TODO build/use prebuilt specific zid

        let query = Query {
            inner: Arc::new(QueryInner {
                key_expr,
                parameters,
                value: body.map(|b| Value {
                    payload: b.payload,
                    encoding: b.data_info.encoding.unwrap_or_default(),
                }),
                qid,
                zid,
                primitives: if local {
                    Arc::new(self.clone())
                } else {
                    primitives
                },
            }),
        };
        for callback in callbacks.iter() {
            callback(query.clone());
        }
    }
}

impl SessionDeclarations for Arc<Session> {
    /// Create a [`Subscriber`](Subscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The resourkey expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received: {:?}", sample);
    ///     }
    /// }).await;
    /// # })
    /// ```
    fn declare_subscriber<'b, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> SubscriberBuilder<'static, 'b, PushMode, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        SubscriberBuilder {
            session: SessionRef::Shared(self.clone()),
            key_expr: key_expr.try_into().map_err(Into::into),
            reliability: Reliability::default(),
            mode: PushMode,
            origin: Locality::default(),
            handler: DefaultHandler,
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
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let queryable = session.declare_queryable("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
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
    fn declare_queryable<'b, TryIntoKeyExpr>(
        &self,
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
            handler: DefaultHandler,
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
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// publisher.put("value").res().await.unwrap();
    /// # })
    /// ```
    fn declare_publisher<'b, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> PublisherBuilder<'static, 'b>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'b>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'b>>>::Error: Into<zenoh_result::Error>,
    {
        PublisherBuilder {
            session: SessionRef::Shared(self.clone()),
            key_expr: key_expr.try_into().map_err(Into::into),
            congestion_control: CongestionControl::default(),
            priority: Priority::default(),
            destination: Locality::default(),
        }
    }

    /// Obtain a [`Liveliness`] struct tied to this Zenoh [`Session`].
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[zenoh_macros::unstable]
    fn liveliness(&self) -> Liveliness<'static> {
        Liveliness {
            session: SessionRef::Shared(self.clone()),
        }
    }
}

impl Primitives for Session {
    fn decl_resource(&self, expr_id: ZInt, wire_expr: &WireExpr) {
        trace!("recv Decl Resource {} {:?}", expr_id, wire_expr);
        let state = &mut zwrite!(self.state);
        match state.remote_key_to_expr(wire_expr) {
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

                state.remote_resources.insert(expr_id, res);
            }
            Err(e) => error!(
                "Received Resource for invalid wire_expr `{}`: {}",
                wire_expr, e
            ),
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
        key_expr: &WireExpr,
        _sub_info: &SubInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Decl Subscriber {:?} , {:?}", key_expr, _sub_info);
        #[cfg(feature = "unstable")]
        {
            let state = zread!(self.state);
            match state.wireexpr_to_keyexpr(key_expr, false) {
                Ok(expr) => {
                    if expr
                        .as_str()
                        .starts_with(crate::liveliness::PREFIX_LIVELINESS)
                    {
                        drop(state);
                        self.handle_data(false, key_expr, None, ZBuf::default());
                    }
                }
                Err(err) => log::error!("Received Forget Subscriber for unkown key_expr: {}", err),
            }
        }
    }

    fn forget_subscriber(&self, key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {
        trace!("recv Forget Subscriber {:?}", key_expr);
        #[cfg(feature = "unstable")]
        {
            let state = zread!(self.state);
            match state.wireexpr_to_keyexpr(key_expr, false) {
                Ok(expr) => {
                    if expr
                        .as_str()
                        .starts_with(crate::liveliness::PREFIX_LIVELINESS)
                    {
                        drop(state);
                        let data_info = DataInfo {
                            kind: SampleKind::Delete,
                            ..Default::default()
                        };
                        self.handle_data(false, key_expr, Some(data_info), ZBuf::default());
                    }
                }
                Err(err) => log::error!("Received Forget Subscriber for unkown key_expr: {}", err),
            }
        }
    }

    fn decl_queryable(
        &self,
        _key_expr: &WireExpr,
        _qabl_info: &QueryableInfo,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!("recv Decl Queryable {:?}", _key_expr);
    }

    fn forget_queryable(&self, _key_expr: &WireExpr, _routing_context: Option<RoutingContext>) {
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
        self.handle_data(false, key_expr, info, payload)
    }

    fn send_query(
        &self,
        key_expr: &WireExpr,
        parameters: &str,
        qid: ZInt,
        target: QueryTarget,
        consolidation: ConsolidationMode,
        body: Option<QueryBody>,
        _routing_context: Option<RoutingContext>,
    ) {
        trace!(
            "recv Query {:?} {:?} {:?} {:?}",
            key_expr,
            parameters,
            target,
            consolidation
        );
        self.handle_query(
            false,
            key_expr,
            parameters,
            qid,
            target,
            consolidation,
            body,
        )
    }

    fn send_reply_data(
        &self,
        qid: ZInt,
        replier_id: ZenohId,
        key_expr: WireExpr,
        data_info: Option<DataInfo>,
        payload: ZBuf,
    ) {
        trace!(
            "recv ReplyData {:?} {:?} {:?} {:?} {:?}",
            qid,
            replier_id,
            key_expr,
            data_info,
            payload
        );
        let mut state = zwrite!(self.state);
        let key_expr = match state.remote_key_to_expr(&key_expr) {
            Ok(key) => key.into_owned(),
            Err(e) => {
                error!("Received ReplyData for unkown key_expr: {}", e);
                return;
            }
        };
        match state.queries.get_mut(&qid) {
            Some(query) => {
                if !matches!(
                    query
                        .selector
                        .parameters()
                        .get_bools([crate::query::_REPLY_KEY_EXPR_ANY_SEL_PARAM]),
                    Ok([true])
                ) && !query.selector.key_expr.intersects(&key_expr)
                {
                    log::warn!(
                        "Received ReplyData for `{}` from `{:?}, which didn't match query `{}`: dropping ReplyData.",
                        key_expr,
                        replier_id,
                        query.selector
                    );
                    return;
                }
                let key_expr = match &query.scope {
                    Some(scope) => {
                        if !key_expr.starts_with(&***scope) {
                            log::warn!(
                                "Received ReplyData for `{}` from `{:?}, which didn't start with scope `{}`: dropping ReplyData.",
                                key_expr,
                                replier_id,
                                scope,
                            );
                            return;
                        }
                        match KeyExpr::try_from(&key_expr[(scope.len() + 1)..]) {
                            Ok(key_expr) => key_expr,
                            Err(e) => {
                                log::warn!(
                                    "Error unscoping received ReplyData for `{}` from `{:?}: {}",
                                    key_expr,
                                    replier_id,
                                    e,
                                );
                                return;
                            }
                        }
                    }
                    None => key_expr,
                };
                let new_reply = Reply {
                    sample: Ok(Sample::with_info(key_expr.into_owned(), payload, data_info)),
                    replier_id,
                };
                let callback = match query.reception_mode {
                    ConsolidationMode::None => Some((query.callback.clone(), new_reply)),
                    ConsolidationMode::Monotonic => {
                        match query
                            .replies
                            .as_ref()
                            .unwrap()
                            .get(new_reply.sample.as_ref().unwrap().key_expr.as_keyexpr())
                        {
                            Some(reply) => {
                                if new_reply.sample.as_ref().unwrap().timestamp
                                    > reply.sample.as_ref().unwrap().timestamp
                                {
                                    query.replies.as_mut().unwrap().insert(
                                        new_reply.sample.as_ref().unwrap().key_expr.clone().into(),
                                        new_reply.clone(),
                                    );
                                    Some((query.callback.clone(), new_reply))
                                } else {
                                    None
                                }
                            }
                            None => {
                                query.replies.as_mut().unwrap().insert(
                                    new_reply.sample.as_ref().unwrap().key_expr.clone().into(),
                                    new_reply.clone(),
                                );
                                Some((query.callback.clone(), new_reply))
                            }
                        }
                    }
                    ConsolidationMode::Latest => {
                        match query
                            .replies
                            .as_ref()
                            .unwrap()
                            .get(new_reply.sample.as_ref().unwrap().key_expr.as_keyexpr())
                        {
                            Some(reply) => {
                                if new_reply.sample.as_ref().unwrap().timestamp
                                    > reply.sample.as_ref().unwrap().timestamp
                                {
                                    query.replies.as_mut().unwrap().insert(
                                        new_reply.sample.as_ref().unwrap().key_expr.clone().into(),
                                        new_reply,
                                    );
                                }
                            }
                            None => {
                                query.replies.as_mut().unwrap().insert(
                                    new_reply.sample.as_ref().unwrap().key_expr.clone().into(),
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
                log::warn!("Received ReplyData for unkown Query: {}", qid);
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
                    std::mem::drop(state);
                    if query.reception_mode == ConsolidationMode::Latest {
                        for (_, reply) in query.replies.unwrap().into_iter() {
                            (query.callback)(reply);
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
        f.debug_struct("Session").field("id", &self.zid()).finish()
    }
}

/// Functions to create zenoh entities with `'static` lifetime.
///
/// This trait contains functions to create zenoh entities like
/// [`Subscriber`](crate::subscriber::Subscriber), and
/// [`Queryable`](crate::queryable::Queryable) with a `'static` lifetime.
/// This is useful to move zenoh entities to several threads and tasks.
///
/// This trait is implemented for `Arc<Session>`.
///
/// # Examples
/// ```no_run
/// # async_std::task::block_on(async {
/// use zenoh::prelude::r#async::*;
///
/// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
/// let subscriber = session.declare_subscriber("key/expression")
///     .res()
///     .await
///     .unwrap();
/// async_std::task::spawn(async move {
///     while let Ok(sample) = subscriber.recv_async().await {
///         println!("Received: {:?}", sample);
///     }
/// }).await;
/// # })
/// ```
pub trait SessionDeclarations {
    /// Create a [`Subscriber`](crate::subscriber::Subscriber) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The resourkey expression to subscribe to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let subscriber = session.declare_subscriber("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// async_std::task::spawn(async move {
    ///     while let Ok(sample) = subscriber.recv_async().await {
    ///         println!("Received: {:?}", sample);
    ///     }
    /// }).await;
    /// # })
    /// ```
    fn declare_subscriber<'a, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> SubscriberBuilder<'static, 'a, PushMode, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'a>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_result::Error>;

    /// Create a [`Queryable`](crate::queryable::Queryable) for the given key expression.
    ///
    /// # Arguments
    ///
    /// * `key_expr` - The key expression matching the queries the
    /// [`Queryable`](crate::queryable::Queryable) will reply to
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let queryable = session.declare_queryable("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
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
    fn declare_queryable<'a, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> QueryableBuilder<'static, 'a, DefaultHandler>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'a>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_result::Error>;

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
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let publisher = session.declare_publisher("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// publisher.put("value").res().await.unwrap();
    /// # })
    /// ```
    fn declare_publisher<'a, TryIntoKeyExpr>(
        &self,
        key_expr: TryIntoKeyExpr,
    ) -> PublisherBuilder<'static, 'a>
    where
        TryIntoKeyExpr: TryInto<KeyExpr<'a>>,
        <TryIntoKeyExpr as TryInto<KeyExpr<'a>>>::Error: Into<zenoh_result::Error>;

    /// Obtain a [`Liveliness`] struct tied to this Zenoh [`Session`].
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::prelude::r#async::*;
    ///
    /// let session = zenoh::open(config::peer()).res().await.unwrap().into_arc();
    /// let liveliness = session
    ///     .liveliness()
    ///     .declare_token("key/expression")
    ///     .res()
    ///     .await
    ///     .unwrap();
    /// # })
    /// ```
    #[zenoh_macros::unstable]
    fn liveliness(&self) -> Liveliness<'static>;
}
