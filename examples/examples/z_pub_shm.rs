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
use zenoh::shm::api::provider::shared_memory_provider::{BlockOn, GarbageCollect};
use zenoh::shm::api::provider::types::AllocAlignment;
use zenoh::{config::Config, shm::api::provider::types::AllocLayout};
use zenoh_examples::CommonArgs;
use zenoh_shm::api::{
    factory::SharedMemoryFactory,
    protocol_implementations::posix::{
        posix_shared_memory_provider_backend::PosixSharedMemoryProviderBackend,
        protocol_id::POSIX_PROTOCOL_ID,
    },
};

const N: usize = 10;
const K: u32 = 3;

#[async_std::main]
async fn main() -> Result<(), zenoh::Error> {
    // Initiate logging
    env_logger::init();

    let (mut config, path, value) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_pub_shm` to operate
    // over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
    // subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
    config.transport.shared_memory.set_enabled(true).unwrap();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Creating Shared Memory Factory...");
    let mut factory = SharedMemoryFactory::builder()
        .provider(POSIX_PROTOCOL_ID, || {
            Ok(Box::new(
                PosixSharedMemoryProviderBackend::builder()
                    .with_size(N * 1024)?
                    .res()?,
            ))
        })
        .unwrap()
        .build();
    println!("Retrieving Shared Memory Provider...");
    let shm = factory.provider(POSIX_PROTOCOL_ID).unwrap();

    println!("Allocating Shared Memory Buffer...");
    let publisher = session.declare_publisher(&path).res().await.unwrap();

    let layout = AllocLayout::new(1024, AllocAlignment::default(), shm).unwrap();

    for idx in 0..(K * N as u32) {
        let mut sbuf = shm
            .alloc()
            .with_policy::<BlockOn<GarbageCollect>>()
            .with_layout(&layout)
            .res_async()
            .await
            .unwrap();

        // We reserve a small space at the beginning of the buffer to include the iteration index
        // of the write. This is simply to have the same format as zn_pub.
        let prefix = format!("[{idx:4}] ");
        let prefix_len = prefix.as_bytes().len();

        // Retrive a mutable slice from the SharedMemoryBuf.
        //
        // This operation is marked unsafe since we cannot guarantee a single mutable reference
        // across multiple processes. Thus if you use it, and you'll inevitable have to use it,
        // you have to keep in mind that if you have multiple process retrieving a mutable slice
        // you may get into concurrent writes. That said, if you have a serial pipeline and
        // the buffer is flowing through the pipeline this will not create any issues.
        //
        // In short, whilst this operation is marked as unsafe, you are safe if you can
        // guarantee that in your application only one process at the time will actually write.
        let slice = unsafe { sbuf.as_mut_slice() };
        let slice_len = prefix_len + value.as_bytes().len();
        slice[0..prefix_len].copy_from_slice(prefix.as_bytes());
        slice[prefix_len..slice_len].copy_from_slice(value.as_bytes());

        // Write the data
        println!(
            "Put SHM Data ('{}': '{}')",
            path,
            String::from_utf8_lossy(&slice[0..slice_len])
        );
        publisher.put(sbuf.clone()).res().await?;

        // Dropping the SharedMemoryBuf means to free it.
        drop(sbuf);
    }

    // Signal the SharedMemoryManager to garbage collect all the freed SharedMemoryBuf.
    let _freed = shm.garbage_collect();

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
