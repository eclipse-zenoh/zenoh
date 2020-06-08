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
use async_std::task;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use rand::RngCore;
use std::time::Instant;
use zenoh_protocol::core::{PeerId, ResKey, ZInt};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::{Primitives, SubInfo, Reliability, SubMode, QueryConsolidation, QueryTarget, Reply, Mux, DeMux, WhatAmI, whatami};
use zenoh_protocol::session::{SessionManager, SessionManagerConfig, SessionHandler, MsgHandler};

const N: usize = 100_000;

struct Stats {
    count: usize,
    start: Instant
}

impl Stats {

    pub fn print(&self) {
        let elapsed = self.start.elapsed().as_secs_f64();
        let thpt = N as f64 / elapsed;
        println!("{} msg/s", thpt);
    }
}

pub struct ThrouputPrimitives {
    stats: Mutex<Stats>,
}

impl ThrouputPrimitives {
    pub fn new() -> ThrouputPrimitives {
        ThrouputPrimitives {
            stats: Mutex::new(Stats {
                count: 0,
                start: Instant::now()
            })
        }
    }
}

impl Default for ThrouputPrimitives {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Primitives for ThrouputPrimitives {

    async fn resource(&self, _rid: ZInt, _reskey: &ResKey) {}
    async fn forget_resource(&self, _rid: ZInt) {}
    
    async fn publisher(&self, _reskey: &ResKey) {}
    async fn forget_publisher(&self, _reskey: &ResKey) {}
    
    async fn subscriber(&self, _reskey: &ResKey, _sub_info: &SubInfo) {}
    async fn forget_subscriber(&self, _reskey: &ResKey) {}
    
    async fn queryable(&self, _reskey: &ResKey) {}
    async fn forget_queryable(&self, _reskey: &ResKey) {}

    async fn data(&self, _reskey: &ResKey, _reliable: bool, _info: &Option<RBuf>, _payload: RBuf) {
        let mut stats = self.stats.lock().await;
        if stats.count == 0 {
            stats.start = Instant::now();
            stats.count += 1;
        } else if stats.count < N {
            stats.count += 1;
        } else {
            stats.print();
            stats.count = 0;
        }  
    }
    async fn query(&self, _reskey: &ResKey, _predicate: &str, _qid: ZInt, _target: QueryTarget, _consolidation: QueryConsolidation) {}
    async fn reply(&self, _qid: ZInt, _reply: &Reply) {}
    async fn pull(&self, _is_final: bool, _reskey: &ResKey, _pull_id: ZInt, _max_samples: &Option<ZInt>) {}

    async fn close(&self) {}
}

struct LightSessionHandler {
    pub handler: Mutex<Option<Arc<dyn MsgHandler + Send + Sync>>>,
}

impl LightSessionHandler {
    pub fn new() -> LightSessionHandler {
        LightSessionHandler { handler: Mutex::new(None),}
    }
}

#[async_trait]
impl SessionHandler for LightSessionHandler {
    async fn new_session(&self, _whatami: WhatAmI, session: Arc<dyn MsgHandler + Send + Sync>) -> Arc<dyn MsgHandler + Send + Sync> {
        *self.handler.lock().await = Some(session);
        Arc::new(DeMux::new(ThrouputPrimitives::new()))
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
    let connect_to: Locator = if let Ok(v) = value.parse() {
        v
    } else {
        return print_usage(bin);
    };

    let session_handler = Arc::new(LightSessionHandler::new());
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: PeerId{id: pid.clone()},
        handler: session_handler.clone()
    };
    let manager = SessionManager::new(config, None);

    task::block_on(async {
        let attachment = None;
        if let Err(_err) =  manager.open_session(&connect_to, &attachment).await {
            println!("Unable to connect to {}!", connect_to);
            return
        }
    
        let primitives = Mux::new(session_handler.handler.lock().await.as_ref().unwrap().clone());

        primitives.resource(1, &"/tp".to_string().into()).await;
        let rid = ResKey::RId(1);
        let sub_info = SubInfo {
            reliability: Reliability::Reliable,
            mode: SubMode::Push,
            period: None
        };
        primitives.subscriber(&rid, &sub_info).await;

        // Wait forever
        future::pending::<()>().await;
    });
}