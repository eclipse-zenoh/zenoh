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
use crate::API_DATA_RECEPTION_CHANNEL_SIZE;

use std::sync::{Arc, Mutex, Weak};
use zenoh_collections::RingBuffer as RingBuffer_inner;
use zenoh_result::ZResult;

/// An alias for `Arc<T>`.
pub type Dyn<T> = std::sync::Arc<T>;

/// An immutable callback function.
pub type Callback<'a, T> = Dyn<dyn Fn(T) + Send + Sync + 'a>;

/// A type that can be converted into a [`Callback`]-handler pair.
///
/// When Zenoh functions accept types that implement these, it intends to use the [`Callback`] as just that,
/// while granting you access to the handler through the returned value via [`std::ops::Deref`] and [`std::ops::DerefMut`].
///
/// Any closure that accepts `T` can be converted into a pair of itself and `()`.
pub trait IntoHandler<'a, T> {
    type Handler;

    fn into_handler(self) -> (Callback<'a, T>, Self::Handler);
}

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
                    log::error!("{}", e)
                }
            }),
            receiver,
        )
    }
}

/// The default handler in Zenoh is a FIFO queue.
pub struct DefaultHandler;

impl<T: Send + 'static> IntoHandler<'static, T> for DefaultHandler {
    type Handler = flume::Receiver<T>;

    fn into_handler(self) -> (Callback<'static, T>, Self::Handler) {
        flume::bounded(*API_DATA_RECEPTION_CHANNEL_SIZE).into_handler()
    }
}

impl<T: Send + Sync + 'static> IntoHandler<'static, T>
    for (std::sync::mpsc::SyncSender<T>, std::sync::mpsc::Receiver<T>)
{
    type Handler = std::sync::mpsc::Receiver<T>;

    fn into_handler(self) -> (Callback<'static, T>, Self::Handler) {
        let (sender, receiver) = self;
        (
            Dyn::new(move |t| {
                if let Err(e) = sender.send(t) {
                    log::error!("{}", e)
                }
            }),
            receiver,
        )
    }
}

pub struct RingBuffer<T> {
    cache: Arc<Mutex<RingBuffer_inner<T>>>,
}

impl<T> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        RingBuffer {
            cache: Arc::new(Mutex::new(RingBuffer_inner::new(capacity))),
        }
    }
}

pub struct RingBufferHandler<T> {
    cache: Weak<Mutex<RingBuffer_inner<T>>>,
}

impl<T> RingBufferHandler<T> {
    pub fn recv(&self) -> ZResult<Option<T>> {
        let Some(cache) = self.cache.upgrade() else {
            bail!("The cache has been deleted.");
        };
        let mut guard = cache.lock().map_err(|e| zerror!("{}", e))?;
        Ok(guard.pull())
    }
}

impl<T: Send + 'static> IntoHandler<'static, T> for RingBuffer<T> {
    type Handler = RingBufferHandler<T>;

    fn into_handler(self) -> (Callback<'static, T>, Self::Handler) {
        let receiver = RingBufferHandler {
            cache: Arc::downgrade(&self.cache),
        };
        (
            Dyn::new(move |t| match self.cache.lock() {
                Ok(mut g) => {
                    // Eventually drop the oldest element.
                    g.push_force(t);
                }
                Err(e) => log::error!("{}", e),
            }),
            receiver,
        )
    }
}

/// A function that can transform a [`FnMut`]`(T)` to
/// a [`Fn`]`(T)` with the help of a [`Mutex`](std::sync::Mutex).
pub fn locked<T>(fnmut: impl FnMut(T)) -> impl Fn(T) {
    let lock = std::sync::Mutex::new(fnmut);
    move |x| zlock!(lock)(x)
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
