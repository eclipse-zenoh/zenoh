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
use async_std::task;
use zenoh::net::*;
use zenoh::net::ResKey::*;

fn main() {
    task::block_on( async {
        // initiate logging
        env_logger::init();

        let args = App::new("zenoh-net throughput pub example")
            .arg("-l, --locator=[LOCATOR] 'Sets the locator used to initiate the zenoh session'")
            .arg("<PAYLOAD_SIZE>          'Sets the size of the payload to publish'")
            .get_matches();

        let locator = args.value_of("locator").unwrap_or("").to_string();
        let size    = args.value_of("PAYLOAD_SIZE").unwrap().parse::<usize>().unwrap();

        let data = (0usize..size).map(|i| (i%10) as u8).collect::<Vec<u8>>();

        println!("Openning session...");
        let session = open(&locator, None).await.unwrap();

        let reskey = RId(session.declare_resource(&RName("/test/thr".to_string())).await.unwrap());
        let _publ = session.declare_publisher(&reskey).await.unwrap();

        loop {
            session.write(&reskey, data.clone()).await.unwrap();
        }
    })
}
