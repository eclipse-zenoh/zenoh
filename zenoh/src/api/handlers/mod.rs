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

//! Callback handler trait.
mod callback;
mod fifo;
mod ring;

pub use callback::*;
pub use fifo::*;
pub use ring::*;

use crate::api::session::API_DATA_RECEPTION_CHANNEL_SIZE;

/// An alias for `Arc<T>`.
pub type Dyn<T> = std::sync::Arc<T>;

/// A type that can be converted into a [`Callback`]-Handler pair.
///
/// When Zenoh functions accept types that implement these, it intends to use the [`Callback`] as just that,
/// while granting you access to the handler through the returned value via [`std::ops::Deref`] and [`std::ops::DerefMut`].
///
/// Any closure that accepts `T` can be converted into a pair of itself and `()`.
pub trait IntoHandler<'a, T> {
    type Handler;

    fn into_handler(self) -> (Callback<'a, T>, Self::Handler);
}

/// The default handler in Zenoh is a FIFO queue.
#[repr(transparent)]
#[derive(Default)]
pub struct DefaultHandler(FifoChannel);

impl<T: Send + 'static> IntoHandler<'static, T> for DefaultHandler {
    type Handler = <FifoChannel as IntoHandler<'static, T>>::Handler;

    fn into_handler(self) -> (Callback<'static, T>, Self::Handler) {
        self.0.into_handler()
    }
}
