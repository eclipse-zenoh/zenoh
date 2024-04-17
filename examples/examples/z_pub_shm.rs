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
use zenoh::prelude::r#async::*;
use zenoh_examples::CommonArgs;

const N: usize = 10;
const K: u32 = 3;

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

    println!("Creating Shared Memory Manager...");
    let id = session.zid();
    let mut shm = SharedMemoryManager::make(id.to_string(), N * 1024).unwrap();

    println!("Allocating Shared Memory Buffer...");
    let publisher = session.declare_publisher(&path).res().await.unwrap();

    println!("Press CTRL-C to quit...");
    for idx in 0..(K * N as u32) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut sbuf = match shm.alloc(1024) {
            Ok(buf) => buf,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                println!(
                    "Afer failing allocation the GC collected: {} bytes -- retrying",
                    shm.garbage_collect()
                );
                println!(
                    "Trying to de-fragment memory... De-fragmented {} bytes",
                    shm.defragment()
                );
                shm.alloc(1024).unwrap()
            }
        };

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
        if idx % K == 0 {
            let freed = shm.garbage_collect();
            println!("The Gargabe collector freed {freed} bytes");
            let defrag = shm.defragment();
            println!("De-framented {defrag} bytes");
        }
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
