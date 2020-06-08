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
use crate::core::ZInt;

use zenoh_util::zerror;
use zenoh_util::core::{ZResult, ZError, ZErrorKind};

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
pub struct SeqNum {
    value: ZInt,
    semi_int: ZInt,
    resolution: ZInt
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
    pub fn make(value: ZInt, resolution: ZInt) -> ZResult<SeqNum> {
        let mut sn = SeqNum { 
            value: 0, 
            semi_int: resolution >> 1, 
            resolution 
        };
        sn.set(value)?;
        Ok(sn)
    }

    #[inline]
    pub fn get(&self) -> ZInt {
        self.value
    }

    #[inline]
    pub fn set(&mut self, value: ZInt) -> ZResult<()> {
        if value < self.resolution {
            self.value = value;
            Ok(())
        }
        else {
            zerror!(ZErrorKind::InvalidResolution {
                descr: "The sequence number value must be smaller than the resolution".to_string()
            })
        }
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
    pub fn precedes(&self, value: ZInt) -> bool {
        if value > self.value {
            value - self.value <= self.semi_int
        } else {
            self.value - value > self.semi_int 
        } 
    }
}


/// Sequence Number Generator
/// 
/// The [`SeqNumGenerator`][SeqNumGenerator] encapsulates the generation of sequence numbers
/// along with a [`precede`][SeqNumGenerator::precede] predicate that checks whether two
/// sequence numbers are in the precede relationship.
#[derive(Clone, Copy, Debug)]
pub struct SeqNumGenerator(SeqNum);

impl SeqNumGenerator {
    /// Create a new sequence number generator with a given resolution.
    /// 
    /// # Arguments
    /// * `sn0` - The initial sequence number. It is a good practice to initialize the
    ///           sequence number generator with a random number
    ///
    /// * `resolution` - The resolution, in bits, to be used for the sequence number generator. 
    ///                  As a consequence of wire zenoh's representation of sequence numbers
    ///                  this should be a multiple of 7.
    /// 
    pub fn make(sn0: ZInt, resolution: ZInt) -> ZResult<SeqNumGenerator> {
        let sn = SeqNum::make(sn0, resolution)?;
        Ok(SeqNumGenerator(sn))
    }

    /// Generates the next sequence number
    pub fn get(&mut self) -> ZInt {
        let now = self.0.get();
        self.0.set((now + 1) % self.0.resolution).unwrap();
        now
    }
}