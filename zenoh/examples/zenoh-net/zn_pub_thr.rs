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
use zenoh::net::ResKey::*;
use zenoh::net::*;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();
    let (config, size) = parse_args();

    let data: RBuf = (0usize..size)
        .map(|i| (i % 10) as u8)
        .collect::<Vec<u8>>()
        .into();

    let session = open(config, None).await.unwrap();

    let reskey = RId(session
        .declare_resource(&RName("/test/thr".to_string()))
        .await
        .unwrap());
    let _publ = session.declare_publisher(&reskey).await.unwrap();

    loop {
        session
            .write_ext(
                &reskey,
                data.clone(),
                encoding::DEFAULT,
                data_kind::DEFAULT,
                CongestionControl::Block, // Make sure to not drop messages because of congestion control
            )
            .await
            .unwrap();
    }
}

fn parse_args() -> (Config, usize) {
    let args = App::new("zenoh-net throughput pub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
                .possible_values(&["peer", "client"])
                .default_value("peer"),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(Arg::from_usage(
            "<PAYLOAD_SIZE>          'Sets the size of the payload to publish'",
        ))
        .get_matches();

    let config = Config::default()
        .mode(
            args.value_of("mode")
                .map(|m| Config::parse_mode(m))
                .unwrap()
                .unwrap(),
        )
        .add_peers(
            args.values_of("peer")
                .map(|p| p.collect())
                .or_else(|| Some(vec![]))
                .unwrap(),
        )
        .add_listeners(
            args.values_of("listener")
                .map(|p| p.collect())
                .or_else(|| Some(vec![]))
                .unwrap(),
        );

    let size = args
        .value_of("PAYLOAD_SIZE")
        .unwrap()
        .parse::<usize>()
        .unwrap();

    (config, size)
}
