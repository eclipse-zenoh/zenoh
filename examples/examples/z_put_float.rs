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
use zenoh_ext::z_serialize;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, key_expr, payload) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Putting Float ('{key_expr}': '{payload}')...");
    // you must have imported `zenoh_ext::ZBytesExt` to use `ZBytes::serialize`
    session.put(&key_expr, z_serialize(&payload)).await.unwrap();

    session.close().await.unwrap();
}

#[derive(clap::Parser, Clone, PartialEq, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/zenoh-rs-put")]
    /// The key expression to write to.
    key: KeyExpr<'static>,
    #[arg(short, long, default_value_t = std::f64::consts::PI)]
    /// The payload to write.
    payload: f64,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, f64) {
    let args = Args::parse();
    (args.common.into(), args.key, args.payload)
}
