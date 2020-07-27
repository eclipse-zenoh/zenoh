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
use async_std::sync::{Mutex, MutexGuard};
use std::sync::atomic::{AtomicIsize, Ordering};

use crate::collections::CircularBuffer;
use crate::sync::Condition;
use crate::zasynclock;

type SpendingClosure<T> = Box<dyn Fn(&T) -> isize + Send + Sync>;

pub struct CreditBuffer<T> {
    capacity: usize,
    credit: isize,
    spending: SpendingClosure<T>,
}

impl<T> CreditBuffer<T> {
    pub fn new(capacity: usize, credit: isize, spending: SpendingClosure<T>) -> CreditBuffer<T> {
        CreditBuffer {
            capacity,
            credit,
            spending,
        }
    }

    pub fn spending_policy<F>(f: F) -> SpendingClosure<T>
    where
        F: Fn(&T) -> isize + Send + Sync + 'static,
    {
        Box::new(f)
    }
}

/// Credit-based queue
///
/// This queue is meant to be used in scenario where a credit-based fair queueing (a.k.a. scheduling) is desired.
/// The [`CreditQueue`][CreditQueue] implementation leverages the [`async-std`](https://docs.rs/async-std) library,
/// making it a good choice for all the applications using the [`async-std`](https://docs.rs/async-std) library.
///
/// Multiple priority queues can be configured with different queue capacity (i.e. number of elements
/// that can be stored), initial credit amount and spending policy.
///
pub struct CreditQueue<T> {
    state: Mutex<Vec<CircularBuffer<T>>>,
    credit: Vec<AtomicIsize>,
    spending: Vec<SpendingClosure<T>>,
    not_full: Condition,
    not_empty: Condition,
}

impl<T> CreditQueue<T> {
    /// Create a new credit-based queue.
    ///
    /// # Arguments
    /// * `queue` - A vector containing the parameters for the queues in the form of tuples: (capacity, credits)      
    ///
    /// * `concurrency_level` - The desired concurrency_level when accessing a single priority queue.
    ///
    pub fn new(mut queues: Vec<CreditBuffer<T>>, concurrency_level: usize) -> CreditQueue<T> {
        let mut state = Vec::with_capacity(queues.len());
        let mut credit = Vec::with_capacity(queues.len());
        let mut spending = Vec::with_capacity(queues.len());
        for buffer in queues.drain(..) {
            state.push(CircularBuffer::new(buffer.capacity));
            credit.push(AtomicIsize::new(buffer.credit));
            spending.push(buffer.spending);
        }

        CreditQueue {
            state: Mutex::new(state),
            credit,
            spending,
            not_full: Condition::new(concurrency_level),
            not_empty: Condition::new(concurrency_level),
        }
    }

    #[inline]
    pub fn get_credit(&self, priority: usize) -> isize {
        self.credit[priority].load(Ordering::Acquire)
    }

    #[inline]
    pub fn spend(&self, priority: usize, amount: isize) {
        self.credit[priority].fetch_sub(amount, Ordering::AcqRel);
    }

    #[inline]
    pub fn recharge(&self, priority: usize, amount: isize) {
        // Recharge the credit for a given priority queue
        self.credit[priority].fetch_add(amount, Ordering::AcqRel);
    }

    #[inline]
    pub async fn recharge_and_wake_up(&self, priority: usize, amount: isize) {
        // Recharge the credit for a given priority queue
        let old = self.credit[priority].fetch_add(amount, Ordering::AcqRel);
        // We had a negative credit, now it is recharged and it might be above zero
        // If the credit is positive, we might be able to pull from the queue
        if old <= 0 && self.get_credit(priority) > 0 {
            let q = zasynclock!(self.state);
            if self.not_empty.has_waiting_list() {
                self.not_empty.notify(q).await;
            }
        }
    }

    pub async fn push(&self, t: T, priority: usize) {
        loop {
            let mut q = zasynclock!(self.state);
            if !q[priority].is_full() {
                q[priority].push(t);
                if self.not_empty.has_waiting_list() {
                    self.not_empty.notify(q).await;
                }
                return;
            }
            self.not_full.wait(q).await;
        }
    }

    pub async fn push_batch(&self, mut v: Vec<T>, priority: usize) {
        while !v.is_empty() {
            loop {
                let mut q = zasynclock!(self.state);
                let tot = (q[priority].capacity() - q[priority].len()).min(v.len());
                // Start draining the elements
                for t in v.drain(0..tot) {
                    q[priority].push(t);
                }
                // If some element has been pushed and there is a waiting list,
                // notify the pullers and return the messages not pushed on the queue
                if tot > 0 {
                    if self.not_empty.has_waiting_list() {
                        self.not_empty.notify(q).await;
                    }
                    break;
                }
                self.not_full.wait(q).await;
            }
        }
    }

    pub async fn pull(&self) -> T {
        loop {
            let mut q = zasynclock!(self.state);
            for priority in 0usize..q.len() {
                if self.get_credit(priority) > 0 {
                    if let Some(e) = q[priority].pull() {
                        self.spend(priority, (self.spending[priority])(&e));
                        if self.not_full.has_waiting_list() {
                            self.not_full.notify(q).await;
                        }
                        return e;
                    }
                }
            }
            self.not_empty.wait(q).await;
        }
    }

    pub async fn drain(&self) -> Drain<'_, T> {
        // Acquire the guard and wait until the queue is not empty
        let guard = loop {
            // Acquire the lock
            let guard = zasynclock!(self.state);
            // Compute the total number of buffers we can drain from
            let can_drain = guard.iter().enumerate().fold(0, |acc, (i, x)| {
                if !x.is_empty() && self.get_credit(i) > 0 {
                    acc + 1
                } else {
                    acc
                }
            });
            // If there are no buffers available, we wait
            if can_drain == 0 {
                self.not_empty.wait(guard).await;
            } else {
                break guard;
            }
        };
        // Return a Drain iterator
        Drain {
            queue: self,
            drained: 0,
            guard,
            priority: 0,
        }
    }

    pub async fn try_drain(&self) -> Drain<'_, T> {
        // Return a Drain iterator
        Drain {
            queue: self,
            drained: 0,
            guard: zasynclock!(self.state),
            priority: 0,
        }
    }
}

pub struct Drain<'a, T> {
    queue: &'a CreditQueue<T>,
    drained: usize,
    guard: MutexGuard<'a, Vec<CircularBuffer<T>>>,
    priority: usize,
}

impl<'a, T> Drain<'a, T> {
    // The drop() on Drain object needs to be manually called since an async
    // destructor is not yet supported in Rust. More information available at:
    // https://internals.rust-lang.org/t/asynchronous-destructors/11127/47
    pub async fn drop(self) {
        if self.drained > 0 && self.queue.not_full.has_waiting_list() {
            self.queue.not_full.notify(self.guard).await;
        } else {
            drop(self.guard);
        }
    }
}

impl<'a, T> Iterator for Drain<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        loop {
            if self.queue.get_credit(self.priority) > 0 {
                if let Some(e) = self.guard[self.priority].pull() {
                    self.drained += 1;
                    self.queue
                        .spend(self.priority, (self.queue.spending[self.priority])(&e));
                    return Some(e);
                }
            }

            self.priority += 1;
            if self.priority == self.guard.len() {
                return None;
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self
            .guard
            .iter()
            .enumerate()
            .fold((0, 0), |(min, max), (i, x)| {
                if !x.is_empty() && self.queue.get_credit(i) > 0 {
                    (min + 1, max + x.len())
                } else {
                    (min, max)
                }
            });
        (min, Some(max))
    }
}
