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
mod path;
pub use path::{path, Path};
mod pathexpr;
pub use pathexpr::{pathexpr, PathExpr};
mod selector;
pub use selector::{selector, Selector};
mod values;
pub use values::*;

use crate::utils::new_reception_timestamp;
use crate::{
    encoding, CallbackSubscriber, DataInfo, Query, Queryable, Receiver, RecvError,
    RecvTimeoutError, RepliesSender, Reply, ReplyReceiver, Sample, SampleReceiver, Subscriber,
    Timestamp, TryRecvError, ZFuture, ZResult,
};
use async_std::pin::Pin;
use async_std::task::{Context, Poll};
use futures_lite::stream::{Stream, StreamExt};
use log::warn;
use std::convert::TryInto;
use std::time::{Duration, Instant};

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

impl From<ReplyReceiver> for DataReceiver {
    fn from(receiver: ReplyReceiver) -> Self {
        DataReceiver {
            receiver,
            decode_value: true,
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
    pub fn close(self) -> impl ZFuture<Output = ZResult<()>> {
        self.subscriber.unregister()
    }
}

impl<'a> From<Subscriber<'a>> for ChangeReceiver<'a> {
    fn from(mut subscriber: Subscriber<'a>) -> Self {
        ChangeReceiver {
            receiver: subscriber.receiver().clone(),
            subscriber,
            decode_value: true,
        }
    }
}

/// A handle returned as result of [`Workspace::subscribe_with_callback()`] operation.
pub struct SubscriberHandle<'a> {
    subscriber: CallbackSubscriber<'a>,
}

impl SubscriberHandle<'_> {
    /// Closes the subscription.
    pub fn close(self) -> impl ZFuture<Output = ZResult<()>> {
        self.subscriber.unregister()
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

/// A [`Stream`] of [`GetRequest`] returned as a result of the [`Workspace::register_eval()`] operation.
///
/// [`Stream`]: async_std::stream::Stream
pub struct GetRequestStream<'a> {
    queryable: Queryable<'a>,
}

impl GetRequestStream<'_> {
    /// Closes the stream and unregister the evaluation function.
    pub fn close(self) -> impl ZFuture<Output = ZResult<()>> {
        self.queryable.unregister()
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

fn query_to_get(query: Query) -> ZResult<GetRequest> {
    Selector::new(query.res_name.as_str(), query.predicate.as_str()).map(|selector| GetRequest {
        selector,
        replies_sender: query.replies_sender,
    })
}

impl<'a> From<Queryable<'a>> for GetRequestStream<'a> {
    fn from(queryable: Queryable<'a>) -> Self {
        GetRequestStream { queryable }
    }
}
