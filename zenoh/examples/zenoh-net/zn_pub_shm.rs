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
use async_std::task::sleep;
use clap::{App, Arg};
use std::time::Duration;
#[cfg(feature = "zero-copy")]
use zenoh::net::*;
#[cfg(feature = "zero-copy")]
use zenoh::Properties;

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
    let mut shm = SharedMemoryManager::new(id, 8_192).unwrap();
    println!("{:?}", &shm);

    for idx in 0..std::u32::MAX {
        sleep(Duration::from_secs(1)).await;
        // Request a shared memory buffer to the SharedMemoryManager
        // NOTE: this operation does not allocate memory. It assisgnes
        //       an already allocated memory region to the requester.
        let mut sbuf = shm.alloc(1_024).unwrap();
        let slice = unsafe { sbuf.as_mut_slice() };
        let mut start = 0;
        // Fill the content in the SharedMemoryBuf
        let count = format!("[{:4}] ", idx);
        let size = count.as_bytes().len();
        slice[start..start + size].copy_from_slice(&count.as_bytes());
        start += size;

        let size = value.as_bytes().len();
        slice[start..start + size].copy_from_slice(&value.as_bytes());
        start += size;

        // Write the data
        println!(
            "Writing SHM Data ('{}': '{}')...",
            path,
            String::from_utf8_lossy(&slice[0..start])
        );
        session.write(&path.clone().into(), sbuf.into()).await?;

        // Signal the SharedMemoryManager to mark as free the region
        // of memory previously assigned.
        let _freed = shm.garbage_collect();
    }

    Ok(())
}

#[cfg(not(feature = "zero-copy"))]
fn main() {
    println!(
        "Please, enable zero-copy feature by rebuilding as follows:\
            \n\n\t$ cargo build --release --features \"zero-copy\"\n"
    );
}

fn parse_args() -> (Properties, String, String) {
    let args = App::new("zenoh-net pub example")
        .arg(
            Arg::from_usage("-m, --mode=[MODE] 'The zenoh session mode.")
                .possible_values(&["peer", "client"])
                .default_value("peer"),
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
