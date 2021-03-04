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
use async_std::sync::Mutex;

use crate::collections::CircularBuffer;
use crate::sync::Condition;
use crate::zasynclock;

pub struct PriorityQueue<T> {
    not_full: Condition,
    not_empty: Condition,
    state: Mutex<Vec<CircularBuffer<T>>>,
}

impl<T> PriorityQueue<T> {
    pub fn new(capacity: Vec<usize>) -> PriorityQueue<T> {
        let mut state = Vec::with_capacity(capacity.len());
        for c in capacity.iter() {
            state.push(CircularBuffer::new(*c));
        }

        PriorityQueue {
            not_full: Condition::new(),
            not_empty: Condition::new(),
            state: Mutex::new(state),
        }
    }

    pub async fn push(&self, t: T, priority: usize) {
        loop {
            let mut q = zasynclock!(self.state);
            // Push on the queue if it is not full
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

    pub async fn pull(&self) -> T {
        loop {
            let mut q = zasynclock!(self.state);
            for priority in 0usize..q.len() {
                if let Some(e) = q[priority].pull() {
                    if self.not_full.has_waiting_list() {
                        self.not_full.notify(q).await;
                    }
                    return e;
                }
            }
            self.not_empty.wait(q).await;
        }
    }
}
