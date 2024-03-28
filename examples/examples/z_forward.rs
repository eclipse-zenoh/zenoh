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
use zenoh_ext::SubscriberForward;

#[tokio::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    let (config, key_expr, forward) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Subscriber on '{key_expr}'...");
    let mut subscriber = session.declare_subscriber(&key_expr).res().await.unwrap();
    println!("Declaring Publisher on '{forward}'...");
    let publisher = session.declare_publisher(&forward).res().await.unwrap();
    println!("Forwarding data from '{key_expr}' to '{forward}'...");
    subscriber.forward(publisher).await.unwrap();
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The key expression to subscribe to.
    key: KeyExpr<'static>,
    #[arg(short, long, default_value = "demo/forward")]
    /// The key expression to forward to.
    forward: KeyExpr<'static>,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, KeyExpr<'static>) {
    let args = Args::parse();
    (args.common.into(), args.key, args.forward)
}
