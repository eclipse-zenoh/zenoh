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
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh_util::init_log();

    let config = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    let info = session.info();
    println!("zid: {}", info.zid().res().await);
    println!(
        "routers zid: {:?}",
        info.routers_zid().res().await.collect::<Vec<ZenohId>>()
    );
    println!(
        "peers zid: {:?}",
        info.peers_zid().res().await.collect::<Vec<ZenohId>>()
    );
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> Config {
    let args = Args::parse();
    args.common.into()
}
