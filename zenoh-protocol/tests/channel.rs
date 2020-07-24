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
use async_std::prelude::*;
use async_std::sync::{Arc, Barrier};
use async_std::task;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use zenoh_protocol::core::{PeerId, ResKey, whatami};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::link::{Link, Locator};
use zenoh_protocol::proto::ZenohMessage;
use zenoh_protocol::session::{SessionEventHandler, Session, SessionHandler, SessionManager, SessionManagerConfig};
use zenoh_util::core::ZResult;


const TIMEOUT: Duration = Duration::from_secs(60);
// Messages to send at each test
const MSG_COUNT: usize = 1_000;


// Session Handler for the router
struct SHRouter {
    barrier: Arc<Barrier>
}

impl SHRouter {
    fn new(barrier: Arc<Barrier>) -> Self {
        Self { barrier }
    }
}

#[async_trait]
impl SessionHandler for SHRouter {
    async fn new_session(&self, _session: Session) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let arc = Arc::new(SCRouter::new(self.barrier.clone()));        
        Ok(arc)
    }
}

// Session Callback for the router
pub struct SCRouter {
    barrier: Arc<Barrier>,
    count: AtomicUsize
}

impl SCRouter {
    pub fn new(barrier: Arc<Barrier>) -> Self {
        Self {
            barrier,
            count: AtomicUsize::new(0)
        }
    }
}

#[async_trait]
impl SessionEventHandler for SCRouter {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::SeqCst);
        if self.count.load(Ordering::SeqCst) == MSG_COUNT {
            self.count.store(0, Ordering::SeqCst);
            let res = self.barrier.wait().timeout(TIMEOUT).await;
            assert!(res.is_ok());            
        }
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}

    async fn del_link(&self, _link: Link) {}

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
    async fn new_session(&self, _session: Session) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(SCClient::new()))
    }
}

// Session Callback for the client
pub struct SCClient {
    count: AtomicUsize
}

impl SCClient {
    pub fn new() -> Self {
        Self {
            count: AtomicUsize::new(0) 
        }
    }
}

#[async_trait]
impl SessionEventHandler for SCClient {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}

    async fn del_link(&self, _link: Link) {}

    async fn close(&self) {}
}


async fn channel_base_inner() {
    // Define the locator
    let locator: Locator = "tcp/127.0.0.1:8888".parse().unwrap();

    // Define client and router IDs
    let client_id = PeerId{id: vec![0u8]};
    let router_id = PeerId{id: vec![1u8]};

    // The barrier for synchronizing the test
    let barrier = Arc::new(Barrier::new(2));

    // Create the router session manager
    let router_handler = Arc::new(SHRouter::new(barrier.clone()));
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id,
        handler: router_handler.clone()
    };
    let router_manager = SessionManager::new(config, None);

    // Create the client session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client_id,
        handler: Arc::new(SHClient::new())
    };
    let client_manager = SessionManager::new(config, None);

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
    println!("Sending {} reliable messages via schedule()", MSG_COUNT);
    for _ in 0..MSG_COUNT { 
        session.schedule(message.clone(), None).await.unwrap();
    }

    // Wait for the messages to arrive to the other side
    let res = barrier.wait().timeout(TIMEOUT).await;    
    assert!(res.is_ok());

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
    println!("Sending {} best effort messages via schedule()", MSG_COUNT);
    for _ in 0..MSG_COUNT { 
        session.schedule(message.clone(), None).await.unwrap();
    }

    // Wait for the messages to arrive to the other side
    let res = barrier.wait().timeout(TIMEOUT).await;    
    assert!(res.is_ok());

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
    println!("Sending {} reliable messages via handle_message()", MSG_COUNT);
    for _ in 0..MSG_COUNT { 
        session.handle_message(message.clone()).await.unwrap();
    }

    // Wait for the messages to arrive to the other side
    let res = barrier.wait().timeout(TIMEOUT).await;    
    assert!(res.is_ok());

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
    println!("Sending {} best effort messages via handle_message()", MSG_COUNT);
    for _ in 0..MSG_COUNT { 
        session.handle_message(message.clone()).await.unwrap();
    }

    // Wait for the messages to arrive to the other side
    let res = barrier.wait().timeout(TIMEOUT).await;    
    assert!(res.is_ok());

    let _ = session.close().await;
}

#[test]
fn channel_base() {
    task::block_on(channel_base_inner());
}