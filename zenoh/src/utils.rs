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

//! Some useful operations for the zenoh API.

/// Helpers to manipulate and compare key expressions.
#[deprecated = "This module is now a separate crate. Use the crate directly for shorter compile-times"]
pub use zenoh_protocol_core::wire_expr;

pub(crate) struct ClosureResolve<F>(pub F);
impl<Output, F: FnOnce() -> Output> zenoh_core::Resolvable for ClosureResolve<F> {
    type Output = Output;
}
impl<Output: Send, F: FnOnce() -> Output> zenoh_core::SyncResolve for ClosureResolve<F> {
    //noinspection ALL
    fn res_sync(self) -> Self::Output {
        self.0()
    }
}
impl<Output: Send, F: FnOnce() -> Output> zenoh_core::AsyncResolve for ClosureResolve<F> {
    type Future = futures::future::Ready<Output>;
    fn res_async(self) -> Self::Future {
        futures::future::ready(self.0())
    }
}
pub(crate) struct FutureResolve<F>(pub F);
impl<F: std::future::Future> zenoh_core::Resolvable for FutureResolve<F> {
    type Output = F::Output;
}
impl<F: std::future::Future + Send> zenoh_core::SyncResolve for FutureResolve<F> {
    fn res_sync(self) -> Self::Output {
        async_global_executor::block_on(self.0)
    }
}
impl<F: std::future::Future + Send> zenoh_core::AsyncResolve for FutureResolve<F> {
    type Future = F;
    fn res_async(self) -> Self::Future {
        self.0
    }
}
