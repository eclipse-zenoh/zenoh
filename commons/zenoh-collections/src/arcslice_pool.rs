//
// Copyright (c) 2022 ZettaScale Technology
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
use crossbeam::queue::ArrayQueue;
use std::{
    fmt,
    ops::{Deref, DerefMut, Drop},
    sync::{Arc, Weak},
};
use zenoh_core::zuninitbuff;

/// Provides a pool of pre-allocated s that are automaticlaly reinserted into
/// the pool when dropped.
pub struct ArcSlicePool {
    inner: Arc<ArrayQueue<Arc<[u8]>>>,
}

impl ArcSlicePool {
    pub fn new(num: usize, capacity: usize) -> ArcSlicePool {
        let inner = Arc::new(ArrayQueue::new(num));
        for _ in 0..num {
            let _ = inner.push(zuninitbuff!(capacity).into());
        }
        ArcSlicePool { inner }
    }

    pub fn take(&self) -> Option<ArcSliceItem> {
        self.inner
            .pop()
            .map(|obj| ArcSliceItem::new(obj, Arc::downgrade(&self.inner)))
    }

    pub fn alloc(&self, capacity: usize) -> ArcSliceItem {
        ArcSliceItem::new(zuninitbuff!(capacity).into(), Arc::downgrade(&self.inner))
    }
}

#[derive(Clone)]
pub struct ArcSliceItem {
    pool: Weak<ArrayQueue<Arc<[u8]>>>,
    object: Arc<[u8]>,
}

impl ArcSliceItem {
    fn new(object: Arc<[u8]>, pool: Weak<ArrayQueue<Arc<[u8]>>>) -> ArcSliceItem {
        ArcSliceItem { pool, object }
    }
}

impl Eq for ArcSliceItem {}

impl PartialEq for ArcSliceItem {
    fn eq(&self, other: &Self) -> bool {
        self.object == other.object
    }
}

impl Deref for ArcSliceItem {
    type Target = Arc<[u8]>;
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.object
    }
}

impl DerefMut for ArcSliceItem {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.object
    }
}

impl From<Arc<[u8]>> for ArcSliceItem {
    fn from(obj: Arc<[u8]>) -> ArcSliceItem {
        ArcSliceItem::new(obj, Weak::new())
    }
}

impl Drop for ArcSliceItem {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            // Check if the ArcSliceItem has been cloned, if yes reinsert it into the
            // pool only when we have one reference to it
            if Arc::get_mut(&mut self.object).is_some() {
                // If the push returns an error, it will automatically drop the item.
                let _ = pool.push(self.object.clone());
            }
        }
    }
}

impl fmt::Debug for ArcSliceItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("").field("inner", &self.object).finish()
    }
}
