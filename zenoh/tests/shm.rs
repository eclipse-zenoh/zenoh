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
use async_std::sync::{Arc, Mutex};
use async_std::task;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use zenoh::net::protocol::core::{whatami, CongestionControl, PeerId, Reliability, ResKey};
use zenoh::net::protocol::io::{RBuf, SharedMemoryManager};
use zenoh::net::protocol::link::{Link, Locator};
use zenoh::net::protocol::proto::{Data, ZenohBody, ZenohMessage};
use zenoh::net::protocol::session::authenticator::SharedMemoryAuthenticator;
use zenoh::net::protocol::session::{
    Session, SessionDispatcher, SessionEventHandler, SessionHandler, SessionManager,
    SessionManagerConfig, SessionManagerOptionalConfig,
};
use zenoh_util::core::ZResult;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

const MSG_COUNT: usize = 1_000;
const MSG_SIZE: usize = 1_024;

// Session Handler for the router
struct SHPeer {
    count: Arc<AtomicUsize>,
    shm: Option<Arc<Mutex<SharedMemoryManager>>>,
}

impl SHPeer {
    fn new(mut shm: Option<SharedMemoryManager>) -> Self {
        let shm = match shm.take() {
            Some(shm) => Some(Arc::new(Mutex::new(shm))),
            None => None,
        };
        Self {
            count: Arc::new(AtomicUsize::new(0)),
            shm,
        }
    }

    fn get_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl SessionHandler for SHPeer {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let arc = Arc::new(SCPeer::new(self.count.clone(), self.shm.clone()));
        Ok(arc)
    }
}

// Session Callback for the peer
pub struct SCPeer {
    count: Arc<AtomicUsize>,
    shm: Option<Arc<Mutex<SharedMemoryManager>>>,
}

impl SCPeer {
    pub fn new(count: Arc<AtomicUsize>, shm: Option<Arc<Mutex<SharedMemoryManager>>>) -> Self {
        Self { count, shm }
    }
}

#[async_trait]
impl SessionEventHandler for SCPeer {
    async fn handle_message(&self, message: ZenohMessage) -> ZResult<()> {
        let len = match self.shm.as_ref() {
            Some(shm) => {
                let mut shm = shm.lock().await;
                match message.body {
                    ZenohBody::Data(Data { payload, .. }) => {
                        print!("s");
                        let sbuf = payload.into_shm(&mut shm).unwrap();
                        sbuf.as_slice().len()
                    }
                    _ => panic!("Unsolicited message"),
                }
            }
            None => match message.body {
                ZenohBody::Data(Data { payload, .. }) => {
                    print!("n");
                    payload.len()
                }
                _ => panic!("Unsolicited message"),
            },
        };
        assert_eq!(len, MSG_SIZE);
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}
    async fn del_link(&self, _link: Link) {}
    async fn closing(&self) {}
    async fn closed(&self) {}
}

async fn run(locator: &Locator) {
    // Define client and router IDs
    let peer_shm01 = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let peer_shm02 = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);
    let peer_net01 = PeerId::new(1, [2u8; PeerId::MAX_SIZE]);

    // Create the SharedMemoryManager
    let mut shm01 = SharedMemoryManager::new("peer_shm01".to_string(), 2 * MSG_SIZE).unwrap();
    let shm02 = SharedMemoryManager::new("peer_shm02".to_string(), 2 * MSG_SIZE).unwrap();

    // Create a peer manager with zero-copy authenticator enabled
    let peer_shm01_handler = Arc::new(SHPeer::new(None));
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: peer_shm01.clone(),
        handler: SessionDispatcher::SessionHandler(peer_shm01_handler.clone()),
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![SharedMemoryAuthenticator::new().into()]),
        link_authenticator: None,
        locator_property: None,
    };
    let peer_shm01_manager = SessionManager::new(config, Some(opt_config));

    // Create a peer manager with zero-copy authenticator enabled
    let peer_shm02_handler = Arc::new(SHPeer::new(Some(shm02)));
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: peer_shm02.clone(),
        handler: SessionDispatcher::SessionHandler(peer_shm02_handler.clone()),
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![SharedMemoryAuthenticator::new().into()]),
        link_authenticator: None,
        locator_property: None,
    };
    let peer_shm02_manager = SessionManager::new(config, Some(opt_config));

    // Create a peer manager with zero-copy authenticator disabled
    let peer_net01_handler = Arc::new(SHPeer::new(None));
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: peer_net01.clone(),
        handler: SessionDispatcher::SessionHandler(peer_net01_handler.clone()),
    };
    let peer_net01_manager = SessionManager::new(config, None);

    // Create the listener on the peer
    println!("\nSession SHM [1a]");
    let _ = peer_shm01_manager
        .add_listener(locator)
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    // Create a session with the peer
    println!("Session SHM [1b]");
    let _ = peer_shm02_manager
        .open_session(locator)
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    // Create a session with the peer
    println!("Session SHM [1c]");
    let _ = peer_net01_manager
        .open_session(locator)
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    // Retrieve the sessions
    println!("Session SHM [2a]");
    let peer_shm02_session = peer_shm01_manager
        .get_session(&peer_shm02)
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    println!("Session SHM [2b]");
    let peer_net01_session = peer_shm01_manager
        .get_session(&peer_net01)
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    // Create the message to send
    println!("Session SHM [4a]");
    let mut sbuf = shm01.alloc(MSG_SIZE).unwrap();
    let bs = unsafe { sbuf.as_mut_slice() };
    bs.fill(0u8);

    println!("Session SHM [4b]");
    let key = ResKey::RName("/test".to_string());
    let payload: RBuf = sbuf.into();
    let reliability = Reliability::Reliable;
    let congestion_control = CongestionControl::Block;
    let data_info = None;
    let routing_context = None;
    let reply_context = None;
    let attachment = None;
    let message = ZenohMessage::make_data(
        key,
        payload,
        reliability,
        congestion_control,
        data_info,
        routing_context,
        reply_context,
        attachment,
    );

    // Send the message
    println!("Session SHM [5a]");
    for _ in 0..MSG_COUNT {
        peer_shm02_session
            .handle_message(message.clone())
            .await
            .unwrap();
    }

    // Wait a little bit
    task::sleep(SLEEP).await;

    // Wait for the messages to arrive to the other side
    println!("\nSession SHM [5b]");
    let count = async {
        while peer_shm02_handler.get_count() != MSG_COUNT {
            task::sleep(SLEEP).await;
        }
    };
    let _ = count.timeout(TIMEOUT).await.unwrap();

    // Send the message
    println!("Session SHM [6a]");
    for _ in 0..MSG_COUNT {
        peer_net01_session
            .handle_message(message.clone())
            .await
            .unwrap();
    }

    // Wait a little bit
    task::sleep(SLEEP).await;

    // Wait for the messages to arrive to the other side
    println!("\nSession SHM [6b]");
    let count = async {
        while peer_net01_handler.get_count() != MSG_COUNT {
            task::sleep(SLEEP).await;
        }
    };
    let _ = count.timeout(TIMEOUT).await.unwrap();

    // Wait a little bit
    task::sleep(SLEEP).await;

    // Close the sessions
    println!("Session SHM [7a]");
    let _ = peer_shm02_session
        .close()
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    println!("Session SHM [7b]");
    let _ = peer_net01_session
        .close()
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    // Wait a little bit
    task::sleep(SLEEP).await;

    // Delete the listener
    println!("Session SHM [8a]");
    let _ = peer_shm01_manager
        .del_listener(locator)
        .timeout(TIMEOUT)
        .await
        .unwrap()
        .unwrap();

    // Wait a little bit
    task::sleep(SLEEP).await;
}

#[cfg(all(feature = "transport_tcp", feature = "zero-copy"))]
#[test]
fn session_tcp_shm() {
    let locator: Locator = "tcp/127.0.0.1:12447".parse().unwrap();
    task::block_on(run(&locator));
}
