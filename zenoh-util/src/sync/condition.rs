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
use async_std::sync::{Receiver, Sender};
use async_std::sync::{channel, MutexGuard};
use std::sync::atomic::{AtomicUsize, Ordering};

/// This is a Condition Variable similar to that provided by POSIX. 
/// As for POSIX condition variables, this assumes that a mutex is 
/// properly used to coordinate behaviour. In other terms there should
/// not be race condition on [notify].
/// 
pub struct Condition {
    wait_rx: Receiver<bool>,
    wait_tx: Sender<bool>,    
    waiters: AtomicUsize
}

impl Condition {
    /// Creates a new condition variable with a given capacity. 
    /// The capacity indicates the maximum number of tasks that 
    /// may be waiting on the condition.
    pub fn new(capacity: usize) -> Condition {
        let (wait_tx, wait_rx) = channel(capacity);        
        Condition {wait_tx, wait_rx, waiters: AtomicUsize::new(0)}
    }

    /// Waits for the condition to be notified
    #[inline]
    pub async fn wait<T>(&self, guard: MutexGuard<'_, T>) {        
        self.waiters.fetch_add(1, Ordering::AcqRel);
        drop(guard);
        let _ = self.wait_rx.recv().await;        
    }

    #[inline]
    pub fn has_waiting_list(&self) -> bool {
        self.waiters.load(Ordering::Acquire) > 0
    }

    /// Notify one task on the waiting list. The waiting list is 
    /// managed as a FIFO queue.
    #[inline]
    pub async fn notify<T>(&self, guard: MutexGuard<'_, T>) {
        if self.has_waiting_list() {
            self.waiters.fetch_sub(1, Ordering::AcqRel);
            drop(guard);
            self.wait_tx.send(true).await;
        }
    }

    pub async fn notify_all<T>(&self, guard: MutexGuard<'_, T>) {
        let w = self.waiters.swap(0, Ordering::AcqRel);
        drop(guard);
        for _ in 0..w {
            self.wait_tx.send(true).await;
        }
    }
}