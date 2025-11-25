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
use zenoh_protocol::{core::Bits, transport::TransportSn};
use zenoh_result::{bail, ZResult};

const RES_U8: TransportSn = (u8::MAX >> 1) as TransportSn; // 1 byte max when encoded
const RES_U16: TransportSn = (u16::MAX >> 2) as TransportSn; // 2 bytes max when encoded
const RES_U32: TransportSn = (u32::MAX >> 4) as TransportSn; // 4 bytes max when encoded
const RES_U64: TransportSn = (u64::MAX >> 1) as TransportSn; // 9 bytes max when encoded

pub(crate) fn get_mask(resolution: Bits) -> TransportSn {
    match resolution {
        Bits::U8 => RES_U8,
        Bits::U16 => RES_U16,
        Bits::U32 => RES_U32,
        Bits::U64 => RES_U64,
    }
}

/// Sequence Number
///
/// Zenoh sequence numbers have a negotiable resolution. Each session can
/// ideally negotiate its resolution and use it across all priorities.
///
/// The [`SeqNum`][SeqNum] encapsulates the sequence numbers along with a
/// the comparison operators that check whether two sequence numbers are
/// less, equal or greater of each other.
///
#[derive(Clone, Copy, Debug)]
pub(crate) struct SeqNum {
    value: TransportSn,
    mask: TransportSn,
}

impl SeqNum {
    /// Create a new sequence number with a given resolution.
    ///
    /// # Arguments
    /// * `value` - The sequence number.
    ///
    /// * `resolution` - The resolution (modulo) to be used for the sequence number.
    ///   As a consequence of wire zenoh's representation of sequence numbers it is
    ///   recommended that the resolution is a power of 2 with exponent multiple of 7.
    ///   Suggested values are:
    ///   - 256 (i.e., 2^7)
    ///   - 16_386 (i.e., 2^14)
    ///   - 2_097_152 (i.e., 2^21)
    ///
    /// # Errors
    ///
    /// This function will return an error if `value` is out of bound w.r.t. `resolution`. That is if
    /// `value` is greater or equal than `resolution`.
    ///
    pub(crate) fn make(value: TransportSn, resolution: Bits) -> ZResult<SeqNum> {
        let mask = get_mask(resolution);
        let mut sn = SeqNum { value: 0, mask };
        sn.set(value)?;
        Ok(sn)
    }

    pub(crate) fn get(&self) -> TransportSn {
        self.value
    }

    #[inline(always)]
    pub(crate) fn next(&self) -> TransportSn {
        self.value.wrapping_add(1) & self.mask
    }

    #[inline(always)]
    pub(crate) fn resolution(&self) -> TransportSn {
        self.mask
    }

    pub(crate) fn set(&mut self, value: TransportSn) -> ZResult<()> {
        if (value & !self.mask) != 0 {
            bail!("The sequence number value must be smaller than the resolution");
        }

        self.value = value;
        Ok(())
    }

    pub(crate) fn increment(&mut self) {
        self.value = self.next();
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
    pub(crate) fn precedes(&self, value: TransportSn) -> ZResult<bool> {
        if (value & !self.mask) != 0 {
            bail!("The sequence number value must be smaller than the resolution");
        }
        let gap = value.wrapping_sub(self.value) & self.mask;
        Ok((gap != 0) && ((gap & !(self.mask >> 1)) == 0))
    }

    /// Checks to see if two sequence number are in a precedence relationship,
    /// while taking into account roll backs AND do update the sn value if check succeed.
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
    pub(crate) fn roll(&mut self, value: TransportSn) -> ZResult<bool> {
        if (value & !self.mask) != 0 {
            bail!("The sequence number value must be smaller than the resolution");
        }
        let gap = value.wrapping_sub(self.value) & self.mask;
        if (gap != 0) && ((gap & !(self.mask >> 1)) == 0) {
            self.value = value;
            return Ok(true);
        }
        Ok(false)
    }

    /// Computes the modulo gap between two sequence numbers.
    #[cfg(test)] // @TODO: remove #[cfg(test)] once reliability is implemented
    pub(crate) fn gap(&self, value: TransportSn) -> ZResult<TransportSn> {
        if (value & !self.mask) != 0 {
            bail!("The sequence number value must be smaller than the resolution");
        }
        Ok(value.wrapping_sub(self.value) & self.mask)
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
    ///   sequence number generator with a random number
    ///
    /// * `sn_resolution` - The resolution, in bits, to be used for the sequence number generator.
    ///   As a consequence of wire zenoh's representation of sequence numbers
    ///   this should be a multiple of 7.
    ///
    /// # Errors
    ///
    /// This function will return an error if `initial_sn` is out of bound w.r.t. `resolution`. That is if
    ///   `initial_sn` is greater or equal than `resolution`.
    ///
    pub(crate) fn make(initial_sn: TransportSn, resolution: Bits) -> ZResult<SeqNumGenerator> {
        let sn = SeqNum::make(initial_sn, resolution)?;
        Ok(SeqNumGenerator(sn))
    }

    pub(crate) fn now(&mut self) -> TransportSn {
        self.0.get()
    }

    /// Generates the next sequence number
    pub(crate) fn get(&mut self) -> TransportSn {
        let now = self.now();
        self.0.increment();
        now
    }

    pub(crate) fn set(&mut self, sn: TransportSn) -> ZResult<()> {
        self.0.set(sn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sn_set() {
        let mask = (u8::MAX >> 1) as TransportSn;

        let mut sn0a = SeqNum::make(0, Bits::U8).unwrap();
        assert_eq!(sn0a.get(), 0);
        assert_eq!(sn0a.mask, mask);

        sn0a.set(mask).unwrap();
        assert_eq!(sn0a.get(), mask);

        assert!(sn0a.set(mask + 1).is_err());
        assert_eq!(sn0a.get(), mask);

        sn0a.increment();
        assert_eq!(sn0a.get(), 0);

        sn0a.increment();
        assert_eq!(sn0a.get(), 1);
    }

    #[test]
    fn sn_gap() {
        let mask = (u8::MAX >> 1) as TransportSn;
        let mut sn0a = SeqNum::make(0, Bits::U8).unwrap();

        assert_eq!(sn0a.gap(0).unwrap(), 0);
        assert_eq!(sn0a.gap(1).unwrap(), 1);
        assert_eq!(sn0a.gap(mask).unwrap(), mask);
        assert!(sn0a.gap(mask + 1).is_err());

        sn0a.set(mask).unwrap();
        assert_eq!(sn0a.gap(mask).unwrap(), 0);
        assert_eq!(sn0a.gap(0).unwrap(), 1);
    }

    #[test]
    fn sn_precedence() {
        let mask = (u8::MAX >> 1) as TransportSn;

        let sn0a = SeqNum::make(0, Bits::U8).unwrap();
        assert!(sn0a.precedes(1).unwrap());
        assert!(!sn0a.precedes(0).unwrap());
        assert!(!sn0a.precedes(mask).unwrap());
        assert!(sn0a.precedes(6).unwrap());
        assert!(sn0a.precedes((mask / 2) - 1).unwrap());
        assert!(sn0a.precedes(mask / 2).unwrap());
        assert!(!sn0a.precedes((mask / 2) + 1).unwrap());
    }

    #[test]
    fn sn_generation() {
        let mask = (u8::MAX >> 1) as TransportSn;
        let mut sn0 = SeqNumGenerator::make(mask, Bits::U8).unwrap();
        let mut sn1 = SeqNumGenerator::make(5, Bits::U8).unwrap();

        assert_eq!(sn0.get(), mask);
        assert_eq!(sn1.get(), 5);

        assert_eq!(sn0.get(), 0);
        assert_eq!(sn1.get(), 6);
    }
}
