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
use async_std::sync::Arc;
use async_std::task;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use zenoh_protocol::core::{whatami, CongestionControl, PeerId, Reliability, ResKey};
use zenoh_protocol::io::RBuf;
use zenoh_protocol::link::{Link, Locator};
use zenoh_protocol::proto::ZenohMessage;
use zenoh_protocol::session::{
    Session, SessionEventHandler, SessionHandler, SessionManager, SessionManagerConfig,
};
use zenoh_util::core::ZResult;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);
const MSG_COUNT: usize = 1_000;
const MSG_SIZE: usize = 1_024;

// Session Handler for the router
struct SHRouter {
    count: Arc<AtomicUsize>,
}

impl SHRouter {
    fn new() -> Self {
        Self {
            count: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SessionHandler for SHRouter {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let arc = Arc::new(SCRouter::new(self.count.clone()));
        Ok(arc)
    }
}

// Session Callback for the router
pub struct SCRouter {
    count: Arc<AtomicUsize>,
}

impl SCRouter {
    pub fn new(count: Arc<AtomicUsize>) -> Self {
        Self { count }
    }
}

#[async_trait]
impl SessionEventHandler for SCRouter {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::SeqCst);
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
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(SCClient::new()))
    }
}

// Session Callback for the client
pub struct SCClient;

impl SCClient {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionEventHandler for SCClient {
    async fn handle_message(&self, _message: ZenohMessage) -> ZResult<()> {
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}

    async fn del_link(&self, _link: Link) {}

    async fn close(&self) {}
}

async fn channel_reliable(locators: Vec<Locator>) {
    // Define client and router IDs
    let client_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    // Create the router session manager
    let router_handler = Arc::new(SHRouter::new());
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: router_handler.clone(),
    };
    let router_manager = SessionManager::new(config, None);

    // Create the client session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client_id,
        handler: Arc::new(SHClient::new()),
    };
    let client_manager = SessionManager::new(config, None);

    // Create the listener on the router
    for l in locators.iter() {
        let res = router_manager.add_locator(l).await;
        assert!(res.is_ok());
    }

    // Create an empty session with the client
    // Open session -> This should be accepted
    let attachment = None;
    for l in locators.iter() {
        let res = client_manager.open_session(l, &attachment).await;
        assert_eq!(res.is_ok(), true);
    }
    let session = client_manager.get_session(&router_id).await.unwrap();

    // Create the message to send
    let key = ResKey::RName("/test".to_string());
    let payload = RBuf::from(vec![0u8; MSG_SIZE]);
    let reliability = Reliability::Reliable;
    let congestion_control = CongestionControl::Block;
    let data_info = None;
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(
        key,
        payload,
        reliability,
        congestion_control,
        data_info,
        reply_context,
        attachment,
    );

    // Send reliable messages by using schedule()
    println!("Sending {} reliable messages...", MSG_COUNT);
    for _ in 0..MSG_COUNT {
        session.schedule(message.clone()).await.unwrap();
    }

    // Wait for the messages to arrive to the other side
    let count = async {
        while router_handler.get_count() != MSG_COUNT {
            task::yield_now().await;
        }
    };
    let res = count.timeout(TIMEOUT).await;
    assert!(res.is_ok());

    let res = session.close().await;
    assert!(res.is_ok());

    for l in locators.iter() {
        let res = router_manager.del_locator(l).await;
        assert!(res.is_ok());
    }

    task::sleep(SLEEP).await;
}

async fn channel_reliable_droppable(locators: Vec<Locator>) {
    // Define client and router IDs
    let client_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    // Create the router session manager
    let router_handler = Arc::new(SHRouter::new());
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: router_handler.clone(),
    };
    let router_manager = SessionManager::new(config, None);

    // Create the client session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client_id,
        handler: Arc::new(SHClient::new()),
    };
    let client_manager = SessionManager::new(config, None);

    // Create the listener on the router
    for l in locators.iter() {
        let res = router_manager.add_locator(l).await;
        assert!(res.is_ok());
    }

    // Create an empty session with the client
    // Open session -> This should be accepted
    let attachment = None;
    for l in locators.iter() {
        let res = client_manager.open_session(l, &attachment).await;
        assert_eq!(res.is_ok(), true);
    }
    let session = client_manager.get_session(&router_id).await.unwrap();

    // Create the message to send
    let key = ResKey::RName("/test".to_string());
    let payload = RBuf::from(vec![0u8; MSG_SIZE]);
    let reliability = Reliability::Reliable;
    let congestion_control = CongestionControl::Drop;
    let data_info = None;
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(
        key,
        payload,
        reliability,
        congestion_control,
        data_info,
        reply_context,
        attachment,
    );

    /* [1] */
    // Send unreliable messages by using schedule()
    println!("Sending {} reliable droppable messages...", MSG_COUNT);
    for _ in 0..MSG_COUNT {
        session.schedule(message.clone()).await.unwrap();
    }

    // Wait to receive something
    let count = async {
        while router_handler.get_count() == 0 {
            task::yield_now().await;
        }
    };
    let res = count.timeout(TIMEOUT).await;
    assert!(res.is_ok());

    // Check if at least one message has arrived to the other side
    assert_ne!(router_handler.get_count(), 0);

    let res = session.close().await;
    assert!(res.is_ok());

    for l in locators.iter() {
        let res = router_manager.del_locator(l).await;
        assert!(res.is_ok());
    }

    task::sleep(SLEEP).await;
}

async fn channel_best_effort(locators: Vec<Locator>) {
    // Define client and router IDs
    let client_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    // Create the router session manager
    let router_handler = Arc::new(SHRouter::new());
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: router_handler.clone(),
    };
    let router_manager = SessionManager::new(config, None);

    // Create the client session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client_id,
        handler: Arc::new(SHClient::new()),
    };
    let client_manager = SessionManager::new(config, None);

    // Create the listener on the router
    for l in locators.iter() {
        let res = router_manager.add_locator(l).await;
        assert!(res.is_ok());
    }

    // Create an empty session with the client
    // Open session -> This should be accepted
    let attachment = None;
    for l in locators.iter() {
        let res = client_manager.open_session(l, &attachment).await;
        assert_eq!(res.is_ok(), true);
    }
    let session = client_manager.get_session(&router_id).await.unwrap();

    // Create the message to send
    let key = ResKey::RName("/test".to_string());
    let payload = RBuf::from(vec![0u8; MSG_SIZE]);
    let reliability = Reliability::BestEffort;
    let congestion_control = CongestionControl::Block;
    let data_info = None;
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(
        key,
        payload,
        reliability,
        congestion_control,
        data_info,
        reply_context,
        attachment,
    );

    /* [1] */
    // Send unreliable messages by using schedule()
    println!("Sending {} best effort messages...", MSG_COUNT);
    for _ in 0..MSG_COUNT {
        session.schedule(message.clone()).await.unwrap();
    }

    // Wait to receive something
    let count = async {
        while router_handler.get_count() == 0 {
            task::yield_now().await;
        }
    };
    let res = count.timeout(TIMEOUT).await;
    assert!(res.is_ok());

    // Check if at least one message has arrived to the other side
    assert_ne!(router_handler.get_count(), 0);

    let res = session.close().await;
    assert!(res.is_ok());

    for l in locators.iter() {
        let res = router_manager.del_locator(l).await;
        assert!(res.is_ok());
    }

    task::sleep(SLEEP).await;
}

async fn channel_best_effort_droppable(locators: Vec<Locator>) {
    // Define client and router IDs
    let client_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);

    // Create the router session manager
    let router_handler = Arc::new(SHRouter::new());
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: router_handler.clone(),
    };
    let router_manager = SessionManager::new(config, None);

    // Create the client session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client_id,
        handler: Arc::new(SHClient::new()),
    };
    let client_manager = SessionManager::new(config, None);

    // Create the listener on the router
    for l in locators.iter() {
        let res = router_manager.add_locator(l).await;
        assert!(res.is_ok());
    }

    // Create an empty session with the client
    // Open session -> This should be accepted
    let attachment = None;
    for l in locators.iter() {
        let res = client_manager.open_session(l, &attachment).await;
        assert_eq!(res.is_ok(), true);
    }
    let session = client_manager.get_session(&router_id).await.unwrap();

    // Create the message to send
    let key = ResKey::RName("/test".to_string());
    let payload = RBuf::from(vec![0u8; MSG_SIZE]);
    let reliability = Reliability::BestEffort;
    let congestion_control = CongestionControl::Drop;
    let data_info = None;
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(
        key,
        payload,
        reliability,
        congestion_control,
        data_info,
        reply_context,
        attachment,
    );

    /* [1] */
    // Send unreliable messages by using schedule()
    println!("Sending {} best effort messages...", MSG_COUNT);
    for _ in 0..MSG_COUNT {
        session.schedule(message.clone()).await.unwrap();
    }

    // Wait to receive something
    let count = async {
        while router_handler.get_count() == 0 {
            task::yield_now().await;
        }
    };
    let res = count.timeout(TIMEOUT).await;
    assert!(res.is_ok());

    // Check if at least one message has arrived to the other side
    assert_ne!(router_handler.get_count(), 0);

    let res = session.close().await;
    assert!(res.is_ok());

    for l in locators.iter() {
        let res = router_manager.del_locator(l).await;
        assert!(res.is_ok());
    }

    task::sleep(SLEEP).await;
}

#[test]
fn channel_tcp() {
    // Define the locator
    let locator: Vec<Locator> = vec!["tcp/127.0.0.1:7447".parse().unwrap()];
    task::block_on(async {
        channel_reliable(locator.clone()).await;
        channel_reliable_droppable(locator.clone()).await;
        channel_best_effort(locator.clone()).await;
        channel_best_effort_droppable(locator).await;
    });
}

#[test]
fn channel_udp() {
    // Define the locator
    let locator: Vec<Locator> = vec!["udp/127.0.0.1:7447".parse().unwrap()];
    task::block_on(async {
        // channel_reliable_droppable(locator.clone()).await;
        channel_best_effort(locator.clone()).await;
        // channel_best_effort_droppable(locator).await;
    });
}

#[test]
fn channel_tcp_udp() {
    // Define the locator
    let locator: Vec<Locator> = vec![
        "tcp/127.0.0.1:7448".parse().unwrap(),
        "udp/127.0.0.1:7448".parse().unwrap(),
    ];
    task::block_on(async {
        channel_reliable(locator.clone()).await;
        channel_reliable_droppable(locator.clone()).await;
        channel_best_effort(locator.clone()).await;
        channel_best_effort_droppable(locator).await;
    });
}
