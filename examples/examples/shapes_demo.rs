//
// Copyright (c) 2024 Atostek Oy
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   Juhana Helovuo <juhana.helovuo@atostek.com>
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
// This is derived from pre-existing Zenoh example progams by ZettaScale Technology,
// and RustDDS source codes by Atostek Oy, according to their open-source licences.

use clap::Parser;
use std::time::Duration;
use zenoh::config::Config;
use zenoh::prelude::r#async::*;
use zenoh_examples::CommonArgs;

use byteorder::{BigEndian, LittleEndian};
use cdr_encoding as cdr;
use serde::{Deserialize, Serialize};
use tokio::join;

// This is an example program to test the interoperability of Zettascale's Zenoh-DDS-bridge
// against various DDS implementations.
//
// Most DDS implementations have a "Shapes Demo" application available.
// These applications can be set up to subscribe and/or publish
// data on geometric shapes, such as squares, circles, and triangles. They can be
// used to test interoperability of various DDS/RTPS implementations.
//
// This program is a Zenoh version of the same. It should be able to talk to DDS
// Shapes Demo programs through the Zenoh-DDS-bridge/plugin.

// Usage:
//
// * Start the Zenoh-DDS-bridge
//
// * Start a DDS Shapes Demo application, e.g. using the RustDDS crate:
//   `cargo run --example=shapes_demo -- -P -t Square`
//
// * Start this program, e.g.
//   `cargo run --example=shapes_demo -- -S -t Square`
//
// Expected result: Zenoh Shapes demo receives what DDS Shapes Demo publishes.
// Startup order of the programs should not matter.
//
// Swap the arguments -P and -S between programs to test the other direction.
//
// Other DDS implementations, e.g. FastDDS, CycloneDDS, or RTI Connext
// are also expected to work with their respective Shapes Demo programs.

// The ShapeType data type is originally from OMG publication
// "The Real-time Publish-Subscribe Protocol DDS Interoperability Wire Protocol (DDSI-RTPSTM) Specification"
// version 2.5 (https://www.omg.org/spec/DDSI-RTPS)
// Section "10.7 Example for User-defined Topic Data"
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct ShapeType {
    color: String,
    x: i32,
    y: i32,
    size: i32,
}

#[tokio::main]
async fn main() {
    // Initiate logging
    zenoh_util::try_init_log_from_env();

    let Args {
        topic,
        color,
        common_args,
        do_publisher,
        do_subscriber,
    } = Args::parse();

    println!("Opening Zenoh session...");
    let config: Config = common_args.into();
    let session = zenoh::open(config).res().await.unwrap();

    println!("Press CTRL-C to quit...");

    // Zenoh-DDS-bridge maps topic "Square" to key expression "Square".
    let key_expr: KeyExpr<'static> = topic.try_into().unwrap();

    if !(do_publisher || do_subscriber) {
        println!("Please specify --publisher or --subscriber to get anything done.");
        std::process::exit(-1);
    }

    join!(
        async {
            if do_publisher {
                println!("Declaring Publisher on '{key_expr}'...");
                let publisher = session.declare_publisher(&key_expr).res().await.unwrap();
                for idx in 2000..i32::MAX {
                    // make up some Shape data
                    let shape = ShapeType {
                        color: color.clone(),
                        x: 20 + idx % 101,
                        y: 20 + idx % 123,
                        size: 32,
                    };
                    println!("Putting Data ('{}': {:?})...", &key_expr, &shape);
                    let mut payload_buffer = vec![0x00, 0x01, 0x00, 0x00];
                    payload_buffer.append(
                        &mut cdr_encoding::to_vec::<ShapeType, LittleEndian>(&shape).unwrap(),
                    );
                    publisher.put(payload_buffer).res().await.unwrap();
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        },
        async {
            if do_subscriber {
                println!("Declaring Subscriber on '{key_expr}'...");
                let subscriber = session.declare_subscriber(&key_expr).res().await.unwrap();
                while let Ok(sample) = subscriber.recv_async().await {
                    // This is an ad-hoc parser for RTPS SerializedPayload
                    // and contained CDR encoding of ShapeType
                    // See RTPS spec v2.5 Section "10 Serialized Payload Representation"
                    // subsections 10.2 and 10.5 to follow what is going on.
                    let ser_payload = Vec::<u8>::try_from(sample.value).unwrap();
                    let shape = if ser_payload.len() < 4 {
                        Err("Too short SerializedPayload.".to_string())
                    } else {
                        let (id_and_opts, value) = ser_payload.split_at(4);
                        match id_and_opts[0..2] {
                            [0x00, 0x01] => {
                                Ok(cdr::from_bytes::<ShapeType, LittleEndian>(value).unwrap().0)
                            }
                            [0x00, 0x00] => {
                                Ok(cdr::from_bytes::<ShapeType, BigEndian>(value).unwrap().0)
                            }
                            ref r => Err(format!("Unknown RepresentationIdentifier {r:?}")),
                        }
                    };
                    println!(
                        "Received {} '{}': {:?}",
                        sample.kind,
                        sample.key_expr.as_str(),
                        shape,
                    );
                }
            }
        }
    );
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[arg(short, long, default_value = "Square")]
    /// Shape topic. Different shapes are distinguished by Topic name.
    /// This is the key expression in the Zenoh translation.
    topic: String,

    #[arg(long, default_value = "GREEN")]
    /// Shape default color.
    color: String,

    #[arg(id = "publisher", short = 'P', long)]
    /// Act as a publisher
    do_publisher: bool,

    #[arg(id = "subscriber", short = 'S', long)]
    /// Act as a subscriber
    do_subscriber: bool,

    #[command(flatten)]
    common_args: CommonArgs,
}
