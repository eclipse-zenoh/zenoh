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
#[cfg(all(feature = "shared-memory", feature = "unstable"))]
use zenoh::shm::zshm;
use zenoh::{bytes::ZBytes, config::Config, key_expr::KeyExpr, prelude::*};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh::try_init_log_from_env();

    let (mut config, key_expr) = parse_args();

    // A probing procedure for shared memory is performed upon session opening. To enable `z_pub_shm` to operate
    // over shared memory (and to not fallback on network mode), shared memory needs to be enabled also on the
    // subscriber side. By doing so, the probing procedure will succeed and shared memory will operate as expected.
    config.transport.shared_memory.set_enabled(true).unwrap();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring Subscriber on '{}'...", &key_expr);
    let subscriber = session.declare_subscriber(&key_expr).await.unwrap();

    println!("Press CTRL-C to quit...");
    while let Ok(sample) = subscriber.recv_async().await {
        // Print overall payload information
        let (payload_type, payload) = handle_bytes(sample.payload());
        print!(
            ">> [Subscriber] Received {} ('{}': '{}') [{}] ",
            sample.kind(),
            sample.key_expr().as_str(),
            payload,
            payload_type,
        );

        // Print attachment information
        if let Some(att) = sample.attachment() {
            let (attachment_type, attachment) = handle_bytes(att);
            print!(" ({}: {})", attachment_type, attachment);
        }

        println!();
    }

    // // Try to get a mutable reference to the SHM buffer. If this subscriber is the only subscriber
    // // holding a reference to the SHM buffer, then it will be able to get a mutable reference to it.
    // // With the mutable reference at hand, it's possible to mutate in place the SHM buffer content.
    //
    // use zenoh::shm::zshmmut;

    // while let Ok(mut sample) = subscriber.recv_async().await {
    //     let kind = sample.kind();
    //     let key_expr = sample.key_expr().to_string();
    //     match sample.payload_mut().deserialize_mut::<&mut zshmmut>() {
    //         Ok(payload) => println!(
    //             ">> [Subscriber] Received {} ('{}': '{:02x?}')",
    //             kind, key_expr, payload
    //         ),
    //         Err(e) => {
    //             println!(">> [Subscriber] Not a ShmBufInner: {:?}", e);
    //         }
    //     }
    // }
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

fn handle_bytes(bytes: &ZBytes) -> (&str, String) {
    // Determine buffer type for indication purpose
    let bytes_type = {
        // if Zenoh is built without SHM support, the only buffer type it can receive is RAW
        #[cfg(not(feature = "shared-memory"))]
        {
            "RAW"
        }

        // if Zenoh is built with SHM support but without SHM API (that is unstable), it can
        // receive buffers of any type, but there is no way to detect the buffer type
        #[cfg(all(feature = "shared-memory", not(feature = "unstable")))]
        {
            "UNKNOWN"
        }

        // if Zenoh is built with SHM support and with SHM API  we can detect the exact buffer type
        #[cfg(all(feature = "shared-memory", feature = "unstable"))]
        match bytes.deserialize::<&zshm>() {
            Ok(_) => "SHM",
            Err(_) => "RAW",
        }
    };

    // In order to indicate the real underlying buffer type the code above is written ^^^
    // Sample is SHM-agnostic: Sample handling code works both with SHM and RAW data transparently.
    // In other words, the common application compiled with "shared-memory" feature will be able to
    // handle incoming SHM data without any changes in the application code.
    //
    // Refer to z_bytes.rs to see how to deserialize different types of message
    let bytes_string = bytes
        .deserialize::<String>()
        .unwrap_or_else(|e| format!("{}", e));

    (bytes_type, bytes_string)
}
