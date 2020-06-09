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

use crate::zasynclock;
use crate::collections::CircularBuffer;
use crate::sync::Condition;


pub struct FifoQueue<T> {
    buffer: Mutex<CircularBuffer<T>>,
    not_empty: Condition,
    not_full: Condition
}

impl<T> FifoQueue<T> {
    pub fn new(capacity: usize, concurrency_level: usize) -> FifoQueue<T> {
        FifoQueue { 
            buffer: Mutex::new(CircularBuffer::new(capacity)),
            not_empty: Condition::new(concurrency_level),
            not_full: Condition::new(concurrency_level)            
        }
    }

    pub async fn push(&self, x: T) {
        loop {
            let mut q = zasynclock!(self.buffer);
            if !q.is_full() {
                q.push(x);
                if self.not_empty.has_waiting_list() {
                    self.not_empty.notify(q).await;
                }                    
                return;                                    
            }
            self.not_full.wait(q).await;            
        }            
    }

    pub async fn pull(&self) -> T {
        loop {
            let mut q = zasynclock!(self.buffer);
            if let Some(e) = q.pull() {
                if self.not_full.has_waiting_list() {
                    self.not_full.notify(q).await;
                }                   
                return e;
            }          
            self.not_empty.wait(q).await;
        }
    }

    // pub async fn drain(&self) -> Vec<T> {
    //     let mut q = zasynclock!(self.buffer);
    //     let mut xs = Vec::with_capacity(q.len());        
    //     while let Some(x) = q.pull() {
    //         xs.push(x);
    //     }         
    //     if self.not_full.has_waiting_list() {
    //         self.not_full.notify_all(q).await;
    //       }                   
    //     xs
    // }

    // pub async fn drain_into(&self, xs: &mut Vec<T>){
    //     let mut q = zasynclock!(self.buffer);
    //     while let Some(x) = q.pull() {
    //         xs.push(x);
    //     }                 
    //     if self.not_full.has_waiting_list() {
    //         self.not_full.notify_all(q).await;
    //     }
    // }

    pub async fn drain(&self) -> Drain<'_, T> {
        // Acquire the guard and wait until the queue is not empty
        let guard = loop {
            // Acquire the lock
            let guard = zasynclock!(self.buffer);
            // If there are no messages available, we wait
            if guard.is_empty() {
                self.not_empty.wait(guard).await;
            } else {
                break guard;
            }
        };
        // Return a Drain iterator
        Drain {
            queue: self,
            drained: false,
            guard
        }
    }

    pub async fn try_drain(&self) -> Drain<'_, T> {
        // Return a Drain iterator
        Drain {
            queue: self,
            drained: false,
            guard: zasynclock!(self.buffer)
        }
    }
}

pub struct Drain<'a, T> {
    queue: &'a FifoQueue<T>,
    drained: bool,
    guard: MutexGuard<'a, CircularBuffer<T>>
}

impl<'a, T> Drain<'a, T> {
    // The drop() on Drain object needs to be manually called since an async
    // destructor is not yet supported in Rust. More information available at:
    // https://internals.rust-lang.org/t/asynchronous-destructors/11127/47
    pub async fn drop(self) {
        if self.drained && self.queue.not_full.has_waiting_list() {
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
        if let Some(e) = self.guard.pull() {
            self.drained = true;
            return Some(e)
        }
        None
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.guard.len(), Some(self.guard.len()))
    }
}