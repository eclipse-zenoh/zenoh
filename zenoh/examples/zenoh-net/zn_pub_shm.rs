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

    println!("Allocating a Shared Memory Buffer...");
    // Request a shared memory buffer to the SharedMemoryManager
    // NOTE: this operation does not allocate memory. It assisgnes
    //       an already allocated memory region to the requester.
    let mut sbuf = shm.alloc(1_024).unwrap();

    // We reserve a small space at the beginning of the buffer to include the iteration index
    // of the write. This is simply to have the same format as zn_pub.
    let mut prefix = format!("[{:4}] ", 0);
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
    //
    // In case of this example, we are the only one that writes on the SharedMemoryBuf, so
    // we are actually safe.
    let slice = unsafe { sbuf.as_mut_slice() };
    let slice_len = prefix_len + value.as_bytes().len();
    slice[prefix_len..slice_len].copy_from_slice(&value.as_bytes());
    drop(slice);

    for idx in 0..std::u32::MAX {
        sleep(Duration::from_secs(1)).await;
        // Update the prefix in the SharedMemoryBuf
        let slice = unsafe { sbuf.as_mut_slice() };
        prefix = format!("[{:4}] ", idx);
        slice[0..prefix_len].copy_from_slice(&prefix.as_bytes());

        // Write the data
        println!(
            "Writing SHM Data ('{}': '{}')...",
            path,
            String::from_utf8_lossy(&slice[0..slice_len])
        );
        session
            .write(&path.clone().into(), sbuf.clone().into())
            .await?;
    }

    // Dropping the SharedMemoryBuf means to free it.
    drop(sbuf);
    // Signal the SharedMemoryManager to garbage collect all the freed SharedMemoryBuf.
    let _freed = shm.garbage_collect();

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
