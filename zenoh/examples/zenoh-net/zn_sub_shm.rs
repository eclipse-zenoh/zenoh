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
use clap::{App, Arg};
#[cfg(feature = "zero-copy")]
use futures::prelude::*;
#[cfg(feature = "zero-copy")]
use futures::select;
#[cfg(feature = "zero-copy")]
use zenoh::net::*;
#[cfg(feature = "zero-copy")]
use zenoh::Properties;

#[cfg(feature = "zero-copy")]
#[async_std::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initiate logging
    env_logger::init();

    let (config, selector) = parse_args();

    println!("Opening session...");
    let session = open(config.into()).await.unwrap();

    let id = session.id().await;
    println!("Creating Shared Memory Manager...");
    let mut shm = SharedMemoryManager::new(id, 8_192).unwrap();
    println!("{:?}", &shm);

    println!("Declaring Subscriber on {}", selector);

    let sub_info = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };

    let mut subscriber = session
        .declare_subscriber(&selector.into(), &sub_info)
        .await
        .unwrap();

    let mut stdin = async_std::io::stdin();
    let mut input = [0u8];

    loop {
        select!(
            sample = subscriber.stream().next().fuse() => {
                let sample = sample.unwrap();
                let sbuf = sample.payload.into_shm(&mut shm).unwrap();
                println!(">> [Subscription listener] Received ('{}': '{}')",
                    sample.res_name, String::from_utf8_lossy(&sbuf.as_slice()));
            },

            _ = stdin.read_exact(&mut input).fuse() => {
                if input[0] == b'q' {
                    return Ok(());
                }
            }
        );
    }
}

#[cfg(not(feature = "zero-copy"))]
fn main() {
    println!(
        "Please, enable zero-copy feature by rebuilding as follows:\
            \n\n\t$ cargo build --release --features \"tcp udp zero-copy\"\n"
    );
}

#[cfg(feature = "zero-copy")]
fn parse_args() -> (Properties, String) {
    let args = App::new("zenoh-net sub example")
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
        .arg(
            Arg::from_usage("-s, --selector=[SELECTOR] 'The selection of resources to subscribe'")
                .default_value("/demo/example/**"),
        )
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Properties::from(std::fs::read_to_string(conf_file).unwrap())
    } else {
        Properties::default()
    };
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }
    if args.is_present("no-multicast-scouting") {
        config.insert("multicast_scouting".to_string(), "false".to_string());
    }

    let selector = args.value_of("selector").unwrap().to_string();

    (config, selector)
}
