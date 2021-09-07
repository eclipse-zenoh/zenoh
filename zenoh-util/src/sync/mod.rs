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
use futures_lite::FutureExt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub mod backoff;
pub use backoff::*;
pub mod channel;
pub mod condition;
pub use condition::*;
pub mod mvar;
pub use mvar::*;
pub mod signal;
pub use signal::*;

pub fn get_mut_unchecked<T>(arc: &mut std::sync::Arc<T>) -> &mut T {
    unsafe { &mut (*(std::sync::Arc::as_ptr(arc) as *mut T)) }
}

/// A future which output can be accessed synchronously or asynchronously.
pub trait ZFuture: Future + Send {
    /// Synchronously waits for the output of this future.
    fn wait(self) -> Self::Output;
}

/// Creates a ZFuture that is immediately ready with a value.
///
/// This `struct` is created by [`zready()`]. See its
/// documentation for more.
#[derive(Debug, Clone)]
#[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
pub struct ZReady<T>(Option<T>);

impl<T> Unpin for ZReady<T> {}

impl<T> Future for ZReady<T> {
    type Output = T;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<T> {
        Poll::Ready(self.0.take().expect("Ready polled after completion"))
    }
}

impl<T> ZFuture for ZReady<T>
where
    T: Unpin + Send,
{
    #[inline]
    fn wait(self) -> T {
        self.0.unwrap()
    }
}

/// Creates a ZFuture that is immediately ready with a value.
///
/// ZFutures created through this function are functionally similar to those
/// created through `async {}`. The main difference is that futures created
/// through this function are named and implement `Unpin` and `ZFuture`.
///
/// # Examples
///
/// ```
/// use zenoh_util::sync::zready;
///
/// # async fn run() {
/// let a = zready(1);
/// assert_eq!(a.await, 1);
/// # }
/// ```
#[inline]
pub fn zready<T>(t: T) -> ZReady<T> {
    ZReady(Some(t))
}

#[macro_export]
macro_rules! zready_try {
    ($val:expr) => {
        zenoh_util::sync::zready({
            let f = || $val;
            f()
        })
    };
}

/// An alias for `Pin<Box<dyn Future<Output = T> + Send>>`.
#[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
pub struct ZPinBoxFuture<T>(Pin<Box<dyn Future<Output = T> + Send>>);

impl<T> Future for ZPinBoxFuture<T> {
    type Output = T;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll(cx)
    }
}

impl<T> ZFuture for ZPinBoxFuture<T> {
    #[inline]
    fn wait(self) -> T {
        async_std::task::block_on(self.0)
    }
}

#[inline]
pub fn zpinbox<T>(fut: impl Future<Output = T> + Send + 'static) -> ZPinBoxFuture<T> {
    ZPinBoxFuture(Box::pin(fut))
}

pub trait Runnable {
    type Output;

    fn run(&mut self) -> Self::Output;
}

#[macro_export]
macro_rules! derive_zfuture{
    (
     $(#[$meta:meta])*
     $vis:vis struct $struct_name:ident$(<$( $lt:lifetime ),+>)? {
        $(
        $(#[$field_meta:meta])*
        $field_vis:vis $field_name:ident : $field_type:ty,
        )*
    }
    ) => {
        $(#[$meta])*
        #[must_use = "ZFutures do nothing unless you `.wait()`, `.await` or poll them"]
        $vis struct $struct_name$(<$( $lt ),+>)? {
            $(
            $(#[$field_meta:meta])*
            $field_vis $field_name : $field_type,
            )*
        }

        impl$(<$( $lt ),+>)? Future for $struct_name$(<$( $lt ),+>)? {
            type Output = <Self as Runnable>::Output;

            #[inline]
            fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<<Self as Future>::Output> {
                Poll::Ready(self.run())
            }
        }

        impl$(<$( $lt ),+>)? ZFuture for $struct_name$(<$( $lt ),+>)? {
            #[inline]
            fn wait(mut self) -> Self::Output {
                self.run()
            }
        }
    }
}
