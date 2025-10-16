//
// Copyright (c) 2023 ZettaScale Technology
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

//! Callback handler trait.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use zenoh_result::ZResult;

use crate::api::handlers::{
    callback::Callback, CallbackParameter, IntoHandler, API_DATA_RECEPTION_CHANNEL_SIZE,
};

/// A handler implementing FIFO semantics.
///
/// Note that pushing on a [`FifoChannel`] that is full will block until a slot is available.
/// E.g., a slow subscriber could block the underlying Zenoh thread because it is not emptying the
/// [`FifoChannel`] fast enough. In this case, you may want to look into [`handlers::RingChannel`](crate::api::handlers::RingChannel) that
/// will drop samples when full.
pub struct FifoChannel {
    capacity: usize,
}

impl FifoChannel {
    /// Initialize the [`FifoChannel`] with the capacity size.
    pub fn new(capacity: usize) -> Self {
        Self { capacity }
    }
}

impl Default for FifoChannel {
    fn default() -> Self {
        Self::new(*API_DATA_RECEPTION_CHANNEL_SIZE)
    }
}

/// [`FifoChannel`] handler.
/// This is the receiver side of the channel, which implements all data-receiving methods.
#[derive(Debug, Clone)]
pub struct FifoChannelHandler<T>(flume::Receiver<T>);

impl<T: CallbackParameter + Send + 'static> IntoHandler<T> for FifoChannel {
    type Handler = FifoChannelHandler<T>;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        let (sender, receiver) = flume::bounded(self.capacity);
        (
            Callback::from(move |t| {
                if let Err(error) = sender.send(t) {
                    tracing::error!(%error)
                }
            }),
            FifoChannelHandler(receiver),
        )
    }
}

impl<T> FifoChannelHandler<T> {
    /// Attempt to fetch an incoming value from the channel associated with this receiver, returning
    /// an error if the channel is empty or if all senders have been dropped.
    ///
    /// If the channel is empty, this will return [`None`].
    pub fn try_recv(&self) -> ZResult<Option<T>> {
        match self.0.try_recv() {
            Ok(value) => Ok(Some(value)),
            Err(flume::TryRecvError::Empty) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Wait for an incoming value from the channel associated with this receiver, returning an
    /// error if all senders have been dropped.
    pub fn recv(&self) -> ZResult<T> {
        self.0.recv().map_err(Into::into)
    }

    /// Wait for an incoming value from the channel associated with this receiver, returning an
    /// error if all senders have been dropped or the deadline has passed.
    ///
    /// If the deadline has expired, this will return [`None`].
    pub fn recv_deadline(&self, deadline: Instant) -> ZResult<Option<T>> {
        match self.0.recv_deadline(deadline) {
            Ok(value) => Ok(Some(value)),
            Err(flume::RecvTimeoutError::Timeout) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Wait for an incoming value from the channel associated with this receiver, returning an
    /// error if all senders have been dropped or the timeout has expired.
    ///
    /// If the timeout has expired, this will return [`None`].
    pub fn recv_timeout(&self, duration: Duration) -> ZResult<Option<T>> {
        match self.0.recv_timeout(duration) {
            Ok(value) => Ok(Some(value)),
            Err(flume::RecvTimeoutError::Timeout) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Create a blocking iterator over the values received on the channel that finishes iteration
    /// when all senders have been dropped.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter(self.0.iter())
    }

    /// A non-blocking iterator over the values received on the channel that finishes iteration when
    /// all senders have been dropped or the channel is empty.
    pub fn try_iter(&self) -> TryIter<'_, T> {
        TryIter(self.0.try_iter())
    }

    /// Take all msgs currently sitting in the channel and produce an iterator over them. Unlike
    /// `try_iter`, the iterator will not attempt to fetch any more values from the channel once
    /// the function has been called.
    pub fn drain(&self) -> Drain<'_, T> {
        Drain(self.0.drain())
    }

    /// Returns true if all senders for this channel have been dropped.
    pub fn is_disconnected(&self) -> bool {
        self.0.is_disconnected()
    }

    /// Returns true if the channel is empty.
    /// Note: Zero-capacity channels are always empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns true if the channel is full.
    /// Note: Zero-capacity channels are always full.
    pub fn is_full(&self) -> bool {
        self.0.is_full()
    }

    /// Returns the number of messages in the channel.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// If the channel is bounded, returns its capacity.
    pub fn capacity(&self) -> Option<usize> {
        self.0.capacity()
    }

    /// Get the number of senders that currently exist.
    pub fn sender_count(&self) -> usize {
        self.0.sender_count()
    }

    /// Get the number of receivers that currently exist, including this one.
    pub fn receiver_count(&self) -> usize {
        self.0.receiver_count()
    }

    /// Returns whether the receivers belong to the same channel.
    pub fn same_channel(&self, other: &Self) -> bool {
        self.0.same_channel(&other.0)
    }
}

/// This exists as a shorthand for [`FifoChannelHandler::iter`].
impl<'a, T> IntoIterator for &'a FifoChannelHandler<T> {
    type Item = T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        Iter(self.0.iter())
    }
}

impl<T> IntoIterator for FifoChannelHandler<T> {
    type Item = T;
    type IntoIter = IntoIter<T>;

    /// Creates a self-owned but semantically equivalent alternative to [`FifoChannelHandler::iter`].
    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.0.into_iter())
    }
}

/// An iterator over the msgs received from a channel.
pub struct Iter<'a, T>(flume::Iter<'a, T>);

impl<T> Iterator for Iter<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

/// A non-blocking iterator over the msgs received from a channel.
pub struct TryIter<'a, T>(flume::TryIter<'a, T>);

impl<T> Iterator for TryIter<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

/// A fixed-size iterator over the msgs drained from a channel.
#[derive(Debug)]
pub struct Drain<'a, T>(flume::Drain<'a, T>);

impl<T> Iterator for Drain<'_, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<T> ExactSizeIterator for Drain<'_, T> {
    fn len(&self) -> usize {
        self.0.len()
    }
}

/// An owned iterator over the msgs received from a channel.
pub struct IntoIter<T>(flume::IntoIter<T>);

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<T> FifoChannelHandler<T> {
    /// Asynchronously receive a value from the channel, returning an error if all senders have been
    /// dropped. If the channel is empty, the returned future will yield to the async runtime.
    pub fn recv_async(&self) -> RecvFut<'_, T> {
        RecvFut(self.0.recv_async())
    }

    /// Convert this receiver into a future that asynchronously receives a single message from the
    /// channel, returning an error if all senders have been dropped. If the channel is empty, this
    /// future will yield to the async runtime.
    pub fn into_recv_async<'a>(self) -> RecvFut<'a, T> {
        RecvFut(self.0.into_recv_async())
    }
}

/// A future which allows asynchronously receiving a message.
///
/// Can be created via [`FifoChannelHandler::recv_async`] or [`FifoChannelHandler::into_recv_async`].
#[must_use = "futures/streams/sinks do nothing unless you `.await` or poll them"]
pub struct RecvFut<'a, T>(flume::r#async::RecvFut<'a, T>);

impl<T> Future for RecvFut<'_, T> {
    type Output = ZResult<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Future::poll(Pin::new(&mut self.0), cx).map_err(Into::into)
    }
}

impl<T> futures::future::FusedFuture for RecvFut<'_, T> {
    fn is_terminated(&self) -> bool {
        futures::future::FusedFuture::is_terminated(&self.0)
    }
}

impl<T> FifoChannelHandler<T> {
    /// Create an asynchronous stream that uses this receiver to asynchronously receive messages
    /// from the channel. The receiver will continue to be usable after the stream has been dropped.
    pub fn stream(&self) -> RecvStream<'_, T> {
        RecvStream(self.0.stream())
    }

    /// Convert this receiver into a stream that allows asynchronously receiving messages from the
    /// channel.
    pub fn into_stream<'a>(self) -> RecvStream<'a, T> {
        RecvStream(self.0.into_stream())
    }
}

/// A stream which allows asynchronously receiving messages.
///
/// Can be created via [`FifoChannelHandler::stream`] or [`FifoChannelHandler::into_stream`].
#[derive(Clone)]
pub struct RecvStream<'a, T>(flume::r#async::RecvStream<'a, T>);

impl<T> RecvStream<'_, T> {
    /// See [`FifoChannelHandler::is_disconnected`].
    pub fn is_disconnected(&self) -> bool {
        self.0.is_disconnected()
    }

    /// See [`FifoChannelHandler::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// See [`FifoChannelHandler::is_full`].
    pub fn is_full(&self) -> bool {
        self.0.is_full()
    }

    /// See [`FifoChannelHandler::len`].
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// See [`FifoChannelHandler::capacity`].
    pub fn capacity(&self) -> Option<usize> {
        self.0.capacity()
    }

    /// Returns whether the SendSinks belong to the same channel.
    pub fn same_channel(&self, other: &Self) -> bool {
        self.0.same_channel(&other.0)
    }
}

impl<T> futures::stream::Stream for RecvStream<'_, T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        futures::stream::Stream::poll_next(Pin::new(&mut self.0), cx)
    }
}

impl<T> futures::stream::FusedStream for RecvStream<'_, T> {
    fn is_terminated(&self) -> bool {
        futures::stream::FusedStream::is_terminated(&self.0)
    }
}

impl<T: CallbackParameter + Clone + Send + Sync + 'static> IntoHandler<T>
    for (std::sync::mpsc::SyncSender<T>, std::sync::mpsc::Receiver<T>)
{
    type Handler = std::sync::mpsc::Receiver<T>;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        let (sender, receiver) = self;
        (
            Callback::from(move |t: T| {
                if let Err(error) = sender.send(t.clone()) {
                    tracing::error!(%error)
                }
            }),
            receiver,
        )
    }
}
