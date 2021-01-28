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
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zenoh_protocol::core::{whatami, CongestionControl, PeerId, Reliability, ResKey};
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::ZenohMessage;
use zenoh_protocol::session::{
    DummySessionEventHandler, Session, SessionEventHandler, SessionHandler, SessionManager,
    SessionManagerConfig,
};
use zenoh_util::core::ZResult;

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
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(DummySessionEventHandler::new()))
    }
}

fn print_usage(bin: String) {
    println!(
        "Usage:
    cargo run --release --bin {} <locator to connect to> <payload size in bytes> <interval in seconds>
Example:
    cargo run --release --bin {} tcp/127.0.0.1:7447 32 1",
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

    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: pid,
        handler: Arc::new(MySH::new()),
    };
    let manager = SessionManager::new(config, None);

    // Connect to publisher
    task::block_on(async {
        let attachment = None;
        let session = manager
            .open_session(&connect_to, &attachment)
            .await
            .unwrap();

        let mut count: u64 = 0;
        loop {
            // Send reliable messages
            let reliability = Reliability::Reliable;
            let congestion_control = CongestionControl::Block;
            let key = ResKey::RName("/test/ping".to_string());
            let info = None;
            let reply_context = None;
            let attachment = None;

            // u64 (8 bytes) for seq num
            // u128 (16 bytes) for system time in nanoseconds
            let mut payload = vec![0u8; size];
            let count_bytes: [u8; 8] = count.to_le_bytes();
            let now_bytes: [u8; 16] = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .to_le_bytes();
            payload[0..8].copy_from_slice(&count_bytes);
            payload[8..24].copy_from_slice(&now_bytes);

            let message = ZenohMessage::make_data(
                key,
                payload.into(),
                reliability,
                congestion_control,
                info,
                reply_context,
                attachment,
            );

            let _ = session.handle_message(message.clone()).await.unwrap();

            task::sleep(Duration::from_secs_f64(interval)).await;
            count += 1;
        }
    });
}
