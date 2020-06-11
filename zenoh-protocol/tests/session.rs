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
use std::time::Duration;

use zenoh_protocol::core::{PeerId, ZInt};
use zenoh_protocol::link::Locator;
use zenoh_protocol::proto::{WhatAmI, whatami};
use zenoh_protocol::session::{
    DummyHandler,
    MsgHandler,
    SessionHandler,
    SessionManager,
    SessionManagerConfig,
    SessionManagerOptionalConfig
};


// Session Handler for the router
struct SHRouter {}

impl SHRouter {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SessionHandler for SHRouter {
    async fn new_session(&self, _whatami: WhatAmI, _session: Arc<dyn MsgHandler + Send + Sync>) -> Arc<dyn MsgHandler + Send + Sync> {
        Arc::new(DummyHandler::new())
    }
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
        Arc::new(DummyHandler::new())
    }
}

async fn session_lease(locator: Locator) {
    let attachment = None;
    
    // Common session lease in milliseconds
    let lease: ZInt = 1_000;
    
    // The timeout for veritification
    // Set it to 1000 ms for testing purposes
    let timeout: u64 = 1_000;

    /* [ROUTER] */
    let router_id = PeerId { id: vec![0u8] };

    // Create the router session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: Arc::new(SHRouter::new())
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: Some(lease),
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None 
    };
    let router_manager = SessionManager::new(config, Some(opt_config));


    /* [CLIENT] */
    let client01_id = PeerId { id: vec![1u8] };

    // Create the transport session manager for the first client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client01_id.clone(),
        handler: Arc::new(SHClient::new())
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: Some(lease),
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None 
    };
    let client01_manager = SessionManager::new(config, Some(opt_config));

    /* [1] */
    println!("Session Lease [1a1]");
    // Add the locator on the router
    let res = router_manager.add_locator(&locator).await; 
    println!("Session Lease [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Lease [1a2]");
    let locators = router_manager.get_locators().await;
    println!("Session Lease [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a session from the client to the router 
    println!("Session Lease [2c1]");
    let res = client01_manager.open_session(&locator, &attachment).await;    
    println!("Session Lease [2c2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    println!("Session Lease [2d1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Lease [2d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses1.get_peer().unwrap(), router_id);
    println!("Session Lease [2e1]");
    let links = c_ses1.get_links().await.unwrap();
    println!("Session Lease [2e2]: {:?}", links);
    assert_eq!(links.len(), 1);

    /* [3] */
    // Verify that the session has been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    println!("Session Lease [3a1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Lease [3b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    println!("Session Lease [3c1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Lease [3d2]: {:?}", links);
    assert_eq!(links.len(), 1);
    
    /* [4] */
    // Close all the links to trigger session lease expiration
    println!("Session Lease [4a1]");
    let mut links = c_ses1.get_links().await.unwrap();
    println!("Session Lease [4a2]: {:?}", links);
    assert_eq!(links.len(), 1);
    for l in links.drain(..) {
        let res = c_ses1.close_link(&l).await;
        println!("Session Lease [4a3]: {:?}", res);
        assert!(res.is_ok());
    }

    // Wait for the session to expire
    task::sleep(Duration::from_millis(3 * lease as u64)).await;

    /* [5] */
    // Verify that the session has been closed on the router
    println!("Session Lease [5a1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Lease [5a2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has been closed on the client
    println!("Session Lease [5b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Lease [5b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session handler is no longer valid
    println!("Session Lease [5c1]");
    let peer = c_ses1.get_peer();
    println!("Session Lease [5c2]: {:?}", peer);
    assert!(peer.is_err());

    /* [6] */
    // Perform clean up of the open locators
    println!("Session Open Close [6a1]");
    let res = router_manager.del_locator(&locator).await;
    println!("Session Open Close [6a2]: {:?}", res);
}

async fn session_open_close(locator: Locator) {
    let attachment = None;

    /* [ROUTER] */
    let router_id = PeerId { id: vec![0u8] };

    // Create the router session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: Arc::new(SHRouter::new())
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: Some(1),
        max_links: Some(2) 
    };
    let router_manager = SessionManager::new(config, Some(opt_config));


    /* [CLIENT] */
    let client01_id = PeerId { id: vec![1u8] };
    let client02_id = PeerId { id: vec![2u8] };

    // The timeout when opening a session
    // Set it to 1000 ms for testing purposes
    let timeout: u64 = 1_000;
    let retries = 1;

    // Create the transport session manager for the first client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client01_id.clone(),
        handler: Arc::new(SHClient::new())
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: Some(timeout),
        retries: Some(retries),
        max_sessions: None,
        max_links: None 
    };
    let client01_manager = SessionManager::new(config, Some(opt_config));

    // Create the transport session manager for the second client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client02_id.clone(),
        handler: Arc::new(SHClient::new())
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: Some(timeout),
        retries: Some(retries),
        max_sessions: None,
        max_links: None 
    };
    let client02_manager = SessionManager::new(config, Some(opt_config));


    /* [1] */
    println!("Session Open Close [1a1]");
    // Add the locator on the router
    let res = router_manager.add_locator(&locator).await; 
    println!("Session Open Close [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [1a2]");
    let locators = router_manager.get_locators().await;
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
    assert_eq!(c_ses1.get_peer().unwrap(), router_id);
    println!("Session Open Close [1e1]");
    let links = c_ses1.get_links().await.unwrap();
    println!("Session Open Close [1e2]: {:?}", links);
    assert_eq!(links.len(), 1);

    // Verify that the session has been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    println!("Session Open Close [1f1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [1f2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    println!("Session Open Close [1g1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [1g2]: {:?}", links);
    assert_eq!(links.len(), 1);


    /* [2] */
    // Open a second session from the client to the router 
    // -> This should be accepted
    println!("Session Open Close [2a1]");
    let res = client01_manager.open_session(&locator, &attachment).await;
    println!("Session Open Close [2a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();
    println!("Session Open Close [2b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [2b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses2.get_peer().unwrap(), router_id);
    println!("Session Open Close [2c1]");
    let links = c_ses2.get_links().await.unwrap();
    println!("Session Open Close [2c2]: {:?}", links);
    assert_eq!(links.len(), 2);
    assert_eq!(c_ses2, c_ses1);

    // Verify that the session has been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    println!("Session Open Close [2d1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [2d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    println!("Session Open Close [2e1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [2e2]: {:?}", links);
    assert_eq!(links.len(), 2);


    /* [3] */
    // Open session -> This should be rejected because
    // of the maximum limit of links per session
    println!("Session Open Close [3a1]");
    let res = client01_manager.open_session(&locator, &attachment).await;
    println!("Session Open Close [3a2]: {:?}", res);
    assert!(res.is_err());
    println!("Session Open Close [3b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [3b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses1.get_peer().unwrap(), router_id);
    println!("Session Open Close [3c1]");
    let links = c_ses1.get_links().await.unwrap();
    println!("Session Open Close [3c2]: {:?}", links);
    assert_eq!(links.len(), 2);

    // Verify that the session has not been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    println!("Session Open Close [3d1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [3d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    println!("Session Open Close [3e1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [3e2]: {:?}", links);
    assert_eq!(links.len(), 2);


    /* [4] */
    // Close the open session on the client
    println!("Session Open Close [4a1]");
    let res = c_ses1.close().await;
    println!("Session Open Close [4a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [4b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [4b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has been closed also on the router
    task::sleep(Duration::from_millis(timeout)).await;
    println!("Session Open Close [4c1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [4c2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);


    /* [5] */
    // Open session -> This should be accepted because
    // the number of links should be back to 0
    println!("Session Open Close [5a1]");
    let res = client01_manager.open_session(&locator, &attachment).await;
    println!("Session Open Close [5a2]: {:?}", res);
    assert!(res.is_ok());
    let c_ses3 = res.unwrap();
    println!("Session Open Close [5b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [5b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    assert_eq!(c_ses3.get_peer().unwrap(), router_id);
    println!("Session Open Close [5c1]");
    let links = c_ses3.get_links().await.unwrap();
    println!("Session Open Close [5c2]: {:?}", links);
    assert_eq!(links.len(), 1);

    // Verify that the session has not been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    println!("Session Open Close [5d1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [5d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    println!("Session Open Close [5e1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [5e2]: {:?}", links);
    assert_eq!(links.len(), 1);


    /* [6] */
    // Open session -> This should be rejected because
    // of the maximum limit of sessions
    println!("Session Open Close [6a1]");
    let res = client02_manager.open_session(&locator, &attachment).await;
    println!("Session Open Close [6a2]: {:?}", res);
    assert!(res.is_err());
    println!("Session Open Close [6b1]");
    let sessions = client02_manager.get_sessions().await;
    println!("Session Open Close [6b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has not been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    println!("Session Open Close [6c1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [6c2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    println!("Session Open Close [6d1]");
    let links = r_ses1.get_links().await.unwrap();
    println!("Session Open Close [6d2]: {:?}", links);
    assert_eq!(links.len(), 1);


    /* [7] */
    // Close the open session on the client
    println!("Session Open Close [7a1]");
    let res = c_ses3.close().await;
    println!("Session Open Close [7a2]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Open Close [7b1]");
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [7b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has been closed also on the router
    task::sleep(Duration::from_millis(timeout)).await;
    println!("Session Open Close [7c1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [7c2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);


    /* [8] */
    // Open session -> This should be accepted because
    // the number of sessions should be back to 0
    println!("Session Open Close [8a1]");
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

    // Verify that the session has not been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    println!("Session Open Close [8d1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [8d2]: {:?}", sessions);
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client02_id);
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
    let sessions = client01_manager.get_sessions().await;
    println!("Session Open Close [9b2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    // Verify that the session has been closed also on the router
    task::sleep(Duration::from_millis(timeout)).await;
    println!("Session Open Close [9c1]");
    let sessions = router_manager.get_sessions().await;
    println!("Session Open Close [9c2]: {:?}", sessions);
    assert_eq!(sessions.len(), 0);

    /* [10] */
    // Perform clean up of the open locators
    println!("Session Open Close [10a1]");
    let res = router_manager.del_locator(&locator).await;
    println!("Session Open Close [10a2]: {:?}", res);
}

#[test]
fn session_tcp() {
    env_logger::init();

    let locator: Locator = "tcp/127.0.0.1:8888".parse().unwrap();
    task::block_on(async {
        session_open_close(locator.clone()).await;
        session_lease(locator).await;
    });
}