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

pub mod zresult;
pub use zresult::Error;
pub use zresult::ZResult as Result;

/// Zenoh's trait for resolving builder patterns with a synchronous operation.
///
/// Many builder patterns in Zenoh can be resolved with either [`SyncResolve`] or [`AsyncResolve`],
/// we advise sticking to either one or the other, rather than mixing them up.
pub trait SyncResolve: Sized + Resolvable + AsyncResolve {
    fn res_sync(self) -> Self::Output;
    /// Resolves the builder pattern synchronously
    ///
    /// This method is just a convenience alias to [`SyncResolve::res_sync`]
    fn res(self) -> Self::Output {
        self.res_sync()
    }
}

/// Zenoh's trait for resolving builder patterns with an asynchronous operation.
///
/// Many builder patterns in Zenoh can be resolved with either [`SyncResolve`] or [`AsyncResolve`],
/// we advise sticking to either one or the other, rather than mixing them up.
pub trait AsyncResolve: Sized + Resolvable {
    /// This type is only exposed because trait functions can't return `impl Future<Self::Output> + Send`, do not rely on it being stable
    type Future: std::future::Future<Output = Self::Output> + Send;
    fn res_async(self) -> Self::Future;
    /// Resolves the builder pattern asynchronously
    ///
    /// This method is just a convenience alias to [`AsyncResolve::res_async`]
    fn res(self) -> Self::Future {
        self.res_async()
    }
}

/// Any Zenoh builder pattern is implemented as a [`Resolvable`] data structure.
/// Resolving into [`Resolvable::Output`] can be done synchronously through [`SyncResolve::res`] (or equivalently [`SyncResolve::res_sync`]),
/// or asynchronously through [`AsyncResolve::res`] (equivalent to [`AsyncResolve::res_async`])
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub trait Resolvable {
    type Output;
}

/// A convenience trait to ease notation on functions that return `impl Resolvable<Output = Output> + SyncResolve + AsyncResolve + Send`
#[must_use = "Resolvables do nothing unless you resolve them using the `res` method from either `SyncResolve` or `AsyncResolve`"]
pub trait Resolve<Output>: Resolvable<Output = Output> + SyncResolve + AsyncResolve + Send {}
//noinspection ALL
impl<T, Output> Resolve<Output> for T where
    T: Resolvable<Output = Output> + SyncResolve + AsyncResolve + Send
{
}
