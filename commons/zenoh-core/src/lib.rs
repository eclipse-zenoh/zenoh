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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
pub use lazy_static::lazy_static;
pub mod macros;

use std::future::{Future, IntoFuture, Ready};

// Re-exports after moving ZError/ZResult to zenoh-result
pub use zenoh_result::{bail, to_zerror, zerror};
pub mod zresult {
    pub use zenoh_result::*;
}
pub use zresult::{Error, ZResult as Result};

/// A resolvable execution, either sync or async
pub trait Resolvable {
    type To: Sized;
}

/// Trick used to mark `<Resolve as IntoFuture>::IntoFuture` bound as Send
#[doc(hidden)]
pub trait IntoSendFuture: Resolvable {
    type IntoFuture: Future<Output = Self::To> + Send;
}

impl<T> IntoSendFuture for T
where
    T: Resolvable + IntoFuture<Output = Self::To>,
    T::IntoFuture: Send,
{
    type IntoFuture = T::IntoFuture;
}

/// Synchronous execution of a resolvable
pub trait Wait: Resolvable {
    /// Synchronously execute and wait
    fn wait(self) -> Self::To;
}

/// Zenoh's trait for resolving builder patterns.
///
/// Builder patterns in Zenoh can be resolved by awaiting them, in async context,
/// and [`Wait::wait`] in sync context.
/// We advise to prefer the usage of asynchronous execution, and to use synchronous one with caution
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub trait Resolve<Output>:
    Resolvable<To = Output>
    + Wait
    + IntoSendFuture
    + IntoFuture<IntoFuture = <Self as IntoSendFuture>::IntoFuture, Output = Output>
    + Send
{
}

impl<T, Output> Resolve<Output> for T where
    T: Resolvable<To = Output>
        + Wait
        + IntoSendFuture
        + IntoFuture<IntoFuture = <Self as IntoSendFuture>::IntoFuture, Output = Output>
        + Send
{
}

// Closure to wait
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct ResolveClosure<C, To>(C)
where
    To: Sized + Send,
    C: FnOnce() -> To + Send;

impl<C, To> ResolveClosure<C, To>
where
    To: Sized + Send,
    C: FnOnce() -> To + Send,
{
    pub fn new(c: C) -> Self {
        Self(c)
    }
}

impl<C, To> Resolvable for ResolveClosure<C, To>
where
    To: Sized + Send,
    C: FnOnce() -> To + Send,
{
    type To = To;
}

impl<C, To> IntoFuture for ResolveClosure<C, To>
where
    To: Sized + Send,
    C: FnOnce() -> To + Send,
{
    type Output = <Self as Resolvable>::To;
    type IntoFuture = Ready<<Self as Resolvable>::To>;

    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(self.wait())
    }
}

impl<C, To> Wait for ResolveClosure<C, To>
where
    To: Sized + Send,
    C: FnOnce() -> To + Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        self.0()
    }
}

// Future to wait
#[must_use = "Resolvables do nothing unless you resolve them using `.await` or `zenoh::Wait::wait`"]
pub struct ResolveFuture<F, To>(F)
where
    To: Sized + Send,
    F: Future<Output = To> + Send;

impl<F, To> ResolveFuture<F, To>
where
    To: Sized + Send,
    F: Future<Output = To> + Send,
{
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F, To> Resolvable for ResolveFuture<F, To>
where
    To: Sized + Send,
    F: Future<Output = To> + Send,
{
    type To = To;
}

impl<F, To> IntoFuture for ResolveFuture<F, To>
where
    To: Sized + Send,
    F: Future<Output = To> + Send,
{
    type Output = To;
    type IntoFuture = F;

    fn into_future(self) -> Self::IntoFuture {
        self.0
    }
}

impl<F, To> Wait for ResolveFuture<F, To>
where
    To: Sized + Send,
    F: Future<Output = To> + Send,
{
    fn wait(self) -> <Self as Resolvable>::To {
        zenoh_runtime::ZRuntime::Application.block_in_place(self.0)
    }
}

pub use zenoh_result::{likely, unlikely};
