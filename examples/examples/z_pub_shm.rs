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
use async_std::task::sleep;
use clap::{App, Arg};
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh::shm::SharedMemoryManager;

const N: usize = 10;
const K: u32 = 3;

#[async_std::main]
async fn main() -> Result<(), zenoh::Error> {
    // Initiate logging
    env_logger::init();

    let (config, path, value) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Creating Shared Memory Manager...");
    let id = session.zid();
    let mut shm = SharedMemoryManager::make(id.to_string(), N * 1024).unwrap();

    println!("Allocating Shared Memory Buffer...");
    let publisher = session.declare_publisher(&path).res().await.unwrap();

    for idx in 0..(K * N as u32) {
        let mut sbuf = match shm.alloc(1024) {
            Ok(buf) => buf,
            Err(_) => {
                sleep(Duration::from_millis(100)).await;
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
        // sleep(Duration::from_millis(100)).await;
        // Dropping the SharedMemoryBuf means to free it.
        drop(sbuf);
    }

    // Signal the SharedMemoryManager to garbage collect all the freed SharedMemoryBuf.
    let _freed = shm.garbage_collect();

    Ok(())
}

fn parse_args() -> (Config, String, String) {
    let args = App::new("zenoh shared-memory pub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --connect=[ENDPOINT]...  'Endpoints to connect to.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listen=[ENDPOINT]...   'Endpoints to listen on.'",
        ))
        .arg(
            Arg::from_usage("-p, --path=[PATH]        'The key expression to publish onto.'")
                .default_value("demo/example/zenoh-rs-pub"),
        )
        .arg(
            Arg::from_usage("-v, --value=[VALUE]      'The value of to publish.'")
                .default_value("Pub from SharedMemory Rust!"),
        )
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage(
            "--no-multicast-scouting 'Disable the multicast-based scouting mechanism.'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Config::from_file(conf_file).unwrap()
    } else {
        Config::default()
    };
    if let Some(Ok(mode)) = args.value_of("mode").map(|mode| mode.parse()) {
        config.set_mode(Some(mode)).unwrap();
    }
    if let Some(values) = args.values_of("connect") {
        config
            .connect
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if let Some(values) = args.values_of("listen") {
        config
            .listen
            .endpoints
            .extend(values.map(|v| v.parse().unwrap()))
    }
    if args.is_present("no-multicast-scouting") {
        config.scouting.multicast.set_enabled(Some(false)).unwrap();
    }

    let path = args.value_of("path").unwrap();
    let value = args.value_of("value").unwrap();

    (config, path.to_string(), value.to_string())
}
