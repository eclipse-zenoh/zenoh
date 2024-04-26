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
use zenoh::shm::slice::zsliceshm::zsliceshm;
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh_util::try_init_log_from_env();

    let (mut config, key_expr) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_pub_shm` to operate
    // over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
    // subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
    config.transport.shared_memory.set_enabled(true).unwrap();

    println!("Opening session...");
    let session = zenoh::open(config).res().await.unwrap();

    println!("Declaring Subscriber on '{}'...", &key_expr);
    let subscriber = session.declare_subscriber(&key_expr).res().await.unwrap();

    println!("Press CTRL-C to quit...");
    // while let Ok(sample) = subscriber.recv_async().await {
    //     match sample.payload().deserialize::<&zsliceshm>() {
    //         Ok(payload) => println!(
    //             ">> [Subscriber] Received {} ('{}': '{:02x?}')",
    //             sample.kind(),
    //             sample.key_expr().as_str(),
    //             payload
    //         ),
    //         Err(e) => {
    //             println!(">> [Subscriber] Not a SharedMemoryBuf: {:?}", e);
    //         }
    //     }
    // }

    // // Try to get a mutable reference to the SHM buffer. If this subscriber is the only subscriber
    // // holding a reference to the SHM buffer, then it will be able to get a mutable reference to it.
    // // With the mutable reference at hand, it's possible to mutate in place the SHM buffer content.
    //
    use zenoh::shm::slice::zsliceshmmut::zsliceshmmut;

    while let Ok(mut sample) = subscriber.recv_async().await {
        let kind = sample.kind();
        let key_expr = sample.key_expr().to_string();
        match sample.payload_mut().deserialize_mut::<&mut zsliceshmmut>() {
            Ok(payload) => println!(
                ">> [Subscriber] Received {} ('{}': '{:02x?}')",
                kind, key_expr, payload
            ),
            Err(e) => {
                println!(">> [Subscriber] Not a SharedMemoryBuf: {:?}", e);
            }
        }
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct SubArgs {
    #[arg(short, long, default_value = "demo/example/**")]
    /// The Key Expression to subscribe to.
    key: KeyExpr<'static>,
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> (Config, KeyExpr<'static>) {
    let args = SubArgs::parse();
    (args.common.into(), args.key)
}
