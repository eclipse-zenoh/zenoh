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
use std::convert::TryInto;
use std::time::{Duration, Instant};
use zenoh::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, size, interval) = parse_args();

    let zenoh = Zenoh::new(config.into()).await.unwrap();
    let zenoh = Arc::new(zenoh);

    // The hashmap with the pings
    let pending = Arc::new(Mutex::new(HashMap::<u64, Instant>::new()));
    let barrier = Arc::new(Barrier::new(2));

    let c_pending = pending.clone();
    let c_barrier = barrier.clone();
    let c_zenoh = zenoh.clone();
    task::spawn(async move {
        let workspace = c_zenoh.workspace(None).await.unwrap();
        let mut sub = workspace
            .subscribe(&"/test/pong/".to_string().try_into().unwrap())
            .await
            .unwrap();

        // Notify that the subscriber has been created
        c_barrier.wait().await;

        while let Some(change) = sub.next().await {
            match change.value.unwrap() {
                Value::Raw(_, mut payload) => {
                    let mut count_bytes = [0u8; 8];
                    payload.read_bytes(&mut count_bytes).unwrap();
                    let count = u64::from_le_bytes(count_bytes);

                    let instant = c_pending.lock().await.remove(&count).unwrap();
                    println!(
                        "{} bytes: seq={} time={:?}",
                        payload.len(),
                        count,
                        instant.elapsed()
                    );
                }
                _ => panic!("Invalid value"),
            }
        }
    });

    // Wait for the subscriber to be declared
    barrier.wait().await;

    let workspace = zenoh.workspace(None).await.unwrap();
    let mut count: u64 = 0;
    loop {
        let count_bytes: [u8; 8] = count.to_le_bytes();
        let mut payload = vec![0u8; size];
        payload[0..8].copy_from_slice(&count_bytes);

        pending.lock().await.insert(count, Instant::now());

        workspace
            .put(&"/test/ping".try_into().unwrap(), payload.into())
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

    let interval: f64 = args.value_of("interval").unwrap().parse().unwrap();
    let size: usize = args.value_of("PAYLOAD_SIZE").unwrap().parse().unwrap();

    (config, size, interval)
}
