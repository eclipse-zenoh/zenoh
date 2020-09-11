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
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use rand::RngCore;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use zenoh_protocol::core::PeerId;
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::{whatami, ZenohMessage};
use zenoh_protocol::session::{
    SessionEventHandler, SessionHandler, SessionManager, SessionManagerConfig,
};
use zenoh_util::core::ZResult;

// Session Handler for the peer
struct MySH {
    counter: Arc<AtomicUsize>,
    active: AtomicBool,
}

impl MySH {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        Self {
            counter,
            active: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl SessionHandler for MySH {
    async fn new_session(
        &self,
        _whatami: whatami::Type,
        _session: Arc<dyn SessionEventHandler + Send + Sync>,
    ) -> Arc<dyn SessionEventHandler + Send + Sync> {
        if !self.active.swap(true, Ordering::Acquire) {
            let count = self.counter.clone();
            task::spawn(async move {
                loop {
                    task::sleep(Duration::from_secs(1)).await;
                    let c = count.swap(0, Ordering::Relaxed);
                    println!("{} msg/s", c);
                }
            });
        }
        Arc::new(MyMH::new(self.counter.clone()))
    }
}

// Message Handler for the peer
struct MyMH {
    counter: Arc<AtomicUsize>,
}

impl MyMH {
    fn new(counter: Arc<AtomicUsize>) -> Self {
        Self { counter }
    }
}

#[async_trait]
impl SessionEventHandler for MyMH {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        self.counter.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn close(&self) {
        std::process::exit(-1);
    }
}

fn print_usage(bin: String) {
    println!(
        "Usage:
    cargo run --release --bin {} <locator to connect to>
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

    let count = Arc::new(AtomicUsize::new(0));

    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: pid,
        handler: Arc::new(MySH::new(count)),
    };
    let manager = SessionManager::new(config, None);

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
    let connect_to: Locator = if let Ok(v) = value.parse() {
        v
    } else {
        return print_usage(bin);
    };

    let attachment = None;

    // Connect to publisher
    task::block_on(async {
        if manager.open_session(&connect_to, &attachment).await.is_ok() {
            println!("Opened session on {}", connect_to);
        } else {
            println!("Failed to open session on {}", connect_to);
            return;
        };

        // Stop forever
        future::pending::<()>().await;
    });
}
