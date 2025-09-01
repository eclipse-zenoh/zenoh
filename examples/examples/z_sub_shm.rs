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
use std::borrow::Cow;

use clap::Parser;
#[cfg(all(feature = "shared-memory", feature = "unstable"))]
use zenoh::shm::zshmmut;
use zenoh::{bytes::ZBytes, config::Config, key_expr::KeyExpr};
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh::init_log_from_env_or("error");

    let (config, key_expr) = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    println!("Declaring Subscriber on '{}'...", &key_expr);
    let subscriber = session.declare_subscriber(&key_expr).await.unwrap();

    println!("Press CTRL-C to quit...");
    while let Ok(mut sample) = subscriber.recv_async().await {
        let kind = sample.kind();
        let key_str = sample.key_expr().as_str().to_owned();

        // Print overall payload information
        let (payload_type, payload) = handle_bytes(sample.payload_mut());
        print!(">> [Subscriber] Received {kind} ('{key_str}': '{payload}') [{payload_type}] ",);

        // Print attachment information
        if let Some(att) = sample.attachment_mut() {
            let (attachment_type, attachment) = handle_bytes(att);
            print!(" ({attachment_type}: {attachment})");
        }

        println!();
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

fn handle_bytes(bytes: &mut ZBytes) -> (&str, Cow<'_, str>) {
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

        // if Zenoh is built with SHM support and with SHM API we can detect the exact buffer type
        #[cfg(all(feature = "shared-memory", feature = "unstable"))]
        match bytes.as_shm_mut() {
            // try to mutate SHM buffer to get it's mutability property
            Some(shm) => match <&mut zshmmut>::try_from(shm) {
                Ok(_shm_mut) => "SHM (MUT)",
                Err(_) => "SHM (IMMUT)",
            },
            None => "RAW",
        }
    };

    // In order to indicate the real underlying buffer type the code above is written ^^^
    // Sample is SHM-agnostic: Sample handling code works both with SHM and RAW data transparently.
    // In other words, the common application compiled with "shared-memory" feature will be able to
    // handle incoming SHM data without any changes in the application code.
    //
    // Refer to z_bytes.rs to see how to deserialize different types of message
    let bytes_string = bytes
        .try_to_string()
        .unwrap_or_else(|e| e.to_string().into());

    (bytes_type, bytes_string)
}
