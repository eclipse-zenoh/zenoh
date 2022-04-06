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
use std::time::{Duration, Instant};

pub use flume::r#async::RecvFut;
pub use flume::{Iter, RecvError, RecvTimeoutError, TryIter, TryRecvError};

/// A trait that mimics the [`std::sync::mpsc::Receiver`](std::sync::mpsc::Receiver).
///
/// Most structs implementing this trait in zenoh also implement the [`Stream`](async_std::stream::Stream)
/// trait so that values can be accessed synchronously or asynchronously.
pub trait Receiver<T> {
    /// Asynchronously receive a value on this receiver, returning an error if all
    /// senders have been dropped. If the channel is empty, the returned future will
    /// yield to the async runtime.
    fn recv_async(&self) -> RecvFut<'_, T>;

    /// Attempts to wait for a value on this receiver, returning an error if the
    /// corresponding channel has hung up.
    ///
    /// This function will always block the current thread if there is no data
    /// available and it's possible for more data to be sent (at least one sender
    /// still exists). Once a message is sent to the corresponding sender,
    /// this receiver will wake up and return that message.
    ///
    /// If the corresponding sender has disconnected, or it disconnects while
    /// this call is blocking, this call will wake up and return [`Err`] to
    /// indicate that no more messages can ever be received on this channel.
    /// However, since channels are buffered, messages sent before the disconnect
    /// will still be properly received.
    fn recv(&self) -> Result<T, RecvError>;

    /// Attempts to return a pending value on this receiver without blocking.
    ///
    /// This method will never block the caller in order to wait for data to
    /// become available. Instead, this will always return immediately with a
    /// possible option of pending data on the channel.
    ///
    /// This is useful for a flavor of "optimistic check" before deciding to
    /// block on a receiver.
    ///
    /// Compared with [`recv`], this function has two failure cases instead of one
    /// (one for disconnection, one for an empty buffer).
    ///
    /// [`recv`]: Self::recv
    fn try_recv(&self) -> Result<T, TryRecvError>;

    /// Attempts to wait for a value on this receiver, returning an error if the
    /// corresponding channel has hung up, or if it waits more than `timeout`.
    ///
    /// This function will always block the current thread if there is no data
    /// available and it's possible for more data to be sent (at least one sender
    /// still exists). Once a message is sent to the corresponding sender,
    /// this receiver will wake up and return that
    /// message.
    ///
    /// If the corresponding sender has disconnected, or it disconnects while
    /// this call is blocking, this call will wake up and return [`Err`] to
    /// indicate that no more messages can ever be received on this channel.
    /// However, since channels are buffered, messages sent before the disconnect
    /// will still be properly received.
    fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError>;

    /// Attempts to wait for a value on this receiver, returning an error if the
    /// corresponding channel has hung up, or if `deadline` is reached.
    ///
    /// This function will always block the current thread if there is no data
    /// available and it's possible for more data to be sent. Once a message is
    /// sent to the corresponding sender, then this
    /// receiver will wake up and return that message.
    ///
    /// If the corresponding sender has disconnected, or it disconnects while
    /// this call is blocking, this call will wake up and return [`Err`] to
    /// indicate that no more messages can ever be received on this channel.
    /// However, since channels are buffered, messages sent before the disconnect
    /// will still be properly received.
    fn recv_deadline(&self, deadline: Instant) -> Result<T, RecvTimeoutError>;
}

#[macro_export]
macro_rules! zreceiver{
    (
     $(#[$meta:meta])*
     $vis:vis struct $struct_name:ident$(<$( $lt:lifetime ),+>)? : Receiver<$recv_type:ident> {
        $(
        $(#[$field_meta:meta])*
        $field_vis:vis $field_name:ident : $field_type:ty,
        )*
    }
    ) => {
        $(#[$meta])*
        $vis struct $struct_name$(<$( $lt ),+>)? {
            $(
            $(#[$field_meta:meta])*
            $field_vis $field_name : $field_type,
            )*
            pub(crate) receiver: flume::Receiver<$recv_type>,
            pub(crate) stream: flume::r#async::RecvStream<'static, $recv_type>,
        }

        impl$(<$( $lt ),+>)? $struct_name$(<$( $lt ),+>)? {
            pub(crate) fn new(
                $($field_name : $field_type,)*
                receiver: flume::Receiver<$recv_type>)
            -> Self {
                $struct_name{
                    $($field_name,)*
                    receiver: receiver.clone(),
                    stream: receiver.into_stream(),
                }
            }

            /// Returns this `Receiver<T>` as a `TryStream<Ok = T, Error = E>` i.e: a `Stream<Result<T, E>>`.
            pub fn as_trystream<'selflifetime, E:'selflifetime>(&'selflifetime mut self) -> impl futures::TryStream<Ok = $recv_type, Error = E, Item = Result<$recv_type, E>> + 'selflifetime {
                futures::StreamExt::map(self.receiver.stream(), Ok)
            }

            /// A future that completes after the given `Receiver` has been fully processed
            /// into the sink and the sink has been flushed and closed.
            ///
            /// This future will drive the `Receiver` to keep producing items until it is exhausted,
            /// sending each item to the sink. It will complete once the `Receiver` is exhausted,
            /// the sink has received and flushed all items, and the sink is closed.
            /// Note that neither the original stream nor provided sink will be output by this future.
            /// Pass the sink by Pin<&mut S> (for example, via forward(&mut sink) inside an async fn/block)
            /// in order to preserve access to the Sink.
            pub fn forward<'selflifetime, E:'selflifetime, S>(&'selflifetime mut self, sink: S) -> futures::stream::Forward<impl futures::TryStream<Ok = $recv_type, Error = E, Item = Result<$recv_type, E>> + 'selflifetime, S>
            where
                S: futures::sink::Sink<$recv_type, Error = E>,
            {
                futures::StreamExt::forward(self.as_trystream(), sink)
            }
        }

        impl$(<$( $lt ),+>)? Receiver<$recv_type> for $struct_name$(<$( $lt ),+>)? {
            #[inline(always)]
            fn recv_async(&self) -> RecvFut<'_, $recv_type> {
                self.receiver.recv_async()
            }

            #[inline(always)]
            fn recv(&self) -> core::result::Result<$recv_type, RecvError> {
                self.receiver.recv()
            }

            #[inline(always)]
            fn try_recv(&self) -> core::result::Result<$recv_type, TryRecvError> {
                self.receiver.try_recv()
            }

            #[inline(always)]
            fn recv_timeout(&self, timeout: std::time::Duration) -> core::result::Result<$recv_type, RecvTimeoutError> {
                self.receiver.recv_timeout(timeout)
            }

            #[inline(always)]
            fn recv_deadline(&self, deadline: std::time::Instant) -> core::result::Result<$recv_type, RecvTimeoutError> {
                self.receiver.recv_deadline(deadline)
            }
        }

        impl$(<$( $lt ),+>)? futures::stream::Stream for $struct_name$(<$( $lt ),+>)? {
            type Item = $recv_type;

            #[inline(always)]
            fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
                use ::futures::stream::StreamExt;
                self.stream.poll_next_unpin(cx)
            }
        }

        impl$(<$( $lt ),+>)? futures::stream::FusedStream for $struct_name$(<$( $lt ),+>)? {
            #[inline(always)]
            fn is_terminated(&self) -> bool {
                self.stream.is_terminated()
            }
        }

        impl$(<$( $lt ),+>)? $struct_name$(<$( $lt ),+>)? {
            #[inline(always)]
            pub fn iter(&self) -> Iter<'_, $recv_type> {
                self.receiver.iter()
            }

            #[inline(always)]
            pub fn try_iter(&self) -> TryIter<'_, $recv_type> {
                self.receiver.try_iter()
            }
        }
    }
}

#[macro_export]
macro_rules! ztranscoder {
    (
        $(#[$meta:meta])*
        $vis:vis $dstrcv:ident$(<$( $lt:lifetime ),+>)? : Receiver<$dsttype:ident> <- $srcrcv:ident : Receiver<$srctype:ident>
        with
            $dstiter:ident : Iterator<$dstitertype:ident>,
            $dsttryiter:ident : Iterator<$dsttryitertype:ident>,
        {
            $(
            $(#[$field_meta:meta])*
            $field_vis:vis $field_name:ident : $field_type:ty,
            )*
        }
    ) => {
        $(#[$meta])*
        $vis struct $dstrcv$(<$( $lt ),+>)? {
            receiver: $srcrcv,
            $(
            $(#[$field_meta:meta])*
            $field_vis $field_name : $field_type,
            )*
        }

        impl$(<$( $lt ),+>)? Receiver<$dsttype> for $dstrcv$(<$( $lt ),+>)? {
            fn recv(&self) -> Result<$dsttype, RecvError> {
                loop {
                    match self.transcode(self.receiver.recv()?) {
                        Err(err) => warn!("Received an invalid {} (drop it): {}", stringify!($srctype), err),
                        Ok(data) => return Ok(data),
                    }
                }
            }

            fn try_recv(&self) -> Result<$dsttype, TryRecvError> {
                loop {
                    match self.transcode(self.receiver.try_recv()?) {
                        Err(err) => warn!("Received an invalid {} (drop it): {}", stringify!($srctype), err),
                        Ok(data) => return Ok(data),
                    }
                }
            }

            fn recv_timeout(&self, timeout: Duration) -> Result<$dsttype, RecvTimeoutError> {
                loop {
                    match self.transcode(self.receiver.recv_timeout(timeout)?) {
                        Err(err) => warn!("Received an invalid {} (drop it): {}", stringify!($srctype), err),
                        Ok(data) => return Ok(data),
                    }
                }
            }

            fn recv_deadline(&self, deadline: Instant) -> Result<$dsttype, RecvTimeoutError> {
                loop {
                    match self.transcode(self.receiver.recv_deadline(deadline)?) {
                        Err(err) => warn!("Received an invalid {} (drop it): {}", stringify!($srctype), err),
                        Ok(data) => return Ok(data),
                    }
                }
            }
        }

        impl$(<$( $lt ),+>)? Stream for $dstrcv$(<$( $lt ),+>)? {
            type Item = $dsttype;

            #[inline(always)]
            fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
                match self.receiver.poll_next(cx) {
                    Poll::Ready(Some(src)) => match self.transcode(src) {
                        Ok(dst) => Poll::Ready(Some(dst)),
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

        impl$(<$( $lt ),+>)? $dstrcv$(<$( $lt ),+>)? {
            pub fn iter(&self) -> $dstiter<'_, $($( $lt ),+)?> {
                $dstiter{
                    receiver: self,
                }
            }

            pub fn try_iter(&self) -> $dsttryiter<'_, $($( $lt ),+)?> {
                $dsttryiter{
                    receiver: self,
                }
            }
        }

        pub struct $dstiter<'w, $($( $lt ),+)?> {
            receiver: &'w $dstrcv$(<$( $lt ),+>)?,
        }

        impl<'w, $($( $lt ),+)?> Iterator for $dstiter<'w, $($( $lt ),+)?> {
            type Item = $dsttype;

            fn next(&mut self) -> Option<Self::Item> {
                self.receiver.recv().ok()
            }
        }

        pub struct $dsttryiter<'w, $($( $lt ),+)?> {
            receiver: &'w $dstrcv$(<$( $lt ),+>)?,
        }

        impl<'w, $($( $lt ),+)?> Iterator for $dsttryiter<'w, $($( $lt ),+)?> {
            type Item = $dsttype;

            fn next(&mut self) -> Option<Self::Item> {
                self.receiver.try_recv().ok()
            }
        }
    }
}
