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
use std::time::Duration;
use log::warn;
use async_std::sync::Mutex;
use crate::zasynclock;

mod ntp64;
pub use ntp64::*;

mod timestamp;
pub use timestamp::*;

mod clock;
pub use clock::*;

// Counter size (in bits) 
const CSIZE: u8 = 4u8;
// Bit-mask of the counter part within the 64 bits time
const CMASK: u64 = (1u64 << CSIZE) - 1u64;
// Bit-mask of the logical clock part within the 64 bits time
const LMASK: u64 = ! CMASK;

// HLC Delta in milliseconds: maximum accepted drift for an external timestamp.
// I.e.: if an incoming timestamp has a time > now() + delta, then the HLC is not updated.
const DELTA_MS: u64 = 100;

pub struct HLC<Clock>
{
    id: Vec<u8>,
    clock: Clock,
    delta: NTP64,
    last_time: Mutex<NTP64>
}

impl HLC<()> {
    pub fn with_system_time(id: Vec<u8>) -> HLC<fn() -> NTP64>
    {
        HLC::with_clock(id, system_time_clock)
    }
}


impl<Clock: Fn() -> NTP64> HLC<Clock>
{

    pub fn with_clock(id: Vec<u8>, clock: Clock) -> HLC<Clock>
    {
        let delta = NTP64::from(Duration::from_millis(DELTA_MS));
        HLC { id, clock, delta, last_time: Default::default() }
    }

    pub async fn new_timestamp(&self) -> Timestamp {
        let mut now = (self.clock)();
        now.0 &= LMASK;
        let mut last_time = zasynclock!(self.last_time);
        if now.0 > (last_time.0 & LMASK) {
            *last_time = now
        } else {
            *last_time += 1;
        }
        Timestamp::new(*last_time, self.id.clone())
    }

    pub async fn update_with_timestamp(&self, timestamp: &Timestamp) -> Result<(), String> {
        let mut now = (self.clock)();
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
            let mut last_time = zasynclock!(self.last_time);
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
    use std::time::{Duration};
    use async_std::task;
    use futures::join;

    #[test]
    fn hlc_parallel() {
        task::block_on(async{
            let id0: Vec<u8> = vec![0x00];
            let id1: Vec<u8> = vec![0x01];
            let id2: Vec<u8> = vec![0x02];
            let id3: Vec<u8> = vec![0x03];
            let hlc0 = HLC::with_system_time(id0.clone());
            let hlc1 = HLC::with_system_time(id1.clone());
            let hlc2 = HLC::with_system_time(id2.clone());
            let hlc3 = HLC::with_system_time(id3.clone());

            // Make 4 tasks to generate 10000 timestamps each with distinct HLCs
            const NB_TIME: usize = 10000;
            let t0 = task::spawn(async move {
                let mut times: Vec<Timestamp> = Vec::with_capacity(10000);
                for _ in 0..NB_TIME {
                    times.push(hlc0.new_timestamp().await)
                }
                times
            });
            let t1 = task::spawn(async move  {
                let mut times: Vec<Timestamp> = Vec::with_capacity(10000);
                for _ in 0..NB_TIME {
                    times.push(hlc1.new_timestamp().await)
                }
                times
            });
            let t2 = task::spawn(async move  {
                let mut times: Vec<Timestamp> = Vec::with_capacity(10000);
                for _ in 0..NB_TIME {
                    times.push(hlc2.new_timestamp().await)
                }
                times
            });
            let t3 = task::spawn(async move  {
                let mut times: Vec<Timestamp> = Vec::with_capacity(10000);
                for _ in 0..NB_TIME {
                    times.push(hlc3.new_timestamp().await)
                }
                times
            });
            let vecs = join!(t0, t1, t2, t3);

            // test that each timeseries is sorted (i.e. monotonic time)
            assert!(vecs.0.iter().is_sorted());
            assert!(vecs.1.iter().is_sorted());
            assert!(vecs.2.iter().is_sorted());
            assert!(vecs.3.iter().is_sorted());

            // test that there is no duplicate amongst all timestamps
            let mut all_times: Vec<Timestamp> =  vecs.0.into_iter()
                .chain(vecs.1.into_iter())
                .chain(vecs.2.into_iter())
                .chain(vecs.3.into_iter())
                .collect::<Vec<Timestamp>>();
            assert_eq!(NB_TIME*4, all_times.len());
            all_times.sort();
            all_times.dedup();
            assert_eq!(NB_TIME*4, all_times.len());

        });
    }


    #[test]
    fn hlc_update_with_timestamp() {
        task::block_on(async{
            let id: Vec<u8> = vec![
                0x01, 0x02, 0x03, 0x04, 0x05, 0x07, 0x08,
                0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f];
            let hlc = HLC::with_system_time(id.clone());
        
            // Test that updating with an old Timestamp don't break the HLC
            let past_ts = Timestamp::new(Default::default(), id.clone());
            let now_ts = hlc.new_timestamp().await;
            assert!(hlc.update_with_timestamp(&past_ts).await.is_ok());
            assert!(hlc.new_timestamp().await > now_ts);

            // Test that updating with a Timestamp exceeding the delta is refused
            let now_ts = hlc.new_timestamp().await;
            let future_time = now_ts.get_time() + NTP64::from(Duration::from_millis(500));
            let future_ts = Timestamp::new(future_time , id.clone());
            assert!(hlc.update_with_timestamp(&future_ts).await.is_err())
        });
    }
}