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
use zenoh_protocol::core::ZInt;
use zenoh_result::{bail, ZResult};

/// Sequence Number
///
/// Zenoh sequence numbers have a negotiable resolution. Each session can
/// ideally negotiate its resolution and use it across all conduits.
///
/// The [`SeqNum`][SeqNum] encapsulates the sequence numbers along with a
/// the comparison operators that check whether two sequence numbers are
/// less, equeal or greater of each other.
///
#[derive(Clone, Copy, Debug)]
pub(crate) struct SeqNum {
    value: ZInt,
    semi_int: ZInt,
    resolution: ZInt,
}

impl SeqNum {
    /// Create a new sequence number with a given resolution.
    ///
    /// # Arguments
    /// * `value` - The sequence number.
    ///
    /// * `resolution` - The resolution (modulo) to be used for the sequence number.
    ///                  As a consequence of wire zenoh's representation of sequence numbers it is
    ///                  recommended that the resolution is a power of 2 with exponent multiple of 7.
    ///                  Suggested values are:
    ///                  - 256 (i.e., 2^7)
    ///                  - 16_386 (i.e., 2^14)
    ///                  - 2_097_152 (i.e., 2^21)
    ///
    /// This funtion will panic if `value` is out of bound w.r.t. `resolution`. That is if
    /// `value` is greater or equal than `resolution`.
    ///
    pub(crate) fn make(value: ZInt, resolution: ZInt) -> ZResult<SeqNum> {
        let mut sn = SeqNum {
            value: 0,
            semi_int: resolution >> 1,
            resolution,
        };
        sn.set(value)?;
        Ok(sn)
    }

    #[inline(always)]
    pub(crate) fn get(&self) -> ZInt {
        self.value
    }

    #[inline(always)]
    pub(crate) fn resolution(&self) -> ZInt {
        self.resolution
    }

    #[inline(always)]
    pub(crate) fn set(&mut self, value: ZInt) -> ZResult<()> {
        if value >= self.resolution {
            bail!("The sequence number value must be smaller than the resolution");
        }

        self.value = value;
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn increment(&mut self) {
        self.value = (self.value + 1) % self.resolution;
    }

    /// Checks to see if two sequence number are in a precedence relationship,
    /// while taking into account roll backs.
    ///
    /// Two case are considered:
    ///
    /// ## Case 1: sna < snb
    ///
    /// In this case *sna* precedes *snb* iff (snb - sna) <= semi_int where
    /// semi_int is defined as half the sequence number resolution.
    /// In other terms, sna precedes snb iff there are less than half
    /// the length for the interval that separates them.
    ///
    /// ## Case 2: sna > snb
    ///
    /// In this case *sna* precedes *snb* iff (sna - snb) > semi_int.
    ///
    /// # Arguments
    ///
    /// * `value` -  The sequence number which should be checked for precedence relation.
    pub(crate) fn precedes(&self, value: ZInt) -> ZResult<bool> {
        if value >= self.resolution {
            bail!("The sequence number value must be smaller than the resolution");
        }

        let res = if value > self.value {
            value - self.value <= self.semi_int
        } else {
            self.value - value > self.semi_int
        };

        Ok(res)
    }

    /// Computes the modulo gap between two sequence numbers.
    ///
    /// Two case are considered:
    ///
    /// ## Case 1: sna < snb
    ///
    /// In this case the gap is computed as *snb* - *sna*.
    ///
    /// ## Case 2: sna > snb
    ///
    /// In this case the gap is computed as *resolution* - (*sna* - *snb*).
    ///
    /// # Arguments
    ///
    /// * `value` -  The sequence number which should be checked for gap computation.
    #[cfg(test)] // @TODO: remove once reliability is implemented
    pub(crate) fn gap(&self, value: ZInt) -> ZResult<ZInt> {
        if value >= self.resolution {
            bail!("The sequence number value must be smaller than the resolution")
        }

        let gap = if value >= self.value {
            value - self.value
        } else {
            self.resolution - (self.value - value)
        };

        Ok(gap)
    }
}

/// Sequence Number Generator
///
/// The [`SeqNumGenerator`][SeqNumGenerator] encapsulates the generation of sequence numbers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SeqNumGenerator(SeqNum);

impl SeqNumGenerator {
    /// Create a new sequence number generator with a given resolution.
    ///
    /// # Arguments
    /// * `initial_sn` - The initial sequence number. It is a good practice to initialize the
    ///           sequence number generator with a random number
    ///
    /// * `sn_resolution` - The resolution, in bits, to be used for the sequence number generator.
    ///                  As a consequence of wire zenoh's representation of sequence numbers
    ///                  this should be a multiple of 7.
    ///
    /// This funtion will panic if `value` is out of bound w.r.t. `resolution`. That is if
    /// `value` is greater or equal than `resolution`.
    ///
    pub(crate) fn make(initial_sn: ZInt, sn_resolution: ZInt) -> ZResult<SeqNumGenerator> {
        let sn = SeqNum::make(initial_sn, sn_resolution)?;
        Ok(SeqNumGenerator(sn))
    }

    #[inline(always)]
    pub(crate) fn now(&mut self) -> ZInt {
        self.0.get()
    }

    /// Generates the next sequence number
    #[inline(always)]
    pub(crate) fn get(&mut self) -> ZInt {
        let now = self.now();
        self.0.increment();
        now
    }

    #[inline(always)]
    pub(crate) fn set(&mut self, sn: ZInt) -> ZResult<()> {
        self.0.set(sn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sn_set() {
        let mut sn0a = SeqNum::make(0, 14).unwrap();
        assert_eq!(sn0a.get(), 0);
        assert_eq!(sn0a.resolution, 14);

        let res = sn0a.set(13);
        assert!(res.is_ok());
        assert_eq!(sn0a.get(), 13);

        let res = sn0a.set(14);
        assert!(res.is_err());
        assert_eq!(sn0a.get(), 13);

        sn0a.increment();
        assert_eq!(sn0a.get(), 0);

        sn0a.increment();
        assert_eq!(sn0a.get(), 1);
    }

    #[test]
    fn sn_gap() {
        let mut sn0a = SeqNum::make(0, 14).unwrap();
        let sn1a: ZInt = 0;
        let res = sn0a.gap(sn1a);
        assert_eq!(res.unwrap(), 0);

        let sn1a: ZInt = 1;
        let res = sn0a.gap(sn1a);
        assert_eq!(res.unwrap(), 1);

        let sn1a: ZInt = 13;
        let res = sn0a.gap(sn1a);
        assert_eq!(res.unwrap(), 13);

        let sn1a: ZInt = 14;
        let res = sn0a.gap(sn1a);
        assert!(res.is_err());

        let res = sn0a.set(13);
        assert!(res.is_ok());

        let sn1a: ZInt = 13;
        let res = sn0a.gap(sn1a);
        assert_eq!(res.unwrap(), 0);

        let sn1a: ZInt = 0;
        let res = sn0a.gap(sn1a);
        assert_eq!(res.unwrap(), 1);
    }

    #[test]
    fn sn_precedence() {
        let mut sn0a = SeqNum::make(0, 14).unwrap();
        let sn1a: ZInt = 1;
        let res = sn0a.precedes(sn1a);
        assert!(res.unwrap());

        let sn1a: ZInt = 0;
        let res = sn0a.precedes(sn1a);
        assert!(!res.unwrap());

        let sn1a: ZInt = 6;
        let res = sn0a.precedes(sn1a);
        assert!(res.unwrap());

        let sn1a: ZInt = 7;
        let res = sn0a.precedes(sn1a);
        assert!(res.unwrap());

        let res = sn0a.set(13);
        assert!(res.is_ok());

        let sn1a: ZInt = 6;
        let res = sn0a.precedes(sn1a);
        assert!(!res.unwrap());

        let sn1a: ZInt = 1;
        let res = sn0a.precedes(sn1a);
        assert!(res.unwrap());

        let sn1a: ZInt = 5;
        let res = sn0a.precedes(sn1a);
        assert!(res.unwrap());
    }

    #[test]
    fn sn_generation() {
        let mut sn0 = SeqNumGenerator::make(13, 14).unwrap();
        let mut sn1 = SeqNumGenerator::make(5, 14).unwrap();

        assert_eq!(sn0.get(), 13);
        assert_eq!(sn1.get(), 5);

        assert_eq!(sn0.get(), 0);
        assert_eq!(sn1.get(), 6);
    }
}
