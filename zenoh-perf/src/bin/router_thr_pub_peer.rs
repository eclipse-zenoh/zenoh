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
use async_std::task;
use async_std::sync::Arc;

use rand::RngCore;
use std::time::Duration;

use zenoh_protocol::core::{PeerId, ResKey};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::proto::{Mux, whatami};
use zenoh_protocol::session::{SessionManager, SessionManagerConfig, DummyHandler};
use zenoh_router::routing::broker::Broker;


fn print_usage(bin: String) {
    println!(
"Usage:
    cargo run --release --bin {} <payload size in bytes> [<locator to connect to>]
Example: 
    cargo run --release --bin {} 8100 tcp/127.0.0.1:7447",
        bin, bin
    );
}

fn main() {
    // Enable logging
    env_logger::init();

    // Initialize the Peer Id
    let mut pid = vec![0, 0, 0, 0];
    rand::thread_rng().fill_bytes(&mut pid);

    let mut args = std::env::args();
    // Get exe name
    let bin = args.next().unwrap()
                .split(std::path::MAIN_SEPARATOR).last().unwrap().to_string();
    
    // Get next arg
    let value = if let Some(value) = args.next() {
        value
    } else {
        return print_usage(bin);
    };
    let pl_size: usize = if let Ok(v) = value.parse() {
        v
    } else {
        return print_usage(bin);
    };

    let my_primitives = Arc::new(Mux::new(Arc::new(DummyHandler::new())));    
    let broker = Arc::new(Broker::new());

    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: PeerId{id: pid},
        handler: broker.clone()
    };
    let manager = SessionManager::new(config, None);

    task::block_on(async {
        let mut has_locator = false;
        let attachment = None;
        for locator in args {
            has_locator = true;
            if let Err(_err) =  manager.open_session(&locator.parse().unwrap(), &attachment).await {
                println!("Unable to connect to {}!", locator);
                return
            }
        }

        if !has_locator {
            print_usage(bin);
            return
        }
    
        let primitives = broker.new_primitives(my_primitives).await;

        primitives.resource(1, &"/tp".to_string().into()).await;
        let rid = ResKey::RId(1);
        primitives.publisher(&rid).await;

        // @TODO: Fix writer starvation in the RwLock and remove this sleep
        // Wait for the declare to arrive
        task::sleep(Duration::from_millis(1_000)).await;

        let payload = RBuf::from(vec![0u8; pl_size]);
        loop {
            primitives.data(&rid, true, &None, payload.clone()).await;
        }
    });
}