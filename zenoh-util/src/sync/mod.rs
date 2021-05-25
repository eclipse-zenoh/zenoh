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

pub trait ZFuture<T>: Future + Send
where
    T: Unpin + Send,
{
    fn wait(self) -> T;
}

pub struct ZResolvedFuture<T>
where
    T: Unpin + Send,
{
    result: Option<T>,
}

impl<T> ZResolvedFuture<T>
where
    T: Unpin + Send,
{
    #[inline(always)]
    pub fn new(val: T) -> Self {
        ZResolvedFuture { result: Some(val) }
    }
}

#[macro_export]
macro_rules! zresolved {
    ($val:expr) => {
        ZResolvedFuture::new($val)
    };
}

impl<T> Future for ZResolvedFuture<T>
where
    T: Unpin + Send,
{
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(self.result.take().unwrap())
    }
}

impl<T> ZFuture<T> for ZResolvedFuture<T>
where
    T: Unpin + Send,
{
    fn wait(self) -> T {
        self.result.unwrap()
    }
}

pub struct ZPendingFuture<T>
where
    T: Unpin + Send,
{
    result: Pin<Box<dyn Future<Output = T> + Send>>,
}

impl<T> ZPendingFuture<T>
where
    T: Unpin + Send,
{
    #[inline(always)]
    pub fn new(fut: Pin<Box<dyn Future<Output = T> + Send>>) -> Self {
        ZPendingFuture { result: fut }
    }
}

#[macro_export]
macro_rules! zpending {
    ($fut:expr) => {
        ZPendingFuture::new(Box::pin($fut))
    };
}

impl<T> Future for ZPendingFuture<T>
where
    T: Unpin + Send,
{
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.result.poll(cx)
    }
}

impl<T> ZFuture<T> for ZPendingFuture<T>
where
    T: Unpin + Send,
{
    fn wait(self) -> T {
        async_std::task::block_on(self.result)
    }
}
