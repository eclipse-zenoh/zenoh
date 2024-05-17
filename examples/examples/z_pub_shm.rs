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
async fn main() -> Result<(), ZError> {
    // Initiate logging
    zenoh_util::try_init_log_from_env();

    let (mut config, path, value) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_pub_shm` to operate
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

    let publisher = session.declare_publisher(&path).await.unwrap();

    // Create allocation layout for series of similar allocations
    println!("Allocating Shared Memory Buffer...");
    let layout = provider.alloc(1024).into_layout().unwrap();

    println!("Press CTRL-C to quit...");
    for idx in 0..u32::MAX {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // Allocate particular SHM buffer using pre-created layout
        let mut sbuf = layout
            .alloc()
            .with_policy::<BlockOn<GarbageCollect>>()
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
        publisher.put(sbuf).await?;
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
