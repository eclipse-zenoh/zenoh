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
#[cfg(feature = "unstable")]
use zenoh::sample::SampleKind;
use zenoh::session::ZenohId;
use zenoh_examples::CommonArgs;

#[tokio::main]
async fn main() {
    // initiate logging
    zenoh::init_log_from_env_or("error");

    let config = parse_args();

    println!("Opening session...");
    let session = zenoh::open(config).await.unwrap();

    let info = session.info();
    println!("zid: {}", info.zid().await);
    println!(
        "routers zid: {:?}",
        info.routers_zid().await.collect::<Vec<ZenohId>>()
    );
    println!(
        "peers zid: {:?}",
        info.peers_zid().await.collect::<Vec<ZenohId>>()
    );

    // Display current transports
    #[cfg(feature = "unstable")]
    {
        println!("\ntransports:");
        let transports = info.transports().await;
        for transport in transports {
            println!("{:?}", transport);
        }

        // Display current links
        println!("\nlinks:");
        let links = info.links().await;
        for link in links {
            println!("{:?}", link);
        }

        println!("\nConnectivity events (Press CTRL-C to quit):");

        // Set up transport events listener (using default handler)
        let transport_events = info
            .transport_events_listener()
            .history(false) // Don't repeat transports we already printed
            .await;

        // Set up link events listener (using default handler)
        let link_events = info
            .link_events_listener()
            .history(false) // Don't repeat links we already printed
            .await;

        // Listen for events until CTRL-C
        loop {
            tokio::select! {
                Ok(event) = transport_events.recv_async() => {
                    match event.kind() {
                        SampleKind::Put => {
                            println!("[Transport Event] Opened: {:?}", event.transport());
                        }
                        SampleKind::Delete => {
                            println!("[Transport Event] Closed: {:?}", event.transport());
                        }
                    }
                }
                Ok(event) = link_events.recv_async() => {
                    match event.kind() {
                        SampleKind::Put => {
                            println!("[Link Event] Added: {:?}", event.link());
                        }
                        SampleKind::Delete => {
                            println!("[Link Event] Removed: {:?}", event.link());
                        }
                    }
                }
            }
        }
    }
}

#[derive(clap::Parser, Clone, PartialEq, Eq, Hash, Debug)]
struct Args {
    #[command(flatten)]
    common: CommonArgs,
}

fn parse_args() -> zenoh::Config {
    let args = Args::parse();
    args.common.into()
}
