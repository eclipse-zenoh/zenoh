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
use async_std::future;
use async_std::sync::{Arc, Mutex};
use async_std::task;
use async_trait::async_trait;
use rand::RngCore;
use slab::Slab;

use zenoh_protocol::core::PeerId;
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::{whatami, WhatAmI, ZenohMessage};
use zenoh_protocol::session::{
    SessionEventHandler, SessionHandler, SessionManager, SessionManagerConfig,
};
use zenoh_util::core::ZResult;

type Table = Arc<Mutex<Slab<Arc<dyn SessionEventHandler + Send + Sync>>>>;

// Session Handler for the peer
struct MySH {
    table: Table,
}

impl MySH {
    fn new() -> Self {
        Self {
            table: Arc::new(Mutex::new(Slab::new())),
        }
    }
}

#[async_trait]
impl SessionHandler for MySH {
    async fn new_session(
        &self,
        _whatami: WhatAmI,
        session: Arc<dyn SessionEventHandler + Send + Sync>,
    ) -> Arc<dyn SessionEventHandler + Send + Sync> {
        let index = self.table.lock().await.insert(session);
        Arc::new(MyMH::new(self.table.clone(), index))
    }
}

// Message Handler for the peer
struct MyMH {
    table: Table,
    index: usize,
}

impl MyMH {
    fn new(table: Table, index: usize) -> Self {
        Self { table, index }
    }
}

#[async_trait]
impl SessionEventHandler for MyMH {
    async fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        for (i, e) in self.table.lock().await.iter() {
            if i != self.index {
                let _ = e.handle_message(message.clone()).await;
            }
        }
        Ok(())
    }

    async fn close(&self) {
        self.table.lock().await.remove(self.index);
    }
}

fn print_usage(bin: String) {
    println!(
        "Usage:
    cargo run --release --bin {} <locator to listen on>
Example:
    cargo run --release --bin {} tcp/127.0.0.1:7447",
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
    let listen_on: Locator = if let Ok(v) = value.parse() {
        v
    } else {
        return print_usage(bin);
    };

    // Create the session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: pid,
        handler: Arc::new(MySH::new()),
    };
    let manager = SessionManager::new(config, None);

    // Connect to publisher
    task::block_on(async {
        if manager.add_locator(&listen_on).await.is_ok() {
            println!("Listening on {}", listen_on);
        } else {
            println!("Failed to listen on {}", listen_on);
            return;
        };
        // Stop forever
        future::pending::<()>().await;
    });
}
