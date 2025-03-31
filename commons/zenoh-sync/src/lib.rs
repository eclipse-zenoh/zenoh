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
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures::FutureExt;

pub mod event;
pub use event::*;

pub mod fifo_queue;
pub use fifo_queue::*;

pub mod lifo_queue;
pub use lifo_queue::*;

pub mod object_pool;
pub use object_pool::*;

pub mod mvar;
pub use mvar::*;

pub mod condition;
pub use condition::*;

pub mod signal;
pub use signal::*;

pub mod cache;
pub use cache::*;

pub fn get_mut_unchecked<T>(arc: &mut std::sync::Arc<T>) -> &mut T {
    unsafe { &mut (*(std::sync::Arc::as_ptr(arc) as *mut T)) }
}

/// An alias for `Pin<Box<dyn Future<Output = T> + Send>>`.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct PinBoxFuture<T>(Pin<Box<dyn Future<Output = T> + Send>>);

impl<T> Future for PinBoxFuture<T> {
    type Output = T;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.poll_unpin(cx)
    }
}

#[inline]
pub fn pinbox<T>(fut: impl Future<Output = T> + Send + 'static) -> PinBoxFuture<T> {
    PinBoxFuture(Box::pin(fut))
}
