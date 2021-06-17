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
use crate::net::*;
use async_std::pin::Pin;
use async_std::task::{Context, Poll};
use futures_lite::stream::Stream;
use futures_lite::StreamExt;
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

pub struct QueryingSubscriber<'a> {
    session: &'a Session,
    subscriber: Subscriber<'a>,
    query_reskey: ResKey,
    query_predicate: String,
    receiver: QueryingReceiver,
}

impl QueryingSubscriber<'_> {
    pub fn new<'a>(
        session: &'a Session,
        sub_reskey: &ResKey,
        info: &SubInfo,
        query_reskey: &ResKey,
        query_predicate: &str,
    ) -> ZResult<QueryingSubscriber<'a>> {
        // declare subscriber at first
        let mut subscriber = session.declare_subscriber(sub_reskey, info).wait()?;

        let receiver = QueryingReceiver {
            subscriber_recv: subscriber.receiver().clone(),
            query_replies_recv: None,
            merge_queue: Vec::new(),
        };

        let mut query_subscriber = QueryingSubscriber {
            session,
            subscriber,
            query_reskey: query_reskey.clone(),
            query_predicate: query_predicate.to_string(),
            receiver,
        };

        // start query
        query_subscriber.start_query()?;

        Ok(query_subscriber)
    }

    #[inline]
    pub fn undeclare(self) -> ZResolvedFuture<ZResult<()>> {
        self.subscriber.undeclare()
    }

    #[inline]
    pub fn receiver(&mut self) -> &mut QueryingReceiver {
        &mut self.receiver
    }

    pub fn start_query(&mut self) -> ZResult<()> {
        if self.receiver.query_replies_recv.is_none() {
            log::debug!(
                "Start query on {}?{}",
                self.query_reskey,
                self.query_predicate
            );
            let replies_recv = self
                .session
                .query(
                    &self.query_reskey,
                    &self.query_predicate,
                    QueryTarget::default(),
                    QueryConsolidation::default(),
                )
                .wait()?;

            self.receiver.query_replies_recv = Some(replies_recv);
            Ok(())
        } else {
            log::error!(
                "Cannot start query on {}?{} - one is already in progress",
                self.query_reskey,
                self.query_predicate
            );
            zerror!(ZErrorKind::Other {
                descr: "Query already in progress".to_string()
            })
        }
    }
}

pub struct QueryingReceiver {
    subscriber_recv: SampleReceiver,
    query_replies_recv: Option<ReplyReceiver>,
    merge_queue: Vec<Sample>,
}


impl Stream for QueryingReceiver {
    type Item = Sample;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mself = self.get_mut();

        // if a query is in progress
        if let Some(receiver) = &mut mself.query_replies_recv {
            // get all replies and add them to merge_queue
            loop {
                match receiver.poll_next(cx) {
                    Poll::Pending => return Poll::Pending,  // query still in progress - return Pending
                    Poll::Ready(Some(mut reply)) => {
                        log::trace!("Reply received: {}", reply.data.res_name);
                        reply.data.ensure_timestamp();
                        mself.merge_queue.push(reply.data);
                    },
                    Poll::Ready(None) => break,
                }
            }
            mself.query_replies_recv = None;
            log::debug!("Query completed, received {} replies", mself.merge_queue.len());

            // get all publications received during the query and add them to merge_queue
            while let Poll::Ready(Some(mut sample)) = mself.subscriber_recv.poll_next(cx) {
                log::trace!("Pub received in parallel of query: {}", sample.res_name);
                sample.ensure_timestamp();
                mself.merge_queue.push(sample);
            }

            // remove duplicates and sort merge_queue
            mself.merge_queue
                .dedup_by_key(|sample| sample.get_timestamp().unwrap().clone());
            mself.merge_queue
                .sort_by_key(|sample| sample.get_timestamp().unwrap().clone());
            mself.merge_queue.reverse();
            log::debug!(
                "Merged received publications - {} samples to propagate",
                mself.merge_queue.len()
            );
        }

        if mself.merge_queue.is_empty() {
            log::trace!("poll_next: receiving from subscriber...");
            // if merge_queue is empty, receive from subscriber
            mself.subscriber_recv.poll_next(cx)
        } else {
            log::trace!(
                "poll_next: pop sample from merge_queue (len={})",
                mself.merge_queue.len()
            );
            // otherwise, take from merge_queue
            Poll::Ready(Some(mself.merge_queue.pop().unwrap()))
        }
    }
}
