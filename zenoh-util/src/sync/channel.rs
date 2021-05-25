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
use std::time::{Duration, Instant};

pub use flume::{Iter, RecvError, RecvTimeoutError, TryIter, TryRecvError};

pub trait Receiver<T> {
    fn recv(&self) -> Result<T, RecvError>;

    fn try_recv(&self) -> Result<T, TryRecvError>;

    fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError>;

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
        }

        impl$(<$( $lt ),+>)? Receiver<$recv_type> for $struct_name$(<$( $lt ),+>)? {
            #[inline(always)]
            fn recv(&self) -> Result<$recv_type, RecvError> {
                self.receiver.recv()
            }

            #[inline(always)]
            fn try_recv(&self) -> Result<$recv_type, TryRecvError> {
                self.receiver.try_recv()
            }

            #[inline(always)]
            fn recv_timeout(&self, timeout: std::time::Duration) -> Result<$recv_type, RecvTimeoutError> {
                self.receiver.recv_timeout(timeout)
            }

            #[inline(always)]
            fn recv_deadline(&self, deadline: std::time::Instant) -> Result<$recv_type, RecvTimeoutError> {
                self.receiver.recv_deadline(deadline)
            }
        }

        impl $struct_name$(<$( $lt ),+>)?{
            #[inline(always)]
            fn iter(&self) -> Iter<'_, $recv_type> {
                self.receiver.iter()
            }

            #[inline(always)]
            fn try_iter(&self) -> TryIter<'_, $recv_type> {
                self.receiver.try_iter()
            }
        }

        impl$(<$( $lt ),+>)? async_std::stream::Stream for $struct_name$(<$( $lt ),+>)? {
            type Item = $recv_type;

            #[inline(always)]
            fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
                use futures_lite::StreamExt;
                self.stream.poll_next(cx)
            }
        }
    }
}
