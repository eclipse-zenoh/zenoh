//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
use super::LifoQueue;
use async_std::sync::{Arc, Weak};
use async_std::task;
use std::fmt;
use std::ops::{Deref, DerefMut, Drop};

/// Provides a pool of pre-allocated s that are automaticlaly reinserted into
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

    pub async fn take(&self) -> RecyclingObject<T> {
        let obj = self.inner.pull().await;
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

    pub async fn recycle(mut self) {
        if let Some(pool) = self.pool.upgrade() {
            if let Some(obj) = self.object.take() {
                pool.push(obj).await;
            }
        }
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

impl<T> Drop for RecyclingObject<T> {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            if let Some(obj) = self.object.take() {
                task::block_on(pool.push(obj));
            }
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for RecyclingObject<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("").field("inner", &self).finish()
    }
}
