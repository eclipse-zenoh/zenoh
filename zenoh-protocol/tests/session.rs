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

use zenoh_protocol::core::PeerId;
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


async fn run(locator: Locator) {
    let attachment = None;
    
    /* [ROUTER] */
    let router_id = PeerId{id: vec![0u8]};

    // Create the router session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: Arc::new(SHRouter::new())
    };
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        sn_resolution: None,
        batchsize: None,
        timeout: None,
        retries: None,
        max_sessions: Some(1),
        max_links: Some(2) 
    };
    let router_manager = SessionManager::new(config, Some(opt_config));


    /* [CLIENT] */
    let client01_id = PeerId{id: vec![1u8]};
    let client02_id = PeerId{id: vec![2u8]};

    // The timeout when opening a session
    // Set it to 1000 ms for testing purposes
    let timeout = 1_000;
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
        sn_resolution: None,
        batchsize: None,
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
        sn_resolution: None,
        batchsize: None,
        timeout: Some(timeout),
        retries: Some(retries),
        max_sessions: None,
        max_links: None 
    };
    let client02_manager = SessionManager::new(config, Some(opt_config));


    /* [1] */
    // Add the locator on the router
    let res = router_manager.add_locator(&locator).await; 
    assert!(res.is_ok());
    assert_eq!(router_manager.get_locators().await.len(), 1);

    // Open a first session from the client to the router 
    // -> This should be accepted
    let res = client01_manager.open_session(&locator, &attachment).await;    
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();
    assert_eq!(client01_manager.get_sessions().await.len(), 1);
    assert_eq!(c_ses1.get_peer().unwrap(), router_id);
    assert_eq!(c_ses1.get_links().await.unwrap().len(), 1);

    // Verify that the session has been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    let sessions = router_manager.get_sessions().await;
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    assert_eq!(r_ses1.get_links().await.unwrap().len(), 1);


    /* [2] */
    // Open a second session from the client to the router 
    // -> This should be accepted
    let res = client01_manager.open_session(&locator, &attachment).await;
    assert!(res.is_ok());
    let c_ses2 = res.unwrap();
    assert_eq!(client01_manager.get_sessions().await.len(), 1);
    assert_eq!(c_ses2.get_peer().unwrap(), router_id);
    assert_eq!(c_ses2.get_links().await.unwrap().len(), 2);
    assert_eq!(c_ses2, c_ses1);

    // Verify that the session has been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    let sessions = router_manager.get_sessions().await;
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    assert_eq!(r_ses1.get_links().await.unwrap().len(), 2);


    /* [3] */
    // Open session -> This should be rejected because
    // of the maximum limit of links per session
    let res = client01_manager.open_session(&locator, &attachment).await;
    assert!(res.is_err());
    assert_eq!(client01_manager.get_sessions().await.len(), 1);
    assert_eq!(c_ses1.get_peer().unwrap(), router_id);
    assert_eq!(c_ses1.get_links().await.unwrap().len(), 2);

    // Verify that the session has not been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    let sessions = router_manager.get_sessions().await;
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    assert_eq!(r_ses1.get_links().await.unwrap().len(), 2);


    /* [4] */
    // Close the open session on the client
    let res = c_ses1.close().await;
    assert!(res.is_ok());
    assert_eq!(client01_manager.get_sessions().await.len(), 0);

    // Verify that the session has been closed also on the router
    task::sleep(Duration::from_millis(timeout)).await;
    let sessions = router_manager.get_sessions().await;
    assert_eq!(sessions.len(), 0);


    /* [5] */
    // Open session -> This should be accepted because
    // the number of links should be back to 0
    let res = client01_manager.open_session(&locator, &attachment).await;
    assert!(res.is_ok());
    let c_ses3 = res.unwrap();
    assert_eq!(client01_manager.get_sessions().await.len(), 1);
    assert_eq!(c_ses3.get_peer().unwrap(), router_id);
    assert_eq!(c_ses3.get_links().await.unwrap().len(), 1);

    // Verify that the session has not been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    let sessions = router_manager.get_sessions().await;
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    assert_eq!(r_ses1.get_links().await.unwrap().len(), 1);


    /* [6] */
    // Open session -> This should be rejected because
    // of the maximum limit of sessions
    let res = client02_manager.open_session(&locator, &attachment).await;
    assert!(res.is_err());
    assert_eq!(client02_manager.get_sessions().await.len(), 0);

    // Verify that the session has not been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    let sessions = router_manager.get_sessions().await;
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client01_id);
    assert_eq!(r_ses1.get_links().await.unwrap().len(), 1);


    /* [7] */
    // Close the open session on the client
    let res = c_ses3.close().await;
    assert!(res.is_ok());
    assert_eq!(client01_manager.get_sessions().await.len(), 0);

    // Verify that the session has been closed also on the router
    task::sleep(Duration::from_millis(timeout)).await;
    let sessions = router_manager.get_sessions().await;
    assert_eq!(sessions.len(), 0);


    /* [8] */
    // Open session -> This should be accepted because
    // the number of sessions should be back to 0
    let res = client02_manager.open_session(&locator, &attachment).await;
    assert!(res.is_ok());
    let c_ses4 = res.unwrap();
    assert_eq!(client02_manager.get_sessions().await.len(), 1);
    assert_eq!(c_ses4.get_links().await.unwrap().len(), 1);

    // Verify that the session has not been open on the router
    task::sleep(Duration::from_millis(timeout)).await;
    let sessions = router_manager.get_sessions().await;
    assert_eq!(sessions.len(), 1);
    let r_ses1 = &sessions[0];
    assert_eq!(r_ses1.get_peer().unwrap(), client02_id);
    assert_eq!(r_ses1.get_links().await.unwrap().len(), 1);


    /* [9] */
    // Close the open session on the client
    let res = c_ses4.close().await;
    assert!(res.is_ok());
    assert_eq!(client01_manager.get_sessions().await.len(), 0);

    // Verify that the session has been closed also on the router
    task::sleep(Duration::from_millis(timeout)).await;
    let sessions = router_manager.get_sessions().await;
    assert_eq!(sessions.len(), 0);
}

#[test]
fn session_tcp() {
    let locator: Locator = "tcp/127.0.0.1:8888".parse().unwrap();
    task::block_on(run(locator));
}