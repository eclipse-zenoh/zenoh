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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use zenoh_protocol::core::*;
use zenoh_protocol::io::RBuf;
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::{ZenohMessage, SeqNum};
use zenoh_protocol::session::{MsgHandler, SessionHandler, SessionManager, SessionManagerConfig, SessionManagerOptionalConfig};
use zenoh_util::zasynclock;
use zenoh_util::core::ZResult;


// Session Handler for the router
struct SHRouter {
    session: Mutex<Vec<Arc<SCRouter>>>,
    resolution: ZInt
}

impl SHRouter {
    fn new(resolution: ZInt) -> Self {
        Self {
            session: Mutex::new(Vec::new()),
            resolution
        }
    }

    async fn get_count(&self) -> usize {
        self.session.lock().await.iter().fold(0, |acc, x|
            acc + x.count.load(Ordering::Relaxed)
        )
    }

    async fn reset_count(&self) {
        for h in self.session.lock().await.iter() {
            h.count.store(0, Ordering::Relaxed);
        }
    }
}

#[async_trait]
impl SessionHandler for SHRouter {
    async fn new_session(&self, _whatami: WhatAmI, _session: Arc<dyn MsgHandler + Send + Sync>) -> Arc<dyn MsgHandler + Send + Sync> {
        let arc = Arc::new(SCRouter::new(self.resolution));
        self.session.lock().await.push(arc.clone());
        arc
    }
}

// Session Callback for the router
pub struct SCRouter {
    count: AtomicUsize,
    last_reliable: Mutex<SeqNum>,
    last_unreliable: Mutex<SeqNum>
}

impl SCRouter {
    pub fn new(resolution: ZInt) -> Self {
        Self {
            count: AtomicUsize::new(0),
            last_reliable: Mutex::new(SeqNum::new(resolution-1, resolution)),
            last_unreliable: Mutex::new(SeqNum::new(resolution-1, resolution))
        }
    }
}

#[async_trait]
impl MsgHandler for SCRouter {
    async fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::AcqRel);
        if message.is_reliable() {
            zasynclock!(self.last_reliable)
        } else {
            zasynclock!(self.last_unreliable)
        };
        Ok(())
    }

    async fn close(&self) {}
}


// Session Handler for the client
struct SHClient {}

impl SHClient {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SessionHandler for SHClient {
    async fn new_session(&self, _whatami: WhatAmI, _session: Arc<dyn MsgHandler + Send + Sync>) -> Arc<dyn MsgHandler + Send + Sync> {
        Arc::new(SCClient::new())
    }
}

// Session Callback for the client
pub struct SCClient {
    count: AtomicZInt
}

impl SCClient {
    pub fn new() -> Self {
        Self {
            count: AtomicZInt::new(0) 
        }
    }
}

#[async_trait]
impl MsgHandler for SCClient {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    async fn close(&self) {}
}


async fn channel_base_inner() {
    // Define the locator
    let locator: Locator = "tcp/127.0.0.1:8888".parse().unwrap();

    // Default SN resolution
    let resolution: ZInt = 16_384;

    // Define client and router IDs
    let client_id = PeerId{id: vec![0u8]};
    let router_id = PeerId{id: vec![1u8]};

    // Reliable messages to send
    let messages_count: usize = 1_000;

    // The timeout before counting the received messages
    // Set it to 1000 ms for testing purposes
    let timeout = 1_000;

    // Create the router session manager
    let router_handler = Arc::new(SHRouter::new(resolution));
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id,
        handler: router_handler.clone()
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: Some(resolution),
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None 
    };
    let router_manager = SessionManager::new(config, Some(opt_config));

    // Create the client session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client_id,
        handler: Arc::new(SHClient::new())
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: Some(resolution),
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None 
    };
    let client_manager = SessionManager::new(config, Some(opt_config));

    // Create the listener on the router
    let res = router_manager.add_locator(&locator).await; 
    assert!(res.is_ok());

    // Create an empty session with the client
    // Open session -> This should be accepted
    let attachment = None;
    let res = client_manager.open_session(&locator, &attachment).await;
    assert_eq!(res.is_ok(), true);
    let session = res.unwrap();

    /* [1] */
    // Send reliable messages by using schedule()
    let reliable = true;
    let key = ResKey::RName("/test".to_string());
    let info = None;
    let payload = RBuf::from(vec![0u8; 1]);
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(reliable, key, info, payload, reply_context, attachment);

    // Schedule the messages
    for _ in 0..messages_count { 
        session.schedule(message.clone(), None).await.unwrap();
    }

    // Wait for the messages to arrive to the other side
    task::sleep(Duration::from_millis(timeout)).await;
    let count = router_handler.get_count().await; 
    assert_eq!(messages_count, count);

    // Reset the counters
    router_handler.reset_count().await;

    /* [2] */
    // Send unreliable messages by using schedule()
    let reliable = false; 
    let key = ResKey::RName("/test".to_string());
    let info = None;
    let payload = RBuf::from(vec![0u8; 1]);
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(reliable, key, info, payload, reply_context, attachment);

    // Schedule the messages
    for _ in 0..messages_count { 
        session.schedule(message.clone(), None).await.unwrap();
    }

    // Wait for the messages to arrive to the other side
    task::sleep(Duration::from_millis(timeout)).await;
    let count = router_handler.get_count().await;
    assert_eq!(messages_count, count);

    // Reset the counters
    router_handler.reset_count().await;

    /* [3] */
    // Send reliable messages by using handle_message()
    let reliable = true;
    let key = ResKey::RName("/test".to_string());
    let info = None;
    let payload = RBuf::from(vec![0u8; 1]);
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(reliable, key, info, payload, reply_context, attachment);

    // Schedule the messages
    for _ in 0..messages_count { 
        session.handle_message(message.clone()).await.unwrap();
    }

    // Wait for the messages to arrive to the other side
    task::sleep(Duration::from_millis(timeout)).await;
    let count = router_handler.get_count().await;
    assert_eq!(messages_count, count);

    // Reset the counters
    router_handler.reset_count().await;

    /* [4] */
    // Send unreliable messages by using handle_message()
    let reliable = false; 
    let key = ResKey::RName("/test".to_string());
    let info = None;
    let payload = RBuf::from(vec![0u8; 1]);
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(reliable, key, info, payload, reply_context, attachment);

    // Schedule the messages
    for _ in 0..messages_count { 
        session.handle_message(message.clone()).await.unwrap();
    }

    // Wait for the messages to arrive to the other side
    task::sleep(Duration::from_millis(timeout)).await;
    let count = router_handler.get_count().await;
    assert_eq!(messages_count, count);

    let _ = session.close().await;
}

#[test]
fn channel_base() {
    task::block_on(channel_base_inner());
}