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
use crate::data_kind;
use crate::encoding::*;
use crate::net::protocol::proto::DataInfo;
use crate::utils::new_reception_timestamp;
use crate::Properties;
use crate::Selector;
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

#[derive(Clone)]
pub struct Value {
    pub payload: ZBuf,
    pub encoding: ZInt,
}

impl Value {
    pub fn new(payload: ZBuf) -> Self {
        Value {
            payload,
            encoding: APP_OCTET_STREAM,
        }
    }

    pub fn empty() -> Self {
        Value {
            payload: ZBuf::new(),
            encoding: APP_OCTET_STREAM,
        }
    }

    #[inline(always)]
    pub fn encoding(mut self, encoding: ZInt) -> Self {
        self.encoding = encoding;
        self
    }

    /// Returns the encoding description of the Value.
    pub fn encoding_descr(&self) -> String {
        to_string(self.encoding)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Value{{ payload: {}, encoding: {} }}",
            self.payload,
            self.encoding_descr()
        )
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            String::from_utf8(self.payload.to_vec())
                .unwrap_or_else(|_| base64::encode(self.payload.to_vec()))
        )
    }
}

impl From<ZBuf> for Value {
    fn from(buf: ZBuf) -> Self {
        Value {
            payload: buf,
            encoding: APP_OCTET_STREAM,
        }
    }
}

#[cfg(feature = "zero-copy")]
impl From<Arc<SharedMemoryBuf>> for Value {
    fn from(smb: Arc<SharedMemoryBuf>) -> Self {
        Value {
            payload: smb.into(),
            encoding: APP_OCTET_STREAM,
        }
    }
}

#[cfg(feature = "zero-copy")]
impl From<Box<SharedMemoryBuf>> for Value {
    fn from(smb: Box<SharedMemoryBuf>) -> Self {
        Value {
            payload: smb.into(),
            encoding: APP_OCTET_STREAM,
        }
    }
}

#[cfg(feature = "zero-copy")]
impl From<SharedMemoryBuf> for Value {
    fn from(smb: SharedMemoryBuf) -> Self {
        Value {
            payload: smb.into(),
            encoding: APP_OCTET_STREAM,
        }
    }
}

impl From<Vec<u8>> for Value {
    fn from(buf: Vec<u8>) -> Self {
        Value::from(ZBuf::from(buf))
    }
}

impl From<&[u8]> for Value {
    fn from(buf: &[u8]) -> Self {
        Value::from(ZBuf::from(buf))
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value {
            payload: ZBuf::from(s.as_bytes()),
            encoding: STRING,
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::from(s.to_string())
    }
}

impl From<Properties> for Value {
    fn from(p: Properties) -> Self {
        Value {
            payload: ZBuf::from(p.to_string().as_bytes()),
            encoding: APP_PROPERTIES,
        }
    }
}

impl From<&serde_json::Value> for Value {
    fn from(json: &serde_json::Value) -> Self {
        Value {
            payload: ZBuf::from(json.to_string().as_bytes()),
            encoding: APP_JSON,
        }
    }
}

impl From<serde_json::Value> for Value {
    fn from(json: serde_json::Value) -> Self {
        Value::from(&json)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value {
            payload: ZBuf::from(i.to_string().as_bytes()),
            encoding: APP_INTEGER,
        }
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value {
            payload: ZBuf::from(f.to_string().as_bytes()),
            encoding: APP_FLOAT,
        }
    }
}

/// Informations on the source of a zenoh sample.
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub source_id: Option<PeerId>,
    pub source_sn: Option<ZInt>,
    pub first_router_id: Option<PeerId>,
    pub first_router_sn: Option<ZInt>,
}

impl SourceInfo {
    pub(crate) fn empty() -> Self {
        SourceInfo {
            source_id: None,
            source_sn: None,
            first_router_id: None,
            first_router_sn: None,
        }
    }
}

impl From<DataInfo> for SourceInfo {
    fn from(data_info: DataInfo) -> Self {
        SourceInfo {
            source_id: data_info.source_id,
            source_sn: data_info.source_sn,
            first_router_id: data_info.first_router_id,
            first_router_sn: data_info.first_router_sn,
        }
    }
}

impl From<Option<DataInfo>> for SourceInfo {
    fn from(data_info: Option<DataInfo>) -> Self {
        match data_info {
            Some(data_info) => data_info.into(),
            None => SourceInfo::empty(),
        }
    }
}

/// The kind of a [`Sample`].
#[derive(Debug, Clone, PartialEq)]
pub enum SampleKind {
    /// if the [`Sample`] was caused by a `put` operation.
    Put = data_kind::PUT as isize,
    /// if the [`Sample`] was caused by a `patch` operation.
    Patch = data_kind::PATCH as isize,
    /// if the [`Sample`] was caused by a `delete` operation.
    Delete = data_kind::DELETE as isize,
}

impl Default for SampleKind {
    fn default() -> Self {
        SampleKind::Put
    }
}

impl fmt::Display for SampleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SampleKind::Put => write!(f, "PUT"),
            SampleKind::Patch => write!(f, "PATCH"),
            SampleKind::Delete => write!(f, "DELETE"),
        }
    }
}

impl From<ZInt> for SampleKind {
    fn from(kind: ZInt) -> Self {
        match kind {
            data_kind::PUT => SampleKind::Put,
            data_kind::PATCH => SampleKind::Patch,
            data_kind::DELETE => SampleKind::Delete,
            _ => {
                log::warn!(
                    "Received DataInfo with kind={} which doesn't correspond to a SampleKind. \
                       Assume a PUT with RAW encoding",
                    kind
                );
                SampleKind::Put
            }
        }
    }
}

/// A zenoh sample.
#[derive(Clone, Debug)]
pub struct Sample {
    pub res_name: String,
    pub value: Value,
    pub kind: SampleKind,
    pub timestamp: Option<Timestamp>,
    pub source_info: SourceInfo,
}

impl Sample {
    #[inline]
    pub fn new<IntoValue>(res_name: String, value: IntoValue) -> Self
    where
        IntoValue: Into<Value>,
    {
        Sample {
            res_name,
            value: value.into(),
            kind: SampleKind::default(),
            timestamp: None,
            source_info: SourceInfo::empty(),
        }
    }

    #[inline]
    pub(crate) fn with_info(res_name: String, payload: ZBuf, data_info: Option<DataInfo>) -> Self {
        let mut value: Value = payload.into();
        if let Some(data_info) = data_info {
            if let Some(encoding) = data_info.encoding {
                value.encoding = encoding;
            }
            Sample {
                res_name,
                value,
                kind: data_info.kind.unwrap_or(data_kind::DEFAULT).into(),
                timestamp: data_info.timestamp.clone(),
                source_info: data_info.into(),
            }
        } else {
            Sample {
                res_name,
                value,
                kind: SampleKind::default(),
                timestamp: None,
                source_info: SourceInfo::empty(),
            }
        }
    }

    #[inline]
    pub(crate) fn split(self) -> (String, ZBuf, DataInfo) {
        let info = DataInfo {
            kind: None,
            encoding: Some(self.value.encoding),
            timestamp: self.timestamp,
            #[cfg(feature = "zero-copy")]
            sliced: false,
            source_id: self.source_info.source_id,
            source_sn: self.source_info.source_sn,
            first_router_id: self.source_info.first_router_id,
            first_router_sn: self.source_info.first_router_sn,
        };
        (self.res_name, self.value.payload, info)
    }

    #[inline]
    pub fn get_timestamp(&self) -> Option<&Timestamp> {
        self.timestamp.as_ref()
    }

    #[inline]
    pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    #[inline]
    pub fn with_source_info(mut self, source_info: SourceInfo) -> Self {
        self.source_info = source_info;
        self
    }

    #[inline]
    /// Ensure that an associated Timestamp is present in this Sample.
    /// If not, a new one is created with the current system time and 0x00 as id.
    pub fn ensure_timestamp(&mut self) {
        if self.timestamp.is_none() {
            self.timestamp = Some(new_reception_timestamp());
        }
    }
}

/// The callback that will be called on each data for a [CallbackSubscriber](CallbackSubscriber).
pub type DataHandler = dyn FnMut(Sample) + Send + Sync + 'static;

/// Structs received b y a [Queryable](Queryable).
pub struct Query {
    pub(crate) res_name: String,
    pub(crate) predicate: String,
    pub replies_sender: RepliesSender,
}

impl Query {
    #[inline(always)]
    pub fn selector(&self) -> Selector<'_> {
        Selector {
            res_name: &self.res_name,
            predicate: &self.predicate,
        }
    }

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
    pub(crate) reskey: ResKey<'static>,
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
    /// let publisher = session.publishing("/resource/name").await.unwrap();
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
    pub(crate) reskey: ResKey<'static>,
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
    /// let mut subscriber = session.subscribe("/resource/name")
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
    /// let subscriber = session.subscribe("/resource/name").await.unwrap();
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
    /// let subscriber = session.subscribe("/resource/name")
    ///     .callback(|sample| { println!("Received : {} {}", sample.res_name, sample.value); })
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
    /// let subscriber = session.subscribe("/resource/name")
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
    pub(crate) reskey: ResKey<'static>,
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
    /// let queryable = session.register_queryable("/resource/name").await.unwrap();
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
