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
use std::{
    any::Any,
    fmt,
    ops::{Deref, DerefMut, Drop},
    sync::{Arc, Weak},
};

use zenoh_buffers::ZSliceBuffer;

use super::LifoQueue;

/// Provides a pool of pre-allocated objects that are automaticlaly reinserted into
/// the pool when dropped.
pub struct RecyclingObjectPool<T, F>
where
    F: Fn() -> T,
{
    inner: Arc<LifoQueue<T>>,
    f: F,
}

impl<T, F: Fn() -> T> RecyclingObjectPool<T, F> {
    pub fn new(num: usize, f: F) -> RecyclingObjectPool<T, F> {
        let inner: Arc<LifoQueue<T>> = Arc::new(LifoQueue::new(num));
        for _ in 0..num {
            let obj = (f)();
            inner.try_push(obj);
        }
        RecyclingObjectPool { inner, f }
    }

    pub fn alloc(&self) -> RecyclingObject<T> {
        RecyclingObject::new((self.f)(), Weak::new())
    }

    pub fn try_take(&self) -> Option<RecyclingObject<T>> {
        self.inner
            .try_pull()
            .map(|obj| RecyclingObject::new(obj, Arc::downgrade(&self.inner)))
    }

    pub fn take(&self) -> RecyclingObject<T> {
        let obj = self.inner.pull();
        RecyclingObject::new(obj, Arc::downgrade(&self.inner))
    }
}

#[derive(Clone)]
pub struct RecyclingObject<T> {
    pool: Weak<LifoQueue<T>>,
    object: Option<T>,
}

impl<T> RecyclingObject<T> {
    pub fn new(obj: T, pool: Weak<LifoQueue<T>>) -> RecyclingObject<T> {
        RecyclingObject {
            pool,
            object: Some(obj),
        }
    }

    pub fn recycle(mut self) {
        if let Some(pool) = self.pool.upgrade() {
            if let Some(obj) = self.object.take() {
                pool.push(obj);
            }
        }
    }
}

impl<T: PartialEq> Eq for RecyclingObject<T> {}

impl<T: PartialEq> PartialEq for RecyclingObject<T> {
    fn eq(&self, other: &Self) -> bool {
        self.object == other.object
    }
}

impl<T> Deref for RecyclingObject<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.object.as_ref().unwrap()
    }
}

impl<T> DerefMut for RecyclingObject<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.object.as_mut().unwrap()
    }
}

impl<T> From<T> for RecyclingObject<T> {
    fn from(obj: T) -> RecyclingObject<T> {
        RecyclingObject::new(obj, Weak::new())
    }
}

impl<T> Drop for RecyclingObject<T> {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            if let Some(obj) = self.object.take() {
                pool.push(obj);
            }
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for RecyclingObject<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("").field("inner", &self.object).finish()
    }
}

// Buffer impl
impl AsRef<[u8]> for RecyclingObject<Box<[u8]>> {
    fn as_ref(&self) -> &[u8] {
        self.deref()
    }
}

impl AsMut<[u8]> for RecyclingObject<Box<[u8]>> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.deref_mut()
    }
}

impl ZSliceBuffer for RecyclingObject<Box<[u8]>> {
    fn as_slice(&self) -> &[u8] {
        self.as_ref()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
