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
    QueryConsolidation, QueryTarget, Queryable, RBuf, Reliability, RepliesSender, Reply, ResKey,
    Sample, Session, SubInfo, SubMode, Subscriber, ZInt,
};
use crate::utils::new_reception_timestamp;
use crate::{Path, PathExpr, Selector, Timestamp, Value, ZError, ZErrorKind, ZResult, Zenoh};
use async_std::channel::Receiver;
use async_std::pin::Pin;
use async_std::stream::Stream;
use async_std::task::{Context, Poll};
use log::{debug, warn};
use pin_project_lite::pin_project;
use std::convert::TryInto;
use std::fmt;
use zenoh_util::zerror;

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
    pub(crate) async fn new(zenoh: &Zenoh, prefix: Option<Path>) -> ZResult<Workspace<'_>> {
        Ok(Workspace { zenoh, prefix })
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

    async fn canonicalize(&self, path: &str) -> ZResult<String> {
        let abs_path = if path.starts_with('/') {
            path.to_string()
        } else {
            match &self.prefix {
                Some(prefix) => format!("{}/{}", prefix, path),
                None => format!("/{}", path),
            }
        };
        if abs_path.starts_with(LOCAL_ROUTER_PREFIX) {
            match self.zenoh.router_pid().await {
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

    async fn path_to_reskey(&self, path: &Path) -> ZResult<ResKey> {
        self.canonicalize(path.as_str()).await.map(ResKey::from)
    }

    async fn pathexpr_to_reskey(&self, path: &PathExpr) -> ZResult<ResKey> {
        self.canonicalize(path.as_str()).await.map(ResKey::from)
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
    pub async fn put(&self, path: &Path, value: Value) -> ZResult<()> {
        debug!("put on {:?}", path);
        let (encoding, payload) = value.encode();
        self.session()
            .write_ext(
                &self.path_to_reskey(path).await?,
                payload,
                encoding,
                data_kind::PUT,
                CongestionControl::Drop, // TODO: Define the right congestion control value for the put
            )
            .await
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
    pub async fn delete(&self, path: &Path) -> ZResult<()> {
        debug!("delete on {:?}", path);
        self.session()
            .write_ext(
                &self.path_to_reskey(path).await?,
                RBuf::empty(),
                encoding::NONE,
                data_kind::DELETE,
                CongestionControl::Drop, // TODO: Define the right congestion control value for the delete
            )
            .await
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
    pub async fn get(&self, selector: &Selector) -> ZResult<DataStream> {
        debug!("get on {}", selector);
        let reskey = self.pathexpr_to_reskey(&selector.path_expr).await?;
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
            .await
            .map(|receiver| DataStream {
                receiver,
                decode_value,
            })
    }

    /// Subscribe to changes for a selection of [`Path`]/[`Value`] (specified via a [`Selector`]) from zenoh.  
    /// The changes are returned as [`async_std::stream::Stream`] of [`Change`].
    /// This Stream will never end unless it's dropped or explicitly closed via [`ChangeStream::close()`].
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
    pub async fn subscribe(&self, selector: &Selector) -> ZResult<ChangeStream<'_>> {
        debug!("subscribe on {}", selector);
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

        let reskey = self.pathexpr_to_reskey(&selector.path_expr).await?;
        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        };

        self.session()
            .declare_subscriber(&reskey, &sub_info)
            .await
            .map(|subscriber| ChangeStream {
                subscriber,
                decode_value,
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
    pub async fn subscribe_with_callback<SubscribeCallback>(
        &self,
        selector: &Selector,
        mut callback: SubscribeCallback,
    ) -> ZResult<SubscriberHandle<'_>>
    where
        SubscribeCallback: FnMut(Change) + Send + Sync + 'static,
    {
        debug!("subscribe_with_callback on {}", selector);
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

        let reskey = self.pathexpr_to_reskey(&selector.path_expr).await?;
        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        };

        let subscriber =
            self.session()
                .declare_callback_subscriber(&reskey, &sub_info, move |sample| {
                    match Change::from_sample(sample, decode_value) {
                        Ok(change) => callback(change),
                        Err(err) => warn!("Received an invalid Sample (drop it): {}", err),
                    }
                })
                .await?;
        Ok(SubscriberHandle { subscriber })
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
    ///    get_request.reply("/demo/example/eval".try_into().unwrap(), v).await;
    /// }
    /// # })
    /// ```
    pub async fn register_eval(&self, path_expr: &PathExpr) -> ZResult<GetRequestStream<'_>> {
        debug!("eval on {}", path_expr);
        let reskey = self.pathexpr_to_reskey(&path_expr).await?;

        self.session()
            .declare_queryable(&reskey, EVAL)
            .await
            .map(|queryable| GetRequestStream { queryable })
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

fn reply_to_data(reply: Reply, decode_value: bool) -> ZResult<Data> {
    let path: Path = reply.data.res_name.try_into().unwrap();
    let (encoding, timestamp) = if let Some(info) = reply.data.data_info {
        (
            info.encoding.unwrap_or(encoding::APP_OCTET_STREAM),
            info.timestamp.unwrap_or_else(new_reception_timestamp),
        )
    } else {
        (encoding::APP_OCTET_STREAM, new_reception_timestamp())
    };
    let value = if decode_value {
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

pin_project! {
    /// A [`Stream`] of [`Data`] returned as a result of the [`Workspace::get()`] operation.
    ///
    /// [`Stream`]: async_std::stream::Stream
    pub struct DataStream {
        #[pin]
        receiver: Receiver<Reply>,
        decode_value: bool,
    }
}

impl Stream for DataStream {
    type Item = Data;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let decode_value = self.decode_value;
        match self.project().receiver.poll_next(cx) {
            Poll::Ready(Some(reply)) => match reply_to_data(reply, decode_value) {
                Ok(data) => Poll::Ready(Some(data)),
                Err(err) => {
                    warn!("Received an invalid Reply (drop it): {}", err);
                    Poll::Pending
                }
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
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
            kind,
            timestamp,
        })
    }

    /// Convert this [`Change`] into a [`Sample`] to be sent via zenoh-net.
    pub fn into_sample(self) -> Sample {
        let (encoding, payload) = match self.value {
            Some(v) => {
                let (e, p) = v.encode();
                (Some(e), p)
            }
            None => (None, RBuf::empty()),
        };
        let info = DataInfo {
            source_id: None,
            source_sn: None,
            first_router_id: None,
            first_router_sn: None,
            timestamp: Some(self.timestamp),
            kind: Some(self.kind as u64),
            encoding,
        };
        Sample {
            res_name: self.path.to_string(),
            payload,
            data_info: Some(info),
        }
    }
}

pin_project! {
    /// A [`Stream`] of [`Change`] returned as a result of the [`Workspace::subscribe()`] operation.
    ///
    /// [`Stream`]: async_std::stream::Stream
    pub struct ChangeStream<'a> {
        #[pin]
        subscriber: Subscriber<'a>,
        decode_value: bool,
    }
}

impl ChangeStream<'_> {
    // Closes the stream and the subscription.
    pub async fn close(self) -> ZResult<()> {
        self.subscriber.undeclare().await
    }
}

impl Stream for ChangeStream<'_> {
    type Item = Change;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let decode_value = self.decode_value;
        match async_std::pin::Pin::new(self.project().subscriber.stream()).poll_next(cx) {
            Poll::Ready(Some(sample)) => match Change::from_sample(sample, decode_value) {
                Ok(change) => Poll::Ready(Some(change)),
                Err(err) => {
                    warn!("Received an invalid Sample (drop it): {}", err);
                    Poll::Pending
                }
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn path_value_to_sample(path: Path, value: Value) -> Sample {
    let (encoding, payload) = value.encode();
    let info = DataInfo {
        source_id: None,
        source_sn: None,
        first_router_id: None,
        first_router_sn: None,
        timestamp: None,
        kind: None,
        encoding: Some(encoding),
    };
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
    pub async fn close(self) -> ZResult<()> {
        self.subscriber.undeclare().await
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
    pub async fn reply(&self, path: Path, value: Value) {
        self.replies_sender
            .send(path_value_to_sample(path, value))
            .await
    }
}

fn query_to_get(query: Query) -> ZResult<GetRequest> {
    Selector::new(query.res_name.as_str(), query.predicate.as_str()).map(|selector| GetRequest {
        selector,
        replies_sender: query.replies_sender,
    })
}

pin_project! {
    /// A [`Stream`] of [`GetRequest`] returned as a result of the [`Workspace::register_eval()`] operation.
    ///
    /// [`Stream`]: async_std::stream::Stream
    pub struct GetRequestStream<'a> {
        #[pin]
        queryable: Queryable<'a>
    }
}

impl GetRequestStream<'_> {
    /// Closes the stream and unregister the evaluation function.
    pub async fn close(self) -> ZResult<()> {
        self.queryable.undeclare().await
    }
}

impl Stream for GetRequestStream<'_> {
    type Item = GetRequest;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match async_std::pin::Pin::new(self.project().queryable.stream()).poll_next(cx) {
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
