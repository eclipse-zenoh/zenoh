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
use crate::net::Session;
use async_std::channel::{Receiver, Sender, TrySendError};
use async_std::stream::Stream;
use async_std::sync::{Arc, RwLock};
use async_std::task;
use pin_project_lite::pin_project;
use std::collections::HashMap;
use std::fmt;

/// A read-only bytes buffer.
pub use super::protocol::io::RBuf;

/// A writable bytes buffer.
pub use super::protocol::io::WBuf;

// Shared memory with zero-copy support
#[cfg(feature = "zero-copy")]
pub use super::protocol::io::SharedMemoryBuf;
#[cfg(feature = "zero-copy")]
pub use super::protocol::io::SharedMemoryBufInfo;
#[cfg(feature = "zero-copy")]
pub use super::protocol::io::SharedMemoryManager;

/// A numerical Id mapped to a resource name with [declare_resource](Session::declare_resource).
pub use super::protocol::core::ResourceId;

/// Informations to configure a subscription.
pub use super::protocol::core::SubInfo;

/// The global unique id of a zenoh peer.
pub use super::protocol::core::PeerId;

/// A time period.
pub use super::protocol::core::Period;

/// The [Queryable](Queryable)s that should be target of a [query](Session::query).
pub use super::protocol::core::Target;

/// The [Queryable](Queryable)s that should be target of a [query](Session::query).
pub use super::protocol::core::QueryTarget;

/// The kind of consolidation.
pub use super::protocol::core::ConsolidationMode;

/// The kind of consolidation that should be applied on replies to a [query](Session::query)
/// at different stages of the reply process.
pub use super::protocol::core::QueryConsolidation;

/// The kind of congestion control.
pub use super::protocol::core::CongestionControl;

/// The kind of reliability.
pub use super::protocol::core::Reliability;

/// A resource key.
pub use super::protocol::core::ResKey;

/// The subscription mode.
pub use super::protocol::core::SubMode;

/// A zenoh integer.
pub use super::protocol::core::ZInt;

pub use super::protocol::core::whatami;

/// A zenoh Hello message.
pub use super::protocol::proto::Hello;

/// Some informations about the associated data.
///
/// # Examples
/// ```
/// # use zenoh::net::protocol::io::RBuf;
/// # use zenoh::net::protocol::proto::DataInfo;
/// # let sample = zenoh::net::Sample { res_name: "".to_string(), payload: RBuf::new(), data_info: None };
/// if let Some(info) = sample.data_info {
///     match info.timestamp {
///         Some(ts) => println!("Sample's timestamp: {}", ts),
///         None => println!("Sample has no timestamp"),
///     }
/// }
/// ```
pub use super::protocol::proto::DataInfo;

/// A zenoh error.
pub use zenoh_util::core::ZError;

/// The kind of zenoh error.
pub use zenoh_util::core::ZErrorKind;

/// A zenoh result.
pub use zenoh_util::core::ZResult;

pin_project! {
    /// A stream of [Hello](Hello) messages.
    #[derive(Clone, Debug)]
    pub struct HelloStream {
        #[pin]
        pub(crate) hello_receiver: Receiver<Hello>,
        pub(crate) stop_sender: Sender<()>,
    }
}

impl Stream for HelloStream {
    type Item = Hello;

    #[inline(always)]
    fn poll_next(
        self: async_std::pin::Pin<&mut Self>,
        cx: &mut async_std::task::Context,
    ) -> async_std::task::Poll<Option<Self::Item>> {
        self.project().hello_receiver.poll_next(cx)
    }
}

/// A zenoh value.
#[derive(Debug, Clone)]
pub struct Sample {
    pub res_name: String,
    pub payload: RBuf,
    pub data_info: Option<DataInfo>,
}

/// The callback that will be called on each data for a [CallbackSubscriber](CallbackSubscriber).
pub type DataHandler = dyn FnMut(Sample) + Send + Sync + 'static;

/// Structs received b y a [Queryable](Queryable).
pub struct Query {
    pub res_name: String,
    pub predicate: String,
    pub replies_sender: RepliesSender,
}

impl Query {
    #[inline(always)]
    pub async fn reply(&'_ self, msg: Sample) {
        self.replies_sender.send(msg).await
    }

    #[inline(always)]
    pub fn try_reply(&self, msg: Sample) -> Result<(), TrySendError<Sample>> {
        self.replies_sender.try_send(msg)
    }
}

impl fmt::Debug for Query {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Query{{ res_name: '{}', predicate: '{}' }}",
            self.res_name, self.predicate
        )
    }
}

/// Structs returned by a [query](Session::query).
#[derive(Clone, Debug)]
pub struct Reply {
    pub data: Sample,
    pub source_kind: ZInt,
    pub replier_id: PeerId,
}

#[derive(Clone, Debug)]
pub(crate) struct QueryState {
    pub(crate) nb_final: usize,
    pub(crate) reception_mode: ConsolidationMode,
    pub(crate) replies: Option<HashMap<String, Reply>>,
    pub(crate) rep_sender: Sender<Reply>,
}

pub(crate) type Id = usize;

#[derive(Debug)]
pub(crate) struct PublisherState {
    pub(crate) id: Id,
    pub(crate) reskey: ResKey,
}

/// A publisher.
///
/// Publishers are automatically undeclared when dropped.
pub struct Publisher<'a> {
    pub(crate) session: &'a Session,
    pub(crate) state: Arc<PublisherState>,
    pub(crate) alive: bool,
}

impl Publisher<'_> {
    /// Undeclare a [Publisher](Publisher) previously declared with [declare_publisher](Session::declare_publisher).
    ///
    /// Publishers are automatically undeclared when dropped, but you may want to use this function to handle errors or
    /// undeclare the Publisher asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let publisher = session.declare_publisher(&"/resource/name".into()).await.unwrap();
    /// publisher.undeclare().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub async fn undeclare(mut self) -> ZResult<()> {
        self.alive = false;
        Session::undeclare_publisher(self.session, self.state.id).await
    }
}

impl Drop for Publisher<'_> {
    fn drop(&mut self) {
        if self.alive {
            let session = self.session.clone();
            let id = self.state.id;
            let _ = task::block_on(async move {
                task::spawn_blocking(move || {
                    task::block_on(Session::undeclare_publisher(&session, id))
                })
                .await
            });
        }
    }
}

impl fmt::Debug for Publisher<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}

pub(crate) enum SubscriberInvoker {
    Sender(Sender<Sample>),
    Handler(Arc<RwLock<DataHandler>>),
}

pub(crate) struct SubscriberState {
    pub(crate) id: Id,
    pub(crate) reskey: ResKey,
    pub(crate) resname: String,
    pub(crate) invoker: SubscriberInvoker,
}

impl fmt::Debug for SubscriberState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Subscriber{{ id:{}, resname:{} }}",
            self.id, self.resname
        )
    }
}

/// A subscriber that provides data through a stream.
///
/// Subscribers are automatically undeclared when dropped.
pub struct Subscriber<'a> {
    pub(crate) session: &'a Session,
    pub(crate) state: Arc<SubscriberState>,
    pub(crate) alive: bool,
    pub(crate) receiver: Receiver<Sample>,
}

impl Subscriber<'_> {
    /// Get the stream from a [Subscriber](Subscriber).
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// # let sub_info = SubInfo {
    /// #    reliability: Reliability::Reliable,
    /// #    mode: SubMode::Push,
    /// #    period: None,
    /// # };
    /// let mut subscriber = session.declare_subscriber(&"/resource/name".into(), &sub_info).await.unwrap();
    /// while let Some(sample) = subscriber.stream().next().await {
    ///     println!("Received : {:?}", sample);
    /// }
    /// # })
    /// ```
    #[inline]
    pub fn stream(&mut self) -> &mut Receiver<Sample> {
        &mut self.receiver
    }

    /// Pull available data for a pull-mode [Subscriber](Subscriber).
    ///
    /// # Examples
    /// ```
    /// #![feature(async_closure)]
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// # let sub_info = SubInfo {
    /// #     reliability: Reliability::Reliable,
    /// #     mode: SubMode::Pull,
    /// #     period: None
    /// # };
    /// let mut subscriber = session.declare_subscriber(&"/resource/name".into(), &sub_info).await.unwrap();
    /// async_std::task::spawn(subscriber.stream().clone().for_each(
    ///     async move |sample| { println!("Received : {:?}", sample); }
    /// ));
    /// subscriber.pull();
    /// # })
    /// ```
    pub async fn pull(&self) -> ZResult<()> {
        self.session.pull(&self.state.reskey).await
    }

    /// Undeclare a [Subscriber](Subscriber) previously declared with [declare_subscriber](Session::declare_subscriber).
    ///
    /// Subscribers are automatically undeclared when dropped, but you may want to use this function to handle errors or
    /// undeclare the Subscriber asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// # let sub_info = SubInfo {
    /// #     reliability: Reliability::Reliable,
    /// #     mode: SubMode::Push,
    /// #     period: None
    /// # };
    /// let subscriber = session.declare_subscriber(&"/resource/name".into(), &sub_info).await.unwrap();
    /// subscriber.undeclare().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub async fn undeclare(mut self) -> ZResult<()> {
        self.alive = false;
        self.session.undeclare_subscriber(self.state.id).await
    }
}

impl Drop for Subscriber<'_> {
    fn drop(&mut self) {
        if self.alive {
            let session = self.session.clone();
            let id = self.state.id;
            let _ = task::block_on(async move {
                task::spawn_blocking(move || {
                    task::block_on(Session::undeclare_subscriber(&session, id))
                })
                .await
            });
        }
    }
}

impl fmt::Debug for Subscriber<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}

/// A subscriber that provides data through a callback.
///
/// Subscribers are automatically undeclared when dropped.
pub struct CallbackSubscriber<'a> {
    pub(crate) session: &'a Session,
    pub(crate) state: Arc<SubscriberState>,
    pub(crate) alive: bool,
}

impl CallbackSubscriber<'_> {
    /// Pull available data for a pull-mode [CallbackSubscriber](CallbackSubscriber).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// # let sub_info = SubInfo {
    /// #     reliability: Reliability::Reliable,
    /// #     mode: SubMode::Pull,
    /// #     period: None
    /// # };
    /// let subscriber = session.declare_callback_subscriber(&"/resource/name".into(), &sub_info,
    ///     |sample| { println!("Received : {} {}", sample.res_name, sample.payload); }
    /// ).await.unwrap();
    /// subscriber.pull();
    /// # })
    /// ```
    pub async fn pull(&self) -> ZResult<()> {
        self.session.pull(&self.state.reskey).await
    }

    /// Undeclare a [CallbackSubscriber](CallbackSubscriber) previously declared with [declare_callback_subscriber](Session::declare_callback_subscriber).
    ///
    /// CallbackSubscribers are automatically undeclared when dropped, but you may want to use this function to handle errors or
    /// undeclare the CallbackSubscriber asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// # let sub_info = SubInfo {
    /// #     reliability: Reliability::Reliable,
    /// #     mode: SubMode::Push,
    /// #     period: None
    /// # };
    /// # fn data_handler(_sample: Sample) { };
    /// let subscriber = session.declare_callback_subscriber(&"/resource/name".into(), &sub_info, data_handler).await.unwrap();
    /// subscriber.undeclare().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub async fn undeclare(mut self) -> ZResult<()> {
        self.alive = false;
        self.session.undeclare_subscriber(self.state.id).await
    }
}

impl Drop for CallbackSubscriber<'_> {
    fn drop(&mut self) {
        if self.alive {
            let session = self.session.clone();
            let id = self.state.id;
            let _ = task::block_on(async move {
                task::spawn_blocking(move || {
                    task::block_on(Session::undeclare_subscriber(&session, id))
                })
                .await
            });
        }
    }
}

impl fmt::Debug for CallbackSubscriber<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}

pub(crate) struct QueryableState {
    pub(crate) id: Id,
    pub(crate) reskey: ResKey,
    pub(crate) kind: ZInt,
    pub(crate) q_sender: Sender<Query>,
}

impl fmt::Debug for QueryableState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Queryable{{ id:{}, reskey:{} }}", self.id, self.reskey)
    }
}

/// An entity able to reply to queries.
///
/// Queryables are automatically undeclared when dropped.
pub struct Queryable<'a> {
    pub(crate) session: &'a Session,
    pub(crate) state: Arc<QueryableState>,
    pub(crate) alive: bool,
    pub(crate) q_receiver: Receiver<Query>,
}

impl Queryable<'_> {
    /// Get the stream from a [Queryable](Queryable).
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    /// use zenoh::net::queryable::EVAL;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let mut queryable = session.declare_queryable(&"/resource/name".into(), EVAL).await.unwrap();
    /// while let Some(query) = queryable.stream().next().await {
    ///     query.reply(Sample{
    ///         res_name: "/resource/name".to_string(),
    ///         payload: "value".as_bytes().into(),
    ///         data_info: None,
    ///     }).await;
    /// }
    /// # })
    /// ```
    #[inline]
    pub fn stream(&mut self) -> &mut Receiver<Query> {
        &mut self.q_receiver
    }

    /// Undeclare a [Queryable](Queryable) previously declared with [declare_queryable](Session::declare_queryable).
    ///
    /// Queryables are automatically undeclared when dropped, but you may want to use this function to handle errors or
    /// undeclare the Queryable asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::net::*;
    /// use zenoh::net::queryable::EVAL;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let queryable = session.declare_queryable(&"/resource/name".into(), EVAL).await.unwrap();
    /// queryable.undeclare().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub async fn undeclare(mut self) -> ZResult<()> {
        self.alive = false;
        self.session.undeclare_queryable(self.state.id).await
    }
}

impl Drop for Queryable<'_> {
    fn drop(&mut self) {
        if self.alive {
            let session = self.session.clone();
            let id = self.state.id;
            let _ = task::block_on(async move {
                task::spawn_blocking(move || {
                    task::block_on(Session::undeclare_queryable(&session, id))
                })
                .await
            });
        }
    }
}

impl fmt::Debug for Queryable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}

/// Struct used by a [Queryable](Queryable) to send replies to queries.
#[derive(Clone)]
pub struct RepliesSender {
    pub(crate) kind: ZInt,
    pub(crate) sender: Sender<(ZInt, Sample)>,
}

impl RepliesSender {
    #[inline(always)]
    pub async fn send(&'_ self, msg: Sample) {
        let _ = self.sender.send((self.kind, msg)).await;
    }

    #[inline(always)]
    pub fn try_send(&self, msg: Sample) -> Result<(), TrySendError<Sample>> {
        match self.sender.try_send((self.kind, msg)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(sample)) => Err(TrySendError::Full(sample.1)),
            Err(TrySendError::Closed(sample)) => Err(TrySendError::Closed(sample.1)),
        }
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.sender.capacity().unwrap_or(0)
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.sender.is_empty()
    }

    #[inline(always)]
    pub fn is_full(&self) -> bool {
        self.sender.is_full()
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.sender.len()
    }
}
