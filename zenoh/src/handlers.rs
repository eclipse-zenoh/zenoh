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
use zenoh_collections::RingBuffer;
use zenoh_result::ZResult;

/// An alias for `Arc<T>`.
pub type Dyn<T> = std::sync::Arc<T>;
/// An immutable callback function.
pub type Callback<'a, T> = Dyn<dyn Fn(T) + Send + Sync + 'a>;

/// A type that can be converted into a [`Callback`]-receiver pair.
///
/// When Zenoh functions accept types that implement these, it intends to use the [`Callback`] as just that,
/// while granting you access to the receiver through the returned value via [`std::ops::Deref`] and [`std::ops::DerefMut`].
///
/// Any closure that accepts `T` can be converted into a pair of itself and `()`.
pub trait IntoCallbackReceiverPair<'a, T> {
    type Receiver;
    fn into_cb_receiver_pair(self) -> (Callback<'a, T>, Self::Receiver);
}

impl<'a, T, F> IntoCallbackReceiverPair<'a, T> for F
where
    F: Fn(T) + Send + Sync + 'a,
{
    type Receiver = ();
    fn into_cb_receiver_pair(self) -> (Callback<'a, T>, Self::Receiver) {
        (Dyn::from(self), ())
    }
}

impl<T: Send + 'static> IntoCallbackReceiverPair<'static, T>
    for (flume::Sender<T>, flume::Receiver<T>)
{
    type Receiver = flume::Receiver<T>;

    fn into_cb_receiver_pair(self) -> (Callback<'static, T>, Self::Receiver) {
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

pub struct DefaultHandler;
impl<T: Send + 'static> IntoCallbackReceiverPair<'static, T> for DefaultHandler {
    type Receiver = flume::Receiver<T>;
    fn into_cb_receiver_pair(self) -> (Callback<'static, T>, Self::Receiver) {
        flume::bounded(*API_DATA_RECEPTION_CHANNEL_SIZE).into_cb_receiver_pair()
    }
}
impl<T: Send + Sync + 'static> IntoCallbackReceiverPair<'static, T>
    for (std::sync::mpsc::SyncSender<T>, std::sync::mpsc::Receiver<T>)
{
    type Receiver = std::sync::mpsc::Receiver<T>;
    fn into_cb_receiver_pair(self) -> (Callback<'static, T>, Self::Receiver) {
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
pub struct CallbackPair<Callback, DropFn>
where
    DropFn: FnMut() + Send + Sync + 'static,
{
    pub callback: Callback,
    pub drop: DropFn,
}

impl<Callback, DropFn> Drop for CallbackPair<Callback, DropFn>
where
    DropFn: FnMut() + Send + Sync + 'static,
{
    fn drop(&mut self) {
        (self.drop)()
    }
}

impl<'a, OnEvent, Event, DropFn> IntoCallbackReceiverPair<'a, Event>
    for CallbackPair<OnEvent, DropFn>
where
    OnEvent: Fn(Event) + Send + Sync + 'a,
    DropFn: FnMut() + Send + Sync + 'static,
{
    type Receiver = ();
    fn into_cb_receiver_pair(self) -> (Callback<'a, Event>, Self::Receiver) {
        (Dyn::from(move |evt| (self.callback)(evt)), ())
    }
}

/// A handler that implements a cache storing the last N elements.
/// The cache can be pulled via the [`PullHandler`] and its [`pull()`](PullHandler::pull) operation.
/// This hangler is usually used in combination with a subscriber.
/// E.g.: `session.declare_subscriber().with(PullCache::new(10))`.
pub struct PullCache<T> {
    cache: Arc<Mutex<RingBuffer<T>>>,
}

impl<T> PullCache<T> {
    pub fn new(capacity: usize) -> Self {
        PullCache {
            cache: Arc::new(Mutex::new(RingBuffer::new(capacity))),
        }
    }
}

pub struct PullHandler<T> {
    cache: Weak<Mutex<RingBuffer<T>>>,
}

impl<T> PullHandler<T> {
    pub fn pull(&self) -> ZResult<Option<T>> {
        let Some(cache) = self.cache.upgrade() else {
            bail!("The cache has been deleted.");
        };
        let mut guard = cache.lock().map_err(|e| zerror!("{}", e))?;
        Ok(guard.pull())
    }
}

impl<T: Send + 'static> IntoCallbackReceiverPair<'static, T> for PullCache<T> {
    type Receiver = PullHandler<T>;

    fn into_cb_receiver_pair(self) -> (Callback<'static, T>, Self::Receiver) {
        let receiver = PullHandler {
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
