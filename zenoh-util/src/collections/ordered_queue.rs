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
use std::fmt;


// Structs for OrderedQueue
pub struct OrderedElement<T> {
    element: T,
    sn: usize
}

impl<T> OrderedElement<T> {
    fn new(element: T, sn: usize) -> Self {
        Self {
            element,
            sn
        }
    }

    fn into_inner(self) -> T {
        self.element
    }
}

impl<T: Clone> OrderedElement<T> {
    fn inner_clone(&self) -> T {
        self.element.clone()
    }
}


pub struct OrderedQueue<T> {
    buff: Vec<Option<OrderedElement<T>>>,
    // resolution: ZInt,
    pointer: usize,
    counter: usize,
    first: usize,
    last: usize,
}

impl<T> OrderedQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let mut buff = Vec::with_capacity(capacity);
        buff.resize_with(capacity, || None);
        Self {
            buff,
            pointer: 0,
            counter: 0,
            first: 0,
            last: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    pub fn capacity(&self) -> usize {
        self.buff.capacity()
    }

    pub fn len(&self) -> usize {
        self.counter
    }

    pub fn get_mask(&self) -> usize {
        let mut mask: usize = 0;

        // Check if the queue is empty
        if self.is_empty() {
            return mask
        }

        // Create the bitmask
        let mut iteration = 0;
        let mut index = self.pointer;
        loop {
            match &self.buff[index] {
                Some(element) => if element.sn == self.last {
                    break
                },
                None => mask |= 1 << iteration
            }
            iteration += 1;
            index = (index + 1) % self.capacity();
        }
        
        mask
    }

    pub fn get_base(&self) -> usize {
        self.first
    }

    pub fn set_base(&mut self, base: usize) {
        // Compute the circular gaps
        let gap_base = base.wrapping_sub(self.first);
        let gap_last = self.last.wrapping_sub(self.first);
        
        let count = if gap_base <= gap_last {
            gap_base 
        } else {
            self.capacity()
        };

        // Iterate over the queue and consume the inner elements
        for _ in 0..count {
            if self.buff[self.pointer].take().is_some() {
                // Decrement the counter
                self.counter -= 1;
            }
            // Increment the pointer
            self.pointer = (self.pointer + 1) % self.capacity();
        }

        // Align the first and last sequence numbers
        self.first = base;
        if self.is_empty() {
            self.last = self.first;
        }
    }

    // This operation does not modify the base or the pointer
    // It simply removes an element if it matches the sn 
    pub fn try_remove(&mut self, sn: usize) -> Option<T> {
        if !self.is_empty() {
            let gap = sn.wrapping_sub(self.first);
            let index = (self.pointer + gap) % self.capacity();
            if let Some(element) = &self.buff[index] {
                if element.sn == sn {
                    // The element is the right one, take with unwrap
                    let element = self.buff[index].take().unwrap();
                    // Decrement the counter
                    self.counter -= 1;
                    // Align the last sequence number if the queue is empty
                    if self.is_empty() {
                        self.last = self.first;
                    }
                    return Some(element.into_inner())
                }
            }
        }
        None
    }

    pub fn try_pop(&mut self) -> Option<T> {
        if !self.is_empty() {
            if let Some(element) = self.buff[self.pointer].take() {
                // Update the pointer in the buffer
                self.pointer = (self.pointer + 1) % self.capacity();
                // Decrement the counter
                self.counter -= 1;
                // Increment the next target sequence number
                self.first = self.first.wrapping_add(1);
                // Align the last sequence number if the queue is empty
                if self.is_empty() {
                    self.last = self.first;
                }
                return Some(element.into_inner())
            }
        }
        None
    }

    pub fn try_push(&mut self, element: T, sn: usize) -> Option<T> {
        // Do a modulo substraction
        let gap = sn.wrapping_sub(self.first);
        // Return error if the gap is larger than the capacity
        if gap >= self.capacity() {
            return Some(element)
        }

        // Increment the counter
        self.counter += 1;

        // Update the sequence number
        if sn > self.last {
            self.last = sn;
        }
        // Insert the element in the queue
        let index = (self.pointer + gap) % self.capacity();
        self.buff[index] = Some(OrderedElement::new(element, sn));
        
        None
    }
}

impl<T: Clone> OrderedQueue<T> {
    // This operation does not modify the base or the pointer
    // It simply gets a clone of an element if it matches the sn 
    pub fn try_get(&self, sn: usize) -> Option<T> {
        if !self.is_empty() {
            let gap = sn.wrapping_sub(self.first);
            let index = (self.pointer + gap) % self.capacity();
            if let Some(element) = &self.buff[index] {
                if element.sn == sn {
                    // The element is the right one, take with unwrap
                    return Some(self.buff[index].as_ref().unwrap().inner_clone())
                }
            }
        }
        None
    }
}

impl<T> fmt::Debug for OrderedQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = "[".to_string();
        let mut first = true;
        let mut index = self.pointer;
        for _ in 0..self.buff.capacity() {
            if first {
                first = false;
            } else {
                s.push_str(", ");
            }
            match &self.buff[index] {
                Some(e) => s.push_str(&format!("{}", e.sn)),
                None => s.push_str("None")
            }
            index = (index + 1) % self.capacity();
        }
        s.push_str("]\n");
        write!(f, "{}", s)
    }
}
