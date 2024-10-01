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

use std::collections::HashMap;

use clap::Parser;
use futures::select;
use zenoh::{
    key_expr::{keyexpr, KeyExpr},
    sample::{Sample, SampleKind},
    Config,
};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, key_expr, complete) = parse_args();

    let mut stored: HashMap<String, Sample> = HashMap::new();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring Subscriber on '{key_expr}'...");
    let subscriber = session.declare_subscriber(&key_expr).await.unwrap();

    println!("Declaring Queryable on '{key_expr}'...");
    let queryable = session
        .declare_queryable(&key_expr)
        .complete(complete)
        .await
        .unwrap();

    println!("Press CTRL-C to quit...");
    loop {
        select!(
            sample = subscriber.recv_async() => {
                let sample = sample.unwrap();
                let payload = sample.payload().try_to_string().unwrap_or_else(|e| e.to_string().into());
                println!(">> [Subscriber] Received {} ('{}': '{}')", sample.kind(), sample.key_expr().as_str(),payload);
                match sample.kind() {
                    SampleKind::Delete => stored.remove(&sample.key_expr().to_string()),
                    SampleKind::Put => stored.insert(sample.key_expr().to_string(), sample),
                };
            },

            query = queryable.recv_async() => {
                let query = query.unwrap();
                println!(">> [Queryable ] Received Query '{}'", query.selector());
                for (stored_name, sample) in stored.iter() {
                    if query.key_expr().intersects(unsafe {keyexpr::from_str_unchecked(stored_name)}) {
                        query.reply(sample.key_expr().clone(), sample.payload().clone()).await.unwrap();
                    }
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
