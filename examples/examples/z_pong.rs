//
// Copyright (c) 2022 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use clap::{App, Arg};
use zenoh::config::Config;
use zenoh::prelude::sync::SyncResolve;
use zenoh::publication::CongestionControl;

fn main() {
    // initiate logging
    env_logger::init();

    let config = parse_args();

    let session = zenoh::open(config).res().unwrap();

    // The key expression to read the data from
    let key_expr_ping = session.declare_keyexpr("test/ping").res().unwrap();

    // The key expression to echo the data back
    let key_expr_pong = session.declare_keyexpr("test/pong").res().unwrap();

    let sub = session.declare_subscriber(&key_expr_ping).res().unwrap();

    while let Ok(sample) = sub.recv() {
        session
            .put(&key_expr_pong, sample.value)
            // Make sure to not drop messages because of congestion control
            .congestion_control(CongestionControl::Block)
            .res()
            .unwrap();
    }
}

fn parse_args() -> Config {
    let args = App::new("zenoh roundtrip pong example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE]  'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --connect=[ENDPOINT]...   'Endpoints to connect to.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listen=[ENDPOINT]...   'Endpoints to listen on.'",
        ))
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Config::from_file(conf_file).unwrap()
    } else {
        Config::default()
    };
    if let Some(Ok(mode)) = args.value_of("mode").map(|mode| mode.parse()) {
        config.set_mode(Some(mode)).unwrap();
    }
    if let Some(values) = args.values_of("connect") {
        config
            .connect
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if let Some(values) = args.values_of("listen") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    config
}
