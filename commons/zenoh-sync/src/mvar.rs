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
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::Mutex;
use zenoh_core::zasynclock;

use crate::Condition;

pub struct Mvar<T> {
    inner: Mutex<Option<T>>,
    cond_put: Condition,
    cond_take: Condition,
    wait_put: AtomicUsize,
    wait_take: AtomicUsize,
}

impl<T> Mvar<T> {
    pub fn new() -> Mvar<T> {
        Mvar {
            inner: Mutex::new(None),
            cond_put: Condition::new(),
            cond_take: Condition::new(),
            wait_put: AtomicUsize::new(0),
            wait_take: AtomicUsize::new(0),
        }
    }

    pub fn has_take_waiting(&self) -> bool {
        self.wait_take.load(Ordering::Acquire) > 0
    }

    pub fn has_put_waiting(&self) -> bool {
        self.wait_put.load(Ordering::Acquire) > 0
    }

    pub async fn try_take(&self) -> Option<T> {
        let mut guard = zasynclock!(self.inner);
        if let Some(inner) = guard.take() {
            drop(guard);
            self.cond_put.notify_one();
            return Some(inner);
        }
        None
    }

    pub async fn take(&self) -> T {
        loop {
            let mut guard = zasynclock!(self.inner);
            if let Some(inner) = guard.take() {
                self.wait_take.fetch_sub(1, Ordering::AcqRel);
                drop(guard);
                self.cond_put.notify_one();
                return inner;
            }
            self.wait_take.fetch_add(1, Ordering::AcqRel);
            self.cond_take.wait(guard).await;
        }
    }

    pub async fn put(&self, inner: T) {
        loop {
            let mut guard = zasynclock!(self.inner);
            if guard.is_some() {
                self.wait_put.fetch_add(1, Ordering::AcqRel);
                self.cond_put.wait(guard).await;
            } else {
                *guard = Some(inner);
                self.wait_put.fetch_sub(1, Ordering::AcqRel);
                drop(guard);
                self.cond_take.notify_one();
                return;
            }
        }
    }
}

impl<T> Default for Mvar<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use zenoh_result::ZResult;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn mvar() -> ZResult<()> {
        use std::{sync::Arc, time::Duration};

        use super::Mvar;

        const TIMEOUT: Duration = Duration::from_secs(60);

        let count: usize = 1_000;
        let mvar: Arc<Mvar<usize>> = Arc::new(Mvar::new());

        let c_mvar = mvar.clone();
        let ch = tokio::task::spawn(async move {
            for _ in 0..count {
                let n = c_mvar.take().await;
                print!("-{n} ");
            }
        });

        let ph = tokio::task::spawn(async move {
            for i in 0..count {
                mvar.put(i).await;
                print!("+{i} ");
            }
        });

        let _ = tokio::time::timeout(TIMEOUT, ph).await?;
        let _ = tokio::time::timeout(TIMEOUT, ch).await?;
        println!();
        Ok(())
    }
}
