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
use std::collections::HashMap;
use std::time::{Duration, Instant};
use zenoh_protocol::core::{whatami, CongestionControl, PeerId, Reliability, ResKey};
use zenoh_protocol::io::{RBuf, WBuf};
use zenoh_protocol::link::{Link, Locator};
use zenoh_protocol::proto::{Data, ZenohBody, ZenohMessage};
use zenoh_protocol::session::{
    Session, SessionEventHandler, SessionHandler, SessionManager, SessionManagerConfig,
};
use zenoh_util::core::ZResult;

// Session Handler for the peer
struct MySH {
    pending: Arc<Mutex<HashMap<u64, Instant>>>,
}

impl MySH {
    fn new(pending: Arc<Mutex<HashMap<u64, Instant>>>) -> Self {
        Self { pending }
    }
}

#[async_trait]
impl SessionHandler for MySH {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(MyMH::new(self.pending.clone())))
    }
}

// Message Handler for the peer
struct MyMH {
    pending: Arc<Mutex<HashMap<u64, Instant>>>,
}

impl MyMH {
    fn new(pending: Arc<Mutex<HashMap<u64, Instant>>>) -> Self {
        Self { pending }
    }
}

#[async_trait]
impl SessionEventHandler for MyMH {
    async fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        match message.body {
            ZenohBody::Data(Data { mut payload, .. }) => {
                let mut count_bytes = [0u8; 8];
                payload.read_bytes(&mut count_bytes).unwrap();
                let count = u64::from_le_bytes(count_bytes);
                let instant = self.pending.lock().await.remove(&count).unwrap();
                println!(
                    "{} bytes: seq={} time={:?}",
                    payload.len(),
                    count,
                    instant.elapsed()
                );
            }
            _ => panic!("Invalid message"),
        }
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}
    async fn del_link(&self, _link: Link) {}
    async fn closing(&self) {}
    async fn closed(&self) {}
}

fn print_usage(bin: String) {
    println!(
        "Usage:
    cargo run --release --bin {} <locator to connect to> <payload size in bytes> <interval in secs>
Example:
    cargo run --release --bin {} tcp/127.0.0.1:7447 64 1",
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

    let pending: Arc<Mutex<HashMap<u64, Instant>>> = Arc::new(Mutex::new(HashMap::new()));
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: pid,
        handler: Arc::new(MySH::new(pending.clone())),
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

    // Get next arg
    let value = if let Some(value) = args.next() {
        value
    } else {
        return print_usage(bin);
    };
    let size: usize = if let Ok(v) = value.parse() {
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
    let interval: f64 = if let Ok(v) = value.parse() {
        v
    } else {
        return print_usage(bin);
    };

    // Connect to publisher
    task::block_on(async {
        let attachment = None;
        let session = match manager.open_session(&connect_to, &attachment).await {
            Ok(s) => s,
            Err(e) => {
                println!("Failed to open session on {}: {}", connect_to, e);
                return;
            }
        };

        let payload = vec![0u8; size - 8];
        let mut count: u64 = 0;
        loop {
            // Create and send the message
            let reliability = Reliability::Reliable;
            let congestion_control = CongestionControl::Block;
            let key = ResKey::RName("/test/ping".to_string());
            let info = None;

            let mut data: WBuf = WBuf::new(size, true);
            let count_bytes: [u8; 8] = count.to_le_bytes();
            data.write_bytes(&count_bytes);
            data.write_bytes(&payload);
            let data: RBuf = data.into();

            let reply_context = None;
            let attachment = None;

            let message = ZenohMessage::make_data(
                key,
                data,
                reliability,
                congestion_control,
                info,
                reply_context,
                attachment,
            );

            // Insert the pending ping
            pending.lock().await.insert(count, Instant::now());

            session.handle_message(message.clone()).await.unwrap();

            task::sleep(Duration::from_secs_f64(interval)).await;
            count += 1;
        }
    });
}
