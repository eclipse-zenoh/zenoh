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

use super::{IntoHandler, RingChannelSender};

/// A function that can transform a [`FnMut`]`(T)` to
/// a [`Fn`]`(T)` with the help of a [`Mutex`](std::sync::Mutex).
pub fn locked<T>(fnmut: impl FnMut(T)) -> impl Fn(T) {
    let lock = std::sync::Mutex::new(fnmut);
    move |x| zlock!(lock)(x)
}

enum CallbackInner<T> {
    Dyn(Arc<dyn Fn(&T) + Send + Sync>),
    Flume(flume::Sender<T>),
    Ring(RingChannelSender<T>),
}

pub struct Callback<T>(CallbackInner<T>);

impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Self(match &self.0 {
            CallbackInner::Dyn(cb) => CallbackInner::Dyn(cb.clone()),
            CallbackInner::Flume(tx) => CallbackInner::Flume(tx.clone()),
            CallbackInner::Ring(tx) => CallbackInner::Ring(tx.clone()),
        })
    }
}

impl<T> Callback<T> {
    pub fn new(cb: Arc<dyn Fn(&T) + Send + Sync>) -> Self {
        Self(CallbackInner::Dyn(cb))
    }

    pub(crate) fn new_flume(sender: flume::Sender<T>) -> Self {
        Self(CallbackInner::Flume(sender))
    }

    pub(crate) fn new_ring(sender: RingChannelSender<T>) -> Self {
        Self(CallbackInner::Ring(sender))
    }

    #[inline]
    pub fn call(&self, arg: &T)
    where
        T: Clone,
    {
        match &self.0 {
            CallbackInner::Dyn(cb) => cb(arg),
            CallbackInner::Flume(tx) => {
                if let Err(error) = tx.send(arg.clone()) {
                    tracing::error!(%error)
                }
            }
            CallbackInner::Ring(tx) => tx.send(arg.clone()),
        }
    }

    #[inline]
    pub(crate) fn call_by_value(&self, arg: T) {
        match &self.0 {
            CallbackInner::Dyn(cb) => cb(&arg),
            CallbackInner::Flume(tx) => {
                if let Err(error) = tx.send(arg) {
                    tracing::error!(%error)
                }
            }
            CallbackInner::Ring(tx) => tx.send(arg),
        }
    }
}

impl<T> IntoHandler<T> for Callback<T> {
    type Handler = ();
    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        (self, ())
    }
}

impl<T: Send> IntoHandler<T> for (flume::Sender<T>, flume::Receiver<T>) {
    type Handler = flume::Receiver<T>;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        let (sender, receiver) = self;
        (Callback::new_flume(sender), receiver)
    }
}

impl<T: Send> IntoHandler<T> for flume::Sender<T> {
    type Handler = ();

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        (Callback::new_flume(self), ())
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
pub struct CallbackDrop<F, DropFn>
where
    DropFn: FnMut() + Send + Sync + 'static,
{
    pub callback: F,
    pub drop: DropFn,
}

impl<F, DropFn> Drop for CallbackDrop<F, DropFn>
where
    DropFn: FnMut() + Send + Sync + 'static,
{
    fn drop(&mut self) {
        (self.drop)()
    }
}

impl<F, T, DropFn> IntoHandler<T> for CallbackDrop<F, DropFn>
where
    F: Fn(&T) + Send + Sync + 'static,
    DropFn: FnMut() + Send + Sync + 'static,
{
    type Handler = ();

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        (Callback::new(Arc::new(move |evt| (self.callback)(evt))), ())
    }
}
