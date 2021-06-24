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
use crate::net::queryable::EVAL;
use crate::net::{
    data_kind, encoding, CallbackSubscriber, CongestionControl, DataInfo, Query,
    QueryConsolidation, QueryTarget, Queryable, Receiver, RecvError, RecvTimeoutError, Reliability,
    RepliesSender, Reply, ReplyReceiver, ResKey, Sample, SampleReceiver, Session, SubInfo, SubMode,
    Subscriber, TryRecvError, ZBuf, ZFuture, ZInt, ZResolvedFuture,
};
use crate::utils::new_reception_timestamp;
use crate::{Path, PathExpr, Selector, Timestamp, Value, ZError, ZErrorKind, ZResult, Zenoh};
use async_std::pin::Pin;
use async_std::task::{Context, Poll};
use futures_lite::stream::{Stream, StreamExt};
use log::{debug, warn};
use std::convert::TryInto;
use std::fmt;
use std::time::{Duration, Instant};
use zenoh_util::{zerror, zresolved};

/// A Workspace to operate on zenoh.
///
/// A Workspace has an optional [Path] prefix from which relative [Path]s or [Selector]s can be used.
///
/// # Examples
///
/// ```
/// use zenoh::*;
/// use std::convert::TryInto;
///
/// #[async_std::main]
/// async fn main() {
///     let zenoh = Zenoh::new(net::config::default()).await.unwrap();
///
///     // Create a Workspace using prefix "/demo/example"
///     let workspace = zenoh.workspace(Some("/demo/example".try_into().unwrap())).await.unwrap();
///
///     // Put using a relative path: "/demo/example/" + "hello"
///     workspace.put(&"hello".try_into().unwrap(),
///         "Hello World!".into()
///     ).await.unwrap();
///
///     // Note that absolute paths and selectors can still be used:
///     workspace.put(&"/demo/exmaple/hello2".try_into().unwrap(),
///         "Hello World!".into()
///     ).await.unwrap();
///
///     zenoh.close().await.unwrap();
/// }
/// ```
pub struct Workspace<'a> {
    zenoh: &'a Zenoh,
    prefix: Option<Path>,
}

const LOCAL_ROUTER_PREFIX: &str = "/@/router/local";

impl Workspace<'_> {
    pub(crate) fn new(
        zenoh: &Zenoh,
        prefix: Option<Path>,
    ) -> ZResolvedFuture<ZResult<Workspace<'_>>> {
        zresolved!(Ok(Workspace { zenoh, prefix }))
    }

    /// Returns the prefix that was used to create this Workspace (calling [`Zenoh::workspace()`]).
    pub fn prefix(&self) -> &Option<Path> {
        &self.prefix
    }

    /// Returns the zenoh-net [`Session`] used by this workspace.
    /// This is for advanced use cases requiring fine usage of the zenoh-net API.
    #[inline]
    pub fn session(&self) -> &Session {
        &self.zenoh.session
    }

    fn canonicalize(&self, path: &str) -> ZResult<String> {
        let abs_path = if path.starts_with('/') {
            path.to_string()
        } else {
            match &self.prefix {
                Some(prefix) => format!("{}/{}", prefix, path),
                None => format!("/{}", path),
            }
        };
        if abs_path.starts_with(LOCAL_ROUTER_PREFIX) {
            match self.zenoh.router_pid().wait() {
                Some(pid) => Ok(format!(
                    "/@/router/{}{}",
                    pid,
                    abs_path.strip_prefix(LOCAL_ROUTER_PREFIX).unwrap()
                )),
                None => zerror!(ZErrorKind::Other {
                    descr: "Not connected to a router; can't resolve '/@/router/local' path".into()
                }),
            }
        } else {
            Ok(abs_path)
        }
    }

    fn path_to_reskey(&self, path: &Path) -> ZResult<ResKey> {
        self.canonicalize(path.as_str()).map(ResKey::from)
    }

    fn pathexpr_to_reskey(&self, path: &PathExpr) -> ZResult<ResKey> {
        self.canonicalize(path.as_str()).map(ResKey::from)
    }

    /// Put a [`Path`]/[`Value`] into zenoh.  
    /// The corresponding [`Change`] will be received by all matching subscribers and all matching storages.
    /// Note that the [`Path`] can be absolute or relative to this Workspace.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use std::convert::TryInto;
    ///
    /// let zenoh = Zenoh::new(net::config::default()).await.unwrap();
    /// let workspace = zenoh.workspace(None).await.unwrap();
    /// workspace.put(
    ///     &"/demo/example/hello".try_into().unwrap(),
    ///     "Hello World!".into()
    /// ).await.unwrap();
    /// # })
    /// ```
    pub fn put(&self, path: &Path, value: Value) -> ZResolvedFuture<ZResult<()>> {
        debug!("put on {:?}", path);
        let (encoding, payload) = value.encode();
        match self.path_to_reskey(path) {
            Ok(reskey) => self.session().write_ext(
                &reskey,
                payload,
                encoding,
                data_kind::PUT,
                CongestionControl::Drop, // TODO: Define the right congestion control value for the put
            ),
            Err(e) => zresolved!(Err(e)),
        }
    }

    /// Delete a [`Path`] and its [`Value`] from zenoh.  
    /// The corresponding [`Change`] will be received by all matching subscribers and all matching storages.
    /// Note that the [`Path`] can be absolute or relative to this Workspace.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use std::convert::TryInto;
    ///
    /// let zenoh = Zenoh::new(net::config::default()).await.unwrap();
    /// let workspace = zenoh.workspace(None).await.unwrap();
    /// workspace.delete(
    ///     &"/demo/example/hello".try_into().unwrap()
    /// ).await.unwrap();
    /// # })
    /// ```
    pub fn delete(&self, path: &Path) -> ZResolvedFuture<ZResult<()>> {
        debug!("delete on {:?}", path);
        match self.path_to_reskey(path) {
            Ok(reskey) => self.session().write_ext(
                &reskey,
                ZBuf::new(),
                encoding::NONE,
                data_kind::DELETE,
                CongestionControl::Drop, // TODO: Define the right congestion control value for the delete
            ),
            Err(e) => zresolved!(Err(e)),
        }
    }

    /// Get a selection of [`Path`]/[`Value`] from zenoh.  
    /// The selection is returned as a [`async_std::stream::Stream`] of [`Data`].
    /// Note that the [`Selector`] can be absolute or relative to this Workspace.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use std::convert::TryInto;
    /// use futures::prelude::*;
    ///
    /// let zenoh = Zenoh::new(net::config::default()).await.unwrap();
    /// let workspace = zenoh.workspace(None).await.unwrap();
    /// let mut data_stream = workspace.get(&"/demo/example/**".try_into().unwrap()).await.unwrap();
    /// while let Some(data) = data_stream.next().await {
    ///     println!(">> {} : {:?} at {}",
    ///         data.path, data.value, data.timestamp
    ///     )
    /// }
    /// # })
    /// ```
    pub fn get(&self, selector: &Selector) -> ZResolvedFuture<ZResult<DataReceiver>> {
        debug!("get on {}", selector);
        zresolved_try!({
            let reskey = self.pathexpr_to_reskey(&selector.path_expr)?;
            let decode_value = !selector.properties.contains_key("raw");
            let consolidation = if selector.has_time_range() {
                QueryConsolidation::none()
            } else {
                QueryConsolidation::default()
            };

            self.session()
                .query(
                    &reskey,
                    &selector.predicate,
                    QueryTarget::default(),
                    consolidation,
                )
                .wait()
                .map(|receiver| DataReceiver {
                    receiver,
                    decode_value,
                })
        })
    }

    /// Subscribe to changes for a selection of [`Path`]/[`Value`] (specified via a [`Selector`]) from zenoh.  
    /// The changes are returned as [`async_std::stream::Stream`] of [`Change`].
    /// This Stream will never end unless it's dropped or explicitly closed via [`ChangeReceiver::close()`].
    /// Note that the [`Selector`] can be absolute or relative to this Workspace.
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use std::convert::TryInto;
    /// use futures::prelude::*;
    ///
    /// let zenoh = Zenoh::new(net::config::default()).await.unwrap();
    /// let workspace = zenoh.workspace(None).await.unwrap();
    /// let mut change_stream =
    ///     workspace.subscribe(&"/demo/example/**".try_into().unwrap()).await.unwrap();
    /// while let Some(change) = change_stream.next().await {
    ///     println!(">> {:?} for {} : {:?} at {}",
    ///         change.kind, change.path, change.value, change.timestamp
    ///     )
    /// }
    /// # })
    /// ```
    pub fn subscribe(&self, selector: &Selector) -> ZResolvedFuture<ZResult<ChangeReceiver<'_>>> {
        debug!("subscribe on {}", selector);
        zresolved_try!({
            if selector.filter.is_some() {
                return zerror!(ZErrorKind::Other {
                    descr: "Filter not supported in selector for subscribe()".into()
                });
            }
            if selector.fragment.is_some() {
                return zerror!(ZErrorKind::Other {
                    descr: "Fragment not supported in selector for subscribe()".into()
                });
            }
            let decode_value = !selector.properties.contains_key("raw");

            let reskey = self.pathexpr_to_reskey(&selector.path_expr)?;
            let sub_info = SubInfo {
                reliability: Reliability::Reliable,
                mode: SubMode::Push,
                period: None,
            };

            self.session()
                .declare_subscriber(&reskey, &sub_info)
                .wait()
                .map(|mut subscriber| ChangeReceiver {
                    receiver: subscriber.receiver().clone(),
                    subscriber,
                    decode_value,
                })
        })
    }

    /// Subscribe to changes for a selection of [`Path`]/[`Value`] (specified via a [`Selector`]) from zenoh.  
    /// For each change, the `callback` will be called.
    /// A [`SubscriberHandle`] is returned, allowing to close the subscription via [`SubscriberHandle::close()`].
    /// Note that the [`Selector`] can be absolute or relative to this Workspace.
    ///
    /// # Examples
    /// ```
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use std::convert::TryInto;
    ///
    /// let zenoh = Zenoh::new(net::config::default()).await.unwrap();
    /// let workspace = zenoh.workspace(None).await.unwrap();
    /// let mut change_stream = workspace.subscribe_with_callback(
    ///     &"/demo/example/**".try_into().unwrap(),
    ///     move |change| {
    ///        println!(">> {:?} for {} : {:?} at {}",
    ///            change.kind, change.path, change.value, change.timestamp
    ///        )}
    /// ).await.unwrap();
    /// # })
    /// ```
    pub fn subscribe_with_callback<SubscribeCallback>(
        &self,
        selector: &Selector,
        mut callback: SubscribeCallback,
    ) -> ZResolvedFuture<ZResult<SubscriberHandle<'_>>>
    where
        SubscribeCallback: FnMut(Change) + Send + Sync + 'static,
    {
        debug!("subscribe_with_callback on {}", selector);
        zresolved_try!({
            if selector.filter.is_some() {
                return zerror!(ZErrorKind::Other {
                    descr: "Filter not supported in selector for subscribe()".into()
                });
            }
            if selector.fragment.is_some() {
                return zerror!(ZErrorKind::Other {
                    descr: "Fragment not supported in selector for subscribe()".into()
                });
            }
            let decode_value = !selector.properties.contains_key("raw");

            let reskey = self.pathexpr_to_reskey(&selector.path_expr)?;
            let sub_info = SubInfo {
                reliability: Reliability::Reliable,
                mode: SubMode::Push,
                period: None,
            };

            let subscriber = self
                .session()
                .declare_callback_subscriber(&reskey, &sub_info, move |sample| {
                    match Change::from_sample(sample, decode_value) {
                        Ok(change) => callback(change),
                        Err(err) => warn!("Received an invalid Sample (drop it): {}", err),
                    }
                })
                .wait()?;
            Ok(SubscriberHandle { subscriber })
        })
    }

    /// Registers an evaluation function under the provided [`PathExpr`].  
    /// A [`async_std::stream::Stream`] of [`GetRequest`] is returned.
    /// All `get` requests matching the [`PathExpr`] will be added to this stream as a [`GetRequest`],
    /// allowing the implementation to send a reply as a result of the evaluation function via [`GetRequest::reply()`].
    /// This Stream will never end unless it's dropped or explicitly closed via [`GetRequestStream::close()`].
    /// Note that the [`PathExpr`] can be absolute or relative to this Workspace.
    ///
    /// # Examples
    /// ```no_run
    /// # async_std::task::block_on(async {
    /// use zenoh::*;
    /// use std::convert::TryInto;
    /// use futures::prelude::*;
    ///
    /// let zenoh = Zenoh::new(net::config::default()).await.unwrap();
    /// let workspace = zenoh.workspace(None).await.unwrap();
    /// let mut get_stream =
    ///     workspace.register_eval(&"/demo/example/eval".try_into().unwrap()).await.unwrap();
    /// while let Some(get_request) = get_stream.next().await {
    ///    println!(
    ///        ">> [Eval function] received get with selector: {}",
    ///        get_request.selector
    ///    );
    ///    // return the Value as a result of this evaluation function :
    ///    let v = Value::StringUtf8(format!("Result for get on {}", get_request.selector));
    ///    get_request.reply_async("/demo/example/eval".try_into().unwrap(), v).await;
    /// }
    /// # })
    /// ```
    pub fn register_eval(
        &self,
        path_expr: &PathExpr,
    ) -> ZResolvedFuture<ZResult<GetRequestStream<'_>>> {
        debug!("eval on {}", path_expr);
        zresolved_try!({
            let reskey = self.pathexpr_to_reskey(&path_expr)?;

            self.session()
                .declare_queryable(&reskey, EVAL)
                .wait()
                .map(|queryable| GetRequestStream { queryable })
        })
    }
}

impl fmt::Debug for Workspace<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Workspace{{ prefix:{:?} }}", self.prefix)
    }
}

/// A Data returned as a result of a [`Workspace::get()`] operation.
///
/// It contains the [`Path`], its associated [`Value`] and a [`Timestamp`] which corresponds to the time
/// at which the path/value has been put into zenoh.
#[derive(Debug)]
pub struct Data {
    pub path: Path,
    pub value: Value,
    pub timestamp: Timestamp,
}

ztranscoder! {
    /// A [`Stream`] of [`Data`] returned as a result of the [`Workspace::get()`] operation.
    ///
    /// [`Stream`]: async_std::stream::Stream
    #[derive(Clone)]
    pub DataReceiver: Receiver<Data> <- ReplyReceiver: Receiver<Reply>
    with
        DataIter: Iterator<Data>,
        DataTryIter: Iterator<Data>,
    {
        decode_value: bool,
    }
}

impl DataReceiver {
    fn transcode(&self, reply: Reply) -> ZResult<Data> {
        let path: Path = reply.data.res_name.try_into().unwrap();
        let (encoding, timestamp) = if let Some(info) = reply.data.data_info {
            (
                info.encoding.unwrap_or(encoding::APP_OCTET_STREAM),
                info.timestamp.unwrap_or_else(new_reception_timestamp),
            )
        } else {
            (encoding::APP_OCTET_STREAM, new_reception_timestamp())
        };
        let value = if self.decode_value {
            Value::decode(encoding, reply.data.payload)?
        } else {
            Value::Raw(encoding, reply.data.payload)
        };
        Ok(Data {
            path,
            value,
            timestamp,
        })
    }
}

/// The kind of a [`Change`].
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeKind {
    /// if the [`Change`] was caused by a `put` operation.
    Put = data_kind::PUT as isize,
    /// if the [`Change`] was caused by a `patch` operation.
    Patch = data_kind::PATCH as isize,
    /// if the [`Change`] was caused by a `delete` operation.
    Delete = data_kind::DELETE as isize,
}

impl fmt::Display for ChangeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChangeKind::Put => write!(f, "PUT"),
            ChangeKind::Patch => write!(f, "PATCH"),
            ChangeKind::Delete => write!(f, "DELETE"),
        }
    }
}

impl From<ZInt> for ChangeKind {
    fn from(kind: ZInt) -> Self {
        match kind {
            data_kind::PUT => ChangeKind::Put,
            data_kind::PATCH => ChangeKind::Patch,
            data_kind::DELETE => ChangeKind::Delete,
            _ => {
                warn!(
                    "Received DataInfo with kind={} which doesn't correspond to a ChangeKind. \
                       Assume a PUT with RAW encoding",
                    kind
                );
                ChangeKind::Put
            }
        }
    }
}

/// The notification of a change occured on a path/value and reported to a subscription.
///
/// See [`Workspace::subscribe()`] and [`Workspace::subscribe_with_callback()`].
#[derive(Debug, Clone)]
pub struct Change {
    /// the [`Path`] related to this change.
    pub path: Path,
    /// the new [`Value`] if the kind is `PUT`. `None` if the kind is `DELETE`.
    pub value: Option<Value>,
    /// the [`Timestamp`] of the change
    pub timestamp: Timestamp,
    /// the kind of change (`PUT` or `DELETE`).
    pub kind: ChangeKind,
}

impl Change {
    /// Convert a [`Sample`] into a [`Change`].
    /// If the Sample's kind is DELETE, the Change's value is set to `None`.
    /// Otherwise, if decode_value is `true` the payload is decoded as a typed [`Value`].
    /// If decode_value is `false`, the payload is converted into a [`Value::Raw`].
    pub fn from_sample(sample: Sample, decode_value: bool) -> ZResult<Change> {
        let path = sample.res_name.try_into()?;
        let (kind, encoding, timestamp) = if let Some(info) = sample.data_info {
            (
                info.kind.map_or(ChangeKind::Put, ChangeKind::from),
                info.encoding.unwrap_or(encoding::APP_OCTET_STREAM),
                info.timestamp.unwrap_or_else(new_reception_timestamp),
            )
        } else {
            (
                ChangeKind::Put,
                encoding::APP_OCTET_STREAM,
                new_reception_timestamp(),
            )
        };
        let value = if kind == ChangeKind::Delete {
            None
        } else if decode_value {
            Some(Value::decode(encoding, sample.payload)?)
        } else {
            Some(Value::Raw(encoding, sample.payload))
        };

        Ok(Change {
            path,
            value,
            timestamp,
            kind,
        })
    }

    /// Convert this [`Change`] into a [`Sample`] to be sent via zenoh-net.
    pub fn into_sample(self) -> Sample {
        let mut info = DataInfo::new();
        info.kind = Some(self.kind as ZInt);
        info.timestamp = Some(self.timestamp);

        let payload = match self.value {
            Some(v) => {
                let (e, p) = v.encode();
                info.encoding = Some(e);
                p
            }
            None => ZBuf::new(),
        };

        Sample {
            res_name: self.path.to_string(),
            payload,
            data_info: Some(info),
        }
    }
}

ztranscoder! {
    /// A [`Stream`] of [`Change`] returned as a result of the [`Workspace::subscribe()`] operation.
    ///
    /// [`Stream`]: async_std::stream::Stream
    pub ChangeReceiver<'a>: Receiver<Change> <- SampleReceiver: Receiver<Sample>
    with
        ChangeIter: Iterator<Change>,
        ChangeTryIter: Iterator<Change>,
    {
        subscriber: Subscriber<'a>,
        decode_value: bool,
    }
}

impl ChangeReceiver<'_> {
    fn transcode(&self, sample: Sample) -> ZResult<Change> {
        Change::from_sample(sample, self.decode_value)
    }

    // Closes the stream and the subscription.
    pub fn close(self) -> ZResolvedFuture<ZResult<()>> {
        self.subscriber.undeclare()
    }
}

fn path_value_to_sample(path: Path, value: Value) -> Sample {
    let (encoding, payload) = value.encode();
    let mut info = DataInfo::new();
    info.encoding = Some(encoding);

    Sample {
        res_name: path.to_string(),
        payload,
        data_info: Some(info),
    }
}

/// A handle returned as result of [`Workspace::subscribe_with_callback()`] operation.
pub struct SubscriberHandle<'a> {
    subscriber: CallbackSubscriber<'a>,
}

impl SubscriberHandle<'_> {
    /// Closes the subscription.
    pub fn close(self) -> ZResolvedFuture<ZResult<()>> {
        self.subscriber.undeclare()
    }
}

/// A `GET` request received by an evaluation function (see [`Workspace::register_eval()`]).
#[derive(Clone)]
pub struct GetRequest {
    pub selector: Selector,
    replies_sender: RepliesSender,
}

impl GetRequest {
    /// Send a [`Path`]/[`Value`] as a reply to the requester.
    #[inline(always)]
    pub fn reply(&self, path: Path, value: Value) {
        self.replies_sender.send(path_value_to_sample(path, value))
    }

    /// Send a [`Path`]/[`Value`] as a reply to the requester.
    #[inline(always)]
    pub async fn reply_async(&self, path: Path, value: Value) {
        self.replies_sender
            .send_async(path_value_to_sample(path, value))
            .await
    }
}

fn query_to_get(query: Query) -> ZResult<GetRequest> {
    Selector::new(query.res_name.as_str(), query.predicate.as_str()).map(|selector| GetRequest {
        selector,
        replies_sender: query.replies_sender,
    })
}

/// A [`Stream`] of [`GetRequest`] returned as a result of the [`Workspace::register_eval()`] operation.
///
/// [`Stream`]: async_std::stream::Stream
pub struct GetRequestStream<'a> {
    queryable: Queryable<'a>,
}

impl GetRequestStream<'_> {
    /// Closes the stream and unregister the evaluation function.
    pub fn close(self) -> ZResolvedFuture<ZResult<()>> {
        self.queryable.undeclare()
    }
}

impl Stream for GetRequestStream<'_> {
    type Item = GetRequest;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.get_mut().queryable.receiver().poll_next(cx) {
            Poll::Ready(Some(query)) => match query_to_get(query) {
                Ok(get) => Poll::Ready(Some(get)),
                Err(err) => {
                    warn!("Error in receveid get(): {}. Ignore it.", err);
                    Poll::Pending
                }
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
