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
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use zenoh_util::core::{ZError, ZErrorKind, ZResult};
use zenoh_util::sync::channel::{RecvError, RecvTimeoutError, TryRecvError};
use zenoh_util::{zerror, zwrite};

pub struct QueryingSubscriber<'a> {
    session: &'a Session,
    subscriber: Subscriber<'a>,
    query_reskey: ResKey,
    query_predicate: String,
    receiver: QueryingSubscriberReceiver,
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

        let receiver = QueryingSubscriberReceiver::new(subscriber.receiver().clone());

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
    pub fn receiver(&mut self) -> &mut QueryingSubscriberReceiver {
        &mut self.receiver
    }

    pub fn start_query(&mut self) -> ZResult<()> {
        let mut state = zwrite!(self.receiver.state);

        if state.query_replies_recv.is_none() {
            log::debug!(
                "Start query on {}?{}",
                self.query_reskey,
                self.query_predicate
            );
            state.query_replies_recv = Some(
                self.session
                    .query(
                        &self.query_reskey,
                        &self.query_predicate,
                        QueryTarget::default(),
                        QueryConsolidation::default(),
                    )
                    .wait()?,
            );
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

pub struct QueryingSubscriberReceiver {
    state: Arc<RwLock<InnerState>>,
}

impl QueryingSubscriberReceiver {
    fn new(subscriber_recv: SampleReceiver) -> QueryingSubscriberReceiver {
        QueryingSubscriberReceiver {
            state: Arc::new(RwLock::new(InnerState {
                subscriber_recv,
                query_replies_recv: None,
                merge_queue: Vec::new(),
            })),
        }
    }
}

impl Stream for QueryingSubscriberReceiver {
    type Item = Sample;

    #[inline(always)]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let state = &mut zwrite!(self.state);
        state.poll_next(cx)
    }
}

impl Receiver<Sample> for QueryingSubscriberReceiver {
    fn recv(&self) -> Result<Sample, RecvError> {
        let state = &mut zwrite!(self.state);
        state.recv()
    }

    fn try_recv(&self) -> Result<Sample, TryRecvError> {
        let state = &mut zwrite!(self.state);
        state.try_recv()
    }

    fn recv_timeout(&self, timeout: Duration) -> Result<Sample, RecvTimeoutError> {
        let state = &mut zwrite!(self.state);
        state.recv_timeout(timeout)
    }

    fn recv_deadline(&self, deadline: Instant) -> Result<Sample, RecvTimeoutError> {
        let state = &mut zwrite!(self.state);
        state.recv_deadline(deadline)
    }
}

struct InnerState {
    subscriber_recv: SampleReceiver,
    query_replies_recv: Option<ReplyReceiver>,
    merge_queue: Vec<Sample>,
}

impl Stream for InnerState {
    type Item = Sample;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mself = self.get_mut();

        // if a query is in progress
        if let Some(query_replies_recv) = &mut mself.query_replies_recv {
            // get all replies and add them to merge_queue
            loop {
                match query_replies_recv.poll_next(cx) {
                    Poll::Ready(Some(mut reply)) => {
                        log::trace!("Reply received: {}", reply.data.res_name);
                        reply.data.ensure_timestamp();
                        mself.merge_queue.push(reply.data);
                    }
                    Poll::Ready(None) => break, // query completed - break loop
                    Poll::Pending => return Poll::Pending, // query still in progress - return the same
                }
            }
            log::debug!(
                "Query completed, received {} replies",
                mself.merge_queue.len()
            );
            mself.query_replies_recv = None;

            // get all publications received during the query and add them to merge_queue
            while let Poll::Ready(Some(mut sample)) = mself.subscriber_recv.poll_next(cx) {
                log::trace!("Pub received in parallel of query: {}", sample.res_name);
                sample.ensure_timestamp();
                mself.merge_queue.push(sample);
            }

            // remove duplicates and sort merge_queue
            mself
                .merge_queue
                .dedup_by_key(|sample| sample.get_timestamp().unwrap().clone());
            mself
                .merge_queue
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

impl InnerState {
    fn recv(&mut self) -> Result<Sample, RecvError> {
        // if a query is in progress
        if let Some(query_replies_recv) = &mut self.query_replies_recv {
            // get all replies and add them to merge_queue
            while let Ok(mut reply) = query_replies_recv.recv() {
                log::trace!("Reply received: {}", reply.data.res_name);
                reply.data.ensure_timestamp();
                self.merge_queue.push(reply.data);
            }
            log::debug!(
                "Query completed, received {} replies",
                self.merge_queue.len()
            );
            self.query_replies_recv = None;

            // get all publications received during the query and add them to merge_queue
            while let Ok(mut sample) = self.subscriber_recv.try_recv() {
                log::trace!("Pub received in parallel of query: {}", sample.res_name);
                sample.ensure_timestamp();
                self.merge_queue.push(sample);
            }

            // remove duplicates and sort merge_queue
            self.merge_queue
                .dedup_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue
                .sort_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue.reverse();
            log::debug!(
                "Merged received publications - {} samples to propagate",
                self.merge_queue.len()
            );
        }

        if self.merge_queue.is_empty() {
            log::trace!("poll_next: receiving from subscriber...");
            // if merge_queue is empty, receive from subscriber
            self.subscriber_recv.recv()
        } else {
            log::trace!(
                "poll_next: pop sample from merge_queue (len={})",
                self.merge_queue.len()
            );
            // otherwise, take from merge_queue
            Ok(self.merge_queue.pop().unwrap())
        }
    }

    fn try_recv(&mut self) -> Result<Sample, TryRecvError> {
        // if a query is in progress
        if let Some(query_replies_recv) = &mut self.query_replies_recv {
            // get all replies and add them to merge_queue
            loop {
                match query_replies_recv.try_recv() {
                    Ok(mut reply) => {
                        log::trace!("Reply received: {}", reply.data.res_name);
                        reply.data.ensure_timestamp();
                        self.merge_queue.push(reply.data);
                    }
                    Err(TryRecvError::Disconnected) => break, // query completed - break loop
                    Err(TryRecvError::Empty) => return Err(TryRecvError::Empty), // query still in progress - return the same
                }
            }
            log::debug!(
                "Query completed, received {} replies",
                self.merge_queue.len()
            );
            self.query_replies_recv = None;

            // get all publications received during the query and add them to merge_queue
            while let Ok(mut sample) = self.subscriber_recv.try_recv() {
                log::trace!("Pub received in parallel of query: {}", sample.res_name);
                sample.ensure_timestamp();
                self.merge_queue.push(sample);
            }

            // remove duplicates and sort merge_queue
            self.merge_queue
                .dedup_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue
                .sort_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue.reverse();
            log::debug!(
                "Merged received publications - {} samples to propagate",
                self.merge_queue.len()
            );
        }

        if self.merge_queue.is_empty() {
            log::trace!("poll_next: receiving from subscriber...");
            // if merge_queue is empty, receive from subscriber
            self.subscriber_recv.try_recv()
        } else {
            log::trace!(
                "poll_next: pop sample from merge_queue (len={})",
                self.merge_queue.len()
            );
            // otherwise, take from merge_queue
            Ok(self.merge_queue.pop().unwrap())
        }
    }

    fn recv_timeout(&mut self, timeout: Duration) -> Result<Sample, RecvTimeoutError> {
        let deadline = Instant::now() + timeout;
        self.recv_deadline(deadline)
    }

    fn recv_deadline(&mut self, deadline: Instant) -> Result<Sample, RecvTimeoutError> {
        // if a query is in progress
        if let Some(query_replies_recv) = &mut self.query_replies_recv {
            // get all replies and add them to merge_queue
            loop {
                match query_replies_recv.recv_deadline(deadline) {
                    Ok(mut reply) => {
                        log::trace!("Reply received: {}", reply.data.res_name);
                        reply.data.ensure_timestamp();
                        self.merge_queue.push(reply.data);
                    }
                    Err(RecvTimeoutError::Disconnected) => break, // query completed - break loop
                    Err(RecvTimeoutError::Timeout) => return Err(RecvTimeoutError::Timeout), // timeout - return the same
                }
            }
            log::debug!(
                "Query completed, received {} replies",
                self.merge_queue.len()
            );
            self.query_replies_recv = None;

            // get all publications received during the query and add them to merge_queue
            while let Ok(mut sample) = self.subscriber_recv.try_recv() {
                log::trace!("Pub received in parallel of query: {}", sample.res_name);
                sample.ensure_timestamp();
                self.merge_queue.push(sample);
            }

            // remove duplicates and sort merge_queue
            self.merge_queue
                .dedup_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue
                .sort_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue.reverse();
            log::debug!(
                "Merged received publications - {} samples to propagate",
                self.merge_queue.len()
            );
        }

        if self.merge_queue.is_empty() {
            log::trace!("poll_next: receiving from subscriber...");
            // if merge_queue is empty, receive from subscriber
            self.subscriber_recv.recv_deadline(deadline)
        } else {
            log::trace!(
                "poll_next: pop sample from merge_queue (len={})",
                self.merge_queue.len()
            );
            // otherwise, take from merge_queue
            Ok(self.merge_queue.pop().unwrap())
        }
    }
}
