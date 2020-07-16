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
use crate::*;
use crate::net::*;
use crate::net::queryable::EVAL;
use log::{debug, warn};
use async_std::sync::Receiver;
use async_std::stream::Stream;
use async_std::pin::Pin;
use async_std::task::{Context, Poll};
use std::convert::TryInto;
use pin_project_lite::pin_project;

pub struct Workspace {
    session: Session,
    prefix: Path
}


impl Workspace {

    pub(crate) async fn new(session: Session, prefix: Option<Path>) -> ZResult<Workspace> {
        Ok(Workspace { session, prefix: prefix.unwrap_or_else(|| "/".try_into().unwrap()) })
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

    pub async fn put(&self, path: &Path, value: &dyn Value) -> ZResult<()> {
        debug!("put on {:?}", path);
        self.session.write_wo(
            &self.path_to_reskey(path),
            value.into(),
            value.encoding(),
            kind::PUT
        ).await
    }

    pub async fn delete(&self, path: &Path) -> ZResult<()> {
        debug!("delete on {:?}", path);
        self.session.write_wo(
            &self.path_to_reskey(path),
            RBuf::empty(),
            encoding::RAW,
            kind::DELETE
        ).await
    }

    pub async fn get(&self, selector: &Selector) -> ZResult<DataStream> {
        debug!("get on {}", selector);
        let reskey = self.pathexpr_to_reskey(&selector.path_expr);

        self.session.query(
            &reskey,
            &selector.predicate,
            QueryTarget::default(),
            QueryConsolidation::default()
        ).await
        .map(|receiver| DataStream { receiver })
    }

    pub async fn subscribe(&self, path_expr: &PathExpr) -> ZResult<ChangeStream> {
        debug!("subscribe on {}", path_expr);
        let reskey = self.pathexpr_to_reskey(&path_expr);
        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None
        };
    
        self.session.declare_subscriber(&reskey, &sub_info).await
            .map(|subscriber| ChangeStream { subscriber })
    }

    pub async fn register_eval(&self, path_expr: &PathExpr) -> ZResult<GetStream> {
        debug!("eval on {}", path_expr);
        let reskey = self.pathexpr_to_reskey(&path_expr);

        self.session.declare_queryable(&reskey, EVAL).await
            .map(|queryable| GetStream { queryable })
    }

}




pub struct Data {
    pub path: Path,
    pub value: Box<dyn Value>
}

fn reply_to_data(reply: Option<Reply>) -> Option<Data> {
    reply.map(|r| Data {
        path: r.data.res_name.try_into().unwrap(),
        value: Box::new(RawValue::from(r.data.payload))
    })
}

fn data_to_sample(data: Data) -> Sample {
    Sample { res_name: data.path.to_string(), payload: data.value.as_rbuf(), data_info: None }
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
        self.project().receiver.poll_next(cx).map(reply_to_data)
    }
}

pub enum ChangeKind {
    PUT = kind::PUT as isize,
    PATCH = kind::UPDATE as isize,
    DELETE = kind::DELETE as isize
}

pub struct Change {
    pub path: Path,
    pub value: Option<Box<dyn Value>>,
    //pub timestamp
    pub kind: ChangeKind
}

fn sample_to_change(sample: Option<Sample>) -> Option<Change> {
    sample.map(|s| Change {
        path: s.res_name.try_into().unwrap(),
        value: Some(Box::new(RawValue::from(s.payload))),
        // timestamp:            // TODO
        kind: ChangeKind::PUT    // TODO
    })
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
        self.project().subscriber.poll_next(cx).map(sample_to_change)
    }
}

pub struct GetRequest {
    pub selector: Selector,
    pub data_sender: DataSender
}

fn query_to_get(query: Query) -> ZResult<GetRequest> {
    Selector::new(query.res_name.as_str(), query.predicate.as_str())
        .map(|selector| GetRequest { selector, data_sender: DataSender { replies_sender: query.replies_sender } })
}

pin_project! {
    pub struct GetStream {
        #[pin]
        queryable: Queryable
    }
}

impl Stream for GetStream {
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
            Poll::Pending => Poll::Pending
        }
    }
}

pub struct DataSender {
    replies_sender: RepliesSender
}

impl DataSender {
    pub async fn send(&self, data: Data) {
        self.replies_sender.send(data_to_sample(data)).await
    }
}