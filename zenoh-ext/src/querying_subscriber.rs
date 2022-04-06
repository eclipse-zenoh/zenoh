//
// Copyright (c) 2022 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use async_std::pin::Pin;
use async_std::task::{Context, Poll};
use futures_lite::stream::Stream;
use futures_lite::StreamExt;
use std::future::Future;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use zenoh::prelude::{KeyExpr, Receiver, Sample, Selector, ZFuture};
use zenoh::query::{QueryConsolidation, QueryTarget, ReplyReceiver, Target};
use zenoh::queryable::STORAGE;
use zenoh::subscriber::{Reliability, SampleReceiver, SubMode, Subscriber};
use zenoh::sync::channel::{RecvError, RecvTimeoutError, TryRecvError};
use zenoh::sync::zready;
use zenoh::time::Period;
use zenoh::Result as ZResult;
use zenoh_core::{zread, zwrite};

use crate::session_ext::SessionRef;

use super::PublicationCache;

const MERGE_QUEUE_INITIAL_CAPCITY: usize = 32;
const REPLIES_RECV_QUEUE_INITIAL_CAPCITY: usize = 3;

/// The builder of QueryingSubscriber, allowing to configure it.
#[derive(Clone)]
pub struct QueryingSubscriberBuilder<'a, 'b> {
    session: SessionRef<'a>,
    sub_key_expr: KeyExpr<'b>,
    reliability: Reliability,
    mode: SubMode,
    period: Option<Period>,
    query_key_expr: KeyExpr<'b>,
    query_value_selector: String,
    query_target: QueryTarget,
    query_consolidation: QueryConsolidation,
}

impl<'a, 'b> QueryingSubscriberBuilder<'a, 'b> {
    pub(crate) fn new(
        session: SessionRef<'a>,
        sub_key_expr: KeyExpr<'b>,
    ) -> QueryingSubscriberBuilder<'a, 'b> {
        // By default query all matching publication caches and storages
        let query_target = QueryTarget {
            kind: PublicationCache::QUERYABLE_KIND | STORAGE,
            target: Target::All,
        };

        // By default no query consolidation, to receive more than 1 sample per-resource
        // (in history of publications is available)
        let query_consolidation = QueryConsolidation::none();

        QueryingSubscriberBuilder {
            session,
            sub_key_expr: sub_key_expr.clone(),
            reliability: Reliability::default(),
            mode: SubMode::default(),
            period: None,
            query_key_expr: sub_key_expr,
            query_value_selector: "".into(),
            query_target,
            query_consolidation,
        }
    }

    /// Change the subscription reliability.
    #[inline]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.reliability = reliability;
        self
    }

    /// Change the subscription reliability to Reliable.
    #[inline]
    pub fn reliable(mut self) -> Self {
        self.reliability = Reliability::Reliable;
        self
    }

    /// Change the subscription reliability to BestEffort.
    #[inline]
    pub fn best_effort(mut self) -> Self {
        self.reliability = Reliability::BestEffort;
        self
    }

    /// Change the subscription mode.
    #[inline]
    pub fn mode(mut self, mode: SubMode) -> Self {
        self.mode = mode;
        self
    }

    /// Change the subscription mode to Push.
    #[inline]
    pub fn push_mode(mut self) -> Self {
        self.mode = SubMode::Push;
        self.period = None;
        self
    }

    /// Change the subscription mode to Pull.
    #[inline]
    pub fn pull_mode(mut self) -> Self {
        self.mode = SubMode::Pull;
        self
    }

    /// Change the subscription period.
    #[inline]
    pub fn period(mut self, period: Option<Period>) -> Self {
        self.period = period;
        self
    }

    /// Change the selector to be used for queries.
    #[inline]
    pub fn query_selector<IntoSelector>(mut self, query_selector: IntoSelector) -> Self
    where
        IntoSelector: Into<Selector<'b>>,
    {
        let selector = query_selector.into();
        self.query_key_expr = selector.key_selector.to_owned();
        self.query_value_selector = selector.value_selector.to_string();
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

    fn with_static_keys(self) -> QueryingSubscriberBuilder<'a, 'static> {
        QueryingSubscriberBuilder {
            session: self.session,
            sub_key_expr: self.sub_key_expr.to_owned(),
            reliability: self.reliability,
            mode: self.mode,
            period: self.period,
            query_key_expr: self.query_key_expr.to_owned(),
            query_value_selector: "".to_string(),
            query_target: self.query_target,
            query_consolidation: self.query_consolidation,
        }
    }
}

impl<'a, 'b> Future for QueryingSubscriberBuilder<'a, 'b> {
    type Output = ZResult<QueryingSubscriber<'a>>;

    #[inline]
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(QueryingSubscriber::new(
            Pin::into_inner(self).clone().with_static_keys(),
        ))
    }
}

impl<'a, 'b> ZFuture for QueryingSubscriberBuilder<'a, 'b> {
    #[inline]
    fn wait(self) -> ZResult<QueryingSubscriber<'a>> {
        QueryingSubscriber::new(self.with_static_keys())
    }
}

pub struct QueryingSubscriber<'a> {
    conf: QueryingSubscriberBuilder<'a, 'a>,
    subscriber: Subscriber<'a>,
    receiver: QueryingSubscriberReceiver,
}

impl<'a> QueryingSubscriber<'a> {
    fn new(conf: QueryingSubscriberBuilder<'a, 'a>) -> ZResult<QueryingSubscriber<'a>> {
        use zenoh::prelude::EntityFactory;
        // declare subscriber at first
        let mut subscriber = match conf.session.clone() {
            SessionRef::Borrow(session) => session
                .subscribe(&conf.sub_key_expr)
                .reliability(conf.reliability)
                .mode(conf.mode)
                .period(conf.period)
                .wait()?,
            SessionRef::Shared(session) => session
                .subscribe(&conf.sub_key_expr)
                .reliability(conf.reliability)
                .mode(conf.mode)
                .period(conf.period)
                .wait()?,
        };

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

    /// Close this QueryingSubscriber
    #[inline]
    pub fn close(self) -> impl ZFuture<Output = ZResult<()>> {
        self.subscriber.close()
    }

    /// Return the QueryingSubscriberReceiver associated to this subscriber.
    #[inline]
    pub fn receiver(&mut self) -> &mut QueryingSubscriberReceiver {
        &mut self.receiver
    }

    /// Issue a new query using the configured selector.
    #[inline]
    pub fn query(&mut self) -> impl ZFuture<Output = ZResult<()>> {
        self.query_on_selector(
            Selector {
                key_selector: self.conf.query_key_expr.clone(),
                value_selector: self.conf.query_value_selector.clone().into(),
            },
            self.conf.query_target.clone(),
            self.conf.query_consolidation.clone(),
        )
    }

    /// Issue a new query on the specified selector.
    #[inline]
    pub fn query_on<'c, IntoSelector>(
        &mut self,
        selector: IntoSelector,
        target: QueryTarget,
        consolidation: QueryConsolidation,
    ) -> impl ZFuture<Output = ZResult<()>>
    where
        IntoSelector: Into<Selector<'c>>,
    {
        self.query_on_selector(selector.into(), target, consolidation)
    }

    #[inline]
    fn query_on_selector(
        &mut self,
        selector: Selector,
        target: QueryTarget,
        consolidation: QueryConsolidation,
    ) -> impl ZFuture<Output = ZResult<()>> {
        let mut state = zwrite!(self.receiver.state);
        log::debug!("Start query on {}", selector);
        match self
            .conf
            .session
            .get(selector)
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

impl Stream for QueryingSubscriber<'_> {
    type Item = Sample;

    #[inline(always)]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.receiver.poll_next(cx)
    }
}

impl futures::stream::FusedStream for QueryingSubscriber<'_> {
    #[inline(always)]
    fn is_terminated(&self) -> bool {
        self.receiver.is_terminated()
    }
}

impl Receiver<Sample> for QueryingSubscriber<'_> {
    #[inline(always)]
    fn recv_async(&self) -> flume::r#async::RecvFut<'_, Sample> {
        self.receiver.recv_async()
    }

    #[inline(always)]
    fn recv(&self) -> Result<Sample, RecvError> {
        self.receiver.recv()
    }

    #[inline(always)]
    fn try_recv(&self) -> Result<Sample, TryRecvError> {
        self.receiver.try_recv()
    }

    #[inline(always)]
    fn recv_timeout(&self, timeout: Duration) -> Result<Sample, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    #[inline(always)]
    fn recv_deadline(&self, deadline: Instant) -> Result<Sample, RecvTimeoutError> {
        self.receiver.recv_deadline(deadline)
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

impl futures::stream::FusedStream for QueryingSubscriberReceiver {
    #[inline(always)]
    fn is_terminated(&self) -> bool {
        let state = &mut zread!(self.state);
        state.is_terminated()
    }
}

impl Receiver<Sample> for QueryingSubscriberReceiver {
    fn recv_async(&self) -> flume::r#async::RecvFut<'_, Sample> {
        // TODO find a better way to forge a RecvFut
        let (sender, receiver) = flume::bounded(1);
        let _ = sender.send(self.recv().unwrap());
        receiver.into_recv_async()
    }

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
                            log::trace!("Reply received: {}", reply.sample.key_expr);
                            reply.sample.ensure_timestamp();
                            mself.merge_queue.push(reply.sample);
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
                log::trace!("Pub received in parallel of query: {}", sample.key_expr);
                sample.ensure_timestamp();
                mself.merge_queue.push(sample);
            }

            // sort and remove duplicates from merge_queue
            mself
                .merge_queue
                .sort_by_key(|sample| *sample.get_timestamp().unwrap());
            mself
                .merge_queue
                .dedup_by_key(|sample| *sample.get_timestamp().unwrap());
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

impl futures::stream::FusedStream for InnerState {
    #[inline(always)]
    fn is_terminated(&self) -> bool {
        self.replies_recv_queue.is_empty() && self.subscriber_recv.is_terminated()
    }
}

impl InnerState {
    fn recv(&mut self) -> Result<Sample, RecvError> {
        // if there are queries is in progress
        if !self.replies_recv_queue.is_empty() {
            // get all replies and add them to merge_queue
            for recv in self.replies_recv_queue.drain(..) {
                while let Ok(mut reply) = recv.recv() {
                    log::trace!("Reply received: {}", reply.sample.key_expr);
                    reply.sample.ensure_timestamp();
                    self.merge_queue.push(reply.sample);
                }
            }
            log::debug!(
                "All queries completed, received {} replies",
                self.merge_queue.len()
            );

            // get all publications received during the query and add them to merge_queue
            while let Ok(mut sample) = self.subscriber_recv.try_recv() {
                log::trace!("Pub received in parallel of query: {}", sample.key_expr);
                sample.ensure_timestamp();
                self.merge_queue.push(sample);
            }

            // sort and remove duplicates from merge_queue
            self.merge_queue
                .sort_by_key(|sample| *sample.get_timestamp().unwrap());
            self.merge_queue
                .dedup_by_key(|sample| *sample.get_timestamp().unwrap());
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
                            log::trace!("Reply received: {}", reply.sample.key_expr);
                            reply.sample.ensure_timestamp();
                            self.merge_queue.push(reply.sample);
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
                log::trace!("Pub received in parallel of query: {}", sample.key_expr);
                sample.ensure_timestamp();
                self.merge_queue.push(sample);
            }

            // sort and remove duplicates from merge_queue
            self.merge_queue
                .sort_by_key(|sample| *sample.get_timestamp().unwrap());
            self.merge_queue
                .dedup_by_key(|sample| *sample.get_timestamp().unwrap());
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
                            log::trace!("Reply received: {}", reply.sample.key_expr);
                            reply.sample.ensure_timestamp();
                            self.merge_queue.push(reply.sample);
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
                log::trace!("Pub received in parallel of query: {}", sample.key_expr);
                sample.ensure_timestamp();
                self.merge_queue.push(sample);
            }

            // sort and remove duplicates from merge_queue
            self.merge_queue
                .sort_by_key(|sample| *sample.get_timestamp().unwrap());
            self.merge_queue
                .dedup_by_key(|sample| *sample.get_timestamp().unwrap());
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
