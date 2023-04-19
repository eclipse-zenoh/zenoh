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
use clap::{App, Arg};
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh::publication::CongestionControl;
use zenoh::shm::SharedMemoryManager;

#[async_std::main]
async fn main() {
    // initiate logging
    env_logger::init();
    let (config, sm_size, size) = parse_args();

    let z = zenoh::open(config).res().await.unwrap();
    let id = z.zid();
    let mut shm = SharedMemoryManager::make(id.to_string(), sm_size).unwrap();
    let mut buf = shm.alloc(size).unwrap();
    let bs = unsafe { buf.as_mut_slice() };
    for b in bs {
        *b = rand::random::<u8>();
    }

    let publisher = z.declare_publisher("test/thr")
    // Make sure to not drop messages because of congestion control
    .congestion_control(CongestionControl::Block).res().await.unwrap();

    loop {
        publisher.put(buf.clone()).res().await.unwrap();
    }
}

fn parse_args() -> (Config, usize, usize) {
    let args = App::new("zenoh shared-memory throughput pub example")
        .arg(
            Arg::from_usage("-s, --shared-memory=[MB]  'shared memory size in MBytes'")
                .default_value("32"),
        )
        .arg(Arg::from_usage(
            "-e, --connect=[ENDPOINT]...  'Endpoints to connect to.'",
        ))
        .arg(Arg::from_usage(
            "-c, --config=[FILE]      'A configuration file.'",
        ))
        .arg(Arg::from_usage(
            "<PAYLOAD_SIZE>           'Sets the size of the payload to publish'",
        ))
        .get_matches();

    let config = Config::default();
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
