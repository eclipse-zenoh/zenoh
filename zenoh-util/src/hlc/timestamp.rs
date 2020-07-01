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
use super::ntp64::NTP64;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    time: NTP64,
    id: Vec<u8>
}

impl Timestamp {

    pub fn new(time: NTP64, id: Vec<u8>) -> Timestamp {
        Timestamp { time, id }
    }

    pub fn get_time(&self) -> &NTP64 {
        &self.time
    }

    pub fn get_id(&self) -> &[u8] {
        &self.id[..]
    }

}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", self.time, hex::encode_upper(&self.id))
    }
}

impl fmt::Debug for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}/{}", self.time, hex::encode_upper(&self.id))
    }
}



#[cfg(test)]
mod tests {
    use crate::hlc::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn test_timestamp() {
        let id1: Vec<u8> = vec![
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 
            0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10];
        let id2: Vec<u8> = vec![
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 
            0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x11];
    
        let ts1_epoch = Timestamp::new(Default::default(), id1.clone());
        assert_eq!(ts1_epoch.get_time().as_system_time(), UNIX_EPOCH);
        assert_eq!(ts1_epoch.get_id(), &id1[..]);


        let ts2_epoch = Timestamp::new(Default::default(), id2.clone());
        assert_eq!(ts2_epoch.get_time().as_system_time(), UNIX_EPOCH);
        assert_eq!(ts2_epoch.get_id(), &id2[..]);

        // Test that 2 Timestamps with same time but different ids are different and ordered
        assert_ne!(ts1_epoch, ts2_epoch);
        assert!(ts1_epoch < ts2_epoch);

        let now = system_time_clock();
        let ts1_now = Timestamp::new(NTP64::from(now), id1);
        let ts2_now = Timestamp::new(NTP64::from(now), id2);
        assert_ne!(ts1_now, ts2_now);
        assert!(ts1_now < ts2_now);
        assert!(ts1_epoch < ts1_now);
        assert!(ts2_epoch < ts2_now);
    }
}