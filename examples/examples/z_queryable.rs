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
use zenoh::{key_expr::KeyExpr, Config};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, key_expr, payload, complete) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring Queryable on '{key_expr}'...");
    let queryable = session
        .declare_queryable(&key_expr)
        // // By default queryable receives queries from a FIFO.
        // // Uncomment this line to use a ring channel instead.
        // // More information on the ring channel are available in the z_pull example.
        // .with(zenoh::handlers::RingChannel::default())
        .complete(complete)
        .await
        .unwrap();

    println!("Press CTRL-C to quit...");
    while let Ok(query) = queryable.recv_async().await {
        match query.payload() {
            None => println!(">> [Queryable ] Received Query '{}'", query.selector()),
            Some(query_payload) => {
                // Refer to z_bytes.rs to see how to deserialize different types of message
                let deserialized_payload = query_payload
                    .try_to_string()
                    .unwrap_or_else(|e| e.to_string().into());
                println!(
                    ">> [Queryable ] Received Query '{}' with payload '{}'",
                    query.selector(),
                    deserialized_payload
                )
            }
        }
        println!(
            ">> [Queryable ] Responding ('{}': '{}')",
            key_expr.as_str(),
            payload,
        );
        // Refer to z_bytes.rs to see how to serialize different types of message
        query
            .reply(key_expr.clone(), payload.clone())
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
    /// The payload to reply to queries.
    payload: String,
    #[arg(long)]
    /// Declare the queryable as complete w.r.t. the key expression.
    complete: bool,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, String, bool) {
    let args = Args::parse();
    (args.common.into(), args.key, args.payload, args.complete)
}
