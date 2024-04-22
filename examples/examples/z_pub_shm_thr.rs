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
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh::publication::CongestionControl;
use zenoh::shm::SharedMemoryManager;
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh_util::try_init_log_from_env();
    let (mut config, sm_size, size) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_pub_shm_thr` to operate
    // over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
    // subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
    config.transport.shared_memory.set_enabled(true).unwrap();

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

    println!("Press CTRL-C to quit...");
    loop {
        publisher.put(buf.clone()).res().await.unwrap();
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
