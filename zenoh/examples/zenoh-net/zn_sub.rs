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
use futures::select;
use zenoh::net::*;
use zenoh::Properties;

#[async_std::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    let (config, selector) = parse_args();

    println!("Opening session...");
    let session = open(config.into()).await.unwrap();

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
            sample = subscriber.receiver().next().fuse() => {
                let sample = sample.unwrap();
                println!(">> [Subscription listener] Received ('{}': '{}')",
                    sample.res_name, String::from_utf8_lossy(&sample.payload.contiguous()));

                // Calling contiguous() allows to acces to the whole payload as a contigous &[u8] via the
                // ZSlice type. However, this operation has a drawback when the original message was large
                // enough to cause network fragmentation. Because of that, the actual message payload may have
                // been received in multiple fragments (i.e. ZSlice) which are non-contigous in memory. In order
                // to retrieve the content of the payload without allocating, it is possible to loop over the
                // different payload ZSlice. Finally, iterating over the payload is also recommended when
                // working with shared memory.
                //
                // payload.zslices_num() returns the number of ZSlices the payload is composed of. If
                // the returned value is greater than 1, then contigous() will allocate.
                //
                // let mut sample = sample.unwrap();
                // println!(">> [Subscription listener] Received '{}', {} ZSlice:", sample.res_name, sample.payload.zslices_num());
                // for z in sample.payload.next() {
                //     println!("   {}: '{}'", z.get_kind(), String::from_utf8_lossy(z.as_slice()));
                // }
            },

            _ = stdin.read_exact(&mut input).fuse() => {
                if input[0] == b'q' {break}
            }
        );
    }
}

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
