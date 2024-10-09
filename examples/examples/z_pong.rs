//
// Copyright (c) 2023 ZettaScale Technology
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
use clap::Parser;
use zenoh::{key_expr::keyexpr, qos::CongestionControl, Config, Wait};
use zenoh_examples::CommonArgs;

fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, express) = parse_args();

    let session = zenoh::open(config).wait().unwrap();

    // The key expression to read the data from
    let key_expr_ping = keyexpr::new("test/ping").unwrap();

    // The key expression to echo the data back
    let key_expr_pong = keyexpr::new("test/pong").unwrap();

    let publisher = session
        .declare_publisher(key_expr_pong)
        .congestion_control(CongestionControl::Block)
        .express(express)
        .wait()
        .unwrap();

    session
        .declare_subscriber(key_expr_ping)
        .callback(move |sample| publisher.put(sample.payload().clone()).wait().unwrap())
        .background()
        .wait()
        .unwrap();
    std::thread::park();
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    /// express for sending data
    #[arg(long, default_value = "false")]
    no_express: bool,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, bool) {
    let args = Args::parse();
    (args.common.into(), !args.no_express)
}
