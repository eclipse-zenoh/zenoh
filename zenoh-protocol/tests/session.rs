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
use async_std::sync::{Arc, Barrier, Mutex};
use async_std::task;
use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use zenoh_protocol::core::{whatami, PeerId};
use zenoh_protocol::link::{Link, Locator};
use zenoh_protocol::proto::ZenohMessage;
use zenoh_protocol::session::{
    DummyHandler, Session, SessionEventHandler, SessionHandler, SessionManager,
    SessionManagerConfig, SessionManagerOptionalConfig,
};

use zenoh_util::core::ZResult;
use zenoh_util::zasynclock;

const TIMEOUT: Duration = Duration::from_secs(60);
const SLEEP: Duration = Duration::from_secs(1);

#[cfg(test)]
struct SHRouterOpenClose {
    new_ses_bar: Arc<Barrier>,
    sessions: Mutex<HashMap<PeerId, Arc<Barrier>>>,
}

impl SHRouterOpenClose {
    fn new(barrier: Arc<Barrier>) -> Self {
        Self {
            new_ses_bar: barrier,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    async fn get_barrier(&self, peer: &PeerId) -> Arc<Barrier> {
        zasynclock!(self.sessions).get(peer).unwrap().clone()
    }
}

#[async_trait]
impl SessionHandler for SHRouterOpenClose {
    async fn new_session(
        &self,
        session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        let barrier = Arc::new(Barrier::new(2));
        let mh = Arc::new(MHRouterOpenClose::new(barrier.clone()));
        let peer = session.get_pid()?;
        zasynclock!(self.sessions).insert(peer, barrier);
        self.new_ses_bar.wait().await;

        Ok(mh)
    }
}

struct MHRouterOpenClose {
    barrier: Arc<Barrier>,
}

impl MHRouterOpenClose {
    fn new(barrier: Arc<Barrier>) -> Self {
        Self { barrier }
    }
}

#[async_trait]
impl SessionEventHandler for MHRouterOpenClose {
    async fn handle_message(&self, _msg: ZenohMessage) -> ZResult<()> {
        Ok(())
    }

    async fn new_link(&self, _link: Link) {
        self.barrier.wait().await;
    }

    async fn del_link(&self, _link: Link) {}

    async fn closing(&self) {
        println!("Session Open Close [***]: session is being closed on the router...");
    }

    async fn closed(&self) {
        println!("Session Open Close [***]: session has been closed on the router...");
        self.barrier.wait().await;
    }
}

// Session Handler for the client
struct SHClientOpenClose {}

impl SHClientOpenClose {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SessionHandler for SHClientOpenClose {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(DummyHandler::new()))
    }
}

async fn session_open_close(locator: Locator) {
    let attachment = None;

    /* [ROUTER] */
    let router_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);

    // Create the barrier to detect when a new session is open
    let router_new_barrier = Arc::new(Barrier::new(2));

    let router_handler = Arc::new(SHRouterOpenClose::new(router_new_barrier.clone()));
    // Create the router session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: router_handler.clone(),
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: Some(1),
        max_links: Some(2),
    };
    let router_manager = SessionManager::new(config, Some(opt_config));

    /* [CLIENT] */
    let client01_id = PeerId::new(1, [1u8; PeerId::MAX_SIZE]);
    let client02_id = PeerId::new(1, [2u8; PeerId::MAX_SIZE]);

    // Create the transport session manager for the first client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client01_id.clone(),
        handler: Arc::new(SHClientOpenClose::new()),
    };
    let client01_manager = SessionManager::new(config, None);

    // Create the transport session manager for the second client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client02_id.clone(),
        handler: Arc::new(SHClientOpenClose::new()),
    };
    let client02_manager = SessionManager::new(config, None);

    /* [1] */
    println!("\nSession Open Close [1a1]");
    // Add the locator on the router
    let res = router_manager.add_listener(&locator).await;
    println!("Session Open Close [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [1a2]");
    let locators = router_manager.get_listeners().await;
    println!("Session Open Close [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    // Open a first session from the client to the router
    // -> This should be accepted
    println!("Session Open Close [1c1]");
    let res = client01_manager.open_session(&locator, &attachment).await;
    println!("Session Open Close [1c2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    println!("Session Open Close [1d1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [1d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses1.get_pid().unwrap(), router_id);
    println!("Session Open Close [1e1]");
    let links = c_ses1.get_links().await.unwrap();
    println!("Session Open Close [1e2]: {:?}", links);
    assert_eq!(links.len(), 1);

    // Verify that the session has been open on the router
    let res = router_new_barrier.wait().timeout(TIMEOUT).await;
    assert!(res.is_ok());

    println!("Session Open Close [1f1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [1f2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_pid().unwrap(), client01_id);
    println!("Session Open Close [1g1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [1g2]: {:?}", links);
    assert_eq!(links.len(), 1);

    /* [2] */
    // Open a second session from the client to the router
    // -> This should be accepted
    println!("\nSession Open Close [2a1]");
    let res = client01_manager.open_session(&locator, &attachment).await;
    println!("Session Open Close [2a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();
    println!("Session Open Close [2b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [2b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses2.get_pid().unwrap(), router_id);
    println!("Session Open Close [2c1]");
    let links = c_ses2.get_links().await.unwrap();
    println!("Session Open Close [2c2]: {:?}", links);
    assert_eq!(links.len(), 2);
    assert_eq!(c_ses2, c_ses1);

    // Verify that the session has been open on the router
    let barrier = router_handler.get_barrier(&client01_id).await;
    let res = barrier.wait().timeout(TIMEOUT).await;
    assert!(res.is_ok());

    println!("Session Open Close [2d1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [2d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_pid().unwrap(), client01_id);
    println!("Session Open Close [2e1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [2e2]: {:?}", links);
    assert_eq!(links.len(), 2);

    /* [3] */
    // Open session -> This should be rejected because
    // of the maximum limit of links per session
    println!("\nSession Open Close [3a1]");
    let res = client01_manager.open_session(&locator, &attachment).await;
    println!("Session Open Close [3a2]: {:?}", res);
    assert!(res.is_err());
    println!("Session Open Close [3b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [3b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses1.get_pid().unwrap(), router_id);
    println!("Session Open Close [3c1]");
    let links = c_ses1.get_links().await.unwrap();
    println!("Session Open Close [3c2]: {:?}", links);
    assert_eq!(links.len(), 2);

    // Verify that the session has not been open on the router
    println!("Session Open Close [3d1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [3d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_pid().unwrap(), client01_id);
    println!("Session Open Close [3e1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [3e2]: {:?}", links);
    assert_eq!(links.len(), 2);

    /* [4] */
    // Close the open session on the client
    println!("\nSession Open Close [4a1]");
    let res = c_ses1.close().await;
    println!("Session Open Close [4a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [4b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [4b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has been closed also on the router
    let barrier = router_handler.get_barrier(&client01_id).await;
    let res = barrier.wait().timeout(TIMEOUT).await;
    assert!(res.is_ok());

    println!("Session Open Close [4c1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [4c2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    /* [5] */
    // Open session -> This should be accepted because
    // the number of links should be back to 0
    println!("\nSession Open Close [5a1]");
    let res = client01_manager.open_session(&locator, &attachment).await;
    println!("Session Open Close [5a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses3 = res.unwrap();
    println!("Session Open Close [5b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [5b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses3.get_pid().unwrap(), router_id);
    println!("Session Open Close [5c1]");
    let links = c_ses3.get_links().await.unwrap();
    println!("Session Open Close [5c2]: {:?}", links);
    assert_eq!(links.len(), 1);

    // Verify that the session has not been open on the router
    let res = router_new_barrier.wait().timeout(TIMEOUT).await;
    assert!(res.is_ok());

    println!("Session Open Close [5d1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [5d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_pid().unwrap(), client01_id);
    println!("Session Open Close [5e1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [5e2]: {:?}", links);
    assert_eq!(links.len(), 1);

    /* [6] */
    // Open session -> This should be rejected because
    // of the maximum limit of sessions
    println!("\nSession Open Close [6a1]");
    let res = client02_manager.open_session(&locator, &attachment).await;
    println!("Session Open Close [6a2]: {:?}", res);
    assert!(res.is_err());
    println!("Session Open Close [6b1]");
    let sessions = client02_manager.get_sessions().await;
    println!("Session Open Close [6b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has not been open on the router
    println!("Session Open Close [6c1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [6c2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_pid().unwrap(), client01_id);
    println!("Session Open Close [6d1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [6d2]: {:?}", links);
    assert_eq!(links.len(), 1);

    /* [7] */
    // Close the open session on the client
    println!("\nSession Open Close [7a1]");
    let res = c_ses3.close().await;
    println!("Session Open Close [7a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [7b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [7b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has been closed also on the router
    let barrier = router_handler.get_barrier(&client01_id).await;
    let res = barrier.wait().timeout(TIMEOUT).await;
    assert!(res.is_ok());

    println!("Session Open Close [7c1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [7c2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    /* [8] */
    // Open session -> This should be accepted because
    // the number of sessions should be back to 0
    println!("\nSession Open Close [8a1]");
    let res = client02_manager.open_session(&locator, &attachment).await;
    println!("Session Open Close [8a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses4 = res.unwrap();
    println!("Session Open Close [8b1]");
    let sessions = client02_manager.get_sessions().await;
    println!("Session Open Close [8b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    println!("Session Open Close [8c1]");
    let links = c_ses4.get_links().await.unwrap();
    println!("Session Open Close [8c2]: {:?}", links);
    assert_eq!(links.len(), 1);

    // Verify that the session has been open on the router
    let res = router_new_barrier.wait().timeout(TIMEOUT).await;
    assert!(res.is_ok());

    println!("Session Open Close [8d1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [8d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_pid().unwrap(), client02_id);
    println!("Session Open Close [8e1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [8e2]: {:?}", links);
    assert_eq!(links.len(), 1);

    /* [9] */
    // Close the open session on the client
    println!("Session Open Close [9a1]");
    let res = c_ses4.close().await;
    println!("Session Open Close [9a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [9b1]");
    let sessions = client02_manager.get_sessions().await;
    println!("Session Open Close [9b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has been closed also on the router
    let barrier = router_handler.get_barrier(&client02_id).await;
    let res = barrier.wait().timeout(TIMEOUT).await;
    assert!(res.is_ok());

    println!("Session Open Close [9c1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [9c2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    /* [10] */
    // Perform clean up of the open locators
    println!("\nSession Open Close [10a1]");
    let res = router_manager.del_listener(&locator).await;
    println!("Session Open Close [10a2]: {:?}", res);
    assert!(res.is_ok());

    task::sleep(SLEEP).await;
}

#[cfg(feature = "transport_tcp")]
#[test]
fn session_tcp() {
    let locator: Locator = "tcp/127.0.0.1:7447".parse().unwrap();
    task::block_on(async {
        session_open_close(locator.clone()).await;
    });
}

#[cfg(feature = "transport_udp")]
#[test]
fn session_udp() {
    let locator: Locator = "udp/127.0.0.1:7447".parse().unwrap();
    task::block_on(async {
        session_open_close(locator.clone()).await;
    });
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn session_unix() {
    env_logger::init();
    let _ = std::fs::remove_file("zenoh-test-unix-socket-9.sock");
    let locator: Locator = "unixsock-stream/zenoh-test-unix-socket-9.sock"
        .parse()
        .unwrap();
    task::block_on(async {
        session_open_close(locator.clone()).await;
    });
    let _ = std::fs::remove_file("zenoh-test-unix-socket-9.sock");
}
