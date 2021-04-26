//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
#[cfg(feature = "zero-copy")]
use async_std::task::sleep;
#[cfg(feature = "zero-copy")]
use clap::{App, Arg};
#[cfg(feature = "zero-copy")]
use std::time::Duration;
#[cfg(feature = "zero-copy")]
use zenoh::net::*;
#[cfg(feature = "zero-copy")]
use zenoh::Properties;

const N: usize = 10;
const K: u32 = 3;
#[cfg(feature = "zero-copy")]
#[async_std::main]

async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initiate logging
    env_logger::init();

    let (config, path, value) = parse_args();

    println!("Opening session...");
    let session = open(config.into()).await.unwrap();

    println!("Creating Shared Memory Manager...");
    let id = session.id().await;
    let mut shm = SharedMemoryManager::new(id, N * 1024).unwrap();

    println!("Allocating a Shared Memory Buffer...");

    for idx in 1..=(K * N as u32 * 100) {
        println!("{:?}", shm);
        let mut sbuf = match shm.alloc(1024) {
            Some(buf) => buf,
            None => {
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
        let mut prefix = format!("[{:6}] ", 0);
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
        // guarantee that your in applications only one process at the time will actually write.
        let slice = unsafe { sbuf.as_mut_slice() };
        let slice_len = prefix_len + value.as_bytes().len();
        prefix = format!("[{:6}] ", idx);
        slice[0..prefix_len].copy_from_slice(&prefix.as_bytes());

        // Write the data
        println!(
            "=================== Writing SHM Data ('{}': '{}')...",
            path,
            String::from_utf8_lossy(&slice[0..slice_len])
        );
        session
            .write(&path.clone().into(), sbuf.clone().into())
            .await?;
        if idx % K == 0 {
            let freed = shm.garbage_collect();
            println!("The Gargabe collector freed {} bytes", freed);
            let defrag = shm.defragment();
            println!("De-framented {} bytes", defrag);
        }
        // sleep(Duration::from_millis(100)).await;
    }

    // Dropping the SharedMemoryBuf means to free it.
    // drop(sbuf);
    // Signal the SharedMemoryManager to garbage collect all the freed SharedMemoryBuf.
    // let _freed = shm.garbage_collect();

    Ok(())
}

#[cfg(not(feature = "zero-copy"))]
fn main() {
    println!(
        "Please, enable zero-copy feature by rebuilding as follows:\
            \n\n\t$ cargo build --release --features \"zero-copy\"\n"
    );
}

#[cfg(feature = "zero-copy")]
fn parse_args() -> (Properties, String, String) {
    let args = App::new("zenoh-net pub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode (peer by default).")
                .possible_values(&["peer", "client"]),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-l, --listener=[LOCATOR]...   'Locators to listen on.'",
        ))
        .arg(
            Arg::from_usage("-p, --path=[PATH]        'The name of the resource to publish.'")
                .default_value("/demo/example/zenoh-rs-pub"),
        )
        .arg(
            Arg::from_usage("-v, --value=[VALUE]      'The value of the resource to publish.'")
                .default_value("Pub from SharedMemory Rust!"),
        )
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .get_matches();

    let mut config = if let Some(conf_file) = args.value_of("config") {
        Properties::from(std::fs::read_to_string(conf_file).unwrap())
    } else {
        Properties::default()
    };
    for key in ["mode", "peer", "listener"].iter() {
        if let Some(value) = args.values_of(key) {
            config.insert(key.to_string(), value.collect::<Vec<&str>>().join(","));
        }
    }
    if args.is_present("no-multicast-scouting") {
        config.insert("multicast_scouting".to_string(), "false".to_string());
    }

    let path = args.value_of("path").unwrap();
    let value = args.value_of("value").unwrap();

    (config, path.to_string(), value.to_string())
}
