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

use std::sync::Arc;

use crate::api::handlers::IntoHandler;

/// A function that can transform a [`FnMut`]`(T)` to
/// a [`Fn`]`(T)` with the help of a [`Mutex`](std::sync::Mutex).
pub fn locked<T>(fnmut: impl FnMut(T)) -> impl Fn(T) {
    let lock = std::sync::Mutex::new(fnmut);
    move |x| zlock!(lock)(x)
}

pub trait CallbackParameter: 'static {
    type Message<'a>;

    fn from_message(msg: Self::Message<'_>) -> Self;
}

trait CallbackImpl<T: CallbackParameter>: Send + Sync {
    fn call(&self, t: T);

    fn call_with_message(&self, msg: T::Message<'_>) {
        self.call(T::from_message(msg))
    }
}

impl<T: CallbackParameter, F: Fn(T) + Send + Sync> CallbackImpl<T> for F {
    fn call(&self, t: T) {
        self(t)
    }
}

/// Callback type used by zenoh entities.
pub struct Callback<T: CallbackParameter>(Arc<dyn CallbackImpl<T>>);

impl<T: CallbackParameter> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: CallbackParameter> Callback<T> {
    // TODO deprecate
    /// Instantiate a `Callback` from a callback function.
    pub fn new(cb: Arc<dyn Fn(T) + Send + Sync>) -> Self {
        Self::from(move |msg| cb(msg))
    }

    /// Call the inner callback.
    #[inline]
    pub fn call(&self, arg: T) {
        self.0.call(arg)
    }

    pub(crate) fn call_with_message(&self, msg: T::Message<'_>) {
        self.0.call_with_message(msg)
    }
}

impl<T: CallbackParameter, F: Fn(T) + Send + Sync + 'static> From<F> for Callback<T> {
    fn from(value: F) -> Self {
        Self(Arc::new(value))
    }
}

impl<T: CallbackParameter> IntoHandler<T> for Callback<T> {
    type Handler = ();
    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        (self, ())
    }
}

impl<T: CallbackParameter, F, H> IntoHandler<T> for (F, H)
where
    F: Fn(T) + Send + Sync + 'static,
{
    type Handler = H;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        (Callback::from(self.0), self.1)
    }
}

impl<T: CallbackParameter, H> IntoHandler<T> for (Callback<T>, H) {
    type Handler = H;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        self
    }
}

impl<T: CallbackParameter + Send + 'static> IntoHandler<T>
    for (flume::Sender<T>, flume::Receiver<T>)
{
    type Handler = flume::Receiver<T>;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        let (sender, receiver) = self;
        (
            Callback::from(move |t| {
                if let Err(e) = sender.send(t) {
                    tracing::error!("{}", e)
                }
            }),
            receiver,
        )
    }
}

/// A handler containing 2 callback functions:
///  - `callback`: the typical callback function. `context` will be passed as its last argument.
///  - `drop`: a callback called when this handler is dropped.
///
/// It is guaranteed that:
///
///   - `callback` will never be called once `drop` has started.
///   - `drop` will only be called **once**, and **after every** `callback` has ended.
///   - The two previous guarantees imply that `call` and `drop` are never called concurrently.
pub struct CallbackDrop<Callback, DropFn>
where
    DropFn: FnMut() + Send + Sync + 'static,
{
    pub callback: Callback,
    pub drop: DropFn,
}

impl<Callback, DropFn> Drop for CallbackDrop<Callback, DropFn>
where
    DropFn: FnMut() + Send + Sync + 'static,
{
    fn drop(&mut self) {
        (self.drop)()
    }
}

impl<OnEvent, Event: CallbackParameter, DropFn> IntoHandler<Event> for CallbackDrop<OnEvent, DropFn>
where
    OnEvent: Fn(Event) + Send + Sync + 'static,
    DropFn: FnMut() + Send + Sync + 'static,
{
    type Handler = ();

    fn into_handler(self) -> (Callback<Event>, Self::Handler) {
        (move |evt| (self.callback)(evt), ()).into_handler()
    }
}
