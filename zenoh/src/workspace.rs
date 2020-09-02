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
    data_kind, encoding, DataInfo, Query, QueryConsolidation, QueryTarget, Queryable, RBuf,
    Reliability, RepliesSender, Reply, ResKey, Sample, Session, SubInfo, SubMode, Subscriber, ZInt,
};
use crate::{utils, Path, PathExpr, Selector, Timestamp, Value, ZError, ZErrorKind, ZResult};
use async_std::pin::Pin;
use async_std::stream::Stream;
use async_std::sync::Receiver;
use async_std::task::{Context, Poll};
use log::{debug, warn};
use pin_project_lite::pin_project;
use std::convert::TryInto;
use zenoh_util::zerror;

#[derive(Clone)]
pub struct Workspace {
    session: Session,
    prefix: Path,
}

impl Workspace {
    pub(crate) async fn new(session: Session, prefix: Option<Path>) -> ZResult<Workspace> {
        Ok(Workspace {
            session,
            prefix: prefix.unwrap_or_else(|| "/".try_into().unwrap()),
        })
    }

    #[doc(hidden)]
    pub fn session(&self) -> &Session {
        &self.session
    }

    fn path_to_reskey(&self, path: &Path) -> ResKey {
        if path.is_relative() {
            ResKey::from(path.with_prefix(&self.prefix))
        } else {
            ResKey::from(path)
        }
    }

    fn pathexpr_to_reskey(&self, path: &PathExpr) -> ResKey {
        if path.is_relative() {
            ResKey::from(path.with_prefix(&self.prefix))
        } else {
            ResKey::from(path)
        }
    }

    pub async fn put(&self, path: &Path, value: Value) -> ZResult<()> {
        debug!("put on {:?}", path);
        let (encoding, payload) = value.encode();
        self.session
            .write_ext(
                &self.path_to_reskey(path),
                payload,
                encoding,
                data_kind::PUT,
                Reliability::Reliable,
            )
            .await
    }

    pub async fn delete(&self, path: &Path) -> ZResult<()> {
        debug!("delete on {:?}", path);
        self.session
            .write_ext(
                &self.path_to_reskey(path),
                RBuf::empty(),
                encoding::NONE,
                data_kind::DELETE,
                Reliability::Reliable,
            )
            .await
    }

    pub async fn get(&self, selector: &Selector) -> ZResult<DataStream> {
        debug!("get on {}", selector);
        let reskey = self.pathexpr_to_reskey(&selector.path_expr);
        let decode_value = !selector.properties.contains_key("raw");

        self.session
            .query(
                &reskey,
                &selector.predicate,
                QueryTarget::default(),
                QueryConsolidation::default(),
            )
            .await
            .map(|receiver| DataStream {
                receiver,
                decode_value,
            })
    }

    pub async fn subscribe(&self, selector: &Selector) -> ZResult<ChangeStream> {
        debug!("subscribe on {}", selector);
        if selector.projection.is_some() {
            return zerror!(ZErrorKind::Other {
                descr: "Projection not supported in selector for subscribe()".into()
            });
        }
        if selector.fragment.is_some() {
            return zerror!(ZErrorKind::Other {
                descr: "Fragment not supported in selector for subscribe()".into()
            });
        }
        let decode_value = !selector.properties.contains_key("raw");

        let reskey = self.pathexpr_to_reskey(&selector.path_expr);
        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        };

        self.session
            .declare_subscriber(&reskey, &sub_info)
            .await
            .map(|subscriber| ChangeStream {
                subscriber,
                decode_value,
            })
    }

    pub async fn subscribe_with_callback<SubscribeCallback>(
        &self,
        selector: &Selector,
        mut callback: SubscribeCallback,
    ) -> ZResult<()>
    where
        SubscribeCallback: FnMut(Change) + Send + Sync + 'static,
    {
        debug!("subscribe_with_callback on {}", selector);
        if selector.projection.is_some() {
            return zerror!(ZErrorKind::Other {
                descr: "Projection not supported in selector for subscribe()".into()
            });
        }
        if selector.fragment.is_some() {
            return zerror!(ZErrorKind::Other {
                descr: "Fragment not supported in selector for subscribe()".into()
            });
        }
        let decode_value = !selector.properties.contains_key("raw");

        let reskey = self.pathexpr_to_reskey(&selector.path_expr);
        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        };

        let _ = self
            .session
            .declare_callback_subscriber(&reskey, &sub_info, move |sample| {
                match Change::new(
                    &sample.res_name,
                    sample.payload,
                    sample.data_info,
                    decode_value,
                ) {
                    Ok(change) => callback(change),
                    Err(err) => warn!("Received an invalid Sample (drop it): {}", err),
                }
            })
            .await;
        Ok(())
    }

    pub async fn register_eval(&self, path_expr: &PathExpr) -> ZResult<GetRequestStream> {
        debug!("eval on {}", path_expr);
        let reskey = self.pathexpr_to_reskey(&path_expr);

        self.session
            .declare_queryable(&reskey, EVAL)
            .await
            .map(|queryable| GetRequestStream { queryable })
    }
}

pub struct Data {
    pub path: Path,
    pub value: Value,
    pub timestamp: Timestamp,
}

fn reply_to_data(reply: Reply, decode_value: bool) -> ZResult<Data> {
    let path: Path = reply.data.res_name.try_into().unwrap();
    let (_, encoding, timestamp) = utils::decode_data_info(reply.data.data_info);
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

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeKind {
    PUT = data_kind::PUT as isize,
    PATCH = data_kind::PATCH as isize,
    DELETE = data_kind::DELETE as isize,
}

impl From<ZInt> for ChangeKind {
    fn from(kind: ZInt) -> Self {
        match kind {
            data_kind::PUT => ChangeKind::PUT,
            data_kind::PATCH => ChangeKind::PATCH,
            data_kind::DELETE => ChangeKind::DELETE,
            _ => {
                warn!(
                    "Received DataInfo with kind={} which doesn't correspond to a ChangeKind. \
                       Assume a PUT with RAW encoding",
                    kind
                );
                ChangeKind::PUT
            }
        }
    }
}

#[derive(Debug)]
pub struct Change {
    pub path: Path,
    pub value: Option<Value>,
    pub timestamp: Timestamp,
    pub kind: ChangeKind,
}

impl Change {
    fn new(
        res_name: &str,
        payload: RBuf,
        data_info: Option<DataInfo>,
        decode_value: bool,
    ) -> ZResult<Change> {
        let path = res_name.try_into()?;
        let (kind, encoding, timestamp) = utils::decode_data_info(data_info);
        let value = if kind == ChangeKind::DELETE {
            None
        } else if decode_value {
            Some(Value::decode(encoding, payload)?)
        } else {
            Some(Value::Raw(encoding, payload))
        };

        Ok(Change {
            path,
            value,
            kind,
            timestamp,
        })
    }
}

pin_project! {
    pub struct ChangeStream {
        #[pin]
        subscriber: Subscriber,
        decode_value: bool,
    }
}

impl Stream for ChangeStream {
    type Item = Change;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let decode_value = self.decode_value;
        match self.project().subscriber.poll_next(cx) {
            Poll::Ready(Some(sample)) => {
                match Change::new(
                    &sample.res_name,
                    sample.payload,
                    sample.data_info,
                    decode_value,
                ) {
                    Ok(change) => Poll::Ready(Some(change)),
                    Err(err) => {
                        warn!("Received an invalid Sample (drop it): {}", err);
                        Poll::Pending
                    }
                }
            }
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
        first_broker_id: None,
        first_broker_sn: None,
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

pub struct GetRequest {
    pub selector: Selector,
    replies_sender: RepliesSender,
}

impl GetRequest {
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
    pub struct GetRequestStream {
        #[pin]
        queryable: Queryable
    }
}

impl Stream for GetRequestStream {
    type Item = GetRequest;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.project().queryable.poll_next(cx) {
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
