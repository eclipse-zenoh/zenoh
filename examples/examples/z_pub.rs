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
use std::time::Duration;
use zenoh::config::Config;
use zenoh::core::AsyncResolve;
use zenoh::key_expr::KeyExpr;
use zenoh::sample::SampleBuilderTrait;
use zenoh::session::SessionDeclarations;
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // Initiate logging
    env_logger::init();

    let (config, key_expr, value, attachment) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Publisher on '{key_expr}'...");
    let publisher = session.declare_publisher(&key_expr).res().await.unwrap();

    println!("Press CTRL-C to quit...");
    for idx in 0..u32::MAX {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let buf = format!("[{idx:4}] {value}");
        println!("Putting Data ('{}': '{}')...", &key_expr, buf);
        let mut put = publisher.put(buf);
        if let Some(attachment) = &attachment {
            put = put.attachment(Some(
                attachment
                    .split('&')
                    .map(|pair| split_once(pair, '='))
                    .collect(),
            ))
        }
        put.res().await.unwrap();
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/zenoh-rs-pub")]
    /// The key expression to write to.
    key: KeyExpr<'static>,
    #[arg(short, long, default_value = "Pub from Rust!")]
    /// The value to write.
    value: String,
    #[arg(short, long)]
    /// The attachments to add to each put.
    ///
    /// The key-value pairs are &-separated, and = serves as the separator between key and value.
    attach: Option<String>,
    #[command(flatten)]
    common: CommonArgs,
}

fn split_once(s: &str, c: char) -> (&[u8], &[u8]) {
    let s_bytes = s.as_bytes();
    match s.find(c) {
        Some(index) => {
            let (l, r) = s_bytes.split_at(index);
            (l, &r[1..])
        }
        None => (s_bytes, &[]),
    }
}

fn parse_args() -> (Config, KeyExpr<'static>, String, Option<String>) {
    let args = Args::parse();
    (args.common.into(), args.key, args.value, args.attach)
}
