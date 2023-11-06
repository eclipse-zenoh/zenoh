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
use std::collections::VecDeque;

pub struct StackBuffer<T> {
    buffer: VecDeque<T>,
}

impl<T> StackBuffer<T> {
    #[must_use]
    pub fn new(capacity: usize) -> StackBuffer<T> {
        let buffer = VecDeque::<T>::with_capacity(capacity);
        StackBuffer { buffer }
    }

    #[inline]
    pub fn push(&mut self, elem: T) -> Option<T> {
        if self.len() < self.capacity() {
            self.buffer.push_front(elem);
            None
        } else {
            Some(elem)
        }
    }

    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        self.buffer.pop_front()
    }

    #[allow(dead_code)]
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    #[inline]
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    #[inline]
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }
}
