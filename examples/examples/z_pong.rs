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
use zenoh::{key_expr::keyexpr, prelude::*, publisher::CongestionControl, Config};
use zenoh_examples::CommonArgs;

fn main() {
    // initiate logging
    zenoh_util::try_init_log_from_env();

    let (mut config, express) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_ping_shm` to operate
    // over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
    // subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
    config.transport.shared_memory.set_enabled(true).unwrap();

    let session = zenoh::open(config).wait().unwrap().into_arc();

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

    let _sub = session
        .declare_subscriber(key_expr_ping)
        .callback(move |sample| publisher.put(sample.payload().clone()).wait().unwrap())
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
