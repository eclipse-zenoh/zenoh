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

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh_util::try_init_log_from_env();
    let (mut config, sm_size, size) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_pub_shm_thr` to operate
    // over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
    // subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
    config.transport.shared_memory.set_enabled(true).unwrap();

    let z = zenoh::open(config).await.unwrap();

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
        let provider_layout = MemoryLayout::new(sm_size, provider_alignment).unwrap();

        PosixSharedMemoryProviderBackend::builder()
            .with_layout(provider_layout)
            .res()
            .unwrap()
    };

    // Construct an SHM provider for particular backend and POSIX_PROTOCOL_ID
    let shared_memory_provider = SharedMemoryProviderBuilder::builder()
        .protocol_id::<POSIX_PROTOCOL_ID>()
        .backend(backend)
        .res();

    let mut buf = shared_memory_provider
        .alloc_layout()
        .size(size)
        .res()
        .unwrap()
        .alloc()
        .res()
        .unwrap();

    for b in buf.as_mut() {
        *b = rand::random::<u8>();
    }

    let publisher = z
        .declare_publisher("test/thr")
        // Make sure to not drop messages because of congestion control
        .congestion_control(CongestionControl::Block)
        .await
        .unwrap();

    let buf: ZSlice = buf.into();

    println!("Press CTRL-C to quit...");
    loop {
        publisher.put(buf.clone()).await.unwrap();
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "32")]
    /// shared memory size in MBytes.
    shared_memory: usize,
    /// Sets the size of the payload to publish.
    payload_size: usize,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, usize, usize) {
    let args = Args::parse();
    let sm_size = args.shared_memory * 1024 * 1024;
    let size = args.payload_size;
    (args.common.into(), sm_size, size)
}
