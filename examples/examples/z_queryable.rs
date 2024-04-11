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
    env_logger::init();

    let (config, key_expr, value, complete) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Queryable on '{key_expr}'...");
    let queryable = session
        .declare_queryable(&key_expr)
        // // By default queryable receives queries from a FIFO. 
        // // Uncomment this line to use a ring channel instead. 
        // // More information on the ring channel are available in the z_pull example.
        // .with(zenoh::handlers::RingChannel::default())
        .complete(complete)
        .res()
        .await
        .unwrap();

    println!("Press CTRL-C to quit...");
    while let Ok(query) = queryable.recv_async().await {
        match query.value() {
            None => println!(">> [Queryable ] Received Query '{}'", query.selector()),
            Some(value) => {
                let payload = value
                    .payload()
                    .deserialize::<String>()
                    .unwrap_or_else(|e| format!("{}", e));
                println!(
                    ">> [Queryable ] Received Query '{}' with payload '{}'",
                    query.selector(),
                    payload
                )
            }
        }
        println!(
            ">> [Queryable ] Responding ('{}': '{}')",
            key_expr.as_str(),
            value,
        );
        query
            .reply(key_expr.clone(), value.clone())
            .res()
            .await
            .unwrap_or_else(|e| println!(">> [Queryable ] Error sending reply: {e}"));
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/zenoh-rs-queryable")]
    /// The key expression matching queries to reply to.
    key: KeyExpr<'static>,
    #[arg(short, long, default_value = "Queryable from Rust!")]
    /// The value to reply to queries.
    value: String,
    #[arg(long)]
    /// Declare the queryable as complete w.r.t. the key expression.
    complete: bool,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, String, bool) {
    let args = Args::parse();
    (args.common.into(), args.key, args.value, args.complete)
}
