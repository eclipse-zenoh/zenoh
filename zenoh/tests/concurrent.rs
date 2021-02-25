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

use zenoh::net::protocol::core::{whatami, CongestionControl, PeerId, Reliability, ResKey, ZInt};
use zenoh::net::protocol::io::RBuf;
use zenoh::net::protocol::link::{Link, Locator};
use zenoh::net::protocol::proto::ZenohMessage;
use zenoh::net::protocol::session::{
    Session, SessionDispatcher, SessionEventHandler, SessionHandler, SessionManager,
    SessionManagerConfig, SessionManagerOptionalConfig,
};

use zenoh_util::core::ZResult;

const MSG_COUNT: usize = 1_000;
const MSG_SIZE: usize = 1_024;
const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_millis(100);

// Session Handler for the router
struct SHPeer {
    count: Arc<AtomicUsize>,
}

impl SHPeer {
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
impl SessionHandler for SHPeer {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let mh = Arc::new(MHPeer::new(self.count.clone()));
        Ok(mh)
    }
}

struct MHPeer {
    count: Arc<AtomicUsize>,
}

impl MHPeer {
    fn new(count: Arc<AtomicUsize>) -> Self {
        Self { count }
    }
}

#[async_trait]
impl SessionEventHandler for MHPeer {
    async fn handle_message(&self, _msg: ZenohMessage) -> ZResult<()> {
        self.count.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    async fn new_link(&self, _link: Link) {}
    async fn del_link(&self, _link: Link) {}
    async fn closing(&self) {}
    async fn closed(&self) {}
}

async fn session_concurrent(locator01: Vec<Locator>, locator02: Vec<Locator>) {
    // Common session lease in milliseconds
    let lease: ZInt = 1_000;

    /* [Peers] */
    let peer_id01 = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);
    let peer_id02 = PeerId::new(1, [2u8; PeerId::MAX_SIZE]);

    // let router_handler = Arc::new(SHPeer::new(router_new_barrier.clone()));

    // Create the peer01 session manager
    let peer_sh01 = Arc::new(SHPeer::new());
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: peer_id01.clone(),
        handler: SessionDispatcher::SessionHandler(peer_sh01.clone()),
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: Some(lease),
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: None,
        link_authenticator: None,
        locator_property: None,
    };
    let peer01_manager = SessionManager::new(config, Some(opt_config));

    // Create the peer01 session manager
    let peer_sh02 = Arc::new(SHPeer::new());
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::PEER,
        id: peer_id02.clone(),
        handler: SessionDispatcher::SessionHandler(peer_sh02.clone()),
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: Some(lease),
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: None,
        link_authenticator: None,
        locator_property: None,
    };
    let peer02_manager = SessionManager::new(config, Some(opt_config));

    // Barrier to synchronize the two tasks
    let barrier_peer = Arc::new(Barrier::new(2));
    let barrier_open_wait = Arc::new(Barrier::new(locator01.len() + locator02.len()));
    let barrier_open_done = Arc::new(Barrier::new(locator01.len() + locator02.len() + 2));

    // Peer01
    let c_barp = barrier_peer.clone();
    let c_barow = barrier_open_wait.clone();
    let c_barod = barrier_open_done.clone();
    let c_pid02 = peer_id02.clone();
    let c_loc01 = locator01.clone();
    let c_loc02 = locator02.clone();
    let peer01_task = task::spawn(async move {
        // Add the locators on the first peer
        for loc in c_loc01.iter() {
            let res = peer01_manager.add_listener(&loc).await;
            println!("[Session Peer 01] => Adding locator {:?}: {:?}", loc, res);
            assert!(res.is_ok());
        }
        let locs = peer01_manager.get_listeners().await;
        println!(
            "[Session Peer 01] => Getting locators: {:?} {:?}",
            c_loc01, locs
        );
        assert_eq!(c_loc01.len(), locs.len());

        // Open the session with the second peer
        for loc in c_loc02.iter() {
            let cc_barow = c_barow.clone();
            let cc_barod = c_barod.clone();
            let c_p01m = peer01_manager.clone();
            let c_loc = loc.clone();
            task::spawn(async move {
                println!("[Session Peer 01] => Waiting for opening session");
                // Syncrhonize before opening the sessions
                cc_barow.wait().timeout(TIMEOUT).await.unwrap();

                let res = c_p01m.open_session(&c_loc).await;
                println!(
                    "[Session Peer 01] => Opening session with {:?}: {:?}",
                    c_loc, res
                );
                assert!(res.is_ok());

                // Syncrhonize after opening the sessions
                cc_barod.wait().timeout(TIMEOUT).await.unwrap();
            });
        }

        // Syncrhonize after opening the sessions
        c_barod.wait().timeout(TIMEOUT).await.unwrap();

        // Verify that the session has been correctly open
        assert_eq!(peer01_manager.get_sessions().await.len(), 1);
        let s02 = peer01_manager.get_session(&c_pid02).await.unwrap();
        assert_eq!(
            s02.get_links().await.unwrap().len(),
            c_loc01.len() + c_loc02.len()
        );

        // Create the message to send
        let key = ResKey::RName("/test02".to_string());
        let payload = RBuf::from(vec![0u8; MSG_SIZE]);
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

        // Synchronize wit the peer
        c_barp.wait().timeout(TIMEOUT).await.unwrap();
        for _ in 0..MSG_COUNT {
            s02.schedule(message.clone()).await.unwrap();
        }

        // Wait for the messages to arrive to the other side
        let count = async {
            while peer_sh02.get_count() != MSG_COUNT {
                task::sleep(SLEEP).await;
            }
        };
        let res = count.timeout(TIMEOUT).await;
        assert!(res.is_ok());

        // Synchronize wit the peer
        c_barp.wait().timeout(TIMEOUT).await.unwrap();

        let _ = s02.close().await;
    });

    // Peer02
    let c_barp = barrier_peer.clone();
    let c_barow = barrier_open_wait.clone();
    let c_barod = barrier_open_done.clone();
    let c_pid01 = peer_id01.clone();
    let c_loc01 = locator01.clone();
    let c_loc02 = locator02.clone();
    let peer02_task = task::spawn(async move {
        // Add the locators on the first peer
        for loc in c_loc02.iter() {
            let res = peer02_manager.add_listener(&loc).await;
            println!("[Session Peer 02] => Adding locator {:?}: {:?}", loc, res);
            assert!(res.is_ok());
        }
        let locs = peer02_manager.get_listeners().await;
        println!(
            "[Session Peer 02] => Getting locators: {:?} {:?}",
            c_loc02, locs
        );
        assert_eq!(c_loc02.len(), locs.len());

        // Open the session with the first peer
        for loc in c_loc01.iter() {
            let cc_barow = c_barow.clone();
            let cc_barod = c_barod.clone();
            let c_p02m = peer02_manager.clone();
            let c_loc = loc.clone();
            task::spawn(async move {
                println!("[Session Peer 02] => Waiting for opening session");
                // Syncrhonize before opening the sessions
                cc_barow.wait().timeout(TIMEOUT).await.unwrap();

                let res = c_p02m.open_session(&c_loc).await;
                println!(
                    "[Session Peer 02] => Opening session with {:?}: {:?}",
                    c_loc, res
                );
                assert!(res.is_ok());

                // Syncrhonize after opening the sessions
                cc_barod.wait().timeout(TIMEOUT).await.unwrap();
            });
        }

        // Syncrhonize after opening the sessions
        c_barod.wait().timeout(TIMEOUT).await.unwrap();

        // Verify that the session has been correctly open
        println!(
            "[Session Peer 02] => Sessions: {:?}",
            peer02_manager.get_sessions().await
        );
        assert_eq!(peer02_manager.get_sessions().await.len(), 1);
        let s01 = peer02_manager.get_session(&c_pid01).await.unwrap();
        assert_eq!(
            s01.get_links().await.unwrap().len(),
            c_loc01.len() + c_loc02.len()
        );

        // Create the message to send
        let key = ResKey::RName("/test02".to_string());
        let payload = RBuf::from(vec![0u8; MSG_SIZE]);
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

        // Synchronize wit the peer
        c_barp.wait().timeout(TIMEOUT).await.unwrap();
        for _ in 0..MSG_COUNT {
            s01.schedule(message.clone()).await.unwrap();
        }

        // Wait for the messages to arrive to the other side
        let count = async {
            while peer_sh01.get_count() != MSG_COUNT {
                task::sleep(SLEEP).await;
            }
        };
        let res = count.timeout(TIMEOUT).await;
        assert!(res.is_ok());

        // Synchronize wit the peer
        c_barp.wait().timeout(TIMEOUT).await.unwrap();

        let _ = s01.close().await;
    });

    peer01_task.join(peer02_task).await;

    task::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn session_tcp_concurrent() {
    let locator01: Vec<Locator> = vec![
        "tcp/127.0.0.1:7447".parse().unwrap(),
        "tcp/127.0.0.1:7448".parse().unwrap(),
        "tcp/127.0.0.1:7449".parse().unwrap(),
        "tcp/127.0.0.1:7450".parse().unwrap(),
        "tcp/127.0.0.1:7451".parse().unwrap(),
        "tcp/127.0.0.1:7452".parse().unwrap(),
        "tcp/127.0.0.1:7453".parse().unwrap(),
        "tcp/127.0.0.1:7454".parse().unwrap(),
    ];
    let locator02: Vec<Locator> = vec![
        "tcp/127.0.0.1:7455".parse().unwrap(),
        "tcp/127.0.0.1:7456".parse().unwrap(),
        "tcp/127.0.0.1:7457".parse().unwrap(),
        "tcp/127.0.0.1:7458".parse().unwrap(),
        "tcp/127.0.0.1:7459".parse().unwrap(),
        "tcp/127.0.0.1:7460".parse().unwrap(),
        "tcp/127.0.0.1:7461".parse().unwrap(),
        "tcp/127.0.0.1:7462".parse().unwrap(),
    ];

    task::block_on(async {
        session_concurrent(locator01, locator02).await;
    });
}
