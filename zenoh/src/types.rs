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
use crate::utils::new_reception_timestamp;
use crate::Session;
use async_std::sync::Arc;
use flume::*;
use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use std::sync::RwLock;
use std::task::{Context, Poll};
use uhlc::Timestamp;

/// A read-only bytes buffer.
pub use super::net::protocol::io::{ZBuf, ZSlice};

/// A writable bytes buffer.
pub use super::net::protocol::io::WBuf;

// Shared memory with zero-copy support
#[cfg(feature = "zero-copy")]
pub use super::net::protocol::io::SharedMemoryBuf;
#[cfg(feature = "zero-copy")]
pub use super::net::protocol::io::SharedMemoryBufInfo;
#[cfg(feature = "zero-copy")]
pub use super::net::protocol::io::SharedMemoryManager;

/// A numerical Id mapped to a resource name with [register_resource](Session::register_resource).
pub use super::net::protocol::core::ResourceId;

/// Informations to configure a subscription.
pub use super::net::protocol::core::SubInfo;

/// The global unique id of a zenoh peer.
pub use super::net::protocol::core::PeerId;

/// A time period.
pub use super::net::protocol::core::Period;

/// The [Queryable](Queryable)s that should be target of a [query](Session::query).
pub use super::net::protocol::core::Target;

/// The [Queryable](Queryable)s that should be target of a [query](Session::query).
pub use super::net::protocol::core::QueryTarget;

/// The kind of consolidation.
pub use super::net::protocol::core::ConsolidationMode;

/// The kind of consolidation that should be applied on replies to a [query](Session::query)
/// at different stages of the reply process.
pub use super::net::protocol::core::QueryConsolidation;

/// The kind of congestion control.
pub use super::net::protocol::core::CongestionControl;

/// The kind of reliability.
pub use super::net::protocol::core::Reliability;

/// A resource key.
pub use super::net::protocol::core::ResKey;

/// The subscription mode.
pub use super::net::protocol::core::SubMode;

/// A zenoh integer.
pub use super::net::protocol::core::ZInt;

pub use super::net::protocol::core::whatami;

/// A zenoh Hello message.
pub use super::net::protocol::proto::Hello;

pub use zenoh_util::sync::channel::Iter;
pub use zenoh_util::sync::channel::Receiver;
pub use zenoh_util::sync::channel::RecvError;
pub use zenoh_util::sync::channel::RecvTimeoutError;
pub use zenoh_util::sync::channel::TryIter;
pub use zenoh_util::sync::channel::TryRecvError;
pub use zenoh_util::sync::zready;
pub use zenoh_util::sync::ZFuture;
pub use zenoh_util::sync::ZPinBoxFuture;
pub use zenoh_util::sync::ZReady;

/// Some informations about the associated data.
///
/// # Examples
/// ```
/// # use zenoh::ZBuf;
/// # use zenoh::DataInfo;
/// # let sample = zenoh::Sample { res_name: "".to_string(), payload: ZBuf::new(), data_info: None };
/// if let Some(info) = sample.data_info {
///     match info.timestamp {
///         Some(ts) => println!("Sample's timestamp: {}", ts),
///         None => println!("Sample has no timestamp"),
///     }
/// }
/// ```
pub use super::net::protocol::proto::DataInfo;

/// A zenoh error.
pub use zenoh_util::core::ZError;

/// The kind of zenoh error.
pub use zenoh_util::core::ZErrorKind;

/// A zenoh result.
pub use zenoh_util::core::ZResult;

zreceiver! {
    #[derive(Clone)]
    pub struct HelloReceiver : Receiver<Hello> {
        pub(crate) stop_sender: Sender<()>,
    }
}

/// A zenoh value.
#[derive(Debug, Clone)]
pub struct Sample {
    pub res_name: String,
    pub payload: ZBuf,
    pub data_info: Option<DataInfo>,
}

impl Sample {
    /// Returns the associated Timestamp, if any.
    pub fn get_timestamp(&self) -> Option<&Timestamp> {
        self.data_info
            .as_ref()
            .and_then(|info| info.timestamp.as_ref())
    }

    /// Ensure that an associated Timestamp is present in this Sample.
    /// If not, a new one is created with the current system time and 0x00 as id.
    pub fn ensure_timestamp(&mut self) {
        if let Some(data_info) = &mut self.data_info {
            if data_info.timestamp.is_none() {
                data_info.timestamp = Some(new_reception_timestamp());
            }
        } else {
            let data_info = DataInfo {
                timestamp: Some(new_reception_timestamp()),
                ..Default::default()
            };
            self.data_info = Some(data_info);
        }
    }
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
    pub fn reply(&'_ self, msg: Sample) {
        self.replies_sender.send(msg)
    }

    #[inline(always)]
    pub fn try_reply(&self, msg: Sample) -> Result<(), TrySendError<Sample>> {
        self.replies_sender.try_send(msg)
    }

    #[inline(always)]
    pub async fn reply_async(&'_ self, msg: Sample) {
        self.replies_sender.send_async(msg).await
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
    pub replier_kind: ZInt,
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
/// Publishers are automatically unregistered when dropped.
pub struct Publisher<'a> {
    pub(crate) session: &'a Session,
    pub(crate) state: Arc<PublisherState>,
    pub(crate) alive: bool,
}

impl Publisher<'_> {
    /// Undeclare a [Publisher](Publisher) previously declared with [publishing](Session::publishing).
    ///
    /// Publishers are automatically unregistered when dropped, but you may want to use this function to handle errors or
    /// unregister the Publisher asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let publisher = session.publishing(&"/resource/name".into()).await.unwrap();
    /// publisher.unregister().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn unregister(mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.alive = false;
        self.session.unpublishing(self.state.id)
    }
}

impl Drop for Publisher<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.unpublishing(self.state.id).wait();
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

zreceiver! {
    #[derive(Clone)]
    pub struct SampleReceiver : Receiver<Sample> {}
}

/// A subscriber that provides data through a stream.
///
/// Subscribers are automatically unregistered when dropped.
pub struct Subscriber<'a> {
    pub(crate) session: &'a Session,
    pub(crate) state: Arc<SubscriberState>,
    pub(crate) alive: bool,
    pub(crate) receiver: SampleReceiver,
}

impl Subscriber<'_> {
    pub fn receiver(&mut self) -> &mut SampleReceiver {
        &mut self.receiver
    }

    /// Pull available data for a pull-mode [Subscriber](Subscriber).
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use futures::prelude::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let mut subscriber = session.subscribe(&"/resource/name".into())
    ///                             .mode(SubMode::Pull).await.unwrap();
    /// async_std::task::spawn(subscriber.receiver().clone().for_each(
    ///     move |sample| async move { println!("Received : {:?}", sample); }
    /// ));
    /// subscriber.pull();
    /// # })
    /// ```
    pub fn pull(&self) -> impl ZFuture<Output = ZResult<()>> {
        self.session.pull(&self.state.reskey)
    }

    /// Undeclare a [Subscriber](Subscriber) previously declared with [subscribe](Session::subscribe).
    ///
    /// Subscribers are automatically unregistered when dropped, but you may want to use this function to handle errors or
    /// unregister the Subscriber asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let subscriber = session.subscribe(&"/resource/name".into()).await.unwrap();
    /// subscriber.unregister().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn unregister(mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.alive = false;
        self.session.unsubscribe(self.state.id)
    }
}

impl Drop for Subscriber<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.unsubscribe(self.state.id).wait();
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
/// Subscribers are automatically unregistered when dropped.
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
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let subscriber = session.subscribe(&"/resource/name".into())
    ///     .callback(|sample| { println!("Received : {} {}", sample.res_name, sample.payload); })
    ///     .mode(SubMode::Pull).await.unwrap();
    /// subscriber.pull();
    /// # })
    /// ```
    pub fn pull(&self) -> impl ZFuture<Output = ZResult<()>> {
        self.session.pull(&self.state.reskey)
    }

    /// Undeclare a [CallbackSubscriber](CallbackSubscriber) previously declared with [declare_callback_subscriber](Session::declare_callback_subscriber).
    ///
    /// CallbackSubscribers are automatically unregistered when dropped, but you may want to use this function to handle errors or
    /// unregister the CallbackSubscriber asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// # fn data_handler(_sample: Sample) { };
    /// let subscriber = session.subscribe(&"/resource/name".into())
    ///     .callback(data_handler).await.unwrap();
    /// subscriber.unregister().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn unregister(mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.alive = false;
        self.session.unsubscribe(self.state.id)
    }
}

impl Drop for CallbackSubscriber<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.unsubscribe(self.state.id).wait();
        }
    }
}

impl fmt::Debug for CallbackSubscriber<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.state.fmt(f)
    }
}

zreceiver! {
    #[derive(Clone)]
    pub struct ReplyReceiver : Receiver<Reply> {}
}

pub(crate) struct QueryableState {
    pub(crate) id: Id,
    pub(crate) reskey: ResKey,
    pub(crate) kind: ZInt,
    pub(crate) complete: bool,
    pub(crate) sender: Sender<Query>,
}

impl fmt::Debug for QueryableState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Queryable{{ id:{}, reskey:{} }}", self.id, self.reskey)
    }
}

zreceiver! {
    #[derive(Clone)]
    pub struct QueryReceiver : Receiver<Query> {}
}

/// An entity able to reply to queries.
///
/// Queryables are automatically unregistered when dropped.
pub struct Queryable<'a> {
    pub(crate) session: &'a Session,
    pub(crate) state: Arc<QueryableState>,
    pub(crate) alive: bool,
    pub(crate) receiver: QueryReceiver,
}

impl Queryable<'_> {
    pub fn receiver(&mut self) -> &mut QueryReceiver {
        &mut self.receiver
    }

    /// Undeclare a [Queryable](Queryable) previously declared with [register_queryable](Session::register_queryable).
    ///
    /// Queryables are automatically unregistered when dropped, but you may want to use this function to handle errors or
    /// unregister the Queryable asynchronously.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    ///
    /// let session = open(config::peer()).await.unwrap();
    /// let queryable = session.register_queryable(&"/resource/name".into()).await.unwrap();
    /// queryable.unregister().await.unwrap();
    /// # })
    /// ```
    #[inline]
    pub fn unregister(mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.alive = false;
        self.session.unregister_queryable(self.state.id)
    }
}

impl Drop for Queryable<'_> {
    fn drop(&mut self) {
        if self.alive {
            let _ = self.session.unregister_queryable(self.state.id).wait();
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
    pub fn send(&'_ self, msg: Sample) {
        if let Err(e) = self.sender.send((self.kind, msg)) {
            log::error!("Error sending reply: {}", e);
        }
    }

    #[inline(always)]
    pub fn try_send(&self, msg: Sample) -> Result<(), TrySendError<Sample>> {
        match self.sender.try_send((self.kind, msg)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(sample)) => Err(TrySendError::Full(sample.1)),
            Err(TrySendError::Disconnected(sample)) => Err(TrySendError::Disconnected(sample.1)),
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

    #[inline(always)]
    pub async fn send_async(&self, msg: Sample) {
        if let Err(e) = self.sender.send_async((self.kind, msg)).await {
            log::error!("Error sending reply: {}", e);
        }
    }

    // @TODO
    // #[inline(always)]
    // pub fn sink(&self) -> flume::r#async::SendSink<'_, Sample> {
    //     self.sender.sink()
    // }
}
