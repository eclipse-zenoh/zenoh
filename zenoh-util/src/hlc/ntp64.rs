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
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::ops::{Add, AddAssign, Sub};
use humantime::format_rfc3339_nanos;

// maximal number of seconds that can be represented in the 32-bits part
const MAX_NB_SEC: u64 = (1u64 << 32) -1;
// number of NTP fraction per second (2^32)
const FRAC_PER_SEC: u64 = 1u64 << 32;
// Bit-mask for the fraction of a second part within an NTP timestamp
const FRAC_MASK: u64 = 0xFFFF_FFFFu64;

// number of nanoseconds in 1 second
const NANO_PER_SEC: u64 = 1_000_000_000;


// A timestamp using the NTP 64-bits format (https://tools.ietf.org/html/rfc5905#section-6)
// Note that this timestamp in actually similar to a std::time::Duration, as it doesn't
// define an EPOCH. Only the as_system_time() and Display::fmt() operation assume that
// a UNIX_EPOCH (1st Jan 1970) to display the timpestamp in RFC-3339 format.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct NTP64( pub(crate) u64);

impl NTP64 {

    #[inline]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    #[inline]
    pub fn as_secs(&self) -> u32 {
        (self.0 >> 32) as u32
    }

    #[inline]
    pub fn subsec_nanos(&self) -> u32 {
        let frac = self.0 & FRAC_MASK;
        ((frac * NANO_PER_SEC) / FRAC_PER_SEC) as u32
    }

    #[inline]
    pub fn as_duration(&self) -> Duration {
        Duration::new(self.as_secs().into(), self.subsec_nanos())
    }

    #[inline]
    pub fn as_system_time(&self) -> SystemTime {
        UNIX_EPOCH + self.as_duration()
    }
}

impl Add for NTP64 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self (self.0 + other.0)
    }
}

impl<'a> Add<NTP64> for &'a NTP64 {
    type Output = <NTP64 as Add<NTP64>>::Output;

    #[inline]
    fn add(self, other: NTP64) -> <NTP64 as Add<NTP64>>::Output {
        Add::add(*self, other)
    }
}

impl Add<&NTP64> for NTP64 {
    type Output = <NTP64 as Add<NTP64>>::Output;

    #[inline]
    fn add(self, other: &NTP64) -> <NTP64 as Add<NTP64>>::Output {
        Add::add(self, *other)
    }
}

impl Add<&NTP64> for &NTP64 {
    type Output = <NTP64 as Add<NTP64>>::Output;

    #[inline]
    fn add(self, other: &NTP64) -> <NTP64 as Add<NTP64>>::Output {
        Add::add(*self, *other)
    }
}

impl Add<u64> for NTP64 {
    type Output = Self;

    #[inline]
    fn add(self, other: u64) -> Self {
        Self (self.0 + other)
    }
}

impl AddAssign<u64> for NTP64 {
    #[inline]
    fn add_assign(&mut self, other: u64) {
        *self = Self (self.0 + other);
    }
}

impl Sub for NTP64 {
    type Output = Self;

    #[inline]
    fn sub(self, other: Self) -> Self {
        Self (self.0 - other.0)
    }
}

impl fmt::Display for NTP64 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format_rfc3339_nanos(self.as_system_time()))
    }
}

impl fmt::Debug for NTP64 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:x}", self.0)
    }
}

impl From<Duration> for NTP64 {
    fn from(duration: Duration) -> NTP64 {
        let secs = duration.as_secs();
        assert!(secs <= MAX_NB_SEC);
        let nanos: u64 = duration.subsec_nanos().into();
        NTP64((secs << 32) + ((nanos * FRAC_PER_SEC) / NANO_PER_SEC) +1)
    }
}
