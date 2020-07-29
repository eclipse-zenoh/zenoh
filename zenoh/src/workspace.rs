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
use crate::net::*;
use crate::*;
use async_std::pin::Pin;
use async_std::stream::Stream;
use async_std::sync::Receiver;
use async_std::task::{Context, Poll};
use log::{debug, warn};
use pin_project_lite::pin_project;
use std::convert::TryInto;
use std::time::{SystemTime, UNIX_EPOCH};

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
            )
            .await
    }

    pub async fn delete(&self, path: &Path) -> ZResult<()> {
        debug!("delete on {:?}", path);
        self.session
            .write_ext(
                &self.path_to_reskey(path),
                RBuf::empty(),
                encoding::RAW,
                data_kind::DELETE,
            )
            .await
    }

    pub async fn get(&self, selector: &Selector) -> ZResult<DataStream> {
        debug!("get on {}", selector);
        let reskey = self.pathexpr_to_reskey(&selector.path_expr);

        self.session
            .query(
                &reskey,
                &selector.predicate,
                QueryTarget::default(),
                QueryConsolidation::default(),
            )
            .await
            .map(|receiver| DataStream { receiver })
    }

    pub async fn subscribe(&self, path_expr: &PathExpr) -> ZResult<ChangeStream> {
        debug!("subscribe on {}", path_expr);
        let reskey = self.pathexpr_to_reskey(&path_expr);
        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        };

        self.session
            .declare_subscriber(&reskey, &sub_info)
            .await
            .map(|subscriber| ChangeStream { subscriber })
    }

    pub async fn subscribe_with_callback<SubscribeCallback>(
        &self,
        path_expr: &PathExpr,
        mut callback: SubscribeCallback,
    ) -> ZResult<()>
    where
        SubscribeCallback: FnMut(Change) + Send + Sync + 'static,
    {
        debug!("subscribe_with_callback on {}", path_expr);
        let reskey = self.pathexpr_to_reskey(&path_expr);
        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        };

        let _ = self
            .session
            .declare_callback_subscriber(&reskey, &sub_info, move |sample| {
                match Change::new(&sample.res_name, sample.payload, sample.data_info) {
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

// generate a reception timestamp with id=0x00
fn new_reception_timestamp() -> Timestamp {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    Timestamp::new(now.into(), vec![0x00])
}

pub struct Data {
    pub path: Path,
    pub value: Value,
    pub timestamp: Timestamp,
}

fn reply_to_data(reply: Reply) -> ZResult<Data> {
    let path: Path = reply.data.res_name.try_into().unwrap();
    let (encoding, timestamp) =
        reply
            .data
            .data_info
            .map_or(
                (encoding::RAW, new_reception_timestamp()),
                |mut rbuf| match rbuf.read_datainfo() {
                    Ok(info) => (
                        info.encoding.unwrap_or(encoding::RAW),
                        info.timestamp.unwrap_or_else(new_reception_timestamp),
                    ),
                    Err(e) => {
                        warn!(
                        "Received DataInfo that failed to be decoded: {}. Assume it's RAW encoding",
                        e
                    );
                        (encoding::RAW, new_reception_timestamp())
                    }
                },
            );
    let value = Value::decode(encoding, reply.data.payload)?;
    Ok(Data {
        path,
        value,
        timestamp,
    })
}

pin_project! {
    pub struct DataStream {
        #[pin]
        receiver: Receiver<Reply>
    }
}

impl Stream for DataStream {
    type Item = Data;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.project().receiver.poll_next(cx) {
            Poll::Ready(Some(reply)) => match reply_to_data(reply) {
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

pub struct Change {
    pub path: Path,
    pub value: Option<Value>,
    pub timestamp: Timestamp,
    pub kind: ChangeKind,
}

impl Change {
    fn new(res_name: &str, payload: RBuf, data_info: Option<RBuf>) -> ZResult<Change> {
        let path = res_name.try_into()?;
        let (kind, encoding, timestamp) = data_info.map_or_else(
            || (ChangeKind::PUT, encoding::RAW, new_reception_timestamp()),
            |mut rbuf| match rbuf.read_datainfo() {
                Ok(info) => (
                    info.kind.map_or(ChangeKind::PUT, ChangeKind::from),
                    info.encoding.unwrap_or(encoding::RAW),
                    info.timestamp.unwrap_or_else(new_reception_timestamp),
                ),
                Err(e) => {
                    warn!(
                        "Received DataInfo that failed to be decoded: {}. Assume it's for a PUT",
                        e
                    );
                    (ChangeKind::PUT, encoding::RAW, new_reception_timestamp())
                }
            },
        );
        let value = if kind == ChangeKind::DELETE {
            None
        } else {
            Some(Value::decode(encoding, payload)?)
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
        subscriber: Subscriber
    }
}

impl Stream for ChangeStream {
    type Item = Change;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        match self.project().subscriber.poll_next(cx) {
            Poll::Ready(Some(sample)) => {
                match Change::new(&sample.res_name, sample.payload, sample.data_info) {
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
    let mut infobuf = WBuf::new(16, false);
    infobuf.write_datainfo(&info);
    Sample {
        res_name: path.to_string(),
        payload,
        data_info: Some(infobuf.into()),
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
