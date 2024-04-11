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
    // Initiate logging
    zenoh_util::init_log_from_env();

    let (config, key_expr) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Liveliness Subscriber on '{}'...", &key_expr);

    let subscriber = session
        .liveliness()
        .declare_subscriber(&key_expr)
        .res()
        .await
        .unwrap();

    println!("Press CTRL-C to quit...");
    while let Ok(sample) = subscriber.recv_async().await {
        match sample.kind {
            SampleKind::Put => println!(
                ">> [LivelinessSubscriber] New alive token ('{}')",
                sample.key_expr.as_str()
            ),
            SampleKind::Delete => println!(
                ">> [LivelinessSubscriber] Dropped token ('{}')",
                sample.key_expr.as_str()
            ),
        }
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "group1/**")]
    /// The key expression to write to.
    key: KeyExpr<'static>,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>) {
    let args = Args::parse();
    (args.common.into(), args.key)
}
