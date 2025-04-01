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

use std::{sync::Arc, time::Duration};

use crate::api::handlers::IntoHandler;

/// A function that can transform a [`FnMut`]`(T)` to
/// a [`Fn`]`(T)` with the help of a [`Mutex`](std::sync::Mutex).
pub fn locked<T>(fnmut: impl FnMut(T)) -> impl Fn(T) {
    let lock = std::sync::Mutex::new(fnmut);
    move |x| zlock!(lock)(x)
}

/// Callback type used by zenoh entities.
pub struct Callback<T>(Arc<dyn Fn(T) + Send + Sync>);

impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Callback<T> {
    /// Instantiate a `Callback` from a callback function.
    pub fn new(cb: Arc<dyn Fn(T) + Send + Sync>) -> Self {
        Self(cb)
    }

    /// Call the inner callback.
    #[inline]
    pub fn call(&self, arg: T) {
        self.0(arg)
    }
}

impl<T> IntoHandler<T> for Callback<T> {
    type Handler = ();
    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        (self, ())
    }
}

impl<T, F, H> IntoHandler<T> for (F, H)
where
    F: Fn(T) + Send + Sync + 'static,
{
    type Handler = H;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        (Callback::new(Arc::new(self.0)), self.1)
    }
}

impl<T, H> IntoHandler<T> for (Callback<T>, H) {
    type Handler = H;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        self
    }
}

impl<T: Send + 'static> IntoHandler<T> for (flume::Sender<T>, flume::Receiver<T>) {
    type Handler = flume::Receiver<T>;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        let (sender, receiver) = self;
        (
            Callback::new(Arc::new(move |t| {
                if let Err(e) = sender.send(t) {
                    tracing::error!("{}", e)
                }
            })),
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

impl<OnEvent, Event, DropFn> IntoHandler<Event> for CallbackDrop<OnEvent, DropFn>
where
    OnEvent: Fn(Event) + Send + Sync + 'static,
    DropFn: FnMut() + Send + Sync + 'static,
{
    type Handler = ();

    fn into_handler(self) -> (Callback<Event>, Self::Handler) {
        (move |evt| (self.callback)(evt), ()).into_handler()
    }
}

#[cfg(not(feature = "internal"))]
pub(crate) struct WeakCallback<T> {
    use_guard: Arc<()>,
    cb: Callback<T>,
}

#[zenoh_macros::internal]
pub struct WeakCallback<T> {
    use_guard: Arc<()>,
    cb: Callback<T>,
}

impl<T> Clone for WeakCallback<T> {
    fn clone(&self) -> Self {
        Self {
            use_guard: self.use_guard.clone(),
            cb: self.cb.clone(),
        }
    }
}

impl<T> WeakCallback<T> {
    #[inline]
    pub fn call(&self, arg: T) {
        self.cb.call(arg);
    }
}

#[cfg(not(feature = "internal"))]
pub(crate) struct StrongCallback<T> {
    cb: WeakCallback<T>,
}

#[zenoh_macros::internal]
// A Callback that blocks on Drop operation until all of its weak clones are dropped
pub struct StrongCallback<T> {
    cb: WeakCallback<T>,
}

impl<T> StrongCallback<T> {
    #[allow(dead_code)]
    #[inline]
    pub fn call(&self, arg: T) {
        self.cb.call(arg);
    }

    #[inline]
    pub fn weak(&self) -> &WeakCallback<T> {
        &self.cb
    }
}

impl<T> From<Callback<T>> for StrongCallback<T> {
    fn from(value: Callback<T>) -> Self {
        StrongCallback {
            cb: WeakCallback {
                use_guard: Arc::new(()),
                cb: value,
            },
        }
    }
}

impl<T> Drop for StrongCallback<T> {
    fn drop(&mut self) {
        const SLEEP_PERIOD: Duration = Duration::from_millis(1);
        while Arc::strong_count(&self.cb.use_guard) > 1 {
            std::thread::sleep(SLEEP_PERIOD);
        }
        // we are in a drop, so strong count can no longer change, so after this point
        // it is guaranteed that there are no more copies.
    }
}
