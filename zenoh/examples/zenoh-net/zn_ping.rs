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
use clap::{App, Arg};
use futures::prelude::*;
use std::time::Instant;
use zenoh::net::ResKey::*;
use zenoh::net::*;
use zenoh::Properties;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, size) = parse_args();
    let session = open(config.into()).await.unwrap();

    // The resource to publish data on
    let reskey_ping = RId(session
        .declare_resource(&RName("/test/ping".to_string()))
        .await
        .unwrap());

    // The resource to wait the response back
    let reskey_pong = RId(session
        .declare_resource(&RName("/test/pong".to_string()))
        .await
        .unwrap());

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };
    let mut sub = session
        .declare_subscriber(&reskey_pong, &sub_info)
        .await
        .unwrap();

    let data: RBuf = (0usize..size)
        .map(|i| (i % 10) as u8)
        .collect::<Vec<u8>>()
        .into();

    let mut count: u64 = 0;
    loop {
        let data = data.clone();
        let write_time = Instant::now();
        session
            .write_ext(
                &reskey_ping,
                data,
                encoding::DEFAULT,
                data_kind::DEFAULT,
                CongestionControl::Block, // Make sure to not drop messages because of congestion control
            )
            .await
            .unwrap();

        if let Some(sample) = sub.stream().next().await {
            println!(
                "{} bytes: seq={} time={:?}µs",
                sample.payload.len(),
                count,
                write_time.elapsed().as_micros(),
            );
        }

        count += 1;
    }
}

fn parse_args() -> (Properties, usize) {
    let args = App::new("zenoh-net throughput sub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...   'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .arg(Arg::from_usage(
            "<PAYLOAD_SIZE>          'Sets the size of the payload to publish'",
        ))
        .get_matches();

    let mut config = Properties::default();
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }
    if args.is_present("no-multicast-scouting") {
        config.insert("multicast_scouting".to_string(), "false".to_string());
    }

    let size: usize = args.value_of("PAYLOAD_SIZE").unwrap().parse().unwrap();

    (config, size)
}
