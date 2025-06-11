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
use zenoh::{bytes::ZBytes, qos::CongestionControl, shm::ShmProviderBuilder, Config, Wait};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");
    let (config, sm_size, size) = parse_args();

    let z = zenoh::open(config).await.unwrap();

    // Create SHM provider with default backend
    // NOTE: For extended PosixShmProviderBackend API please check z_posix_shm_provider.rs
    let provider = ShmProviderBuilder::default_backend(sm_size).wait().unwrap();

    // Allocate an SHM buffer
    // NOTE: For allocation API please check z_alloc_shm.rs example
    // NOTE: For buf's API please check z_bytes_shm.rs example
    let mut buf = provider.alloc(size).wait().unwrap();

    for b in buf.as_mut() {
        *b = rand::random::<u8>();
    }

    let publisher = z
        .declare_publisher("test/thr")
        // Make sure to not drop messages because of congestion control
        .congestion_control(CongestionControl::Block)
        .await
        .unwrap();

    // convert ZShmMut into ZBytes as ZShmMut does not support Clone
    let buf: ZBytes = buf.into();

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
