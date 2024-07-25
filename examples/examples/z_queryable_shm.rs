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
    bytes::ZBytes,
    key_expr::KeyExpr,
    prelude::*,
    shm::{
        zshm, BlockOn, GarbageCollect, PosixShmProviderBackend, ShmProviderBuilder,
        POSIX_PROTOCOL_ID,
    },
    Config,
};
use zenoh_examples::CommonArgs;

const N: usize = 10;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::try_init_log_from_env();

    let (mut config, key_expr, payload, complete) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_get_shm` to operate
    // over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
    // subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
    config.transport.shared_memory.set_enabled(true).unwrap();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Creating POSIX SHM provider...");
    // create an SHM backend...
    // NOTE: For extended PosixShmProviderBackend API please check z_posix_shm_provider.rs
    let backend = PosixShmProviderBackend::builder()
        .with_size(N * 1024)
        .unwrap()
        .res()
        .unwrap();
    // ...and an SHM provider
    let provider = ShmProviderBuilder::builder()
        .protocol_id::<POSIX_PROTOCOL_ID>()
        .backend(backend)
        .res();

    println!("Declaring Queryable on '{key_expr}'...");
    let queryable = session
        .declare_queryable(&key_expr)
        .complete(complete)
        .await
        .unwrap();

    println!("Press CTRL-C to quit...");
    while let Ok(query) = queryable.recv_async().await {
        // Print overall query payload information
        match query.payload() {
            Some(payload) => {
                let (payload_type, payload) = handle_bytes(payload);
                print!(
                    ">> [Queryable] Received Query ('{}': '{}') [{}]",
                    query.selector(),
                    payload,
                    payload_type,
                );
            }
            None => {
                print!(">> Received Query '{}'", query.selector());
            }
        };

        // Print attachment information
        if let Some(att) = query.attachment() {
            let (attachment_type, attachment) = handle_bytes(att);
            print!(" ({}: {})", attachment_type, attachment);
        }

        println!();

        // Allocate an SHM buffer
        // NOTE: For allocation API please check z_alloc_shm.rs example
        // NOTE: For buf's API please check z_bytes_shm.rs example
        println!("Allocating Shared Memory Buffer...");
        let mut sbuf = provider
            .alloc(1024)
            .with_policy::<BlockOn<GarbageCollect>>()
            .await
            .unwrap();

        sbuf[0..payload.len()].copy_from_slice(payload.as_bytes());

        println!(
            ">> [Queryable] Responding ('{}': '{}')",
            key_expr.as_str(),
            payload,
        );
        query
            .reply(key_expr.clone(), sbuf)
            .await
            .unwrap_or_else(|e| println!(">> [Queryable ] Error sending reply: {e}"));
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/zenoh-rs-queryable")]
    /// The key expression matching queries to reply to.
    key: KeyExpr<'static>,
    #[arg(short, long, default_value = "Queryable from Rust SHM!")]
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

fn handle_bytes(bytes: &ZBytes) -> (&str, String) {
    // Determine buffer type for indication purpose
    let bytes_type = {
        // if Zenoh is built without SHM support, the only buffer type it can receive is RAW
        #[cfg(not(feature = "shared-memory"))]
        {
            "RAW"
        }

        // if Zenoh is built with SHM support but without SHM API (that is unstable), it can
        // receive buffers of any type, but there is no way to detect the buffer type
        #[cfg(all(feature = "shared-memory", not(feature = "unstable")))]
        {
            "UNKNOWN"
        }

        // if Zenoh is built with SHM support and with SHM API  we can detect the exact buffer type
        #[cfg(all(feature = "shared-memory", feature = "unstable"))]
        match bytes.deserialize::<&zshm>() {
            Ok(_) => "SHM",
            Err(_) => "RAW",
        }
    };

    // In order to indicate the real underlying buffer type the code above is written ^^^
    // Sample is SHM-agnostic: Sample handling code works both with SHM and RAW data transparently.
    // In other words, the common application compiled with "shared-memory" feature will be able to
    // handle incoming SHM data without any changes in the application code.
    //
    // Refer to z_bytes.rs to see how to deserialize different types of message
    let bytes_string = bytes
        .deserialize::<String>()
        .unwrap_or_else(|e| format!("{}", e));

    (bytes_type, bytes_string)
}
