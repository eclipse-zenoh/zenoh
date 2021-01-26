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
use std::collections::HashMap;
use zenoh_protocol::core::{whatami, PeerId};
use zenoh_protocol::link::{Link, Locator};
use zenoh_protocol::proto::ZenohMessage;
use zenoh_protocol::session::authenticator::UserPasswordAuthenticator;
use zenoh_protocol::session::{
    DummySessionEventHandler, Session, SessionEventHandler, SessionHandler, SessionManager,
    SessionManagerConfig, SessionManagerOptionalConfig,
};
use zenoh_util::core::ZResult;

#[cfg(test)]
struct SHRouterAuthenticator;

impl SHRouterAuthenticator {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionHandler for SHRouterAuthenticator {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(MHRouterAuthenticator::new()))
    }
}

struct MHRouterAuthenticator;

impl MHRouterAuthenticator {
    fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SessionEventHandler for MHRouterAuthenticator {
    async fn handle_message(&self, _msg: ZenohMessage) -> ZResult<()> {
        Ok(())
    }
    async fn new_link(&self, _link: Link) {}
    async fn del_link(&self, _link: Link) {}
    async fn closing(&self) {}
    async fn closed(&self) {}
}

// Session Handler for the client
struct SHClientAuthenticator {}

impl SHClientAuthenticator {
    fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SessionHandler for SHClientAuthenticator {
    async fn new_session(
        &self,
        _session: Session,
    ) -> ZResult<Arc<dyn SessionEventHandler + Send + Sync>> {
        Ok(Arc::new(DummySessionEventHandler::new()))
    }
}

async fn authenticator_user_password(locator: Locator) {
    let user = "user".to_string();
    let password = "password".to_string();

    /* [ROUTER] */
    let router_id = PeerId::new(1, [0u8; PeerId::MAX_SIZE]);
    let router_handler = Arc::new(SHRouterAuthenticator::new());
    // Create the router session manager
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::ROUTER,
        id: router_id.clone(),
        handler: router_handler.clone(),
    };
    let mut lookup: HashMap<String, String> = HashMap::new();
    lookup.insert(user.clone(), password.clone());
    let peer_authenticator = UserPasswordAuthenticator::new(lookup, ("foo".into(), "foo".into()));
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![Arc::new(peer_authenticator)]),
        link_authenticator: None,
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
        handler: Arc::new(SHClientAuthenticator::new()),
    };
    let lookup: HashMap<String, String> = HashMap::new();
    let peer_authenticator = UserPasswordAuthenticator::new(lookup, (user, password));
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![Arc::new(peer_authenticator)]),
        link_authenticator: None,
    };
    let client01_manager = SessionManager::new(config, Some(opt_config));

    // Create the transport session manager for the second client
    let config = SessionManagerConfig {
        version: 0,
        whatami: whatami::CLIENT,
        id: client02_id.clone(),
        handler: Arc::new(SHClientAuthenticator::new()),
    };
    let user = "user".to_string();
    let password = "invalid".to_string();
    let lookup: HashMap<String, String> = HashMap::new();
    let peer_authenticator = UserPasswordAuthenticator::new(lookup, (user, password));
    let opt_config = SessionManagerOptionalConfig {
        lease: None,
        keep_alive: None,
        sn_resolution: None,
        batch_size: None,
        timeout: None,
        retries: None,
        max_sessions: None,
        max_links: None,
        peer_authenticator: Some(vec![Arc::new(peer_authenticator)]),
        link_authenticator: None,
    };
    let client02_manager = SessionManager::new(config, Some(opt_config));

    /* [1] */
    println!("\nSession Authenticator [1a1]");
    // Add the locator on the router
    let res = router_manager.add_listener(&locator).await;
    println!("Session Authenticator [1a1]: {:?}", res);
    assert!(res.is_ok());
    println!("Session Authenticator [1a2]");
    let locators = router_manager.get_listeners().await;
    println!("Session Authenticator [1a2]: {:?}", locators);
    assert_eq!(locators.len(), 1);

    /* [2] */
    // Open a first session from the client to the router
    // -> This should be accepted
    println!("Session Authenticator [2a1]");
    let res = client01_manager.open_session(&locator).await;
    println!("Session Authenticator [2a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [3] */
    println!("Session Authenticator [3a1]");
    let res = c_ses1.close().await;
    println!("Session Authenticator [3a1]: {:?}", res);
    assert!(res.is_ok());

    /* [4] */
    // Open a second session from the client to the router
    // -> This should be rejected
    println!("Session Authenticator [4a1]");
    let res = client02_manager.open_session(&locator).await;
    println!("Session Authenticator [4a1]: {:?}", res);
    assert!(res.is_err());

    /* [5] */
    // Open a third session from the client to the router
    // -> This should be accepted
    println!("Session Authenticator [5a1]");
    let res = client01_manager.open_session(&locator).await;
    println!("Session Authenticator [5a1]: {:?}", res);
    assert!(res.is_ok());
    let c_ses1 = res.unwrap();

    /* [6] */
    println!("Session Authenticator [6a1]");
    let res = c_ses1.close().await;
    println!("Session Authenticator [6a1]: {:?}", res);
    assert!(res.is_ok());
}

#[cfg(feature = "transport_tcp")]
#[test]
fn authenticator_tcp() {
    let locator: Locator = "tcp/127.0.0.1:7447".parse().unwrap();
    task::block_on(async {
        authenticator_user_password(locator).await;
    });
}

#[cfg(feature = "transport_udp")]
#[test]
fn authenticator_udp() {
    let locator: Locator = "udp/127.0.0.1:7447".parse().unwrap();
    task::block_on(async {
        authenticator_user_password(locator).await;
    });
}

#[cfg(all(feature = "transport_unixsock-stream", target_family = "unix"))]
#[test]
fn authenticator_unix() {
    let _ = std::fs::remove_file("zenoh-test-unix-socket-9.sock");
    let locator: Locator = "unixsock-stream/zenoh-test-unix-socket-9.sock"
        .parse()
        .unwrap();
    task::block_on(async {
        authenticator_user_password(locator).await;
    });
    let _ = std::fs::remove_file("zenoh-test-unix-socket-9.sock");
}
