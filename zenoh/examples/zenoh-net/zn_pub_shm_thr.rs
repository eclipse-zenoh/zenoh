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
use clap::{App, Arg};
#[cfg(feature = "zero-copy")]
use zenoh::net::ResKey::*;
#[cfg(feature = "zero-copy")]
use zenoh::net::*;
#[cfg(feature = "zero-copy")]
use zenoh::Properties;

#[cfg(feature = "zero-copy")]
#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();
    let (config, sm_size, size) = parse_args();

    let z = open(config.into()).await.unwrap();
    let id = z.id().await;
    let mut shm = SharedMemoryManager::new(id, sm_size).unwrap();
    let mut buf = shm.alloc(size).unwrap();
    let bs = unsafe { buf.as_mut_slice() };
    for b in bs {
        *b = rand::random::<u8>();
    }

    let reskey = RId(z
        .declare_resource(&RName("/test/shm/thr".to_string()))
        .await
        .unwrap());
    let _publ = z.declare_publisher(&reskey).await.unwrap();

    loop {
        z.write_ext(
            &reskey,
            buf.clone().into(),
            encoding::DEFAULT,
            data_kind::DEFAULT,
            CongestionControl::Block, // Make sure to not drop messages because of congestion control
        )
        .await
        .unwrap();
    }
}

#[cfg(not(feature = "zero-copy"))]
fn main() {
    println!(
        "Please, enable zero-copy feature by rebuilding as follows:\
            \n\n\t$ cargo build --release --features \"zero-copy\"\n"
    );
}

#[cfg(feature = "zero-copy")]
fn parse_args() -> (Properties, usize, usize) {
    let args = App::new("zenoh-net zero-copy throughput pub example")
        .arg(
            Arg::from_usage("-s, --shared-memory=[MB]  'shared memory size in MBytes'")
                .default_value("32"),
        )
        .arg(Arg::from_usage("-p, --payload=[KB] 'payload in KBytes'").default_value("1"))
        .get_matches();

    let config = Properties::default();
    let sm_size = args
        .value_of("shared-memory")
        .unwrap()
        .parse::<usize>()
        .unwrap()
        * 1024
        * 1024;

    let size = args
        .value_of("shared-memory")
        .unwrap()
        .parse::<usize>()
        .unwrap()
        * 1024;
    (config, sm_size, size)
}
