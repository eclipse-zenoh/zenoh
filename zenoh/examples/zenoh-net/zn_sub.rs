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
use clap::{App, Arg, Values};
use futures::prelude::*;
use futures::select;
use zenoh::net::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, selector) = parse_args();

    println!("Opening session...");
    let session = open(config).await.unwrap();

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
                println!(">> [Subscription listener] Received ('{}': '{}')",
                    sample.res_name, String::from_utf8_lossy(&sample.payload.to_vec()));
            },

            _ = stdin.read_exact(&mut input).fuse() => {
                if input[0] == 'q' as u8 {break}
            }
        );
    }
}

fn parse_args() -> (Properties, String) {
    let args = App::new("zenoh-net sub example")
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
            Arg::from_usage("-s, --selector=[SELECTOR] 'The selection of resources to subscribe'")
                .default_value("/demo/example/**"),
        )
        .get_matches();

    let mut config = config::empty();
    config.push((
        config::ZN_MODE_KEY,
        args.value_of("mode").unwrap().as_bytes().to_vec(),
    ));
    for peer in args
        .values_of("peer")
        .or_else(|| Some(Values::default()))
        .unwrap()
    {
        config.push((config::ZN_PEER_KEY, peer.as_bytes().to_vec()));
    }
    for listener in args
        .values_of("listener")
        .or_else(|| Some(Values::default()))
        .unwrap()
    {
        config.push((config::ZN_LISTENER_KEY, listener.as_bytes().to_vec()));
    }

    let selector = args.value_of("selector").unwrap().to_string();

    (config, selector)
}
