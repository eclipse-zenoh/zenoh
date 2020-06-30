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
use std::cmp;
use std::time::{SystemTime, Duration, UNIX_EPOCH};
use log::warn;
use async_std::sync::Mutex;

mod ntp64;
pub use ntp64::*;

mod timestamp;
pub use timestamp::*;

// Counter size (in bits) 
const CSIZE: u8 = 8u8;
// Bit-mask of the counter part within the 64 bits time
const CMASK: u64 = (1u64 << CSIZE) - 1u64;
// Bit-mask of the logical clock part within the 64 bits time
const LMASK: u64 = ! CMASK;

// HLC Delta in milliseconds: maximum accepted drift for an external timestamp.
// I.e.: if an incoming timestamp has a time > now() + delta, then the HLC is not updated.
const DELTA_MS: u64 = 100;

pub struct HLC {
    id: Vec<u8>,
    delta: NTP64,
    last_time: Mutex<NTP64>
}

impl HLC {

    pub fn new(id: Vec<u8>) -> HLC {
        let delta = NTP64::from(Duration::from_millis(DELTA_MS));
        HLC { id, delta, last_time: Mutex::new(NTP64::from(UNIX_EPOCH)) }
    }

    pub async fn new_timestamp(&self) -> Timestamp {
        let mut now = NTP64::from(SystemTime::now());
        now.0 &= LMASK;
        let mut last_time = self.last_time.lock().await;
        if now.0 > (last_time.0 & LMASK) {
            *last_time = now
        } else {
            *last_time += 1;
        }
        Timestamp::new(*last_time, self.id.clone())
    }

    pub async fn update_with_timestamp(&self, timestamp: Timestamp) -> Result<(), String> {
        let mut now = NTP64::from(SystemTime::now());
        now.0 &= LMASK;
        let msg_time = timestamp.get_time();
        if *msg_time > now && *msg_time - now > self.delta {
            let err_msg = format!("incoming timestamp from {} exceeding delta {}ms is rejected: {} vs. now: {}",
                hex::encode_upper(timestamp.get_id()),
                self.delta.as_duration().as_millis(),
                msg_time, now);
            warn!("{}", err_msg);
            Err(err_msg)
        } else {
            let mut last_time = self.last_time.lock().await;
            let max_time = cmp::max(cmp::max(now, *msg_time), *last_time);
            if max_time == now {
                *last_time = now;
            } else if max_time == *msg_time {
                *last_time = *msg_time + 1; 
            } else {
                *last_time += 1;
            }
            Ok(())
        }
    }
}



#[cfg(test)]
mod tests {
    use crate::hlc::*;
    use std::time::{Duration, UNIX_EPOCH};
    use async_std::task;

    #[test]
    fn test_hlc() {
        task::block_on(async{
            let id: Vec<u8> = vec![
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 
                0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10];
            let hlc = HLC::new(id.clone());
            
            // Test that 10000 generated timestamps in a few time are monotonics
            let mut times: Vec<Timestamp> = Vec::with_capacity(10000);
            for _ in 0..10000 {
                times.push(hlc.new_timestamp().await)
            }

            let mut iter = times.iter().peekable();
            while let Some(ts1) = iter.next() {
                if let Some(ts2) = iter.peek() {
                    assert!(ts1 < ts2);
                    assert_eq!(ts1.get_id(), ts2.get_id());
                }
            }

            // Test that updating with an old Timestamp don't break the HLC
            let past_ts = Timestamp::new(NTP64::from(UNIX_EPOCH), id.clone());
            let now_ts = hlc.new_timestamp().await;
            assert!(hlc.update_with_timestamp(past_ts).await.is_ok());
            assert!(hlc.new_timestamp().await > now_ts);

            // Test that updating with a Timestamp exceeding the delta is refused
            let now_ts = hlc.new_timestamp().await;
            let future_time = now_ts.get_time() + NTP64::from(Duration::from_millis(500));
            let future_ts = Timestamp::new(future_time , id.clone());
            assert!(hlc.update_with_timestamp(future_ts).await.is_err())
        });
    }
}