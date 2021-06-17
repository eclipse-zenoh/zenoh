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
            log::info!(
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

impl QueryingReceiver {
    fn complete_pending_query(&mut self) {
        // if a query is in progress
        if let Some(receiver) = &self.query_replies_recv {
            log::info!("Complete query in progress ...");
            // get all replies and add them to merge_queue
            while let Ok(mut reply) = receiver.recv() {
                reply.data.ensure_timestamp();
                self.merge_queue.push(reply.data);
            }
            self.query_replies_recv = None;
            log::info!("Received {} replies", self.merge_queue.len());

            // get all publications received during the query and add them to merge_queue
            while let Ok(mut sample) = self.subscriber_recv.try_recv() {
                sample.ensure_timestamp();
                self.merge_queue.push(sample);
            }

            // remove duplicates and sort merge_queue
            self.merge_queue
                .dedup_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue
                .sort_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue.reverse();
            log::info!(
                "Merged received publications - {} samples to propagate",
                self.merge_queue.len()
            );
        }
    }
}

impl Stream for QueryingReceiver {
    type Item = Sample;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        log::info!("poll_next");
        let mself = self.get_mut();

        // complete any query in progress
        mself.complete_pending_query();

        if mself.merge_queue.is_empty() {
            log::info!("poll_next: receiving from subscriber...");
            // if merge_queue is empty, receive from subscriber
            use futures_lite::StreamExt;
            mself.subscriber_recv.poll_next(cx)
        } else {
            log::info!(
                "poll_next: pop sample from merge_queue (len={})",
                mself.merge_queue.len()
            );
            // otherwise, take from merge_queue
            Poll::Ready(Some(mself.merge_queue.pop().unwrap()))
        }
    }
}
