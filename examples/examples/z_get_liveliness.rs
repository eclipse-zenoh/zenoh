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
use std::time::Duration;

use clap::Parser;
use zenoh::{key_expr::KeyExpr, Config};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, key_expr, timeout) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Sending Liveliness Query '{key_expr}'...");
    let replies = session
        .liveliness()
        .get(&key_expr)
        .timeout(timeout)
        .await
        .unwrap();
    while let Ok(reply) = replies.recv_async().await {
        match reply.result() {
            Ok(sample) => println!(">> Alive token ('{}')", sample.key_expr().as_str(),),
            Err(err) => {
                let payload = err
                    .payload()
                    .try_to_string()
                    .unwrap_or_else(|e| e.to_string().into());
                println!(">> Received (ERROR: '{payload}')");
            }
        }
    }
}

#[derive(Parser, Clone, Debug)]
struct Args {
    #[arg(short, long, default_value = "group1/**")]
    /// The key expression matching liveliness tokens to query.
    key_expr: KeyExpr<'static>,
    #[arg(short = 'o', long, default_value = "10000")]
    /// The query timeout in milliseconds.
    timeout: u64,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, Duration) {
    let args = Args::parse();
    let timeout = Duration::from_millis(args.timeout);
    (args.common.into(), args.key_expr, timeout)
}
