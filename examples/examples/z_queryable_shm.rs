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
use zenoh::prelude::*;
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
    let session = zenoh::open(config).await.unwrap();

    println!("Creating POSIX SHM provider...");
    // create an SHM backend...
    // NOTE: For extended PosixSharedMemoryProviderBackend API please check z_posix_shm_provider.rs
    let backend = PosixSharedMemoryProviderBackend::builder()
        .with_size(N * 1024)
        .unwrap()
        .res()
        .unwrap();
    // ...and an SHM provider
    let provider = SharedMemoryProviderBuilder::builder()
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
        print!(
            ">> [Queryable] Received Query '{}' ('{}'",
            query.selector(),
            query.key_expr().as_str(),
        );
        if let Some(payload) = query.payload() {
            match payload.deserialize::<&zshm>() {
                Ok(payload) => print!(": '{}'", String::from_utf8_lossy(payload)),
                Err(e) => print!(": 'Not a SharedMemoryBuf: {:?}'", e),
            }
        }
        println!(")");

        // Allocate an SHM buffer
        // NOTE: For allocation API please check z_alloc_shm.rs example
        // NOTE: For buf's API please check z_bytes_shm.rs example
        println!("Allocating Shared Memory Buffer...");
        let mut sbuf = provider
            .alloc_layout()
            .size(1024)
            .res()
            .unwrap()
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
