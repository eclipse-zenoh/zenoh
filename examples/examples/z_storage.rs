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
#![recursion_limit = "256"]

use async_std::task::sleep;
use clap::Parser;
use futures::prelude::*;
use futures::select;
use std::collections::HashMap;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh_examples::CommonArgs;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();

    let (config, key_expr, complete) = parse_args();

    let mut stored: HashMap<String, Sample> = HashMap::new();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Subscriber on '{key_expr}'...");
    let subscriber = session.declare_subscriber(&key_expr).res().await.unwrap();

    println!("Declaring Queryable on '{key_expr}'...");
    let queryable = session
        .declare_queryable(&key_expr)
        .complete(complete)
        .res()
        .await
        .unwrap();

    println!("Enter 'q' to quit...");
    let mut stdin = async_std::io::stdin();
    let mut input = [0u8];
    loop {
        select!(
            sample = subscriber.recv_async() => {
                let sample = sample.unwrap();
                println!(">> [Subscriber] Received {} ('{}': '{}')",
                    sample.kind, sample.key_expr.as_str(), sample.value);
                if sample.kind == SampleKind::Delete {
                    stored.remove(&sample.key_expr.to_string());
                } else {
                    stored.insert(sample.key_expr.to_string(), sample);
                }
            },

            query = queryable.recv_async() => {
                let query = query.unwrap();
                println!(">> [Queryable ] Received Query '{}'", query.selector());
                for (stored_name, sample) in stored.iter() {
                    if query.selector().key_expr.intersects(unsafe {keyexpr::from_str_unchecked(stored_name)}) {
                        query.reply(sample.key_expr.clone(), sample.value.clone()).res().await.unwrap();
                    }
                }
            },

            _ = stdin.read_exact(&mut input).fuse() => {
                match input[0] {
                    b'q' => break,
                    0 => sleep(Duration::from_secs(1)).await,
                    _ => (),
                }
            }
        );
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The selection of resources to store.
    key: KeyExpr<'static>,
    #[arg(long)]
    /// Declare the storage as complete w.r.t. the key expression.
    complete: bool,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, bool) {
    let args = Args::parse();
    (args.common.into(), args.key, args.complete)
}
