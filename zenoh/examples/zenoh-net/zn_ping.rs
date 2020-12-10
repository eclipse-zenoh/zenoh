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
use async_std::stream::StreamExt;
use async_std::sync::{Arc, Barrier, Mutex};
use async_std::task;
use clap::{App, Arg};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zenoh::net::ResKey::*;
use zenoh::net::*;
use zenoh::Properties;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, size, interval) = parse_args();

    let session = open(config.into()).await.unwrap();
    let session = Arc::new(session);

    // The hashmap with the pings
    let pending = Arc::new(Mutex::new(HashMap::<u64, Instant>::new()));
    let barrier = Arc::new(Barrier::new(2));

    let c_pending = pending.clone();
    let c_barrier = barrier.clone();
    let c_session = session.clone();
    task::spawn(async move {
        // The resource to wait the response back
        let reskey_pong = RId(c_session
            .declare_resource(&RName("/test/pong".to_string()))
            .await
            .unwrap());

        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None,
        };
        let mut sub = c_session
            .declare_subscriber(&reskey_pong, &sub_info)
            .await
            .unwrap();

        // Wait for the both publishers and subscribers to be declared
        c_barrier.wait().await;

        while let Some(mut sample) = sub.stream().next().await {
            let mut count_bytes = [0u8; 8];
            sample.payload.read_bytes(&mut count_bytes).unwrap();
            let count = u64::from_le_bytes(count_bytes);
            let instant = c_pending.lock().await.remove(&count).unwrap();
            println!(
                "{} bytes: seq={} time={:?}",
                sample.payload.len(),
                count,
                instant.elapsed()
            );
        }
    });

    // The resource to publish data on
    let reskey_ping = RId(session
        .declare_resource(&RName("/test/ping".to_string()))
        .await
        .unwrap());
    let _publ = session.declare_publisher(&reskey_ping).await.unwrap();

    // Wait for the both publishers and subscribers to be declared
    barrier.wait().await;

    let payload = vec![0u8; size - 8];
    let mut count: u64 = 0;
    loop {
        let mut data: WBuf = WBuf::new(size, true);
        let count_bytes: [u8; 8] = count.to_le_bytes();
        data.write_bytes(&count_bytes);
        data.write_bytes(&payload);

        let data: RBuf = data.into();

        pending.lock().await.insert(count, Instant::now());
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

        task::sleep(Duration::from_secs_f64(interval)).await;
        count += 1;
    }
}

fn parse_args() -> (Properties, usize, f64) {
    let args = App::new("zenoh-net throughput sub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode.")
                .possible_values(&["peer", "client"])
                .default_value("peer"),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...   'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(
            Arg::from_usage("-i, --interval=[number] 'Interval in seconds between data messages.'")
                .default_value("1"),
        )
        .arg(Arg::from_usage(
            "--no-scouting 'Disable the scouting mechanism.'",
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
    if args.is_present("no-scouting") {
        config.insert("multicast_scouting".to_string(), "false".to_string());
    }

    let interval: f64 = args.value_of("interval").unwrap().parse().unwrap();
    let size: usize = args.value_of("PAYLOAD_SIZE").unwrap().parse().unwrap();

    (config, size, interval)
}
