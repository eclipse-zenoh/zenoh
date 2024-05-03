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
use std::{convert::TryInto, fmt};

use zenoh_result::{ZError, ZErrorKind, ZResult};
use zenoh_util::zerror;

use super::{common::seq_num::SeqNum, core::u64};

pub(super) struct ReliabilityQueue<T> {
    sn: SeqNum,
    index: usize,
    len: usize,
    inner: Vec<Option<T>>,
}

impl<T> ReliabilityQueue<T> {
    pub(super) fn new(capacity: usize, initial_sn: u64, sn_resolution: u64) -> ReliabilityQueue<T> {
        let mut inner = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            inner.push(None);
        }

        ReliabilityQueue {
            sn: SeqNum::new(initial_sn, sn_resolution),
            index: 0,
            len: 0,
            inner,
        }
    }

    #[inline]
    pub(super) fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub(super) fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    #[inline]
    pub(super) fn get_base(&self) -> u64 {
        self.sn.get()
    }

    pub(super) fn set_base(&mut self, sn: u64) -> ZResult<()> {
        let gap: usize = match self.sn.gap(sn) {
            Ok(gap) => match gap.try_into() {
                Ok(gap) => gap,
                Err(_) => usize::MAX,
            },
            Err(e) => return Err(e),
        };

        self.sn.set(sn)?;

        if gap >= self.capacity() {
            // If the gap is larger than the capacity, reset the queue
            for i in 0..self.capacity() {
                self.inner[i] = None;
            }
            self.index = 0;
            self.len = 0;
        } else {
            // Reset only a portion of the queue
            for _ in 0..gap {
                if self.inner[self.index].is_some() {
                    self.len -= 1;
                    self.inner[self.index] = None;
                }
                self.index = (self.index + 1) % self.capacity();
            }
        }

        Ok(())
    }

    pub(super) fn insert(&mut self, t: T, sn: u64) -> ZResult<()> {
        let gap: usize = match self.sn.gap(sn) {
            Ok(gap) => match gap.try_into() {
                Ok(gap) => gap,
                Err(e) => {
                    return zerror!(ZErrorKind::InvalidResolution {
                        descr: e.to_string()
                    })
                }
            },
            Err(e) => return Err(e),
        };

        if gap >= self.capacity() {
            let e = format!(
                "Sequence number is out of sequence number window: {}.\
                             Base: {}. Capacity: {}",
                sn,
                self.sn.get(),
                self.capacity()
            );
            tracing::trace!("{}", e);
            return zerror!(ZErrorKind::Other { descr: e });
        }

        self.len += 1;
        let index = (self.index + gap) % self.capacity();
        self.inner[index] = Some(t);

        Ok(())
    }

    pub(super) fn remove(&mut self, sn: u64) -> ZResult<T> {
        let gap: usize = match self.sn.gap(sn) {
            Ok(gap) => match gap.try_into() {
                Ok(gap) => gap,
                Err(e) => {
                    return zerror!(ZErrorKind::InvalidResolution {
                        descr: e.to_string()
                    })
                }
            },
            Err(e) => return Err(e),
        };

        if gap >= self.capacity() {
            let e = format!(
                "Sequence number is out of sequence number window: {}.\
                             Base: {}. Capacity: {}",
                sn,
                self.sn.get(),
                self.capacity()
            );
            tracing::trace!("{}", e);
            return zerror!(ZErrorKind::Other { descr: e });
        }

        let index = (self.index + gap) % self.capacity();
        let res = self.inner[index].take();

        match res {
            Some(t) => {
                self.len -= 1;
                Ok(t)
            }
            None => zerror!(ZErrorKind::Other {
                descr: "Sequence number not found: {}".to_string()
            }),
        }
    }

    pub(super) fn pull(&mut self) -> Option<T> {
        let t = self.inner[self.index].take();
        if t.is_some() {
            self.len -= 1;
            self.index = (self.index + 1) % self.capacity();
            self.sn.increment();
        }
        t
    }

    /// Returns a bitmask of surely missed messages.
    /// A bit is set to 1 iff the position in the queue is empty and
    /// there is at least one message with a higher sequence number.
    pub(super) fn get_mask(&self) -> u64 {
        let mut mask: u64 = 0;
        let mut count = 0;
        let mut i = 0;
        while count < self.len() {
            let index = (self.index + i) % self.capacity();
            if self.inner[index].is_none() {
                mask |= 1 << i;
            } else {
                count += 1;
            }
            i += 1;
        }
        mask
    }
}

impl<T: Clone> ReliabilityQueue<T> {
    pub(super) fn get(&mut self, sn: u64) -> ZResult<T> {
        let gap: usize = match self.sn.gap(sn) {
            Ok(gap) => match gap.try_into() {
                Ok(gap) => gap,
                Err(e) => {
                    return zerror!(ZErrorKind::InvalidResolution {
                        descr: e.to_string()
                    })
                }
            },
            Err(e) => return Err(e),
        };

        if gap >= self.capacity() {
            let e = format!(
                "Sequence number is out of sequence number window: {}.\
                             Base: {}. Capacity: {}",
                sn,
                self.sn.get(),
                self.capacity()
            );
            tracing::trace!("{}", e);
            return zerror!(ZErrorKind::Other { descr: e });
        }

        let index = (self.index + gap) % self.capacity();
        let res = self.inner[index].clone();

        match res {
            Some(t) => Ok(t),
            None => zerror!(ZErrorKind::Other {
                descr: "Sequence number not found: {}".to_string()
            }),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for ReliabilityQueue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReliabilityQueue")
            .field("base", &self.sn.get())
            .field("inner", &self.inner)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use rand::{thread_rng, Rng};

    use super::*;

    #[test]
    fn reliability_queue_simple() {
        let size = 2;
        let mut queue: ReliabilityQueue<u64> = ReliabilityQueue::new(size, 0, 2);

        let mut sn: u64 = 0;
        // Add the first element
        let res = queue.insert(0, sn);
        assert!(res.is_ok());
        let res = queue.pull();
        assert_eq!(res, Some(0));

        // Add the second element
        sn = sn + 1;
        let res = queue.insert(1, sn);
        assert!(res.is_ok());
        let res = queue.pull();
        assert_eq!(res, Some(1));

        // Verify that the queue is empty
        assert!(queue.is_empty());
    }

    #[test]
    fn reliability_queue_order() {
        let size = 2;
        let mut queue: ReliabilityQueue<u64> = ReliabilityQueue::new(size, 0, 3);

        let sn: u64 = 0;

        // Add the second element
        let res = queue.insert(1, sn + 1);
        assert!(res.is_ok());
        let res = queue.pull();
        assert_eq!(res, None);

        // Add the first element
        let res = queue.insert(0, sn);
        assert!(res.is_ok());
        let res = queue.pull();
        assert_eq!(res, Some(0));
        let res = queue.pull();
        assert_eq!(res, Some(1));
        let res = queue.pull();
        assert_eq!(res, None);

        // Verify that the queue is empty
        assert!(queue.is_empty());
    }

    #[test]
    fn reliability_queue_full() {
        let size = 2;
        let mut queue: ReliabilityQueue<u64> = ReliabilityQueue::new(size, 0, 3);

        let mut sn: u64 = 0;

        // Fill the queue
        let res = queue.insert(0, sn);
        assert!(res.is_ok());
        sn += 1;
        let res = queue.insert(1, sn);
        assert!(res.is_ok());
        sn += 1;
        let res = queue.insert(2, sn);
        assert!(res.is_err());

        // Drain the queue
        let res = queue.pull();
        assert_eq!(res, Some(0));
        let res = queue.pull();
        assert_eq!(res, Some(1));

        // Verify that the queue is empty
        assert!(queue.is_empty());
    }

    #[test]
    fn reliability_queue_out_of_sync() {
        let size = 2;
        let mut queue: ReliabilityQueue<u64> = ReliabilityQueue::new(size, 0, 2);

        let sn: u64 = 3;

        let res = queue.insert(sn, sn);
        assert!(res.is_err());

        // Verify that the queue is empty
        assert!(queue.is_empty());
    }

    #[test]
    fn reliability_queue_overflow() {
        // Test the overflow case
        let size = 4;
        let mut queue: ReliabilityQueue<u64> = ReliabilityQueue::new(size, 0, 4);

        let min: u64 = 0;
        let max: u64 = 3;

        let res = queue.set_base(max - 1);
        assert!(res.is_ok());
        let res = queue.insert(0, max - 1);
        assert!(res.is_ok());
        let res = queue.insert(1, max);
        assert!(res.is_ok());
        let res = queue.insert(2, min);
        assert!(res.is_ok());
        let res = queue.insert(3, min + 1);
        assert!(res.is_ok());
        let res = queue.pull();
        assert_eq!(res, Some(0));
        let res = queue.pull();
        assert_eq!(res, Some(1));
        let res = queue.pull();
        assert_eq!(res, Some(2));
        let res = queue.pull();
        assert_eq!(res, Some(3));
        let res = queue.pull();
        assert_eq!(res, None);

        // Verify that the queue is empty
        assert!(queue.is_empty());
    }

    #[test]
    fn reliability_queue_mask() {
        // Test the deterministic insertion of elements and mask
        let size = 8;
        let mut queue: ReliabilityQueue<u64> = ReliabilityQueue::new(size, 0, 8);

        let mut sn: u64 = 0;
        while sn < size as u64 {
            let res = queue.insert(sn, sn);
            assert!(res.is_ok());
            sn = sn + 2;
        }

        // Verify that the mask is correct
        let mask: u64 = 0b00101010;
        assert_eq!(queue.get_mask(), mask);

        // Insert the missing elements
        let mut sn: u64 = 1;
        while sn < size as u64 {
            let res = queue.insert(sn, sn);
            assert!(res.is_ok());
            sn = sn + 2;
        }

        // Verify that the mask is correct
        let mask = 0b0;
        assert_eq!(queue.get_mask(), mask);

        // Drain the queue
        while let Some(_) = queue.pull() {}
        // Verify that the queue is empty
        assert!(queue.is_empty());
    }

    #[test]
    fn reliability_queue_random_mask() {
        // Test the random insertion of elements and the mask
        let size = 64;
        let mut queue: ReliabilityQueue<u64> = ReliabilityQueue::new(size, 0, 64);

        let mut sequence = Vec::<u64>::new();
        for i in 0..size as u64 {
            sequence.push(i);
        }

        let head = 0;
        let mut tail = 0;
        let mut mask: u64 = 0;
        let mut rng = thread_rng();
        while sequence.len() > 0 {
            // Get random sequence number
            let index = rng.gen_range(0..sequence.len());
            let sn = sequence.remove(index);
            // Update the tail
            if sn > tail {
                tail = sn;
            }
            // Push the element on the queue
            let res = queue.insert(sn, sn);
            assert!(res.is_ok());
            // Locally compute the mask
            mask = mask | (1 << sn);
            let shift: u32 = tail.wrapping_sub(head) as u32;
            let window = !u64::max_value().wrapping_shl(shift);
            // Verify that the mask is correct
            assert_eq!(queue.get_mask(), !mask & window);
        }

        // Verify that we have filled the queue
        assert!(queue.is_full());
        // Verify that no elements are marked for retransmission
        assert_eq!(queue.get_mask(), !u64::max_value());

        // Drain the queue
        while let Some(_) = queue.pull() {}
        // Verify that the queue is empty
        assert!(queue.is_empty());

        // Verify that the mask is correct
        let mask = 0b0;
        assert_eq!(queue.get_mask(), mask);
    }

    #[test]
    fn reliability_queue_rebase() {
        let size = 8;
        let mut queue: ReliabilityQueue<u64> = ReliabilityQueue::new(size, 0, 32);

        // Fill the queue
        for i in 0..size as u64 {
            // Push the element on the queue
            let res = queue.insert(i, i);
            assert!(res.is_ok());
        }

        // Verify that the queue is full
        assert!(queue.is_full());

        // Verify that the base is correct
        assert_eq!(queue.get_base(), 0);

        // Rebase the queue
        let res = queue.set_base(4);
        assert!(res.is_ok());

        // Verify that the base is correct
        assert_eq!(queue.get_base(), 4);
        // Verify that the length of the queue is correct
        assert_eq!(queue.len(), 4);

        // Drain the queue
        let res = queue.pull();
        assert_eq!(res, Some(4));
        assert_eq!(queue.get_base(), 5);

        let res = queue.pull();
        assert_eq!(res, Some(5));
        assert_eq!(queue.get_base(), 6);

        let res = queue.pull();
        assert_eq!(res, Some(6));
        assert_eq!(queue.get_base(), 7);

        let res = queue.pull();
        assert_eq!(res, Some(7));
        assert_eq!(queue.get_base(), 8);

        let res = queue.pull();
        assert_eq!(res, None);
        assert_eq!(queue.get_base(), 8);

        // Verify that the length of the queue is correct
        assert!(queue.is_empty());

        // Rebase the queue
        let res = queue.set_base(0);
        assert!(res.is_ok());
        // Verify that the base is correct
        assert_eq!(queue.get_base(), 0);

        // Fill the queue
        for i in 0..size as u64 {
            // Push the element on the queue is correct
            let res = queue.insert(i, i);
            assert!(res.is_ok());
        }

        // Verify that the length of the queue is correct
        assert!(queue.is_full());

        // Rebase beyond the current boundaries triggering a reset
        let base = 2 * size as u64;
        let res = queue.set_base(base);
        assert!(res.is_ok());
        assert_eq!(queue.get_base(), base);

        // Verify that the length of the queue is correct
        assert!(queue.is_empty());

        // Verify that the mask is correct
        let mask = 0b0;
        assert_eq!(queue.get_mask(), mask);
    }

    #[test]
    fn reliability_queue_remove() {
        let size = 8;
        let mut queue: ReliabilityQueue<u64> = ReliabilityQueue::new(size, 0, 8);

        // Fill the queue
        for i in 0..size as u64 {
            // Push the element on the queue
            let res = queue.insert(i, i);
            assert!(res.is_ok());
        }

        // Verify that the length of the queue is correct
        assert!(queue.is_full());

        // Drain the queue
        let res = queue.remove(7);
        assert_eq!(res.unwrap(), 7);
        assert_eq!(queue.len(), 7);

        let res = queue.remove(5);
        assert_eq!(res.unwrap(), 5);
        assert_eq!(queue.len(), 6);

        let res = queue.remove(3);
        assert_eq!(res.unwrap(), 3);
        assert_eq!(queue.len(), 5);

        let res = queue.remove(1);
        assert_eq!(res.unwrap(), 1);
        assert_eq!(queue.len(), 4);

        let res = queue.remove(0);
        assert_eq!(res.unwrap(), 0);
        assert_eq!(queue.len(), 3);

        let res = queue.remove(2);
        assert_eq!(res.unwrap(), 2);
        assert_eq!(queue.len(), 2);

        let res = queue.remove(4);
        assert_eq!(res.unwrap(), 4);
        assert_eq!(queue.len(), 1);

        let res = queue.remove(6);
        assert_eq!(res.unwrap(), 6);
        assert!(queue.is_empty());

        // Check that everything is None
        for i in 0..size as u64 {
            // Remove the element from the queue
            let res = queue.remove(i);
            assert!(res.is_err());
        }

        // Check that the base is 0
        assert_eq!(queue.get_base(), 0);
    }
}
