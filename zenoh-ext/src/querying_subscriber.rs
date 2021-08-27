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
use async_std::pin::Pin;
use async_std::task::{Context, Poll};
use futures_lite::stream::Stream;
use futures_lite::StreamExt;
use std::future::Future;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use zenoh::queryable::STORAGE;
use zenoh::*;
use zenoh_util::zwrite;

use super::publication_cache::PUBLISHER_CACHE_QUERYABLE_KIND;

const MERGE_QUEUE_INITIAL_CAPCITY: usize = 32;
const REPLIES_RECV_QUEUE_INITIAL_CAPCITY: usize = 3;

/// The builder of QueryingSubscriber, allowing to configure it.
#[derive(Clone)]
pub struct QueryingSubscriberBuilder<'a> {
    session: &'a Session,
    sub_reskey: ResKey,
    info: SubInfo,
    query_reskey: ResKey,
    query_predicate: String,
    query_target: QueryTarget,
    query_consolidation: QueryConsolidation,
}

impl QueryingSubscriberBuilder<'_> {
    pub(crate) fn new<'a>(
        session: &'a Session,
        sub_reskey: &ResKey,
    ) -> QueryingSubscriberBuilder<'a> {
        let info = SubInfo::default();

        // By default query all matching publication caches and storages
        let query_target = QueryTarget {
            kind: PUBLISHER_CACHE_QUERYABLE_KIND | STORAGE,
            target: Target::All,
        };

        // By default no query consolidation, to receive more than 1 sample per-resource
        // (in history of publications is available)
        let query_consolidation = QueryConsolidation::none();

        QueryingSubscriberBuilder {
            session,
            sub_reskey: sub_reskey.clone(),
            info,
            query_reskey: sub_reskey.clone(),
            query_predicate: "".to_string(),
            query_target,
            query_consolidation,
        }
    }

    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.info.reliability = reliability;
        self
    }

    /// Change the subscription reliability to Reliable.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.info.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to BestEffort.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.info.reliability = Reliability::BestEffort;
        self
    }

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.info.mode = mode;
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.info.mode = SubMode::Push;
        self.info.period = None;
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.info.mode = SubMode::Pull;
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.info.period = period;
        self
    }

    /// Change the resource key to be used for queries.
    #[inline]
    pub fn query_reskey(mut self, query_reskey: ResKey) -> Self {
        self.query_reskey = query_reskey;
        self
    }

    /// Change the predicate to be used for queries.
    #[inline]
    pub fn query_predicate(mut self, query_predicate: String) -> Self {
        self.query_predicate = query_predicate;
        self
    }

    /// Change the target to be used for queries.
    #[inline]
    pub fn query_target(mut self, query_target: QueryTarget) -> Self {
        self.query_target = query_target;
        self
    }

    /// Change the consolidation mode to be used for queries.
    #[inline]
    pub fn query_consolidation(mut self, query_consolidation: QueryConsolidation) -> Self {
        self.query_consolidation = query_consolidation;
        self
    }
}

impl<'a> Future for QueryingSubscriberBuilder<'a> {
    type Output = ZResult<QueryingSubscriber<'a>>;

    #[inline]
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(QueryingSubscriber::new(Pin::into_inner(self).clone()))
    }
}

impl<'a> ZFuture for QueryingSubscriberBuilder<'a> {
    #[inline]
    fn wait(self) -> ZResult<QueryingSubscriber<'a>> {
        QueryingSubscriber::new(self)
    }
}

pub struct QueryingSubscriber<'a> {
    conf: QueryingSubscriberBuilder<'a>,
    subscriber: Subscriber<'a>,
    receiver: QueryingSubscriberReceiver,
}

impl QueryingSubscriber<'_> {
    fn new(conf: QueryingSubscriberBuilder<'_>) -> ZResult<QueryingSubscriber<'_>> {
        // declare subscriber at first
        let mut subscriber = conf
            .session
            .subscribe(&conf.sub_reskey)
            .reliability(conf.info.reliability)
            .mode(conf.info.mode)
            .period(conf.info.period)
            .wait()?;

        let receiver = QueryingSubscriberReceiver::new(subscriber.receiver().clone());

        let mut query_subscriber = QueryingSubscriber {
            conf,
            subscriber,
            receiver,
        };

        // start query
        query_subscriber.query().wait()?;

        Ok(query_subscriber)
    }

    /// Undeclare this QueryingSubscriber
    #[inline]
    pub fn unregister(self) -> impl ZFuture<Output = ZResult<()>> {
        self.subscriber.unregister()
    }

    /// Return the QueryingSubscriberReceiver associated to this subscriber.
    #[inline]
    pub fn receiver(&mut self) -> &mut QueryingSubscriberReceiver {
        &mut self.receiver
    }

    /// Issue a new query using the configured resource key and predicate.
    #[inline]
    pub fn query(&mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.query_on(
            &self.conf.query_reskey.clone(),
            &self.conf.query_predicate.clone(),
            self.conf.query_target.clone(),
            self.conf.query_consolidation.clone(),
        )
    }

    /// Issue a new query on the specified resource key and predicate.
    pub fn query_on(
        &mut self,
        reskey: &ResKey,
        predicate: &str,
        target: QueryTarget,
        consolidation: QueryConsolidation,
    ) -> impl ZFuture<Output = ZResult<()>> {
        let mut state = zwrite!(self.receiver.state);
        log::debug!("Start query on {}?{}", reskey, predicate);
        match self
            .conf
            .session
            .get(&Selector::from(reskey).with_predicate(predicate))
            .target(target)
            .consolidation(consolidation)
            .wait()
        {
            Ok(recv) => {
                state.replies_recv_queue.push(recv);
                zready(Ok(()))
            }
            Err(err) => zready(Err(err)),
        }
    }
}

#[derive(Clone)]
pub struct QueryingSubscriberReceiver {
    state: Arc<RwLock<InnerState>>,
}

impl QueryingSubscriberReceiver {
    fn new(subscriber_recv: SampleReceiver) -> QueryingSubscriberReceiver {
        QueryingSubscriberReceiver {
            state: Arc::new(RwLock::new(InnerState {
                subscriber_recv,
                replies_recv_queue: Vec::with_capacity(REPLIES_RECV_QUEUE_INITIAL_CAPCITY),
                merge_queue: Vec::with_capacity(MERGE_QUEUE_INITIAL_CAPCITY),
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
    replies_recv_queue: Vec<ReplyReceiver>,
    merge_queue: Vec<Sample>,
}

impl Stream for InnerState {
    type Item = Sample;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let mself = self.get_mut();

        // if there are queries is in progress
        if !mself.replies_recv_queue.is_empty() {
            // get all available replies and add them to merge_queue
            let mut i = 0;
            while i < mself.replies_recv_queue.len() {
                loop {
                    match mself.replies_recv_queue[i].poll_next(cx) {
                        Poll::Ready(Some(mut reply)) => {
                            log::trace!("Reply received: {}", reply.data.res_name);
                            reply.data.ensure_timestamp();
                            mself.merge_queue.push(reply.data);
                        }
                        Poll::Ready(None) => {
                            // query completed - remove the receiver and break loop
                            mself.replies_recv_queue.remove(i);
                            break;
                        }
                        Poll::Pending => break, // query still in progress - break loop
                    }
                }
                i += 1;
            }

            // if the receivers queue is still not empty, it means there are remaining queries
            if !mself.replies_recv_queue.is_empty() {
                return Poll::Pending;
            }
            log::debug!(
                "All queries completed, received {} replies",
                mself.merge_queue.len()
            );

            // get all publications received during the queries and add them to merge_queue
            while let Poll::Ready(Some(mut sample)) = mself.subscriber_recv.poll_next(cx) {
                log::trace!("Pub received in parallel of query: {}", sample.res_name);
                sample.ensure_timestamp();
                mself.merge_queue.push(sample);
            }

            // sort and remove duplicates from merge_queue
            mself
                .merge_queue
                .sort_by_key(|sample| sample.get_timestamp().unwrap().clone());
            mself
                .merge_queue
                .dedup_by_key(|sample| sample.get_timestamp().unwrap().clone());
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
        // if there are queries is in progress
        if !self.replies_recv_queue.is_empty() {
            // get all replies and add them to merge_queue
            for recv in self.replies_recv_queue.drain(..) {
                while let Ok(mut reply) = recv.recv() {
                    log::trace!("Reply received: {}", reply.data.res_name);
                    reply.data.ensure_timestamp();
                    self.merge_queue.push(reply.data);
                }
            }
            log::debug!(
                "All queries completed, received {} replies",
                self.merge_queue.len()
            );

            // get all publications received during the query and add them to merge_queue
            while let Ok(mut sample) = self.subscriber_recv.try_recv() {
                log::trace!("Pub received in parallel of query: {}", sample.res_name);
                sample.ensure_timestamp();
                self.merge_queue.push(sample);
            }

            // sort and remove duplicates from merge_queue
            self.merge_queue
                .sort_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue
                .dedup_by_key(|sample| sample.get_timestamp().unwrap().clone());
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
        // if there are queries is in progress
        if !self.replies_recv_queue.is_empty() {
            // get all available replies and add them to merge_queue
            let mut i = 0;
            while i < self.replies_recv_queue.len() {
                loop {
                    match self.replies_recv_queue[i].try_recv() {
                        Ok(mut reply) => {
                            log::trace!("Reply received: {}", reply.data.res_name);
                            reply.data.ensure_timestamp();
                            self.merge_queue.push(reply.data);
                        }
                        Err(TryRecvError::Disconnected) => {
                            // query completed - remove the receiver and break loop
                            self.replies_recv_queue.remove(i);
                            break;
                        }
                        Err(TryRecvError::Empty) => break, // query still in progress - break loop
                    }
                }
                i += 1;
            }

            // if the receivers queue is still not empty, it means there are remaining queries
            if !self.replies_recv_queue.is_empty() {
                return Err(TryRecvError::Empty);
            }
            log::debug!(
                "All queries completed, received {} replies",
                self.merge_queue.len()
            );

            // get all publications received during the query and add them to merge_queue
            while let Ok(mut sample) = self.subscriber_recv.try_recv() {
                log::trace!("Pub received in parallel of query: {}", sample.res_name);
                sample.ensure_timestamp();
                self.merge_queue.push(sample);
            }

            // sort and remove duplicates from merge_queue
            self.merge_queue
                .sort_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue
                .dedup_by_key(|sample| sample.get_timestamp().unwrap().clone());
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
        // if there are queries is in progress
        if !self.replies_recv_queue.is_empty() {
            // get all available replies and add them to merge_queue
            let mut i = 0;
            while i < self.replies_recv_queue.len() {
                loop {
                    match self.replies_recv_queue[i].recv_deadline(deadline) {
                        Ok(mut reply) => {
                            log::trace!("Reply received: {}", reply.data.res_name);
                            reply.data.ensure_timestamp();
                            self.merge_queue.push(reply.data);
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            // query completed - remove the receiver and break loop
                            self.replies_recv_queue.remove(i);
                            break;
                        }
                        Err(RecvTimeoutError::Timeout) => break, // query still in progress - break loop
                    }
                }
                i += 1;
            }

            // if the receivers queue is still not empty, it means there are remaining queries, and that a timeout occured
            if !self.replies_recv_queue.is_empty() {
                return Err(RecvTimeoutError::Timeout);
            }
            log::debug!(
                "All queries completed, received {} replies",
                self.merge_queue.len()
            );

            // get all publications received during the query and add them to merge_queue
            while let Ok(mut sample) = self.subscriber_recv.try_recv() {
                log::trace!("Pub received in parallel of query: {}", sample.res_name);
                sample.ensure_timestamp();
                self.merge_queue.push(sample);
            }

            // sort and remove duplicates from merge_queue
            self.merge_queue
                .sort_by_key(|sample| sample.get_timestamp().unwrap().clone());
            self.merge_queue
                .dedup_by_key(|sample| sample.get_timestamp().unwrap().clone());
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
