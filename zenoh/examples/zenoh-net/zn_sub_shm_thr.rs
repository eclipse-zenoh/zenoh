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
#[cfg(feature = "zero-copy")]
use async_std::future;
#[cfg(feature = "zero-copy")]
use clap::{App, Arg};
#[cfg(feature = "zero-copy")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "zero-copy")]
use std::time::Instant;
#[cfg(feature = "zero-copy")]
use zenoh::net::ResKey::*;
#[cfg(feature = "zero-copy")]
use zenoh::net::*;
#[cfg(feature = "zero-copy")]
use zenoh::Properties;

#[cfg(feature = "zero-copy")]
#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, m, n) = parse_args();

    let z = open(config.into()).await.unwrap();
    let id = z.id().await;
    let shm = Arc::new(Mutex::new(SharedMemoryManager::new(id, 8192).unwrap()));

    let reskey = RId(z
        .declare_resource(&RName("/test/shm/thr".to_string()))
        .await
        .unwrap());

    let mut count = 0u128;
    let mut start = Instant::now();

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };
    let mut nm = 0;
    let _sub = z
        .declare_callback_subscriber(&reskey, &sub_info, move |sample| {
            let _shm_buf = sample.payload.into_shm(&mut *shm.lock().unwrap());
            if count == 0 {
                start = Instant::now();
                count += 1;
            } else if count < n {
                count += 1;
            } else {
                print_stats(start, n);
                nm += 1;
                count = 0;
                if nm >= m {
                    std::process::exit(0)
                }
            }
        })
        .await
        .unwrap();

    // Stop forever
    future::pending::<()>().await;
}

#[cfg(not(feature = "zero-copy"))]
fn main() {
    println!(
        "Please, enable zero-copy feature by rebuilding as follows:\
            \n\n\t$ cargo build --release --features \"zero-copy\"\n"
    );
}

#[cfg(feature = "zero-copy")]
fn print_stats(start: Instant, n: u128) {
    let elapsed = start.elapsed().as_secs_f64();
    let thpt = (n as f64) / elapsed;
    println!("{} msg/s", thpt);
}

#[cfg(feature = "zero-copy")]
fn parse_args() -> (Properties, u32, u128) {
    let args = App::new("zenoh-net zero-copy throughput sub example")
        .arg(
            Arg::from_usage("-s, --samples=[number] 'Number of throughput measurements.'")
                .default_value("10"),
        )
        .arg(
            Arg::from_usage(
                "-n, --number=[number] 'Number of messages in each throughput measurements.'",
            )
            .default_value("100000"),
        )
        .get_matches();

    let config = Properties::default();

    let samples: u32 = args.value_of("samples").unwrap().parse().unwrap();
    let number: u128 = args.value_of("number").unwrap().parse().unwrap();

    (config, samples, number)
}
