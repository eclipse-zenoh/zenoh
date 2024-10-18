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
//! 
//! The zenoh primitives which receive data (e.g. [`Subscriber`](crate::pubsub::Subscriber), [`Query`](crate::query::Query), etc.) accept in their [`with()`](crate::pubsub::SubscriberBuilder::with) method a handler 
//! which processes received message. The handler is actually a pair of a [`Callback`] and a handler. The callback is called on each received message, the handler is an object of arbitrary type, which can be used to access
//! the data received by callback. When the handler is not needed, the handler type can be `()`. This particular case is handled by the [`callback()`](crate::pubsub::SubscriberBuilder::callback) method which directly accepts a `Fn(T)`.
//! 
//! The [`with()`](crate::pubsub::SubscriberBuilder::with) method accepts any type that implements the [`IntoHandler`] trait which in turn provides a conversion to a pair of [`Callback`] and handler.
//! 
//! The channels [`FifoChannel`] and [`RingChannel`] provided by zenoh implements the [`IntoHandler`] trait which returns a pair of [`Callback`] which pushes the data to the channel and the receiving channel's end [`FifoChannelHandler`] or [`RingChannelHandler`] 
//! correspondingly. This receiving end is stored in the constructed zenoh object (e.g[`Subscriber`](crate::pubsub::Subscriber)) and its methods can be accessed directly on this object, as it implements the 
//! [`Deref`](std::ops::Deref) and [`DerefMut`](std::ops::DerefMut) traits for the handler type.
mod callback;
mod fifo;
mod ring;

pub use callback::*;
pub use fifo::*;
pub use ring::*;

use crate::api::session::API_DATA_RECEPTION_CHANNEL_SIZE;

/// A type that can be converted into a [`Callback`]-Handler pair.
///
/// When Zenoh functions accept types that implement these, it intends to use the [`Callback`] as just that,
/// while granting you access to the handler through the returned value via [`std::ops::Deref`] and [`std::ops::DerefMut`].
///
/// Any closure that accepts `T` can be converted into a pair of itself and `()`.
pub trait IntoHandler<T> {
    type Handler;

    fn into_handler(self) -> (Callback<T>, Self::Handler);
}

/// The default handler in Zenoh is a FIFO queue.
#[repr(transparent)]
#[derive(Default)]
pub struct DefaultHandler(FifoChannel);

impl<T: Send + 'static> IntoHandler<T> for DefaultHandler {
    type Handler = <FifoChannel as IntoHandler<T>>::Handler;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        self.0.into_handler()
    }
}
