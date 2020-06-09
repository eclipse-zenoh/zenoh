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
use clap::App;
use async_std::future;
use async_std::task;
use std::time::Instant;
use zenoh::net::*;
use zenoh::net::ResKey::*;

const N: u128 = 100000;

fn print_stats(start: Instant) {
    let elapsed = start.elapsed().as_secs_f64();
    let thpt = (N as f64) / elapsed;
    println!("{} msg/s", thpt);
}


fn main() {
    task::block_on( async {
        // initiate logging
        env_logger::init();

        let args = App::new("zenoh-net throughput sub example")
            .arg("-l, --locator=[LOCATOR] 'Sets the locator used to initiate the zenoh session'")
            .get_matches();

        let locator = args.value_of("locator").unwrap_or("").to_string();
        
        let session = open(&locator, None).await.unwrap();

        let reskey = RId(session.declare_resource(&RName("/test/thr".to_string())).await.unwrap());

        let mut count = 0u128;
        let mut start = Instant::now();

        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None
        };
        let _ = session.declare_subscriber(&reskey, &sub_info,
            move |_res_name: &str, _payload: RBuf, _data_info: DataInfo| {
                if count == 0 {
                    start = Instant::now();
                    count = count + 1;
                } else if count < N {
                    count = count + 1;
                } else {
                    print_stats(start);
                    count = 0;
                }
            }
        ).await.unwrap();

        // Stop forever
        future::pending::<()>().await;

        // @TODO: Uncomment these once the writer starvation has been solved on the RwLock      
        // session.undeclare_subscriber(sub).await.unwrap();
        // session.close().await.unwrap();
    });
}
