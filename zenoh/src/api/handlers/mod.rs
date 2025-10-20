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

/// A type that can be converted into a [`Callback`]-`Handler` pair.
///
/// When Zenoh functions accept types that implement this, they use the [`Callback`] as such,
/// while granting you access to the handler through the returned value via [`std::ops::Deref`] and [`std::ops::DerefMut`].
///
/// Any closure that accepts `T` can be converted into a pair of itself and `()`.
pub trait IntoHandler<T: CallbackParameter> {
    type Handler;

    fn into_handler(self) -> (Callback<T>, Self::Handler);
}

/// The default handler type.
///
/// When no channel or callback is provided to [`Subscriber`](crate::pubsub::Subscriber) or [`Query`](crate::query::Query),
/// Zenoh uses this type by default.
///
/// Currently, this type is an opaque wrapper over the [`FifoChannel`] with default settings. But the
/// separate type was created to make it possible to change the default handler implementation
/// without breaking API changes.
#[repr(transparent)]
#[derive(Default)]
pub struct DefaultHandler(FifoChannel);

impl<T: CallbackParameter + Send + 'static> IntoHandler<T> for DefaultHandler {
    type Handler = <FifoChannel as IntoHandler<T>>::Handler;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        self.0.into_handler()
    }
}
