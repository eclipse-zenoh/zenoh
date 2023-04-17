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
//! [Click here for Zenoh's documentation](../zenoh/index.html)
pub use lazy_static::lazy_static;
pub mod macros;
pub use macros::*;
use std::future::{Future, Ready};

// Re-exports after moving ZError/ZResult to zenoh-result
pub use zenoh_result::{bail, to_zerror, zerror};
pub mod zresult {
    pub use zenoh_result::*;
}
pub use zresult::Error;
pub use zresult::ZResult as Result;

pub trait Resolvable {
    type To: Sized + Send;
}

pub trait AsyncResolve: Resolvable {
    type Future: Future<Output = <Self as Resolvable>::To> + Send;

    fn res_async(self) -> Self::Future;

    fn res(self) -> Self::Future
    where
        Self: Sized,
    {
        self.res_async()
    }
}

pub trait SyncResolve: Resolvable {
    fn res_sync(self) -> <Self as Resolvable>::To;

    fn res(self) -> <Self as Resolvable>::To
    where
        Self: Sized,
    {
        self.res_sync()
    }
}

/// Zenoh's trait for resolving builder patterns.
///
/// Builder patterns in Zenoh can be resolved with [`AsyncResolve`] in async context and [`SyncResolve`] in sync context.
/// In both async and sync context calling `.res()` resolves the builder.
/// `.res()` maps to `.res_async()` in async context.
/// `.res()` maps to `.res_sync()` in sync context.
/// We advise to prefer the usage of [`AsyncResolve`] and to use [`SyncResolve`] with caution.
#[must_use = "Resolvables do nothing unless you resolve them using `.res()`."]
pub trait Resolve<Output>: Resolvable<To = Output> + SyncResolve + AsyncResolve + Send {}

impl<T, Output> Resolve<Output> for T where
    T: Resolvable<To = Output> + SyncResolve + AsyncResolve + Send
{
}

// Closure to wait
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

impl<C, To> AsyncResolve for ResolveClosure<C, To>
where
    To: Sized + Send,
    C: FnOnce() -> To + Send,
{
    type Future = Ready<<Self as Resolvable>::To>;

    fn res_async(self) -> Self::Future {
        std::future::ready(self.res_sync())
    }
}

impl<C, To> SyncResolve for ResolveClosure<C, To>
where
    To: Sized + Send,
    C: FnOnce() -> To + Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        self.0()
    }
}

// Future to wait
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

impl<F, To> AsyncResolve for ResolveFuture<F, To>
where
    To: Sized + Send,
    F: Future<Output = To> + Send,
{
    type Future = F;

    fn res_async(self) -> Self::Future {
        self.0
    }
}

impl<F, To> SyncResolve for ResolveFuture<F, To>
where
    To: Sized + Send,
    F: Future<Output = To> + Send,
{
    fn res_sync(self) -> <Self as Resolvable>::To {
        async_std::task::block_on(self.0)
    }
}

pub use zenoh_result::{likely, unlikely};
