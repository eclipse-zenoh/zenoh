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
use clap::{App, Arg};
use zenoh::buf::SharedMemoryManager;
use zenoh::prelude::*;
use zenoh::publisher::CongestionControl;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();
    let (config, sm_size, size) = parse_args();

    let z = zenoh::open(config).await.unwrap();
    let id = z.id().await;
    let mut shm = SharedMemoryManager::new(id, sm_size).unwrap();
    let mut buf = shm.alloc(size).unwrap();
    let bs = unsafe { buf.as_mut_slice() };
    for b in bs {
        *b = rand::random::<u8>();
    }

    let key_expr = z.declare_expr("/test/thr").await.unwrap();
    let _publ = z.publishing(&key_expr).await.unwrap();

    loop {
        z.put(&key_expr, buf.clone())
            // Make sure to not drop messages because of congestion control
            .congestion_control(CongestionControl::Block)
            .await
            .unwrap();
    }
}

fn parse_args() -> (Properties, usize, usize) {
    let args = App::new("zenoh shared-memory throughput pub example")
        .arg(
            Arg::from_usage("-s, --shared-memory=[MB]  'shared memory size in MBytes'")
                .default_value("32"),
        )
        .arg(Arg::from_usage(
            "-e, --peer=[LOCATOR]...  'Peer locators used to initiate the zenoh session.'",
        ))
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage(
            "<PAYLOAD_SIZE>           'Sets the size of the payload to publish'",
        ))
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
        .value_of("PAYLOAD_SIZE")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    (config, sm_size, size)
}
