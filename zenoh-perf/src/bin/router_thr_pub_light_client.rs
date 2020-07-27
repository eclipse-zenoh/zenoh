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
use async_std::sync::{Arc, Mutex};
use async_std::task;
use async_trait::async_trait;

use rand::RngCore;
use std::time::Duration;

use zenoh_protocol::core::{PeerId, ResKey};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::{whatami, Mux, Primitives, WhatAmI};
use zenoh_protocol::session::{
    DummyHandler, SessionEventHandler, SessionHandler, SessionManager, SessionManagerConfig,
};

struct LightSessionHandler {
    pub handler: Mutex<Option<Arc<dyn SessionEventHandler + Send + Sync>>>,
}

impl LightSessionHandler {
    pub fn new() -> LightSessionHandler {
        LightSessionHandler {
            handler: Mutex::new(None),
        }
    }
}

#[async_trait]
impl SessionHandler for LightSessionHandler {
    async fn new_session(
        &self,
        _whatami: WhatAmI,
        session: Arc<dyn SessionEventHandler + Send + Sync>,
    ) -> Arc<dyn SessionEventHandler + Send + Sync> {
        *self.handler.lock().await = Some(session);
        Arc::new(DummyHandler::new())
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
    let mut pid = vec![0, 0, 0, 0];
    rand::thread_rng().fill_bytes(&mut pid);

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
    let pl_size: usize = if let Ok(v) = value.parse() {
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

    let session_handler = Arc::new(LightSessionHandler::new());
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: PeerId { id: pid.clone() },
        handler: session_handler.clone(),
    };
    let manager = SessionManager::new(config, None);

    task::block_on(async {
        let attachment = None;
        if let Err(_err) = manager.open_session(&connect_to, &attachment).await {
            println!("Unable to connect to {}!", connect_to);
            return;
        }

        let primitives = Mux::new(
            session_handler
                .handler
                .lock()
                .await
                .as_ref()
                .unwrap()
                .clone(),
        );

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
