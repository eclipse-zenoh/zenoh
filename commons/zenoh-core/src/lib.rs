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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
pub use lazy_static::lazy_static;
pub mod macros;
pub use macros::*;
use std::future::{Future, IntoFuture, Ready};
pub use zenoh_macros::*;
pub mod zresult;
pub use zresult::Error;
pub use zresult::ZResult as Result;

/// Zenoh's trait for resolving builder patterns with a synchronous operation.
///
/// Many builder patterns in Zenoh can be resolved with either [`Resolve`] or [`IntoFuture`],
/// we advise sticking to either one or the other, rather than mixing them up.
pub trait Resolvable {
    type To: Sized + Send;
}

pub trait IntoFutureSend: Resolvable + IntoFuture {
    type Future: Future<Output = <Self as Resolvable>::To> + Send;

    fn into_future_send(self) -> Self::Future;
}

pub trait Resolve<Output>:
    Resolvable<To = Output> + IntoFuture<Output = Output> + IntoFutureSend + Send
{
    fn wait(self) -> <Self as Resolvable>::To
    where
        Self: Sized;
}

// Closure to wait
pub struct ResolveClosure<F, To>(pub F)
where
    To: Sized + Send,
    F: FnOnce() -> To + Send;

impl<F, To> Resolvable for ResolveClosure<F, To>
where
    To: Sized + Send,
    F: FnOnce() -> To + Send,
{
    type To = To;
}

impl<F, To> IntoFutureSend for ResolveClosure<F, To>
where
    To: Sized + Send,
    F: FnOnce() -> To + Send,
{
    type Future = Ready<<Self as Resolvable>::To>;

    fn into_future_send(self) -> Self::Future {
        std::future::ready(self.wait())
    }
}

impl<F, To> Resolve<To> for ResolveClosure<F, To>
where
    To: Sized + Send,
    F: FnOnce() -> To + Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        self.0()
    }
}

impl<F, To> IntoFuture for ResolveClosure<F, To>
where
    To: Sized + Send,
    F: FnOnce() -> To + Send,
{
    type Output = To;
    type IntoFuture = <Self as IntoFutureSend>::Future;

    fn into_future(self) -> Self::IntoFuture {
        self.into_future_send()
    }
}

// Future to wait
pub struct ResolveFuture<F, To>(pub F)
where
    To: Sized + Send,
    F: Future<Output = To> + Send;

impl<F, To> Resolvable for ResolveFuture<F, To>
where
    To: Sized + Send,
    F: Future<Output = To> + Send,
{
    type To = To;
}

impl<F, To> IntoFutureSend for ResolveFuture<F, To>
where
    To: Sized + Send,
    F: Future<Output = To> + Send,
{
    type Future = F;

    fn into_future_send(self) -> Self::Future {
        self.0
    }
}

impl<F, To> Resolve<To> for ResolveFuture<F, To>
where
    To: Sized + Send,
    F: Future<Output = To> + Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        async_std::task::block_on(self.0)
    }
}

impl<F, To> IntoFuture for ResolveFuture<F, To>
where
    To: Sized + Send,
    F: Future<Output = To> + Send,
{
    type Output = To;
    type IntoFuture = <Self as IntoFutureSend>::Future;

    fn into_future(self) -> Self::IntoFuture {
        self.into_future_send()
    }
}
