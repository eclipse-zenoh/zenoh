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
use zenoh::prelude::r#async::*;
use zenoh_examples::CommonArgs;

const N: usize = 10;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh_util::try_init_log_from_env();

    let (mut config, key_expr, value, complete) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_get_shm` to operate
    // over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
    // subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
    config.transport.shared_memory.set_enabled(true).unwrap();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Creating POSIX SHM backend...");
    // Construct an SHM backend
    let backend = {
        // NOTE: code in this block is a specific PosixSharedMemoryProviderBackend API.
        // The initialisation of SHM backend is completely backend-specific and user is free to do
        // anything reasonable here. This code is execuated at the provider's first use

        // Alignment for POSIX SHM provider
        // All allocations will be aligned corresponding to this alignment -
        // that means that the provider will be able to satisfy allocation layouts
        // with alignment <= provider_alignment
        let provider_alignment = AllocAlignment::default();

        // Create layout for POSIX Provider's memory
        let provider_layout = MemoryLayout::new(N * 1024, provider_alignment).unwrap();

        PosixSharedMemoryProviderBackend::builder()
            .with_layout(provider_layout)
            .res()
            .unwrap()
    };

    println!("Creating SHM Provider with POSIX backend...");
    // Construct an SHM provider for particular backend and POSIX_PROTOCOL_ID
    let shared_memory_provider = SharedMemoryProviderBuilder::builder()
        .protocol_id::<POSIX_PROTOCOL_ID>()
        .backend(backend)
        .res();

    println!("Declaring Queryable on '{key_expr}'...");
    let queryable = session
        .declare_queryable(&key_expr)
        .complete(complete)
        .res()
        .await
        .unwrap();

    println!("Press CTRL-C to quit...");
    while let Ok(query) = queryable.recv_async().await {
        print!(
            ">> [Queryable] Received Query '{}' ('{}'",
            query.selector(),
            query.key_expr().as_str(),
        );
        if let Some(payload) = query.payload() {
            match payload.deserialize::<&zsliceshm>() {
                Ok(payload) => print!(": '{}'", String::from_utf8_lossy(payload)),
                Err(e) => print!(": 'Not a SharedMemoryBuf: {:?}'", e),
            }
        }
        println!(")");

        println!("Allocating Shared Memory Buffer...");
        let layout = shared_memory_provider
            .alloc_layout()
            .size(1024)
            .res()
            .unwrap();

        let mut sbuf = layout
            .alloc()
            .with_policy::<BlockOn<GarbageCollect>>()
            .res_async()
            .await
            .unwrap();

        sbuf[0..value.len()].copy_from_slice(value.as_bytes());

        println!(
            ">> [Queryable] Responding ('{}': '{}')",
            key_expr.as_str(),
            value,
        );
        query
            .reply(key_expr.clone(), sbuf)
            .res()
            .await
            .unwrap_or_else(|e| println!(">> [Queryable ] Error sending reply: {e}"));
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/zenoh-rs-queryable")]
    /// The key expression matching queries to reply to.
    key: KeyExpr<'static>,
    #[arg(short, long, default_value = "Queryable from SharedMemory Rust!")]
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
