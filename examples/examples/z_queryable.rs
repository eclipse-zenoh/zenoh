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
use futures::prelude::*;
use tokio::io::AsyncReadExt;
use futures::select;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, key_expr, value, complete) = parse_args();
    let send_errors = std::sync::atomic::AtomicBool::new(false);

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Queryable on '{key_expr}'...");
    let queryable = session
        .declare_queryable(&key_expr)
        .complete(complete)
        .res()
        .await
        .unwrap();

    println!("Enter 'q' to quit, 'e' to reply an error to next query...");
    let mut stdin = tokio::io::stdin();
    let mut input = [0_u8];
    loop {
        select!(
            query = queryable.recv_async() => {
                let query = query.unwrap();
                match query.value() {
                    None => println!(">> [Queryable ] Received Query '{}'", query.selector()),
                    Some(value) => println!(">> [Queryable ] Received Query '{}' with value '{}'", query.selector(), value),
                }
                let reply = if send_errors.swap(false, Relaxed) {
                    println!(
                        ">> [Queryable ] Replying (ERROR: '{}')",
                        value,
                    );
                    Err(value.clone().into())
                } else {
                    println!(
                        ">> [Queryable ] Responding ('{}': '{}')",
                        key_expr.as_str(),
                        value,
                    );
                    Ok(Sample::new(key_expr.clone(), value.clone()))
                };
                query
                    .reply(reply)
                    .res()
                    .await
                    .unwrap_or_else(|e| println!(">> [Queryable ] Error sending reply: {e}"));
            },

            _ = stdin.read_exact(&mut input).fuse() => {
                match input[0] {
                    b'q' => break,
                    0 => tokio::time::sleep(Duration::from_secs(1)).await,
                    b'e' => send_errors.store(true, Relaxed),
                    _ => (),
                }
            }
        );
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
