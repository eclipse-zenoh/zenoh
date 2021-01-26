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
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use rand::RngCore;

use zenoh_protocol::core::{PeerId, ResKey};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::{whatami, WhatAmI, ZenohMessage};
use zenoh_protocol::session::{
    DummySessionEventHandler, SessionEventHandler, SessionHandler, SessionManager, SessionManagerConfig,
};

struct MySH {}

impl MySH {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SessionHandler for MySH {
    async fn new_session(
        &self,
        _whatami: WhatAmI,
        _session: Arc<dyn SessionEventHandler + Send + Sync>,
    ) -> Arc<dyn SessionEventHandler + Send + Sync> {
        Arc::new(DummySessionEventHandler::new())
    }
}

fn print_usage(bin: String) {
    println!(
        "Usage:
    cargo run --release --bin {} <payload size in bytes> <locator to connect to>
Example:
    cargo run --release --bin {} 8100 tcp/127.0.0.1:7447",
        bin, bin
    );
}

fn main() {
    // Enable logging
    env_logger::init();

    // Initialize the Peer Id
    let mut pid = [0u8; PeerId::MAX_SIZE];
    rand::thread_rng().fill_bytes(&mut pid);
    let pid = PeerId::new(1, pid);

    let mut args = std::env::args();
    // Get exe name
    let bin = args
        .next()
        .unwrap()
        .split(std::path::MAIN_SEPARATOR)
        .last()
        .unwrap()
        .to_string();

    // Get next arg
    let value = if let Some(value) = args.next() {
        value
    } else {
        return print_usage(bin);
    };
    let payload: usize = if let Ok(v) = value.parse() {
        v
    } else {
        return print_usage(bin);
    };

    // Get next arg
    let value = if let Some(value) = args.next() {
        value
    } else {
        return print_usage(bin);
    };
    let connect_to: Locator = if let Ok(v) = value.parse() {
        v
    } else {
        return print_usage(bin);
    };

    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: pid,
        handler: Arc::new(MySH::new()),
    };
    let manager = SessionManager::new(config, None);

    let attachment = None;

    // Connect to publisher
    task::block_on(async {
        let session = if let Ok(s) = manager.open_session(&connect_to, &attachment).await {
            println!("Opened session on {}", connect_to);
            s
        } else {
            println!("Failed to open session on {}", connect_to);
            return;
        };

        // Send reliable messages
        let reliable = true;
        let key = ResKey::RName("test".to_string());
        let info = None;
        let payload = RBuf::from(vec![0u8; payload]);
        let reply_context = None;

        let message =
            ZenohMessage::make_data(reliable, key, info, payload, reply_context, attachment);

        loop {
            let res = session.handle_message(message.clone()).await;
            if res.is_err() {
                break;
            }
        }
    });
}
