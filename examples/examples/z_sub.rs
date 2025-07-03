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
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, key_expr) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring Subscriber on '{}'...", &key_expr);
    let subscriber = session.declare_subscriber(&key_expr).await.unwrap();

    println!("Press CTRL-C to quit...");
    while let Ok(sample) = subscriber.recv_async().await {
        // Refer to z_bytes.rs to see how to deserialize different types of message
        let payload = sample
            .payload()
            .try_to_string()
            .unwrap_or_else(|e| e.to_string().into());

        print!(
            ">> [Subscriber] Received {} ('{}': '{}')",
            sample.kind(),
            sample.key_expr().as_str(),
            payload
        );
        if let Some(att) = sample.attachment() {
            let att = att.try_to_string().unwrap_or_else(|e| e.to_string().into());
            print!(" ({att})");
        }
        println!();
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct SubArgs {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The Key Expression to subscribe to.
    key: KeyExpr<'static>,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>) {
    let args = SubArgs::parse();
    (args.common.into(), args.key)
}
