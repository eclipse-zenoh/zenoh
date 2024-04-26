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
async fn main() -> Result<(), ZError> {
    // Initiate logging
    zenoh_util::try_init_log_from_env();

    let (mut config, path, value) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_pub_shm` to operate
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

    let publisher = session.declare_publisher(&path).res().await.unwrap();

    println!("Allocating Shared Memory Buffer...");
    let layout = shared_memory_provider
        .alloc_layout()
        .size(1024)
        .res()
        .unwrap();

    println!("Press CTRL-C to quit...");
    for idx in 0..u32::MAX {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let mut sbuf = layout
            .alloc()
            .with_policy::<BlockOn<GarbageCollect>>()
            .res_async()
            .await
            .unwrap();

        // We reserve a small space at the beginning of the buffer to include the iteration index
        // of the write. This is simply to have the same format as zn_pub.
        let prefix = format!("[{idx:4}] ");
        let prefix_len = prefix.as_bytes().len();
        let slice_len = prefix_len + value.as_bytes().len();

        sbuf[0..prefix_len].copy_from_slice(prefix.as_bytes());
        sbuf[prefix_len..slice_len].copy_from_slice(value.as_bytes());

        // Write the data
        println!(
            "Put SHM Data ('{}': '{}')",
            path,
            String::from_utf8_lossy(&sbuf[0..slice_len])
        );
        publisher.put(sbuf).res().await?;
    }

    Ok(())
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "demo/example/zenoh-rs-pub")]
    /// The key expression to publish onto.
    path: KeyExpr<'static>,
    #[arg(short, long, default_value = "Pub from SharedMemory Rust!")]
    /// The value of to publish.
    value: String,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>, String) {
    let args = Args::parse();
    (args.common.into(), args.path, args.value)
}
