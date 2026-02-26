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
use zenoh::{
    key_expr::KeyExpr,
    shm::{BlockOn, GarbageCollect, ShmProviderBuilder},
    Config, Wait,
};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() -> zenoh::Result<()> {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, path, payload) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Creating POSIX SHM provider...");
    // Create SHM provider with default backend
    // NOTE: For extended PosixShmProviderBackend API please check z_posix_shm_provider.rs
    let provider = ShmProviderBuilder::default_backend(1024 * 1024)
        .wait()
        .unwrap();

    println!("Declaring Publisher on '{path}'...");
    let publisher = session.declare_publisher(&path).await.unwrap();

    println!("Press CTRL-C to quit...");
    for idx in 0..u32::MAX {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // We reserve a small space at the beginning of the buffer to include the iteration index
        // of the write. This is simply to have the same format as zn_pub.
        let prefix = format!("[{idx:4}] ");
        let prefix_len = prefix.len();
        let slice_len = prefix_len + payload.len();

        // Allocate SHM buffer
        let mut sbuf = provider
            .alloc(slice_len)
            .with_policy::<BlockOn<GarbageCollect>>()
            .await
            .unwrap();

        sbuf[0..prefix_len].copy_from_slice(prefix.as_bytes());
        sbuf[prefix_len..slice_len].copy_from_slice(payload.as_bytes());

        // Write the data
        println!(
            "Put SHM Data ('{}': '{}')",
            path,
            String::from_utf8_lossy(&sbuf[0..slice_len])
        );
        publisher.put(sbuf).await?;
    }

    Ok(())
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/zenoh-rs-pub")]
    /// The key expression to publish onto.
    key: KeyExpr<'static>,
    #[arg(short, long, default_value = "Pub from Rust SHM!")]
    /// The payload of to publish.
    payload: String,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, String) {
    let args = Args::parse();
    (args.common.into(), args.key, args.payload)
}
