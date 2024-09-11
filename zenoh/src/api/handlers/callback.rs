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
use super::{Dyn, IntoHandler};

/// A function that can transform a [`FnMut`]`(T)` to
/// a [`Fn`]`(T)` with the help of a [`Mutex`](std::sync::Mutex).
pub fn locked<T>(fnmut: impl FnMut(T)) -> impl Fn(T) {
    let lock = std::sync::Mutex::new(fnmut);
    move |x| zlock!(lock)(x)
}

/// An immutable callback function.
pub type Callback<'a, T> = Dyn<dyn Fn(T) + Send + Sync + 'a>;

impl<'a, T, F> IntoHandler<'a, T> for F
where
    F: Fn(T) + Send + Sync + 'a,
{
    type Handler = ();

    fn into_handler(self) -> (Callback<'a, T>, Self::Handler) {
        (Dyn::from(self), ())
    }
}

impl<T: Send + 'static> IntoHandler<'static, T> for (flume::Sender<T>, flume::Receiver<T>) {
    type Handler = flume::Receiver<T>;

    fn into_handler(self) -> (Callback<'static, T>, Self::Handler) {
        let (sender, receiver) = self;
        (
            Dyn::new(move |t| {
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

impl<'a, OnEvent, Event, DropFn> IntoHandler<'a, Event> for CallbackDrop<OnEvent, DropFn>
where
    OnEvent: Fn(Event) + Send + Sync + 'a,
    DropFn: FnMut() + Send + Sync + 'static,
{
    type Handler = ();

    fn into_handler(self) -> (Callback<'a, Event>, Self::Handler) {
        (Dyn::from(move |evt| (self.callback)(evt)), ())
    }
}
